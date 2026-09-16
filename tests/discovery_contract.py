#!/usr/bin/env python3
"""Exercise provider discovery through isolated installed-CLI fixtures."""

from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


def write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n")


def write_skill(root: Path, name: str, description: str) -> None:
    path = root / name
    path.mkdir(parents=True, exist_ok=True)
    (path / "SKILL.md").write_text(
        f"---\nname: {name}\ndescription: {description}\n---\nProvider fixture body.\n"
    )


def write_plugin_package(path: Path, provider: str, name: str, version: str) -> None:
    manifest_directory = ".codex-plugin" if provider == "codex" else ".claude-plugin"
    path.mkdir(parents=True, exist_ok=True)
    write_json(
        path / manifest_directory / "plugin.json",
        {"name": name, "version": version, "skills": "skills"},
    )
    write_skill(path / "skills", "release", f"{provider} plugin release fixture.")


def toml_string(path: Path) -> str:
    return json.dumps(str(path))


def write_config(path: Path, discovery: str, projects: list[Path] | None = None) -> None:
    lines = ["version = 1", f'discovery = "{discovery}"', "agents = []", "roots = []"]
    for project in projects or []:
        lines.extend(
            [
                "",
                "[[projects]]",
                f"path = {toml_string(project)}",
                "roots = []",
                f'discovery = "{discovery}"',
            ]
        )
    path.write_text("\n".join(lines) + "\n")


def run(
    binary: str,
    env: dict[str, str],
    *args: str,
    cwd: Path | None = None,
    code: int = 0,
) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(
        [binary, *args],
        cwd=cwd,
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
    if code != 0:
        assert result.stdout == "", (args, result.stdout)
    return result


def assert_failed_preserving_cache(
    binary: str,
    env: dict[str, str],
    cache: Path,
    *args: str,
) -> subprocess.CompletedProcess[str]:
    before = cache.read_bytes()
    result = run(binary, env, *args, code=3)
    assert cache.read_bytes() == before, args
    return result


def codex_contract(binary: str, temporary: Path) -> None:
    temporary.mkdir(parents=True)
    root = temporary / "codex"
    cache_root = root / "plugins/cache/market/fixture/1"
    output = temporary / "codex-plugin-list.json"
    args_file = temporary / "codex-args"
    fake_bin = temporary / "bin"
    fake_bin.mkdir()
    fake_codex = fake_bin / "codex"
    fake_codex.write_text(
        "#!/bin/sh\n"
        "set -eu\n"
        ": > \"$CODEX_ARGS_FILE\"\n"
        "for argument in \"$@\"; do printf '%s\\n' \"$argument\" >> \"$CODEX_ARGS_FILE\"; done\n"
        "cat \"$CODEX_OUTPUT\"\n"
    )
    fake_codex.chmod(0o755)
    package = cache_root
    write_plugin_package(package, "codex", "fixture", "1")
    disabled = {
        "pluginId": "disabled@market",
        "installed": True,
        "enabled": False,
    }
    active = {
        "pluginId": "fixture@market",
        "name": "fixture",
        "marketplaceName": "market",
        "version": "1",
        "installed": True,
        "enabled": True,
        "source": {"source": "git", "url": "https://example.invalid/fixture.git"},
    }
    write_json(output, {"installed": [active, disabled]})

    env = os.environ.copy()
    env.update(
        HOME=str(temporary / "home"),
        CODEX_HOME=str(root),
        CLAUDE_CONFIG_DIR=str(temporary / "claude"),
        XDG_CONFIG_HOME=str(temporary / "config"),
        XDG_CACHE_HOME=str(temporary / "cache"),
        XDG_STATE_HOME=str(temporary / "state"),
        CODEX_OUTPUT=str(output),
        CODEX_ARGS_FILE=str(args_file),
        PATH=str(fake_bin) + os.pathsep + env.get("PATH", ""),
    )
    workspace = temporary / "workspace"
    workspace.mkdir()
    config = temporary / "config.toml"
    write_config(config, "auto")
    cache = temporary / "cache/skillwick/index-v4.sqlite"

    listed = json.loads(run(binary, env, "--config", str(config), "--cwd", str(workspace), "list", "--json").stdout)
    assert listed["version"] == 3
    assert listed["total"] == 1
    row = listed["results"][0]
    assert row["name"] == "fixture:release"
    assert row["plugin_id"] == "fixture@market"
    assert args_file.read_text().splitlines() == ["plugin", "list", "--json"]
    cache_before_failures = cache.read_bytes()

    # The active version comes from the host listing. A stale cache copy cannot
    # silently satisfy that version and the published snapshot stays intact.
    stale = dict(active, version="2")
    write_json(output, {"installed": [stale, disabled]})
    missing = assert_failed_preserving_cache(binary, env, cache, "--config", str(config), "--cwd", str(workspace), "list")
    assert "active plugin package is missing" in missing.stderr
    assert cache.read_bytes() == cache_before_failures

    # Missing manifests and paths escaping a package are source failures too.
    write_json(output, {"installed": [active]})
    manifest = package / ".codex-plugin/plugin.json"
    manifest.unlink()
    missing_manifest = assert_failed_preserving_cache(binary, env, cache, "--config", str(config), "--cwd", str(workspace), "list")
    assert "manifest" in missing_manifest.stderr
    write_plugin_package(package, "codex", "fixture", "1")
    write_json(
        manifest,
        {"name": "fixture", "version": "1", "skills": ["../outside"]},
    )
    (root / "plugins/cache/market/fixture/outside").mkdir(parents=True, exist_ok=True)
    escaped = assert_failed_preserving_cache(binary, env, cache, "--config", str(config), "--cwd", str(workspace), "list")
    assert "escapes package" in escaped.stderr
    write_plugin_package(package, "codex", "fixture", "1")

    # Malformed host JSON and conflicting active versions fail closed.
    output.write_text("{malformed\n")
    malformed = assert_failed_preserving_cache(binary, env, cache, "--config", str(config), "--cwd", str(workspace), "list")
    assert "invalid Codex plugin list JSON" in malformed.stderr
    write_json(output, {"installed": [active, dict(active, version="2")]})
    conflict = assert_failed_preserving_cache(binary, env, cache, "--config", str(config), "--cwd", str(workspace), "list")
    assert "multiple active Codex plugin versions" in conflict.stderr

    # Explicit discovery has no provider command or cache dependency.
    explicit_args = temporary / "explicit-codex-args"
    explicit_env = dict(env, CODEX_ARGS_FILE=str(explicit_args))
    explicit_root = temporary / "explicit-root"
    write_skill(explicit_root, "explicit", "Explicit filesystem fixture.")
    explicit_config = temporary / "explicit.toml"
    explicit_config.write_text(
        "\n".join(
            [
                "version = 1",
                'discovery = "explicit"',
                "agents = []",
                f"roots = [{toml_string(explicit_root)}]",
            ]
        )
        + "\n"
    )
    explicit = json.loads(
        run(
            binary,
            explicit_env,
            "--config",
            str(explicit_config),
            "--cwd",
            str(workspace),
            "list",
            "--json",
        ).stdout
    )
    assert explicit["total"] == 1
    assert explicit["results"][0]["name"] == "explicit"
    assert not explicit_args.exists()


def claude_contract(binary: str, temporary: Path) -> None:
    temporary.mkdir(parents=True)
    home = temporary / "claude-home"
    project = temporary / "project"
    inner = project / "child"
    unrelated = temporary / "unrelated"
    outside = temporary / "outside"
    user_package = temporary / "claude-user"
    project_package = temporary / "claude-project"
    local_package = temporary / "claude-local"
    inner_package = temporary / "claude-inner"
    for path in (project, inner, unrelated, outside):
        path.mkdir()
    write_plugin_package(user_package, "claude", "foo", "1")
    write_plugin_package(project_package, "claude", "foo", "2")
    write_plugin_package(local_package, "claude", "foo", "3")
    write_plugin_package(inner_package, "claude", "foo", "4")
    write_json(
        home / "plugins/installed_plugins.json",
        {
            "version": 2,
            "plugins": {
                "foo@market": [
                    {"scope": "user", "installPath": str(user_package), "version": "1"},
                    {
                        "scope": "project",
                        "installPath": str(project_package),
                        "version": "2",
                        "projectPath": str(project),
                    },
                    {
                        "scope": "local",
                        "installPath": str(local_package),
                        "version": "3",
                        "projectPath": str(project),
                    },
                    {
                        "scope": "project",
                        "installPath": str(inner_package),
                        "version": "4",
                        "projectPath": str(inner),
                    },
                ]
            },
        },
    )
    write_json(home / "settings.json", {"enabledPlugins": {"foo@market": True}})
    write_json(
        project / ".claude/settings.json",
        {"enabledPlugins": {"foo@market": True}},
    )
    write_json(
        project / ".claude/settings.local.json",
        {"enabledPlugins": {"foo@market": True}},
    )
    # An unrelated project may be damaged without poisoning the current one.
    (unrelated / ".claude").mkdir()
    (unrelated / ".claude/settings.json").write_text("{broken\n")

    env = os.environ.copy()
    env.update(
        HOME=str(temporary / "home"),
        CODEX_HOME=str(temporary / "codex-home"),
        CLAUDE_CONFIG_DIR=str(home),
        XDG_CONFIG_HOME=str(temporary / "config"),
        XDG_CACHE_HOME=str(temporary / "claude-cache"),
        XDG_STATE_HOME=str(temporary / "state"),
        PATH=os.environ.get("PATH", ""),
    )
    config = temporary / "claude.toml"
    write_config(config, "auto", [project, unrelated, inner])
    listed = json.loads(
        run(
            binary,
            env,
            "--config",
            str(config),
            "--cwd",
            str(project),
            "list",
            "--json",
        ).stdout
    )
    assert listed["total"] == 1
    row = listed["results"][0]
    assert row["name"] == "foo:release"
    assert row["plugin_id"] == "foo@market"
    assert Path(row["source"]).resolve() == (local_package / "skills").resolve()

    outside_list = json.loads(
        run(
            binary,
            env,
            "--config",
            str(config),
            "--cwd",
            str(outside),
            "list",
            "--json",
        ).stdout
    )
    assert outside_list["total"] == 1
    assert (
        Path(outside_list["results"][0]["source"]).resolve()
        == (user_package / "skills").resolve()
    )

    # Removing the local installation exposes the project-scoped version while
    # retaining the user package as the global fallback.
    write_json(
        home / "plugins/installed_plugins.json",
        {
            "version": 2,
            "plugins": {
                "foo@market": [
                    {"scope": "user", "installPath": str(user_package), "version": "1"},
                    {
                        "scope": "project",
                        "installPath": str(project_package),
                        "version": "2",
                        "projectPath": str(project),
                    },
                    {
                        "scope": "project",
                        "installPath": str(inner_package),
                        "version": "4",
                        "projectPath": str(inner),
                    },
                ]
            },
        },
    )
    project_only = json.loads(
        run(
            binary,
            env,
            "--config",
            str(config),
            "--cwd",
            str(project),
            "list",
            "--json",
        ).stdout
    )
    assert project_only["total"] == 1
    assert (
        Path(project_only["results"][0]["source"]).resolve()
        == (project_package / "skills").resolve()
    )

    # The deepest project owns the policy: disabling cannot inherit an
    # enabled ancestor; enabling selects the inner installation.
    write_json(inner / ".claude/settings.json", {"enabledPlugins": {"foo@market": False}})
    nested_disabled = json.loads(
        run(
            binary,
            env,
            "--config",
            str(config),
            "--cwd",
            str(inner),
            "list",
            "--json",
        ).stdout
    )
    assert nested_disabled["total"] == 0
    write_json(inner / ".claude/settings.json", {"enabledPlugins": {"foo@market": True}})
    nested_enabled = json.loads(
        run(
            binary,
            env,
            "--config",
            str(config),
            "--cwd",
            str(inner),
            "list",
            "--json",
        ).stdout
    )
    assert nested_enabled["total"] == 1
    assert (
        Path(nested_enabled["results"][0]["source"]).resolve()
        == (inner_package / "skills").resolve()
    )

    # A project-level disable hides the inherited user package in that scope.
    write_json(project / ".claude/settings.json", {"enabledPlugins": {"foo@market": False}})
    write_json(inner / ".claude/settings.json", {"enabledPlugins": {"foo@market": False}})
    (project / ".claude/settings.local.json").unlink()
    disabled = json.loads(
        run(
            binary,
            env,
            "--config",
            str(config),
            "--cwd",
            str(project),
            "list",
            "--json",
        ).stdout
    )
    assert disabled["total"] == 0


def main() -> None:
    binary = str(Path(sys.argv[1]).resolve())
    with tempfile.TemporaryDirectory(prefix="skillwick-discovery-") as directory:
        temporary = Path(directory)
        codex_contract(binary, temporary / "codex-fixture")
        claude_contract(binary, temporary / "claude-fixture")
    print("Provider discovery contracts passed")


if __name__ == "__main__":
    main()
