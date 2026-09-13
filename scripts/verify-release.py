#!/usr/bin/env python3
"""Verify Skillwick archives, checksums, installer behavior, and formula metadata."""

from __future__ import annotations

import argparse
import hashlib
import os
import platform
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
from typing import Optional
import urllib.error
import urllib.request
from pathlib import Path


TARGETS = ("aarch64-apple-darwin", "x86_64-apple-darwin")
ARCHIVE_MEMBERS = ("LICENSE-APACHE", "LICENSE-MIT", "README.md", "skillwick")
VERSION_PATTERN = re.compile(r"\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?\Z")
URL_PATTERN = re.compile(r'\burl\s+["\']([^"\']+)["\']')
SHA_PATTERN = re.compile(r'\bsha256\s+["\']([0-9A-Fa-f]{64})["\']')


class VerificationError(RuntimeError):
    """An actionable release-contract failure."""


def parser() -> argparse.ArgumentParser:
    root = Path(__file__).resolve().parents[1]
    command = argparse.ArgumentParser(
        description=(
            "Verify both supported Skillwick release archives, their checksums, "
            "the isolated installer path, and Homebrew formula metadata."
        )
    )
    command.add_argument(
        "--version",
        required=True,
        help="release version without (or with) a leading v, for example 0.1.5",
    )
    assets = command.add_mutually_exclusive_group(required=True)
    assets.add_argument(
        "--archive-dir",
        type=Path,
        help="directory containing local archive and .sha256 assets",
    )
    assets.add_argument(
        "--base-url",
        help="release asset directory URL, for example the GitHub download URL",
    )
    command.add_argument(
        "--formula",
        type=Path,
        default=root / "Formula" / "skillwick.rb",
        help="Homebrew formula to verify (default: Formula/skillwick.rb)",
    )
    command.add_argument(
        "--skip-formula",
        action="store_true",
        help="defer package-manager metadata until published archive digests are available",
    )
    command.add_argument(
        "--installer",
        type=Path,
        default=root / "scripts" / "install.sh",
        help="supported shell installer to exercise (default: scripts/install.sh)",
    )
    command.add_argument(
        "--manifest",
        type=Path,
        default=root / "Cargo.toml",
        help="tested Cargo manifest whose package version must match",
    )
    command.add_argument(
        "--source-binary",
        type=Path,
        help="optional already-tested executable whose --version output is checked",
    )
    command.add_argument(
        "--execute-target",
        choices=TARGETS,
        help="archive target to execute (defaults to the host target)",
    )
    command.add_argument(
        "--skip-execution",
        action="store_true",
        help="only for non-macOS shape/checksum gates; do not run an archive binary",
    )
    command.add_argument(
        "--skip-installer",
        action="store_true",
        help="only for non-macOS shape/checksum gates; do not run scripts/install.sh",
    )
    return command


def normalized_version(raw: str) -> str:
    version = raw[1:] if raw.startswith("v") else raw
    if not VERSION_PATTERN.fullmatch(version):
        raise VerificationError(f"invalid release version: {raw!r}")
    return version


def require_file(path: Path, label: str) -> Path:
    if not path.is_file() or path.is_symlink():
        raise VerificationError(f"{label} is not a regular file: {path}")
    return path


def host_target() -> Optional[str]:
    if platform.system() != "Darwin":
        return None
    machine = platform.machine()
    if machine == "arm64":
        return "aarch64-apple-darwin"
    if machine == "x86_64":
        return "x86_64-apple-darwin"
    return None


def execution_command(target: str, host: Optional[str]) -> Optional[list[str]]:
    if target == host:
        return []
    if host == "aarch64-apple-darwin" and target == "x86_64-apple-darwin":
        arch = shutil.which("arch")
        if arch is not None:
            return [arch, "-x86_64"]
    return None


def run_checked(
    argv: list[str], *, env: dict[str, str], cwd: Optional[Path] = None
) -> subprocess.CompletedProcess[str]:
    try:
        result = subprocess.run(
            argv,
            cwd=cwd,
            env=env,
            check=False,
            capture_output=True,
            text=True,
            timeout=60,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise VerificationError(f"could not run {' '.join(argv)}: {error}") from error
    if result.returncode != 0:
        detail = (result.stderr or result.stdout).strip().replace("\n", " | ")
        raise VerificationError(
            f"{' '.join(argv)} failed with exit {result.returncode}: {detail}"
        )
    return result


def isolated_environment(root: Path) -> dict[str, str]:
    environment = {
        "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
        "HOME": str(root / "home"),
        "CODEX_HOME": str(root / "codex"),
        "XDG_CONFIG_HOME": str(root / "config"),
        "XDG_CACHE_HOME": str(root / "cache"),
        "XDG_STATE_HOME": str(root / "state"),
    }
    for key in ("HOME", "CODEX_HOME", "XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_STATE_HOME"):
        Path(environment[key]).mkdir(parents=True, exist_ok=True)
    return environment


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def locate_asset(directory: Path, name: str) -> Path:
    matches = sorted(
        path
        for path in directory.rglob(name)
        if path.is_file() and not path.is_symlink()
    )
    if len(matches) != 1:
        found = ", ".join(str(path) for path in matches) or "none"
        raise VerificationError(f"expected exactly one {name} under {directory}; found {found}")
    return matches[0]


def fetch_assets(
    version: str,
    archive_dir: Optional[Path],
    base_url: Optional[str],
    destination: Path,
) -> tuple[dict[str, Path], str]:
    destination.mkdir(parents=True, exist_ok=True)
    assets: dict[str, Path] = {}
    if archive_dir is not None:
        if not archive_dir.is_dir() or archive_dir.is_symlink():
            raise VerificationError(f"archive directory is not a directory: {archive_dir}")
        for target in TARGETS:
            archive_name = f"skillwick-{target}.tar.xz"
            checksum_name = f"{archive_name}.sha256"
            for name in (archive_name, checksum_name):
                source = locate_asset(archive_dir, name)
                target_path = destination / name
                shutil.copyfile(source, target_path)
                assets[name] = target_path
        return assets, destination.as_uri()

    assert base_url is not None
    clean_url = base_url.rstrip("/")
    for target in TARGETS:
        archive_name = f"skillwick-{target}.tar.xz"
        checksum_name = f"{archive_name}.sha256"
        for name in (archive_name, checksum_name):
            url = f"{clean_url}/{name}"
            request = urllib.request.Request(
                url,
                headers={"User-Agent": f"skillwick-release-verifier/{version}"},
            )
            try:
                with urllib.request.urlopen(request, timeout=60) as response:
                    target_path = destination / name
                    with target_path.open("wb") as stream:
                        shutil.copyfileobj(response, stream)
            except (OSError, urllib.error.URLError) as error:
                raise VerificationError(f"could not download {url}: {error}") from error
            assets[name] = target_path
    return assets, clean_url


def checksum_asset(archive: Path, checksum_file: Path) -> str:
    lines = [line.strip() for line in checksum_file.read_text(encoding="utf-8").splitlines() if line.strip()]
    if len(lines) != 1:
        raise VerificationError(f"{checksum_file.name} must contain one non-empty checksum line")
    fields = lines[0].split()
    if len(fields) != 2 or Path(fields[1].lstrip("*")).name != archive.name:
        raise VerificationError(f"{checksum_file.name} does not name {archive.name}")
    expected = fields[0].lower()
    if not re.fullmatch(r"[0-9a-f]{64}", expected):
        raise VerificationError(f"{checksum_file.name} contains an invalid SHA-256 digest")
    actual = sha256(archive)
    if actual != expected:
        raise VerificationError(f"{archive.name} checksum mismatch: expected {expected}, got {actual}")
    return actual


def archive_layout(archive: Path, target: str, extraction_root: Path) -> Path:
    root_name = f"skillwick-{target}"
    expected_names = {root_name} | {f"{root_name}/{member}" for member in ARCHIVE_MEMBERS}
    try:
        with tarfile.open(archive, mode="r:xz") as bundle:
            members = bundle.getmembers()
            names = {member.name for member in members}
            if names != expected_names or len(members) != len(expected_names):
                missing = sorted(expected_names - names)
                extra = sorted(names - expected_names)
                raise VerificationError(
                    f"{archive.name} layout mismatch (missing={missing or 'none'}, extra={extra or 'none'})"
                )
            root_member = next(member for member in members if member.name == root_name)
            if not root_member.isdir():
                raise VerificationError(f"{archive.name} root entry is not a directory")
            executable_member = next(member for member in members if member.name == f"{root_name}/skillwick")
            for member in members:
                if member.name != root_name and not member.isreg():
                    raise VerificationError(f"{archive.name} contains non-regular file {member.name}")
            if executable_member.mode & 0o111 == 0:
                raise VerificationError(f"{archive.name} executable is not marked executable")
            extracted = extraction_root / root_name / "skillwick"
            extracted.parent.mkdir(parents=True, exist_ok=True)
            source = bundle.extractfile(executable_member)
            if source is None:
                raise VerificationError(f"{archive.name} executable could not be read")
            extracted.write_bytes(source.read())
            extracted.chmod(executable_member.mode & 0o777 or 0o755)
            return extracted
    except (tarfile.TarError, OSError) as error:
        raise VerificationError(f"could not inspect {archive.name}: {error}") from error


def manifest_version(manifest: Path) -> str:
    require_file(manifest, "Cargo manifest")
    text = manifest.read_text(encoding="utf-8")
    match = re.search(r"(?m)^\s*version\s*=\s*[\"']([^\"']+)[\"']", text)
    if match is None:
        raise VerificationError(f"could not find package version in {manifest}")
    return match.group(1)


def formula_digests(formula: Path, version: str) -> dict[str, str]:
    require_file(formula, "Homebrew formula")
    text = formula.read_text(encoding="utf-8")
    version_match = re.search(r"(?m)^\s*version\s+[\"']([^\"']+)[\"']", text)
    if version_match is None or version_match.group(1) != version:
        actual = version_match.group(1) if version_match else "missing"
        raise VerificationError(f"formula version is {actual}, expected {version}")

    urls = list(URL_PATTERN.finditer(text))
    result: dict[str, str] = {}
    for index, url_match in enumerate(urls):
        url = url_match.group(1)
        target_match = re.search(r"skillwick-(aarch64-apple-darwin|x86_64-apple-darwin)\.tar\.xz\Z", url)
        if target_match is None:
            continue
        target = target_match.group(1)
        archive_name = f"skillwick-{target}.tar.xz"
        expected_url = (
            f"https://github.com/churdaa/skillwick/releases/download/v{version}/{archive_name}"
        )
        if url != expected_url:
            raise VerificationError(f"formula URL for {target} is {url}, expected {expected_url}")
        if target in result:
            raise VerificationError(f"formula contains duplicate URL entries for {target}")
        end = urls[index + 1].start() if index + 1 < len(urls) else len(text)
        checksums = SHA_PATTERN.findall(text[url_match.end() : end])
        if len(checksums) != 1:
            raise VerificationError(f"formula URL for {target} must have one following sha256")
        result[target] = checksums[0].lower()
    if set(result) != set(TARGETS):
        raise VerificationError(f"formula must contain both supported targets; found {sorted(result)}")
    return result


def version_output(
    executable: Path,
    version: str,
    environment: dict[str, str],
    command_prefix: Optional[list[str]] = None,
) -> None:
    command = command_prefix or []
    result = run_checked(command + [str(executable), "--version"], env=environment)
    expected = f"skillwick {version}"
    if result.stdout.strip() != expected:
        raise VerificationError(
            f"{executable} reported {result.stdout.strip()!r}, expected {expected!r}"
        )


def isolated_installer(
    installer: Path,
    version: str,
    base_url: str,
    environment: dict[str, str],
    root: Path,
) -> None:
    require_file(installer, "installer")
    prefix = root / "prefix"
    installer_environment = dict(environment)
    installer_environment["SKILLWICK_BASE_URL"] = base_url
    run_checked(
        ["sh", str(installer), "--version", version, "--prefix", str(prefix)],
        env=installer_environment,
    )
    installed = prefix / "bin" / "skillwick"
    require_file(installed, "installed executable")
    version_output(installed, version, environment)
    for name in ("home", "codex", "config", "cache", "state"):
        directory = root / name
        if any(directory.iterdir()):
            raise VerificationError(f"installer wrote user configuration under isolated {name}")


def verify(arguments: argparse.Namespace) -> None:
    version = normalized_version(arguments.version)
    manifest_version_value = manifest_version(arguments.manifest)
    if manifest_version_value != version:
        raise VerificationError(
            f"Cargo manifest version is {manifest_version_value}, expected release {version}"
        )
    formula = None if arguments.skip_formula else formula_digests(arguments.formula, version)
    host = host_target()
    execution_commands: dict[str, list[str]] = {}
    if arguments.skip_execution:
        pass
    elif host is None:
        raise VerificationError(
            "archive execution requires a supported macOS host; pass --skip-execution only for shape checks"
        )
    elif arguments.execute_target is not None:
        command_prefix = execution_command(arguments.execute_target, host)
        if command_prefix is None:
            raise VerificationError(
                f"host cannot execute {arguments.execute_target}; pass --skip-execution for shape checks"
            )
        execution_commands[arguments.execute_target] = command_prefix
    else:
        for target in TARGETS:
            command_prefix = execution_command(target, host)
            if command_prefix is not None:
                execution_commands[target] = command_prefix

    if arguments.skip_installer and not arguments.skip_execution and host is None:
        raise VerificationError("installer can only be exercised on a supported macOS host")
    if not arguments.skip_installer and host is None:
        raise VerificationError(
            "installer execution requires a supported macOS host; pass --skip-installer for shape checks"
        )

    with tempfile.TemporaryDirectory(prefix="skillwick-release-verify-") as temporary:
        workspace = Path(temporary)
        environment = isolated_environment(workspace)
        asset_dir = workspace / "assets"
        assets, installer_base_url = fetch_assets(
            version, arguments.archive_dir, arguments.base_url, asset_dir
        )
        extraction = workspace / "extracted"
        for target in TARGETS:
            archive_name = f"skillwick-{target}.tar.xz"
            checksum_name = f"{archive_name}.sha256"
            archive = assets[archive_name]
            digest = checksum_asset(archive, assets[checksum_name])
            if formula is not None and formula[target] != digest:
                raise VerificationError(
                    f"formula checksum for {target} is {formula[target]}, archive is {digest}"
                )
            executable = archive_layout(archive, target, extraction)
            if target in execution_commands:
                version_output(executable, version, environment, execution_commands[target])
            execution_note = " executable OK" if target in execution_commands else " executable deferred"
            formula_note = "formula deferred;" if formula is None else "formula OK;"
            print(f"archive {archive_name}: checksum and layout OK; {formula_note}{execution_note}")

        if arguments.source_binary is not None:
            source_binary = require_file(arguments.source_binary, "tested source executable")
            version_output(source_binary, version, environment)
            print(f"tested source executable {source_binary}: --version OK")

        if not arguments.skip_installer:
            isolated_installer(
                arguments.installer,
                version,
                installer_base_url,
                environment,
                workspace,
            )
            print("supported installer: isolated prefix and clean user configuration OK")
        else:
            print("supported installer: deferred (shape/checksum mode)")

    print(f"release contract verified for skillwick {version}")


def main() -> int:
    arguments = parser().parse_args()
    try:
        verify(arguments)
    except VerificationError as error:
        print(f"release verifier: ERROR: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
