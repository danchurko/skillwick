#!/usr/bin/env python3
"""Exercise setup ownership, recovery, and dry-run behavior in isolated homes."""

from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


def main() -> None:
    binary = str(Path(sys.argv[1]).resolve())
    with tempfile.TemporaryDirectory(prefix="skillwick-setup-") as directory:
        temporary = Path(directory).resolve()
        root = temporary / "skills"
        skill = root / "example"
        skill.mkdir(parents=True)
        (skill / "SKILL.md").write_text(
            "---\nname: example\ndescription: Setup contract fixture.\n---\nBody.\n"
        )
        project = temporary / "project"
        project.mkdir()
        config_home = temporary / "config"
        cache_home = temporary / "cache"
        state_home = temporary / "state"
        codex_home = temporary / "codex"
        claude_home = temporary / "claude"
        env = os.environ.copy()
        env.update(
            HOME=str(temporary / "home"),
            CODEX_HOME=str(codex_home),
            CLAUDE_CONFIG_DIR=str(claude_home),
            XDG_CONFIG_HOME=str(config_home),
            XDG_CACHE_HOME=str(cache_home),
            XDG_STATE_HOME=str(state_home),
        )
        config = config_home / "skillwick" / "config.toml"
        pending = state_home / "skillwick" / "integration.pending.json"
        journal = state_home / "skillwick" / "integration.json"

        def run(*args: str, code: int = 0) -> subprocess.CompletedProcess[str]:
            result = subprocess.run(
                [binary, *args],
                env=env,
                stdin=subprocess.DEVNULL,
                capture_output=True,
                text=True,
                timeout=30,
            )
            assert result.returncode == code, (
                args,
                result.returncode,
                result.stdout,
                result.stderr,
            )
            return result

        setup_args = (
            "--config",
            str(config),
            "--cwd",
            str(project),
            "init",
            "--yes",
            "--discovery",
            "explicit",
            "--root",
            str(root),
        )

        # Rejected noninteractive setup must not create state either.
        run("init", "--agent", "none", code=2)
        assert not state_home.exists() and not config_home.exists()
        # Dry-run does not create configuration, state, cache, or agent files.
        run(*setup_args, "--dry-run", "--agent", "codex")
        assert not config.exists()
        assert not state_home.exists()
        assert not cache_home.exists()
        assert not (codex_home / "SKILLWICK.md").exists()

        run(*setup_args, "--agent", "codex", "--agent", "claude")
        assert config.exists() and journal.exists()
        assert (codex_home / "SKILLWICK.md").read_text() == run(
            "instructions"
        ).stdout
        assert (claude_home / "SKILLWICK.md").read_text() == run(
            "instructions"
        ).stdout
        assert f"@{codex_home / 'SKILLWICK.md'}\n" in (codex_home / "AGENTS.md").read_text()
        assert f"@{claude_home / 'SKILLWICK.md'}\n" in (claude_home / "CLAUDE.md").read_text()
        tracked = [
            config,
            codex_home / "SKILLWICK.md",
            codex_home / "AGENTS.md",
            claude_home / "SKILLWICK.md",
            claude_home / "CLAUDE.md",
            journal,
        ]
        before = {path: (path.read_bytes(), path.stat().st_mtime_ns) for path in tracked}
        run(*setup_args, "--agent", "codex", "--agent", "claude")
        after = {path: (path.read_bytes(), path.stat().st_mtime_ns) for path in tracked}
        assert before == after
        codex_instructions = codex_home / "AGENTS.md"
        codex_reference = f"@{codex_home / 'SKILLWICK.md'}"
        remaining = [
            line
            for line in codex_instructions.read_text().splitlines()
            if line != codex_reference
        ]
        codex_instructions.write_text("\n".join(remaining) + ("\n" if remaining else ""))
        run(*setup_args, "--agent", "codex", "--agent", "claude")
        assert codex_instructions.read_text().count(codex_reference) == 1
        report = run("--config", str(config), "--cwd", str(project), "doctor", "--strict")
        assert "healthy: true" in report.stdout

        # A context changed after setup blocks uninstall and preserves the receipt.
        changed_context = codex_home / "SKILLWICK.md"
        original_context = changed_context.read_bytes()
        changed_context.write_bytes(b"# user-owned change\n")
        journal_before = journal.read_bytes()
        run("--config", str(config), "uninstall", code=1)
        assert changed_context.read_bytes() == b"# user-owned change\n"
        assert journal.read_bytes() == journal_before
        changed_context.write_bytes(original_context)

        run("--config", str(config), "uninstall")
        assert not journal.exists()
        assert not (codex_home / "SKILLWICK.md").exists()
        assert not (codex_home / "AGENTS.md").exists()
        assert not (claude_home / "SKILLWICK.md").exists()
        assert not (claude_home / "CLAUDE.md").exists()

        # Pre-existing canonical files are borrowed and survive uninstall.
        borrowed_codex = temporary / "borrowed-codex"
        borrowed_codex.mkdir()
        borrowed_context = borrowed_codex / "SKILLWICK.md"
        borrowed_instructions = borrowed_codex / "AGENTS.md"
        context_body = run("instructions").stdout
        borrowed_context.write_text(context_body)
        borrowed_instructions.write_text(f"keep\n@{borrowed_context}\n")
        env["CODEX_HOME"] = str(borrowed_codex)
        run(*setup_args, "--agent", "codex")
        borrowed_receipt = json.loads(journal.read_text())
        target = borrowed_receipt["targets"][0]
        assert not target["context_created"] and not target["reference_added"]
        assert "healthy: true" in run(
            "--config", str(config), "--cwd", str(project), "doctor", "--strict"
        ).stdout
        borrowed_before = (borrowed_context.read_bytes(), borrowed_instructions.read_bytes())
        run("--config", str(config), "uninstall")
        assert (borrowed_context.read_bytes(), borrowed_instructions.read_bytes()) == borrowed_before

        # A pending transaction is reported by dry-run and left byte-for-byte intact.
        pending.parent.mkdir(parents=True, exist_ok=True)
        pending_payload = {
            "version": 1,
            "committed": False,
            "changes": [
                {
                    "path": str(borrowed_codex / "SKILLWICK.md"),
                    "before": None,
                    "after": list(b"partial\n"),
                }
            ],
        }
        pending.write_text(json.dumps(pending_payload))
        pending_before = pending.read_bytes()
        dry_run = run(
            "--config",
            str(config),
            "--cwd",
            str(project),
            "init",
            "--dry-run",
            "--yes",
            "--agent",
            "codex",
            "--discovery",
            "explicit",
            "--root",
            str(root),
        )
        assert "pending Skillwick setup transaction" in dry_run.stderr
        assert pending.read_bytes() == pending_before

        # Mutating setup recovers before collision planning, then applies cleanly.
        partial_context = borrowed_codex / "SKILLWICK.md"
        partial_context.write_bytes(b"partial\n")
        run(
            "--config",
            str(config),
            "--cwd",
            str(project),
            "init",
            "--yes",
            "--agent",
            "codex",
            "--discovery",
            "explicit",
            "--root",
            str(root),
        )
        assert partial_context.read_text() == context_body
        assert not pending.exists()

        # Configured integration without its journal is unhealthy and explicit
        # targets are required for noninteractive re-setup.
        journal.unlink()
        missing = run(
            "--config", str(config), "--cwd", str(project), "doctor", "--strict", code=3
        )
        assert "journal is missing" in missing.stdout
        run("--config", str(config), "--cwd", str(project), "init", "--yes", code=2)

        # Obsolete configurations fail with recovery guidance.
        obsolete = temporary / "obsolete.toml"
        obsolete.write_text('agent = "codex"\n')
        old = run("--config", str(obsolete), "init", "--yes", "--agent", "none", code=1)
        assert "obsolete Skillwick configuration" in old.stderr

def relative_destinations(binary: str) -> None:
    with tempfile.TemporaryDirectory(prefix="skillwick-relative-") as directory:
        temporary = Path(directory).resolve()
        first, second = temporary / "a", temporary / "b"
        first.mkdir()
        second.mkdir()
        skills = temporary / "skills"
        skills.mkdir()
        env = os.environ.copy()
        env.update(HOME=str(temporary / "home"), CODEX_HOME="codex-home",
                   CLAUDE_CONFIG_DIR=str(temporary / "claude"),
                   XDG_CONFIG_HOME=str(temporary / "config"),
                   XDG_STATE_HOME=str(temporary / "state"),
                   XDG_CACHE_HOME=str(temporary / "cache"))
        def run(cwd, *args, code=0):
            result = subprocess.run([binary, *args], cwd=cwd, env=env,
                                    capture_output=True, text=True, timeout=30)
            assert result.returncode == code, (args, result.stdout, result.stderr)
        run(first, "--config", "overlap.toml", "init", "--yes", "--agent", "codex",
            "--discovery", "explicit", "--root", str(skills),
            "--instructions-file", "codex-home/SKILLWICK.md", code=2)
        assert not (first / "codex-home/SKILLWICK.md").exists()
        run(first, "--config", "config.toml", "init", "--yes", "--agent", "codex",
            "--discovery", "explicit", "--root", str(skills), "--instructions-file", "AGENTS.md")
        journal = temporary / "state/skillwick/integration.json"
        receipt = json.loads(journal.read_text())
        target = receipt["targets"][0]
        assert Path(target["instructions_file"]) == first / "AGENTS.md"
        assert Path(target["context_file"]) == first / "codex-home/SKILLWICK.md"
        untouched = (first / "AGENTS.md").read_bytes()
        (second / "AGENTS.md").write_bytes(untouched)
        target["instructions_file"] = "AGENTS.md"
        journal.write_text(json.dumps(receipt))
        run(second, "uninstall", code=1)
        assert (second / "AGENTS.md").read_bytes() == untouched
        target["instructions_file"] = str(first / "AGENTS.md")
        journal.write_text(json.dumps(receipt))
        # Recovery also acts on absolute destinations from the original cwd.
        config = first / "config.toml"
        pending = temporary / "state/skillwick/integration.pending.json"
        pending.write_text(json.dumps({"version": 1, "committed": False, "changes": [
            {"path": str(config), "before": None, "after": list(config.read_bytes())}
        ]}))
        run(second, "uninstall")
        assert not (first / "AGENTS.md").exists()
        assert not (first / "codex-home/SKILLWICK.md").exists()
        assert not config.exists() and not pending.exists()
        assert (second / "AGENTS.md").read_bytes() == untouched

if __name__ == "__main__":
    main()
    relative_destinations(str(Path(sys.argv[1]).resolve()))
    print("Setup contract passed")
