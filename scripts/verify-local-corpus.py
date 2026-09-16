#!/usr/bin/env python3
"""Prepare and hash source inputs for the source-install verification gate.

The shell wrapper intentionally passes paths through NUL-delimited files.  A
path is data here, so neither ``:`` nor a newline is a valid list separator.
The hash stream uses length-delimited canonical JSON records for the same
reason.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import stat
import sys
from pathlib import Path
from typing import Any, Iterable


CONFIG_VERSION = 1
DISCOVERY_VALUES = {"auto", "explicit"}
CONFIG_FIELDS = {
    "version",
    "discovery",
    "roots",
    "projects",
    "agents",
    "instructions_file",
}
PROJECT_FIELDS = {"path", "roots", "discovery"}
AGENT_VALUES = {"codex", "claude", "none"}
LEGACY_CONFIG_FIELDS = {"roots", "projects", "inventory", "agent", "instructions_file"}
LEGACY_PROJECT_FIELDS = {"path", "roots"}
LEGACY_INVENTORIES = {"filesystem", "codex"}
LEGACY_AGENTS = {"none", "codex"}
SOURCE_FILE_NAMES = {"SKILL.md", "installed_plugins.json", "settings.json", "settings.local.json"}
POLICY_FILE_NAME = "openai.yaml"
PLUGIN_MANIFEST_NAMES = {".codex-plugin", ".claude-plugin"}


class CorpusError(RuntimeError):
    """An actionable local corpus preparation or hashing failure."""


def error(message: str) -> None:
    raise CorpusError(message)


def read_nul(path: Path) -> list[str]:
    try:
        data = path.read_bytes()
    except OSError as exc:
        error(f"cannot read path list {path}: {exc}")
    if data and not data.endswith(b"\0"):
        error(f"path list is not NUL terminated: {path}")
    return [os.fsdecode(value) for value in data.split(b"\0") if value]


def write_nul(path: Path, values: Iterable[str]) -> None:
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("wb") as stream:
            for value in values:
                stream.write(os.fsencode(value))
                stream.write(b"\0")
    except OSError as exc:
        error(f"cannot write path list {path}: {exc}")


def dedupe(values: Iterable[str]) -> list[str]:
    result: list[str] = []
    seen: set[bytes] = set()
    for value in values:
        encoded = os.fsencode(value)
        if encoded not in seen:
            seen.add(encoded)
            result.append(value)
    return result


def parse_env_roots(raw: str) -> list[str]:
    """Parse the documented path-list environment variable.

    A JSON string array is accepted for paths containing the platform path
    separator.  Plain values retain the existing path-separator convention;
    when the complete value names an existing directory it is treated as one
    path, which keeps a single colon-containing path usable on macOS/Linux.
    """

    if not raw:
        return []
    if raw.lstrip().startswith("["):
        try:
            value = json.loads(raw)
        except json.JSONDecodeError as exc:
            error(f"SKILLWICK_SOURCE_ROOTS is invalid JSON: {exc}")
        if not isinstance(value, list) or not all(isinstance(item, str) for item in value):
            error("SKILLWICK_SOURCE_ROOTS JSON value must be an array of strings")
        return dedupe(value)

    if os.path.isdir(raw):
        return [raw]
    return dedupe(part for part in raw.split(os.pathsep) if part)


def parse_env_required(raw: str) -> list[str]:
    """Parse required names, with JSON for names containing whitespace."""

    if not raw:
        return []
    if raw.lstrip().startswith("["):
        try:
            value = json.loads(raw)
        except json.JSONDecodeError as exc:
            error(f"SKILLWICK_REQUIRED_SKILLS is invalid JSON: {exc}")
        if not isinstance(value, list) or not all(isinstance(item, str) for item in value):
            error("SKILLWICK_REQUIRED_SKILLS JSON value must be an array of strings")
        return dedupe(value)
    return dedupe(part for part in raw.split() if part)


def string_list(value: Any, label: str) -> list[str]:
    if not isinstance(value, list) or not all(isinstance(item, str) for item in value):
        error(f"{label} must be an array of strings")
    return list(value)


def read_toml(path: Path) -> dict[str, Any]:
    try:
        path.stat()
    except FileNotFoundError:
        return {
            "version": CONFIG_VERSION,
            "discovery": "auto",
            "roots": [],
            "projects": [],
            "agents": [],
        }
    except OSError as exc:
        error(f"cannot inspect Skillwick configuration at {path}: {exc}")
    try:
        import tomllib
    except ImportError as exc:
        error(f"Python tomllib is required to validate configured inputs: {exc}")
    try:
        text = path.read_text(encoding="utf-8")
        document = tomllib.loads(text)
    except (OSError, UnicodeError, ValueError) as exc:
        error(f"invalid Skillwick configuration at {path}: {exc}")
    if not isinstance(document, dict):
        error(f"invalid Skillwick configuration at {path}: top-level value must be a table")
    return document


def normalize_config(document: dict[str, Any], path: Path) -> dict[str, Any]:
    unknown = sorted(set(document) - CONFIG_FIELDS)
    if unknown:
        error(f"invalid Skillwick configuration at {path}: unknown field(s): {', '.join(unknown)}")
    if document.get("version") != CONFIG_VERSION or isinstance(document.get("version"), bool):
        error(
            f"unsupported or missing Skillwick configuration version at {path}; "
            "expected version = 1"
        )
    discovery = document.get("discovery", "auto")
    if discovery not in DISCOVERY_VALUES:
        error(f"invalid Skillwick configuration at {path}: discovery must be auto or explicit")
    roots = string_list(document.get("roots", []), "configuration roots")
    projects = document.get("projects", [])
    if not isinstance(projects, list) or not all(isinstance(item, dict) for item in projects):
        error("configuration projects must be an array of tables")
    normalized_projects: list[dict[str, Any]] = []
    for index, project in enumerate(projects):
        unknown_project = sorted(set(project) - PROJECT_FIELDS)
        if unknown_project:
            error(
                f"invalid Skillwick configuration at {path}: project {index} has unknown field(s): "
                f"{', '.join(unknown_project)}"
            )
        project_path = project.get("path")
        if not isinstance(project_path, str):
            error(f"invalid Skillwick configuration at {path}: project {index} path is required")
        project_discovery = project.get("discovery", "auto")
        if project_discovery not in DISCOVERY_VALUES:
            error(
                f"invalid Skillwick configuration at {path}: project {index} discovery must be auto or explicit"
            )
        normalized_projects.append(
            {
                "path": project_path,
                "roots": string_list(project.get("roots", []), f"project {index} roots"),
                "discovery": project_discovery,
            }
        )
    agents = document.get("agents", [])
    if not isinstance(agents, list) or not all(isinstance(item, str) for item in agents):
        error("configuration agents must be an array of strings")
    if any(agent not in AGENT_VALUES for agent in agents):
        error("configuration agents contains an unsupported value")
    instructions_file = document.get("instructions_file")
    if instructions_file is not None and not isinstance(instructions_file, str):
        error("configuration instructions_file must be a string")
    return {
        "version": CONFIG_VERSION,
        "discovery": discovery,
        "roots": roots,
        "projects": normalized_projects,
        "agents": list(agents),
        "instructions_file": instructions_file,
    }


def normalize_legacy_config(document: dict[str, Any], path: Path) -> dict[str, Any]:
    """Convert the historical shape for this verifier's test harness only."""

    unknown = sorted(set(document) - LEGACY_CONFIG_FIELDS)
    if unknown:
        error(
            f"invalid legacy Skillwick configuration at {path}: unknown field(s): {', '.join(unknown)}"
        )
    inventory = document.get("inventory", "filesystem")
    if not isinstance(inventory, str) or inventory not in LEGACY_INVENTORIES:
        error(f"invalid legacy Skillwick configuration at {path}: unsupported inventory")
    agent = document.get("agent", "none")
    if not isinstance(agent, str) or agent not in LEGACY_AGENTS:
        error(f"invalid legacy Skillwick configuration at {path}: unsupported agent")
    roots = string_list(document.get("roots", []), "legacy configuration roots")
    projects = document.get("projects", [])
    if not isinstance(projects, list) or not all(isinstance(item, dict) for item in projects):
        error("legacy configuration projects must be an array of tables")
    normalized_projects: list[dict[str, Any]] = []
    for index, project in enumerate(projects):
        unknown_project = sorted(set(project) - LEGACY_PROJECT_FIELDS)
        if unknown_project:
            error(
                f"invalid legacy Skillwick configuration at {path}: project {index} has unknown field(s): "
                f"{', '.join(unknown_project)}"
            )
        project_path = project.get("path")
        if not isinstance(project_path, str):
            error(f"invalid legacy Skillwick configuration at {path}: project {index} path is required")
        normalized_projects.append(
            {
                "path": project_path,
                "roots": string_list(project.get("roots", []), f"legacy project {index} roots"),
                "discovery": "auto",
            }
        )
    instructions_file = document.get("instructions_file")
    if instructions_file is not None and not isinstance(instructions_file, str):
        error("legacy configuration instructions_file must be a string")
    # Legacy inventory and agent values are intentionally ignored. The
    # temporary candidate config always exercises current automatic providers
    # and leaves integration ownership to the developer's existing journal.
    return {
        "version": CONFIG_VERSION,
        "discovery": "auto",
        "roots": roots,
        "projects": normalized_projects,
        "agents": [],
        "instructions_file": instructions_file,
    }


def load_config(path: Path, *, allow_test_harness_legacy: bool = False) -> dict[str, Any]:
    document = read_toml(path)
    try:
        return normalize_config(document, path)
    except CorpusError as modern_error:
        if not allow_test_harness_legacy:
            raise
        # Fall back only for the exact historical fields. TOML syntax errors,
        # unknown fields, and unsupported values remain hard failures.
        if not (set(document) & {"inventory", "agent"}) and "version" in document:
            raise
        try:
            return normalize_legacy_config(document, path)
        except CorpusError:
            raise modern_error


def existing_directory(path: str) -> bool:
    try:
        return os.path.isdir(path)
    except OSError:
        return False


def path_from(value: str) -> Path:
    return Path(value)


def project_auto_roots(project: dict[str, Any]) -> list[str]:
    project_path = path_from(project["path"])
    return [
        os.fspath(project_path / ".agents/skills"),
        os.fspath(project_path / ".codex/skills"),
        os.fspath(project_path / ".claude/skills"),
    ]


def extract_install_paths(value: Any) -> list[str]:
    result: list[str] = []
    if isinstance(value, dict):
        for key, item in value.items():
            if key == "installPath" and isinstance(item, str):
                result.append(item)
            result.extend(extract_install_paths(item))
    elif isinstance(value, list):
        for item in value:
            result.extend(extract_install_paths(item))
    return result


def registry_install_paths(path: Path) -> list[str]:
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, ValueError):
        # The installed binary performs the authoritative schema check during
        # the automatic pass.  Preparation only uses valid paths it can read.
        return []
    return extract_install_paths(document)


def toml_string(value: str, label: str) -> str:
    try:
        value.encode("utf-8")
    except UnicodeEncodeError as exc:
        error(f"{label} is not valid UTF-8: {exc}")
    return json.dumps(value, ensure_ascii=False)


def render_explicit_config(
    path: Path,
    global_roots: list[str],
    projects: list[dict[str, Any]],
) -> None:
    lines = [
        f"version = {CONFIG_VERSION}",
        'discovery = "explicit"',
        "agents = []",
        "roots = [",
    ]
    lines.extend(f"  {toml_string(root, 'source root')}," for root in global_roots)
    lines.append("]")
    for project in projects:
        roots = project["roots"]
        if not roots:
            continue
        lines.extend(
            [
                "[[projects]]",
                f"path = {toml_string(project['path'], 'project path')}",
                'discovery = "explicit"',
                "roots = [",
            ]
        )
        lines.extend(f"  {toml_string(root, 'project source root')}," for root in roots)
        lines.append("]")
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    except OSError as exc:
        error(f"cannot write temporary explicit configuration {path}: {exc}")


def render_automatic_config(
    path: Path,
    global_roots: list[str],
    projects: list[dict[str, Any]],
) -> None:
    lines = [
        f"version = {CONFIG_VERSION}",
        'discovery = "auto"',
        "agents = []",
        "roots = [",
    ]
    lines.extend(f"  {toml_string(root, 'source root')}," for root in global_roots)
    lines.append("]")
    for project in projects:
        lines.extend(
            [
                "",
                "[[projects]]",
                f"path = {toml_string(project['path'], 'project path')}",
                'discovery = "auto"',
                "roots = [",
            ]
        )
        lines.extend(
            f"  {toml_string(root, 'project source root')}," for root in project["roots"]
        )
        lines.append("]")
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    except OSError as exc:
        error(f"cannot write temporary automatic configuration {path}: {exc}")


def prepare(arguments: argparse.Namespace) -> None:
    config_path = arguments.config
    config = load_config(
        config_path, allow_test_harness_legacy=arguments.allow_test_harness_legacy
    )
    cli_roots = read_nul(arguments.cli_roots_file)
    cli_required = read_nul(arguments.cli_required_file)
    environment_required = parse_env_required(arguments.env_required_value or "")
    required = dedupe([*environment_required, *cli_required])
    write_nul(arguments.required_file, required)
    environment_roots = parse_env_roots(arguments.env_roots_value or "")
    configured_global = dedupe([*cli_roots, *environment_roots, *config["roots"]])
    top_auto = config["discovery"] == "auto"
    home = os.fspath(arguments.home)
    codex_home = os.fspath(arguments.codex_home)
    claude_home = os.fspath(arguments.claude_home)

    global_auto_all = [
        os.path.join(home, ".agents/skills"),
        os.path.join(codex_home, "skills"),
        os.path.join(claude_home, "skills"),
        os.path.join(codex_home, "plugins"),
        os.path.join(claude_home, "plugins"),
    ] if top_auto else []
    projects: list[dict[str, Any]] = []
    project_hash_roots: list[str] = []
    project_auto_all: list[str] = []
    for project in config["projects"]:
        roots = list(project["roots"])
        if project["discovery"] == "auto":
            auto = project_auto_roots(project)
            project_auto_all.extend(auto)
            roots.extend(path for path in auto if existing_directory(path))
        roots = dedupe(roots)
        projects.append({"path": project["path"], "roots": roots})
        project_hash_roots.extend(project["roots"])

    global_explicit = configured_global
    rendered_projects = [
        {"path": project["path"], "roots": dedupe(project["roots"])}
        for project in projects
    ]
    render_explicit_config(arguments.explicit_config, global_explicit, rendered_projects)
    render_automatic_config(arguments.automatic_config, global_explicit, rendered_projects)

    registry = Path(claude_home) / "plugins/installed_plugins.json"
    install_paths = registry_install_paths(registry) if top_auto and registry.is_file() else []
    auto_roots_all = dedupe([*global_auto_all, *project_auto_all, *install_paths])
    all_roots = dedupe(
        [
            *global_explicit,
            *project_hash_roots,
            *auto_roots_all,
        ]
    )
    metadata_files = [config_path]
    if top_auto:
        metadata_files.extend(
            [
                os.path.join(codex_home, "plugins/installed_plugins.json"),
                os.path.join(codex_home, "plugins/.plugin-appserver"),
                os.path.join(claude_home, "plugins/installed_plugins.json"),
                os.path.join(claude_home, "settings.json"),
            ]
        )
    for project in config["projects"]:
        if project["discovery"] == "auto":
            project_path = project["path"]
            metadata_files.extend(
                [
                    os.path.join(project_path, ".claude/settings.json"),
                    os.path.join(project_path, ".claude/settings.local.json"),
                ]
            )
    write_nul(arguments.roots_file, all_roots)
    metadata_files.extend(
        [
            os.path.join(codex_home, "config.toml"),
            os.path.join(codex_home, "SKILLWICK.md"),
            os.path.join(codex_home, "AGENTS.md"),
            os.path.join(claude_home, "SKILLWICK.md"),
            os.path.join(claude_home, "CLAUDE.md"),
        ]
    )
    write_nul(arguments.files_file, dedupe(metadata_files))
    try:
        config_present = config_path.stat()
    except FileNotFoundError:
        config_present = None
    except OSError as exc:
        error(f"cannot inspect Skillwick configuration at {config_path}: {exc}")
    active_auto = config_present is not None or any(
        os.path.lexists(path) for path in auto_roots_all + metadata_files[1:]
    )
    try:
        arguments.auto_marker.parent.mkdir(parents=True, exist_ok=True)
        if active_auto:
            arguments.auto_marker.write_text("present\n", encoding="ascii")
        else:
            arguments.auto_marker.write_text("", encoding="ascii")
    except OSError as exc:
        error(f"cannot write automatic-source marker {arguments.auto_marker}: {exc}")


def encoded_path(path: str) -> str:
    return base64.b64encode(os.fsencode(path)).decode("ascii")


def record_bytes(record: dict[str, Any]) -> bytes:
    return json.dumps(record, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode(
        "utf-8"
    )


def selected_source_file(path: str) -> bool:
    name = os.path.basename(path)
    if name in SOURCE_FILE_NAMES:
        return True
    if name == POLICY_FILE_NAME and os.path.basename(os.path.dirname(path)) == "agents":
        return True
    return name == "plugin.json" and os.path.basename(os.path.dirname(path)) in PLUGIN_MANIFEST_NAMES


def digest_file(path: str) -> tuple[str, int]:
    digest = hashlib.sha256()
    size = 0
    try:
        with open(path, "rb") as stream:
            while chunk := stream.read(1024 * 1024):
                digest.update(chunk)
                size += len(chunk)
    except OSError as exc:
        error(f"cannot read corpus input {path}: {exc}")
    return digest.hexdigest(), size


def add_file_record(records: dict[tuple[str, str], dict[str, Any]], path: str) -> None:
    canonical = os.path.realpath(path)
    key = (encoded_path(path), encoded_path(canonical))
    try:
        metadata = os.stat(path)
    except FileNotFoundError:
        records[key] = {
            "kind": "file",
            "path": encoded_path(path),
            "canonical": encoded_path(canonical),
            "state": "missing",
        }
        return
    except OSError as exc:
        error(f"cannot inspect corpus input {path}: {exc}")
    if not stat.S_ISREG(metadata.st_mode):
        records[key] = {
            "kind": "file",
            "path": encoded_path(path),
            "canonical": encoded_path(canonical),
            "state": "non-regular",
            "mode": stat.S_IMODE(metadata.st_mode),
            "link": os.readlink(path) if os.path.islink(path) else None,
        }
        return
    digest, size = digest_file(path)
    records[key] = {
        "kind": "file",
        "path": encoded_path(path),
        "canonical": encoded_path(canonical),
        "state": "regular",
        "mode": stat.S_IMODE(metadata.st_mode),
        "link": os.readlink(path) if os.path.islink(path) else None,
        "size": size,
        "sha256": digest,
    }


def walk_root(records: dict[tuple[str, str], dict[str, Any]], root: str) -> None:
    canonical = os.path.realpath(root)
    try:
        metadata = os.stat(root)
    except FileNotFoundError:
        records[("root", encoded_path(root))] = {
            "kind": "root",
            "path": encoded_path(root),
            "state": "missing",
        }
        return
    except OSError as exc:
        error(f"cannot inspect corpus root {root}: {exc}")
    root_key = ("root", encoded_path(root))
    if not stat.S_ISDIR(metadata.st_mode):
        records[root_key] = {
            "kind": "root",
            "path": encoded_path(root),
            "canonical": encoded_path(canonical),
            "state": "non-directory",
        }
        return
    records[root_key] = {
        "kind": "root",
        "path": encoded_path(root),
        "canonical": encoded_path(canonical),
        "state": "directory",
    }
    stack = [(root, False)]
    visited: set[str] = set()
    while stack:
        directory, in_package = stack.pop()
        in_package = in_package or os.path.isfile(os.path.join(directory, "SKILL.md"))
        directory_canonical = os.path.realpath(directory)
        if directory_canonical in visited:
            continue
        visited.add(directory_canonical)
        try:
            entries = sorted(os.scandir(directory), key=lambda entry: os.fsencode(entry.name))
        except OSError as exc:
            error(f"cannot read corpus directory {directory}: {exc}")
        for entry in entries:
            path = entry.path
            try:
                if in_package:
                    add_file_record(records, path)
                if entry.is_dir(follow_symlinks=True):
                    stack.append((path, in_package))
                elif not in_package and entry.is_file(follow_symlinks=True) and selected_source_file(path):
                    add_file_record(records, path)
            except OSError as exc:
                error(f"cannot inspect corpus entry {path}: {exc}")


def hash_inputs(arguments: argparse.Namespace) -> None:
    records: dict[tuple[str, str], dict[str, Any]] = {}
    for root in read_nul(arguments.roots_file):
        walk_root(records, root)
    for path in read_nul(arguments.files_file):
        add_file_record(records, path)
    digest = hashlib.sha256()
    for record in sorted(records.values(), key=record_bytes):
        payload = record_bytes(record)
        digest.update(len(payload).to_bytes(8, "big"))
        digest.update(payload)
    print(digest.hexdigest())


def check_counts(arguments: argparse.Namespace) -> None:
    try:
        inventory = json.loads(arguments.inventory_file.read_text(encoding="utf-8"))
        doctor = json.loads(arguments.doctor_file.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, ValueError) as exc:
        error(f"cannot read corpus reports: {exc}")
    if inventory.get("version") != 3 or doctor.get("version") != 3:
        error("corpus reports must use JSON version 3")
    results = inventory.get("results")
    counts = doctor.get("counts")
    sources = doctor.get("sources")
    if not isinstance(results, list) or not isinstance(counts, dict) or not isinstance(sources, list):
        error("corpus reports have an invalid shape")
    roots = sorted(
        {
            source.get("root")
            for source in sources
            if isinstance(source, dict) and isinstance(source.get("root"), str)
        }
    )
    try:
        import sqlite3

        connection = sqlite3.connect(str(arguments.cache))
    except (ImportError, OSError) as exc:
        error(f"Python sqlite3 is required for corpus count checks: {exc}")
    try:
        integrity = connection.execute("PRAGMA integrity_check").fetchone()
        if integrity != ("ok",):
            error(f"corpus cache integrity check failed: {integrity!r}")
        if roots:
            placeholders = ",".join("?" for _ in roots)
            scope = (
                "EXISTS (SELECT 1 FROM skill_roots sr "
                f"WHERE sr.skill_id=s.id AND sr.root IN ({placeholders}))"
            )
            filesystem = connection.execute(
                "SELECT count(*) FROM skills s "
                "WHERE s.source_kind='filesystem' AND " + scope,
                roots,
            ).fetchone()[0]
            model_discoverable = connection.execute(
                "SELECT count(*) FROM skills s "
                "WHERE s.source_kind='filesystem' AND s.enabled=1 "
                "AND s.model_discoverable=1 AND " + scope,
                roots,
            ).fetchone()[0]
            raw = connection.execute(
                f"SELECT count(*) FROM skill_roots WHERE root IN ({placeholders})", roots
            ).fetchone()[0]
        else:
            filesystem = model_discoverable = raw = 0
    except Exception as exc:  # sqlite3.Error varies across Python versions.
        error(f"corpus SQL count check failed: {exc}")
    finally:
        connection.close()
    expected = {
        "filesystem": filesystem,
        "raw": raw,
        "model_discoverable": model_discoverable,
        "groups": len(results),
    }
    for key, value in expected.items():
        if counts.get(key) != value:
            error(f"corpus count mismatch for {key}: doctor={counts.get(key)!r}, SQL={value!r}")
    if inventory.get("total") != len(results) or counts.get("groups") != inventory.get("total"):
        error("corpus list total does not match grouped SQL/report counts")
    if raw < filesystem:
        error(f"corpus raw count {raw} is below canonical count {filesystem}")
    print(
        "corpus counts: "
        f"filesystem={filesystem} raw={raw} model-discoverable={model_discoverable} groups={len(results)}"
    )


def parser() -> argparse.ArgumentParser:
    command = argparse.ArgumentParser(description=__doc__)
    subcommands = command.add_subparsers(dest="command", required=True)
    prepare_command = subcommands.add_parser("prepare")
    prepare_command.add_argument("--config", type=Path, required=True)
    prepare_command.add_argument("--home", type=Path, required=True)
    prepare_command.add_argument("--codex-home", type=Path, required=True)
    prepare_command.add_argument("--claude-home", type=Path, required=True)
    prepare_command.add_argument("--cli-roots-file", type=Path, required=True)
    prepare_command.add_argument("--cli-required-file", type=Path, required=True)
    prepare_command.add_argument("--env-roots-value", default="")
    prepare_command.add_argument("--env-required-value", default="")
    prepare_command.add_argument("--allow-test-harness-legacy", action="store_true")
    prepare_command.add_argument("--explicit-config", type=Path, required=True)
    prepare_command.add_argument("--automatic-config", type=Path, required=True)
    prepare_command.add_argument("--roots-file", type=Path, required=True)
    prepare_command.add_argument("--files-file", type=Path, required=True)
    prepare_command.add_argument("--auto-marker", type=Path, required=True)
    prepare_command.add_argument("--required-file", type=Path, required=True)
    hash_command = subcommands.add_parser("hash")
    hash_command.add_argument("--roots-file", type=Path, required=True)
    hash_command.add_argument("--files-file", type=Path, required=True)
    check_command = subcommands.add_parser("check")
    check_command.add_argument("--cache", type=Path, required=True)
    check_command.add_argument("--inventory-file", type=Path, required=True)
    check_command.add_argument("--doctor-file", type=Path, required=True)
    return command


def main() -> int:
    arguments = parser().parse_args()
    try:
        if arguments.command == "prepare":
            prepare(arguments)
        elif arguments.command == "hash":
            hash_inputs(arguments)
        else:
            check_counts(arguments)
    except CorpusError as exc:
        print(f"local corpus: ERROR: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
