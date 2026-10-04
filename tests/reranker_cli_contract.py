#!/usr/bin/env python3
"""Offline CLI contracts for setup-selected reranking."""

from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


def isolated_env(root: Path, tools: Path) -> dict[str, str]:
    env = os.environ.copy()
    for name in ("TYPESAFE_API_KEY", "HF_TOKEN", "HUGGING_FACE_HUB_TOKEN"):
        env.pop(name, None)
    env.update(
        HOME=str(root / "home"),
        CODEX_HOME=str(root / "codex"),
        CLAUDE_CONFIG_DIR=str(root / "claude"),
        XDG_CONFIG_HOME=str(root / "config-home"),
        XDG_CACHE_HOME=str(root / "cache-home"),
        XDG_STATE_HOME=str(root / "state-home"),
        HF_HOME=str(root / "hf-home"),
        HF_HUB_OFFLINE="1",
        HF_HUB_DISABLE_TELEMETRY="1",
        PATH=str(tools),
        http_proxy="http://127.0.0.1:9",
        https_proxy="http://127.0.0.1:9",
        HTTP_PROXY="http://127.0.0.1:9",
        HTTPS_PROXY="http://127.0.0.1:9",
    )
    return env


def run(binary: Path, env: dict[str, str], *args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [str(binary), *args],
        env=env,
        stdin=subprocess.DEVNULL,
        capture_output=True,
        text=True,
        timeout=30,
    )


def require(result: subprocess.CompletedProcess[str], code: int, label: str) -> None:
    assert result.returncode == code, (
        f"{label}: exit {result.returncode}, expected {code}; "
        f"stdout bytes={len(result.stdout)}, stderr bytes={len(result.stderr)}"
    )


def materialize(root: Path) -> None:
    skills = root / "skills"
    skills.mkdir(parents=True)
    records = (
        (
            "sqlite-maintenance",
            "Maintain SQLite databases, tune SQL queries, and review indexes.",
        ),
        (
            "cloud-agent-deployment",
            "Deploy AI agents to a cloud service and monitor their runtime.",
        ),
    )
    for name, description in records:
        package = skills / name
        package.mkdir()
        (package / "SKILL.md").write_text(
            f"---\nname: {name}\ndescription: {description}\n---\nFixture instructions.\n",
            encoding="utf-8",
        )


def search(binary: Path, env: dict[str, str], config: Path, workspace: Path, *args: str):
    return run(
        binary,
        env,
        "--config",
        str(config),
        "--cwd",
        str(workspace),
        *args,
    )


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: reranker_cli_contract.py SKILLWICK_BINARY")
    binary = Path(sys.argv[1]).resolve()
    if not binary.is_file():
        raise SystemExit("Skillwick binary does not exist")

    with tempfile.TemporaryDirectory(prefix="skillwick-reranker-contract-") as name:
        temporary = Path(name).resolve()
        tools = temporary / "empty-bin"
        tools.mkdir()
        workspace = temporary / "workspace"
        workspace.mkdir()
        materialize(temporary)
        env = isolated_env(temporary / "main", tools)
        config = temporary / "main" / "config-home" / "skillwick" / "config.toml"
        setup = (
            "--config",
            str(config),
            "--cwd",
            str(workspace),
            "init",
            "--yes",
            "--agent",
            "none",
            "--discovery",
            "explicit",
            "--root",
            str(temporary / "skills"),
        )

        # Initial configuration has no optional runtime and ordinary search is lexical.
        require(run(binary, env, *setup), 0, "initial lexical setup")
        query = search(binary, env, config, workspace, "--json", "search", "SQLite query indexes")
        require(query, 0, "default lexical search")
        payload = json.loads(query.stdout)
        assert payload["version"] == 3 and payload["results"]
        assert payload["results"][0]["name"] == "sqlite-maintenance"
        assert not query.stderr

        no_match = search(binary, env, config, workspace, "--json", "search", "solar telescope calibration")
        require(no_match, 0, "lexical no-match")
        assert json.loads(no_match.stdout)["results"] == []
        limited = search(
            binary,
            env,
            config,
            workspace,
            "--json",
            "search",
            "database cloud agent",
            "--limit",
            "1",
        )
        require(limited, 0, "valid result limit")
        assert len(json.loads(limited.stdout)["results"]) <= 1
        for invalid_limit in ("0", "21"):
            result = search(
                binary,
                env,
                config,
                workspace,
                "search",
                "SQLite",
                "--limit",
                invalid_limit,
            )
            require(result, 2, f"invalid limit {invalid_limit}")

        help_result = run(binary, env, "search", "--help")
        require(help_result, 0, "search help")
        assert "--reranker" not in help_result.stdout

        # Missing runtime configuration degrades to valid lexical JSON and a fixed diagnostic.
        fallback_root = temporary / "fallback"
        fallback_env = isolated_env(fallback_root, tools)
        fallback_config = fallback_root / "manual-config.toml"
        fallback_config.parent.mkdir(parents=True)
        fallback_config.write_text(
            "\n".join(
                (
                    "version = 1",
                    'discovery = "explicit"',
                    f"roots = {json.dumps([str(temporary / 'skills')])}",
                    "agents = []",
                    "[reranker]",
                    'backend = "tinybert"',
                    f"runtime = {json.dumps(str(fallback_root / 'missing-runtime'))}",
                    "",
                )
            ),
            encoding="utf-8",
        )
        fallback = search(
            binary,
            fallback_env,
            fallback_config,
            workspace,
            "--json",
            "search",
            "SQLite query indexes",
        )
        require(fallback, 0, "missing-runtime fallback")
        assert json.loads(fallback.stdout)["results"][0]["name"] == "sqlite-maintenance"
        assert "reranking: runtime_unavailable" in fallback.stderr

        # Malformed configuration fails closed instead of silently selecting lexical.
        malformed = temporary / "malformed.toml"
        malformed.write_text('version = 1\n[reranker]\nbackend = "tinybert"\nunknown = true\n', encoding="utf-8")
        rejected = search(binary, env, malformed, workspace, "search", "SQLite")
        require(rejected, 1, "malformed configuration")
        assert "invalid Skillwick" in rejected.stderr

        # Planning a model choice performs no writes, installation, or download.
        dry_root = temporary / "dry"
        dry_env = isolated_env(dry_root, tools)
        dry_config = dry_root / "config-home" / "skillwick" / "config.toml"
        dry_args = (
            "--config",
            str(dry_config),
            "--cwd",
            str(workspace),
            "init",
            "--yes",
            "--dry-run",
            "--agent",
            "none",
            "--discovery",
            "explicit",
            "--root",
            str(temporary / "skills"),
            "--reranker",
            "tinybert",
        )
        dry = run(binary, dry_env, *dry_args)
        require(dry, 0, "dry-run model selection")
        assert "reranker: tinybert" in dry.stderr
        assert not dry_config.exists()
        for path in (dry_root / "state-home", dry_root / "cache-home", dry_root / "hf-home"):
            assert not path.exists(), f"dry-run created {path.name}"

        # Omitting --reranker on an unrelated dry-run preserves the saved choice byte-for-byte.
        preserved_root = temporary / "preserved"
        preserved_env = isolated_env(preserved_root, tools)
        preserved_config = preserved_root / "config-home" / "skillwick" / "config.toml"
        preserved_config.parent.mkdir(parents=True)
        preserved_runtime = preserved_root / "selected-runtime"
        preserved_config.write_text(
            "\n".join(
                (
                    "version = 1",
                    'discovery = "explicit"',
                    f"roots = {json.dumps([str(temporary / 'skills')])}",
                    "agents = []",
                    "[reranker]",
                    'backend = "tinybert"',
                    f"runtime = {json.dumps(str(preserved_runtime))}",
                    "",
                )
            ),
            encoding="utf-8",
        )
        original_config = preserved_config.read_bytes()
        preserved_dry = run(
            binary,
            preserved_env,
            "--config",
            str(preserved_config),
            "--cwd",
            str(workspace),
            "init",
            "--yes",
            "--dry-run",
            "--agent",
            "none",
        )
        require(preserved_dry, 0, "unrelated dry-run")
        assert "reranker: tinybert" in preserved_dry.stderr
        assert preserved_config.read_bytes() == original_config
        assert not preserved_runtime.exists()
        assert not (preserved_root / "state-home").exists()

        # Establish managed instruction files, then prove missing JEV credentials preserve them.
        codex_init = run(
            binary,
            env,
            "--config",
            str(config),
            "--cwd",
            str(workspace),
            "init",
            "--yes",
            "--agent",
            "codex",
            "--discovery",
            "explicit",
            "--root",
            str(temporary / "skills"),
        )
        require(codex_init, 0, "Codex integration setup")
        state = temporary / "main" / "state-home" / "skillwick"
        tracked = [
            config,
            temporary / "main" / "codex" / "SKILLWICK.md",
            temporary / "main" / "codex" / "AGENTS.md",
            state / "integration.json",
        ]
        before = {path: path.read_bytes() for path in tracked}
        missing_key = run(
            binary,
            env,
            "--config",
            str(config),
            "--cwd",
            str(workspace),
            "init",
            "--yes",
            "--agent",
            "codex",
            "--discovery",
            "explicit",
            "--root",
            str(temporary / "skills"),
            "--reranker",
            "jev",
        )
        require(missing_key, 1, "JEV setup without credentials")
        assert "missing_api_key" in missing_key.stderr
        assert {path: path.read_bytes() for path in tracked} == before
        assert not list(state.rglob("api-key"))

        # A synthetic credential plus an empty PATH reaches preparation but
        # cannot invoke uv or any network client. Failed preparation must not
        # publish the credential or change the active setup.
        synthetic_key = "offline-contract-key-never-real"
        no_uv_env = env.copy()
        no_uv_env["TYPESAFE_API_KEY"] = synthetic_key
        failed_prepare = run(
            binary,
            no_uv_env,
            "--config",
            str(config),
            "--cwd",
            str(workspace),
            "init",
            "--yes",
            "--agent",
            "codex",
            "--discovery",
            "explicit",
            "--root",
            str(temporary / "skills"),
            "--reranker",
            "jev",
        )
        require(failed_prepare, 1, "JEV preparation without uv")
        assert "runtime_unavailable" in failed_prepare.stderr
        assert synthetic_key not in failed_prepare.stdout + failed_prepare.stderr
        assert {path: path.read_bytes() for path in tracked} == before
        assert not list(state.rglob("api-key"))

        # Disabling an unavailable selection restores ordinary lexical search.
        disabled_root = temporary / "disabled"
        disabled_env = isolated_env(disabled_root, tools)
        disabled_config = disabled_root / "manual-config.toml"
        disabled_root.mkdir()
        disabled_config.write_text(fallback_config.read_text(encoding="utf-8"), encoding="utf-8")
        before_disable = search(
            binary,
            disabled_env,
            disabled_config,
            workspace,
            "--json",
            "search",
            "SQLite query indexes",
        )
        require(before_disable, 0, "search before disabling reranker")
        assert "reranking: runtime_unavailable" in before_disable.stderr
        disabled_setup = run(
            binary,
            disabled_env,
            "--config",
            str(disabled_config),
            "--cwd",
            str(workspace),
            "init",
            "--yes",
            "--agent",
            "none",
            "--reranker",
            "none",
        )
        require(disabled_setup, 0, "disable reranker")
        disabled_search = search(
            binary,
            disabled_env,
            disabled_config,
            workspace,
            "--json",
            "search",
            "SQLite query indexes",
        )
        require(disabled_search, 0, "search after disabling reranker")
        assert json.loads(disabled_search.stdout)["results"][0]["name"] == "sqlite-maintenance"
        assert "reranking:" not in disabled_search.stderr

    print("Reranker CLI contract passed (offline, isolated HOME/XDG state).")


if __name__ == "__main__":
    main()
