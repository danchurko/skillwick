#!/usr/bin/env python3
"""Validate the exact staged tree with an offline installed candidate."""

from __future__ import annotations

import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile


def run(command: list[str], *, cwd: Path, env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(command, cwd=cwd, env=env, check=True, text=True)


def snapshot(repo: Path, destination: Path) -> None:
    tree = subprocess.check_output(["git", "write-tree"], cwd=repo, text=True).strip()
    archive = destination / "staged.tar"
    run(["git", "archive", "--format=tar", "--output", str(archive), tree], cwd=repo)
    with tarfile.open(archive, mode="r:") as bundle:
        members = bundle.getmembers()
        for member in members:
            target = (destination / "tree" / member.name).resolve()
            if destination.joinpath("tree").resolve() not in target.parents and target != destination.joinpath("tree").resolve():
                raise RuntimeError(f"staged archive contains unsafe path: {member.name}")
            if member.issym() or member.islnk():
                raise RuntimeError(f"staged archive contains a link: {member.name}")
        (destination / "tree").mkdir()
        bundle.extractall(destination / "tree")


def main() -> int:
    repo = Path(
        subprocess.check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip()
    )
    tmp_parent = os.environ.get("TMPDIR")
    temp_kwargs = {"dir": tmp_parent} if tmp_parent and Path(tmp_parent).is_dir() else {}
    cache_parent = Path(
        os.environ.get(
            "SKILLWICK_PRECOMMIT_CACHE",
            Path(os.environ.get("XDG_CACHE_HOME", Path.home() / ".cache"))
            / "skillwick/pre-commit",
        )
    )
    cache_parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="skillwick-pre-commit-", **temp_kwargs) as directory:
        temporary = Path(directory)
        snapshot(repo, temporary)
        tree = temporary / "tree"
        prefix = temporary / "prefix"
        environment = os.environ.copy()
        environment["CARGO_TARGET_DIR"] = str(cache_parent / "target")
        environment["CARGO_NET_OFFLINE"] = "true"
        run(
            [
                "cargo",
                "install",
                "--locked",
                "--offline",
                "--path",
                str(tree),
                "--root",
                str(prefix),
            ],
            cwd=tree,
            env=environment,
        )
        binary = prefix / "bin" / "skillwick"
        if not binary.is_file() or binary.is_symlink():
            raise RuntimeError(f"candidate installation did not produce a regular binary: {binary}")
        run(["make", "check"], cwd=tree, env=environment)
        run(
            ["sh", "scripts/verify-source-install.sh", "--binary", str(binary), "--skip-build"],
            cwd=tree,
            env=environment,
        )
    print("pre-commit: staged snapshot checks passed")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"pre-commit: ERROR: {error}", file=sys.stderr)
        raise SystemExit(1) from error
