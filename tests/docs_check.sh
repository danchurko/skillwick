#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
binary=${1:-$root/target/debug/skillwick}
case "$binary" in
  /*) ;;
  *) binary="$root/$binary" ;;
esac

[ -x "$binary" ] || {
  echo "documentation check requires an executable: $binary" >&2
  exit 2
}

exec python3 - "$root" "$binary" <<'PY'
from __future__ import annotations

import re
import subprocess
import sys
import tempfile
import unicodedata
from pathlib import Path

root = Path(sys.argv[1])
binary = Path(sys.argv[2])
errors: list[str] = []

required = [
    "AGENTS.md",
    "README.md",
    "CONTEXT.md",
    "CONTRIBUTING.md",
    "SECURITY.md",
    "assets/skillwick/SKILLWICK.md",
    "benchmarks/README.md",
    "docs/AGENTS.md",
    "docs/ARCHITECTURE.md",
    "docs/CHANGELOG.md",
    "docs/COMPATIBILITY.md",
    "docs/DECISIONS.md",
    "docs/GETTING_STARTED.md",
    "docs/IMPLEMENTATION.md",
    "docs/OPERATIONS.md",
    "docs/README.md",
    "docs/REFERENCE.md",
    "docs/RESEARCH.md",
    "docs/USAGE.md",
]

for relative in required:
    path = root / relative
    if not path.is_file():
        errors.append(f"missing required documentation: {relative}")

markdown_paths = {root / relative for relative in required}
markdown_paths.update((root / "docs").rglob("*.md"))
markdown_paths.update((root / "benchmarks").rglob("*.md"))
for path in sorted(markdown_paths):
    if not path.is_file():
        continue
    relative = path.relative_to(root)
    data = path.read_bytes()
    if not data.endswith(b"\n"):
        errors.append(f"{relative}: missing final newline")
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError as error:
        errors.append(f"{relative}: invalid UTF-8 ({error})")
        continue
    for line_number, line in enumerate(text.splitlines(), 1):
        if re.search(r"[ \t]+$", line):
            errors.append(f"{relative}:{line_number}: trailing whitespace")


def github_slug(text: str) -> str:
    text = re.sub(r"<[^>]*>", "", text)
    text = unicodedata.normalize("NFKD", text).encode("ascii", "ignore").decode()
    text = re.sub(r"[^\w\s-]", "", text.lower())
    return re.sub(r"[-\s]+", "-", text).strip("-")


def anchors(text: str) -> set[str]:
    found: set[str] = set()
    counts: dict[str, int] = {}
    for line in text.splitlines():
        match = re.match(r"^\s{0,3}#{1,6}\s+(.+?)\s*#*\s*$", line)
        if not match:
            continue
        slug = github_slug(match.group(1))
        suffix = counts.get(slug, 0)
        counts[slug] = suffix + 1
        found.add(slug if suffix == 0 else f"{slug}-{suffix}")
    found.update(re.findall(r"\bid=[\"']([^\"']+)[\"']", text))
    return found


link_pattern = re.compile(r"(?<!!)\[[^\]]*\]\(([^)\n]+)\)")
for source in sorted(markdown_paths):
    if not source.is_file():
        continue
    text = source.read_text(encoding="utf-8")
    for match in link_pattern.finditer(text):
        raw = match.group(1).strip()
        if raw.startswith("<") and ">" in raw:
            target = raw[1 : raw.index(">")]
        else:
            target = raw.split()[0] if raw else ""
        if not target or re.match(r"(?:[a-z][a-z0-9+.-]*:|//)", target, re.I):
            continue
        target_path, separator, fragment = target.partition("#")
        destination = (source.parent / target_path).resolve() if target_path else source
        try:
            destination.relative_to(root)
        except ValueError:
            errors.append(f"{source.relative_to(root)}: link escapes repository: {target}")
            continue
        if not destination.is_file():
            errors.append(f"{source.relative_to(root)}: missing link target: {target}")
        elif separator and fragment not in anchors(destination.read_text(encoding="utf-8")):
            errors.append(f"{source.relative_to(root)}: missing link anchor: {target}")


docs_map = (root / "docs/README.md").read_text(encoding="utf-8")
for target in (
    "GETTING_STARTED.md",
    "USAGE.md",
    "OPERATIONS.md",
    "REFERENCE.md",
    "COMPATIBILITY.md",
    "ARCHITECTURE.md",
    "AGENTS.md",
    "DECISIONS.md",
    "RESEARCH.md",
    "IMPLEMENTATION.md",
    "CHANGELOG.md",
    "../benchmarks/README.md",
):
    if f"]({target})" not in docs_map:
        errors.append(f"docs/README.md: missing navigation link to {target}")


retired = ("docs/design/current-direction.md", "docs/design/discovery-spec.md", "docs/SPEC.md")
for relative in retired:
    if (root / relative).exists():
        errors.append(f"retired documentation still exists: {relative}")
for path in sorted(markdown_paths):
    if not path.is_file():
        continue
    text = path.read_text(encoding="utf-8")
    for stale in ("current-direction.md", "discovery-spec.md", "docs/SPEC.md", "skillwick hook"):
        if stale in text:
            errors.append(f"{path.relative_to(root)}: stale contract reference: {stale}")


reference = (root / "docs/REFERENCE.md").read_text(encoding="utf-8")
help_result = subprocess.run(
    [str(binary), "--help"], cwd=root, text=True, capture_output=True, check=False
)
if help_result.returncode:
    errors.append(f"CLI help failed with exit {help_result.returncode}: {help_result.stderr.strip()}")
else:
    command_block = help_result.stdout.split("Commands:", 1)[-1].split("Options:", 1)[0]
    commands = set(re.findall(r"^\s{2}([a-z][a-z-]+)\s{2,}", command_block, re.M))
    documented_commands = {
        "search", "read", "inspect", "list", "refresh", "instructions",
        "init", "doctor", "uninstall", "completions",
    }
    for command in documented_commands:
        if command not in commands:
            errors.append(f"CLI help is missing documented command: {command}")
        if f"skillwick {command}" not in reference:
            errors.append(f"docs/REFERENCE.md is missing command: {command}")
    for command in sorted(documented_commands):
        result = subprocess.run(
            [str(binary), command, "--help"], cwd=root, text=True, capture_output=True, check=False
        )
        if result.returncode:
            errors.append(f"CLI {command} help failed with exit {result.returncode}")
            continue
        for option in sorted(set(re.findall(r"(?<![\w-])--[a-z][a-z0-9-]*", result.stdout))):
            if option not in reference:
                errors.append(f"docs/REFERENCE.md is missing CLI option: {option}")
    if "1-20" not in help_result.stdout and "1-20" not in subprocess.run(
        [str(binary), "search", "--help"], cwd=root, text=True, capture_output=True, check=False
    ).stdout:
        errors.append("CLI help is missing the search limit range")
    if "1 through 20" not in reference and "1-20" not in reference:
        errors.append("docs/REFERENCE.md is missing the search limit range")


with tempfile.TemporaryDirectory(prefix="skillwick-docs-") as directory:
    temporary = Path(directory)
    home = temporary / "home"
    work = temporary / "work"
    config = temporary / "config"
    cache = temporary / "cache"
    state = temporary / "state"
    skill = work / ".agents/skills/clean-fixture/SKILL.md"
    skill.parent.mkdir(parents=True)
    home.mkdir()
    work.mkdir(exist_ok=True)
    for path in (config, cache, state):
        path.mkdir()
    skill.write_text(
        "---\nname: clean-fixture\ndescription: Isolated documentation check.\n---\n\nbody\n",
        encoding="utf-8",
    )
    environment = {
        **__import__("os").environ,
        "HOME": str(home),
        "XDG_CONFIG_HOME": str(config),
        "XDG_CACHE_HOME": str(cache),
        "XDG_STATE_HOME": str(state),
    }

    def clean_run(*arguments: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [str(binary), "--cwd", str(work), *arguments],
            cwd=root,
            env=environment,
            text=True,
            capture_output=True,
            check=False,
        )

    initialized = clean_run("init", "--yes", "--agent", "none", "--inventory", "filesystem")
    if initialized.returncode:
        errors.append(f"clean filesystem setup failed: {initialized.stderr.strip()}")
    listed = clean_run("--json", "list")
    if listed.returncode:
        errors.append(f"clean filesystem list failed: {listed.stderr.strip()}")
    else:
        try:
            import json

            results = json.loads(listed.stdout)["results"]
            ids = [item["id"] for item in results if item["name"] == "clean-fixture"]
            if len(ids) != 1:
                errors.append("clean filesystem setup did not expose exactly one fixture")
            else:
                read = clean_run("read", ids[0])
                if read.returncode or "name: clean-fixture" not in read.stdout:
                    errors.append("clean filesystem setup could not read its fixture")
                read_name = clean_run("read", "clean-fixture")
                if (
                    read_name.returncode
                    or f"resolved-id: {ids[0]}" not in read_name.stdout
                    or "name: clean-fixture" not in read_name.stdout
                ):
                    errors.append("clean filesystem setup could not read its fixture by name")
        except (ValueError, KeyError, TypeError) as error:
            errors.append(f"clean filesystem list was not valid JSON: {error}")

if errors:
    for error in errors:
        print(f"docs-check: {error}", file=sys.stderr)
    raise SystemExit(1)

print("Documentation checks passed")
PY
