#!/usr/bin/env python3
"""Exercise setup ownership, recovery, and dry-run behavior in isolated homes."""

from __future__ import annotations

import json
import fcntl
import os
from pathlib import Path
import pty
import select
import subprocess
import sys
import tempfile
import time


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

def reranker_recovery(binary: str) -> None:
    # Recovery runs before ordinary configuration reads, without invoking a provider.
    for committed in (False, True):
        with tempfile.TemporaryDirectory(prefix="skillwick-key-recovery-") as directory:
            temporary = Path(directory).resolve()
            state = temporary / "state"
            config = temporary / "config.toml"
            skills = temporary / "skills"
            skill = skills / "example"
            skill.mkdir(parents=True)
            (skill / "SKILL.md").write_text(
                "---\nname: example\ndescription: Setup recovery fixture.\n---\nBody.\n"
            )
            old, new = temporary / "old-runtime", temporary / "new-runtime"
            for runtime, key in ((old, "previous-key"), (new, "prepared-key")):
                runtime.mkdir(mode=0o700)
                (runtime / "api-key").write_text(key)
                (runtime / "api-key").chmod(0o600)

            def contents(runtime: Path) -> bytes:
                return (
                    "version = 1\n"
                    'discovery = "explicit"\n'
                    f"roots = [{json.dumps(str(skills))}]\n"
                    f'[reranker]\nbackend = "jev"\nruntime = "{runtime}"\n'
                ).encode()
            before, after = contents(old), contents(new)
            config.write_bytes(after)
            pending = state / "skillwick/integration.pending.json"
            pending.parent.mkdir(parents=True)
            pending.write_text(json.dumps({"version": 1, "committed": committed, "changes": [
                {"path": str(config), "before": list(before), "after": list(after)}
            ]}))
            env = os.environ.copy()
            env.update(HOME=str(temporary / "home"), XDG_STATE_HOME=str(state),
                       XDG_CONFIG_HOME=str(temporary / "config"), XDG_CACHE_HOME=str(temporary / "cache"))

            config_before = config.read_bytes()
            pending_before = pending.read_bytes()
            dry_run = subprocess.run(
                [binary, "--config", str(config), "--cwd", str(temporary), "init",
                 "--yes", "--dry-run", "--agent", "none", "--discovery", "explicit",
                 "--root", str(skills)],
                env=env,
                stdin=subprocess.DEVNULL,
                capture_output=True,
                text=True,
                timeout=30,
            )
            assert dry_run.returncode == 0, dry_run.stderr
            assert config.read_bytes() == config_before
            assert pending.read_bytes() == pending_before
            # A reader waits briefly for an active state-directory lock, then recovers the
            # pending transaction before interpreting the config pointer.
            lock_fd = os.open(pending.parent, os.O_RDONLY)
            fcntl.flock(lock_fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            process = subprocess.Popen(
                [binary, "--config", str(config), "--json", "search", "Setup recovery fixture"],
                env=env,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            time.sleep(0.25)
            assert process.poll() is None, "config reader should wait for the active setup lock"
            os.close(lock_fd)
            stdout, stderr = process.communicate(timeout=30)
            assert process.returncode == 0, stderr
            assert json.loads(stdout)["results"]
            assert config.read_bytes() == (after if committed else before)
            assert not pending.exists()
            assert (old / "api-key").read_text() == "previous-key"
            assert (new / "api-key").read_text() == "prepared-key"


def search_during_preparation(binary: str) -> None:
    with tempfile.TemporaryDirectory(prefix="skillwick-slow-setup-") as directory:
        temporary = Path(directory).resolve()
        skills = temporary / "skills"
        skill = skills / "example"
        skill.mkdir(parents=True)
        (skill / "SKILL.md").write_text(
            "---\nname: example\ndescription: Setup preparation fixture.\n---\nBody.\n"
        )
        workspace = temporary / "workspace"
        workspace.mkdir()
        config = temporary / "config" / "skillwick" / "config.toml"
        config.parent.mkdir(parents=True)
        original_config = (
            "version = 1\n"
            'discovery = "explicit"\n'
            f"roots = [{json.dumps(str(skills))}]\n"
            "agents = []\n"
        ).encode()
        config.write_bytes(original_config)
        state = temporary / "state"
        cache = temporary / "cache"
        tools = temporary / "bin"
        tools.mkdir()
        started = temporary / "uv-started"
        release = temporary / "uv-release"
        uv = tools / "uv"
        uv.write_text(
            "#!/bin/sh\n"
            'if [ "$1" = "venv" ]; then\n'
            '  : > "$UV_STARTED"\n'
            '  while [ ! -e "$UV_RELEASE" ]; do sleep 0.02; done\n'
            "  exit 1\n"
            "fi\n"
            "exit 1\n"
        )
        uv.chmod(0o755)
        env = os.environ.copy()
        env.update(
            HOME=str(temporary / "home"),
            XDG_CONFIG_HOME=str(temporary / "config"),
            XDG_STATE_HOME=str(state),
            XDG_CACHE_HOME=str(cache),
            UV_STARTED=str(started),
            UV_RELEASE=str(release),
            PATH=str(tools) + os.pathsep + os.environ.get("PATH", ""),
        )
        setup = subprocess.Popen(
            [binary, "--config", str(config), "--cwd", str(workspace), "init",
             "--yes", "--agent", "none", "--discovery", "explicit", "--root", str(skills),
             "--reranker", "tinybert"],
            env=env,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        try:
            deadline = time.monotonic() + 10
            while not started.exists() and setup.poll() is None and time.monotonic() < deadline:
                time.sleep(0.02)
            assert started.exists(), "setup should reach the stalled runtime preparation"
            search = subprocess.run(
                [binary, "--config", str(config), "--cwd", str(workspace), "--json",
                 "search", "Setup preparation fixture"],
                env=env,
                stdin=subprocess.DEVNULL,
                capture_output=True,
                text=True,
                timeout=3,
            )
        finally:
            release.touch()
            setup_stdout, setup_stderr = setup.communicate(timeout=30)

        assert setup.returncode == 1, (setup_stdout, setup_stderr)
        assert search.returncode == 0, search.stderr
        assert json.loads(search.stdout)["results"]
        assert config.read_bytes() == original_config, "failed preparation must preserve the prior config"


def search_during_interactive_picker(binary: str) -> None:
    with tempfile.TemporaryDirectory(prefix="skillwick-picker-setup-") as directory:
        temporary = Path(directory).resolve()
        skills = temporary / "skills"
        skill = skills / "example"
        skill.mkdir(parents=True)
        (skill / "SKILL.md").write_text(
            "---\nname: example\ndescription: Picker concurrency fixture.\n---\nBody.\n"
        )
        workspace = temporary / "workspace"
        workspace.mkdir()
        config = temporary / "config" / "skillwick" / "config.toml"
        config.parent.mkdir(parents=True)
        config.write_text(
            "version = 1\n"
            'discovery = "explicit"\n'
            f"roots = [{json.dumps(str(skills))}]\n"
            "agents = []\n"
        )
        env = os.environ.copy()
        env.update(
            HOME=str(temporary / "home"),
            CODEX_HOME=str(temporary / "codex"),
            CLAUDE_CONFIG_DIR=str(temporary / "claude"),
            XDG_CONFIG_HOME=str(temporary / "config"),
            XDG_STATE_HOME=str(temporary / "state"),
            XDG_CACHE_HOME=str(temporary / "cache"),
            TERM="xterm-256color",
        )
        master, slave = pty.openpty()
        setup = subprocess.Popen(
            [binary, "--config", str(config), "--cwd", str(workspace), "init",
             "--agent", "none", "--discovery", "explicit", "--root", str(skills)],
            env=env,
            stdin=slave,
            stdout=slave,
            stderr=slave,
            close_fds=True,
        )
        os.close(slave)
        output = bytearray()
        try:
            deadline = time.monotonic() + 10
            while b"Reranking backend" not in output and setup.poll() is None:
                if time.monotonic() >= deadline:
                    break
                ready, _, _ = select.select([master], [], [], 0.05)
                if ready:
                    try:
                        output.extend(os.read(master, 4096))
                    except OSError:
                        break
            assert b"Reranking backend" in output, (
                "setup should pause at the reranking picker; output was "
                + output.decode(errors="replace")
            )
            try:
                search = subprocess.run(
                    [binary, "--config", str(config), "--cwd", str(workspace), "--json",
                     "search", "Picker concurrency fixture"],
                    env=env,
                    stdin=subprocess.DEVNULL,
                    capture_output=True,
                    text=True,
                    timeout=2,
                )
            except subprocess.TimeoutExpired as error:
                raise AssertionError(
                    "ordinary search waited for setup's interactive reranker picker"
                ) from error
            assert setup.poll() is None, "setup should still be paused in the picker"
            assert search.returncode == 0, search.stderr
            assert json.loads(search.stdout)["results"]
        finally:
            if setup.poll() is None:
                setup.terminate()
                try:
                    setup.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    setup.kill()
                    setup.wait()
            os.close(master)


def user_edit_during_preparation(binary: str) -> None:
    with tempfile.TemporaryDirectory(prefix="skillwick-user-edit-setup-") as directory:
        temporary = Path(directory).resolve()
        skills = temporary / "skills"
        skill = skills / "example"
        skill.mkdir(parents=True)
        (skill / "SKILL.md").write_text(
            "---\nname: example\ndescription: User edit fixture.\n---\nBody.\n"
        )
        workspace = temporary / "workspace"
        workspace.mkdir()
        config = temporary / "config" / "skillwick" / "config.toml"
        config.parent.mkdir(parents=True)
        original_config = (
            "version = 1\n"
            'discovery = "explicit"\n'
            f"roots = [{json.dumps(str(skills))}]\n"
            "agents = []\n"
        ).encode()
        config.write_bytes(original_config)
        state = temporary / "state"
        cache = temporary / "cache"
        codex = temporary / "codex"
        codex.mkdir()
        instructions = codex / "AGENTS.md"
        tools = temporary / "bin"
        tools.mkdir()
        started = temporary / "uv-started"
        release = temporary / "uv-release"
        uv = tools / "uv"
        uv.write_text(
            "#!/bin/bash\n"
            'if [ "$1" = "venv" ]; then\n'
            '  venv="${@: -1}"\n'
            '  mkdir -p "$venv/bin"\n'
            '  cat > "$venv/bin/python" <<\'PYTHON\'\n'
            "#!/usr/bin/env python3\n"
            "import json\n"
            "print(json.dumps({\"status\": \"ready\", \"backend\": \"tinybert\", "
            "\"model\": \"cross-encoder/ms-marco-TinyBERT-L2-v2@81d1926f67cb8eee2c2be17ca9f793c7c3bd20cc\"}))\n"
            "PYTHON\n"
            '  chmod +x "$venv/bin/python"\n'
            '  : > "$UV_STARTED"\n'
            '  while [ ! -e "$UV_RELEASE" ]; do sleep 0.02; done\n'
            "  exit 0\n"
            "fi\n"
            'if [ "$1" = "pip" ]; then exit 0; fi\n'
            "exit 1\n"
        )
        uv.chmod(0o755)
        env = os.environ.copy()
        env.update(
            HOME=str(temporary / "home"),
            CODEX_HOME=str(codex),
            XDG_CONFIG_HOME=str(temporary / "config"),
            XDG_STATE_HOME=str(state),
            XDG_CACHE_HOME=str(cache),
            UV_STARTED=str(started),
            UV_RELEASE=str(release),
            PATH=str(tools) + os.pathsep + os.environ.get("PATH", ""),
        )
        setup = subprocess.Popen(
            [binary, "--config", str(config), "--cwd", str(workspace), "init",
             "--yes", "--agent", "codex", "--discovery", "explicit", "--root", str(skills),
             "--reranker", "tinybert"],
            env=env,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        try:
            deadline = time.monotonic() + 10
            while not started.exists() and setup.poll() is None and time.monotonic() < deadline:
                time.sleep(0.02)
            assert started.exists(), "setup should reach runtime preparation"
            instructions.write_text("# User-owned instructions added during setup\n")
        finally:
            release.touch()
            stdout, stderr = setup.communicate(timeout=30)

        assert setup.returncode == 1, (stdout, stderr)
        assert "setup destination changed while waiting" in stderr, stderr
        assert instructions.read_text() == "# User-owned instructions added during setup\n"
        assert config.read_bytes() == original_config, "preflight must reject before writing config"
        assert not (codex / "SKILLWICK.md").exists()
        assert not (state / "skillwick" / "integration.json").exists()
        assert not (state / "skillwick" / "integration.pending.json").exists(), (
            "destination drift must not leave a recovery journal"
        )
        search = subprocess.run(
            [binary, "--config", str(config), "--cwd", str(workspace), "--json",
             "search", "User edit fixture"],
            env=env,
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
            timeout=3,
        )
        assert search.returncode == 0, search.stderr
        assert json.loads(search.stdout)["results"]

def sandbox_cache_contract(binary: str) -> None:
    with tempfile.TemporaryDirectory(prefix="skillwick-sandbox-cache-") as directory:
        temporary = Path(directory).resolve()
        home = temporary / "home"
        skills = temporary / "skills"
        skills.mkdir()
        body = "---\nname: cache-example\ndescription: Sandbox cache fixture.\n---\nComplete café body.\n"
        (skills / "SKILL.md").write_text(body)
        env = {key: value for key, value in os.environ.items()
               if key not in {"XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_STATE_HOME"}}
        env.update(HOME=str(home), CODEX_HOME=str(temporary / "no-codex"),
                   CLAUDE_CONFIG_DIR=str(temporary / "no-claude"))

        def run(*args, environment=env, code=0):
            result = subprocess.run([binary, *args], env=environment, capture_output=True,
                                    text=True, stdin=subprocess.DEVNULL, timeout=30)
            assert result.returncode == code, (args, result.returncode, result.stderr)
            return result

        # Fresh default paths work. Once setup is stable, instruction reads need
        # only a writable derived cache, not writes to authoritative setup state.
        run("init", "--yes", "--agent", "none", "--discovery", "explicit", "--root", str(skills))
        assert run("read", "--raw", "cache-example").stdout == body
        state = home / ".local/state/skillwick"
        cache = home / ".cache/skillwick"
        previous = (cache / "index-v4.sqlite").read_bytes()
        state_before = {path.name: path.read_bytes() for path in state.iterdir() if path.is_file()}
        state.chmod(0o500)
        cache.chmod(0o500)
        try:
            failed = run("read", "--raw", "cache-example", code=3)
            assert failed.stdout == "" and "cache lock failed" in failed.stderr
            assert (cache / "index-v4.sqlite").read_bytes() == previous
            writable_env = dict(env, XDG_CACHE_HOME=str(temporary / "writable-cache"))
            assert run("read", "--raw", "cache-example", environment=writable_env).stdout == body
            assert state_before == {path.name: path.read_bytes() for path in state.iterdir() if path.is_file()}
            cache.chmod(0o700)
            assert run("read", "--raw", "cache-example").stdout == body
        finally:
            state.chmod(0o700)
            cache.chmod(0o700)
        # A real pending journal cannot be hidden by changing the cache. When
        # recovery is forbidden, fail without discarding authoritative state.
        pending = state / "integration.pending.json"
        pending.write_text(json.dumps({"version": 1, "committed": True, "changes": []}))
        pending_before = pending.read_bytes()
        state.chmod(0o500)
        try:
            failed = run("read", "--raw", "cache-example", environment=writable_env, code=1)
            assert failed.stdout == "" and "integration.pending.json" in failed.stderr
            assert pending.read_bytes() == pending_before
        finally:
            state.chmod(0o700)
        assert run("read", "--raw", "cache-example", environment=writable_env).stdout == body
        assert not pending.exists()


if __name__ == "__main__":
    main()
    reranker_recovery(str(Path(sys.argv[1]).resolve()))
    search_during_preparation(str(Path(sys.argv[1]).resolve()))
    search_during_interactive_picker(str(Path(sys.argv[1]).resolve()))
    user_edit_during_preparation(str(Path(sys.argv[1]).resolve()))
    relative_destinations(str(Path(sys.argv[1]).resolve()))
    sandbox_cache_contract(str(Path(sys.argv[1]).resolve()))
    print("Setup contract passed")
