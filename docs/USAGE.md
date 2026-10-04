# Usage

Known guidance can be loaded directly; discovery is optional:

```sh
skillwick read astra-orchestrator codebase-memory caveman
skillwick search "SQLite full text ranking" --limit 3
skillwick inspect ID --files
skillwick read ID
```

An exact name selects one distinct package group. Different packages sharing a
name require an ID. Verified copied packages preserve all origins and member IDs;
use a member ID to select its particular base directory.

## Scripts and shell composition

```sh
skillwick read --raw caveman > /tmp/caveman-instructions.md
skillwick --json read caveman
skillwick --json list
skillwick doctor --strict --require codebase-memory
```

A valid no-match search exits 0. Missing required reads exit 3. Use `&&` when the
next command depends on successful instruction loading; do not suppress failures.
Batch reads validate every target before any body is written. Errors/warnings use
stderr, machine results use stdout, and early-closing pipes are handled normally.

Use `--` for query text or names beginning with a hyphen. Global `--json`, `--cwd`,
and `--config` can occur before or after a command. JSON version 3 preserves exact
strings; parse JSON rather than splitting human output on whitespace.

## Observable operation status

Record each operation, rather than trusting a composed shell command's final
status. These examples preserve normal shell semantics:

```sh
skillwick read missing && skillwick read caveman    # second read is skipped
skillwick read missing; printf 'trailing command\n' # final status can be 0
if skillwick read astra-orchestrator caveman; then
  printf 'required batch read succeeded\n'
else
  status=$?
  printf 'required batch read failed: %s\n' "$status" >&2
fi
```

Use one dedicated batch for known required names. Search additional guidance with
one coherent task query; a list of unrelated known names is not a relevance query.
Unknown commands, obsolete `find` forms, invalid limits/flags and unsupported
output formats fail with exit 2. An unavailable or invocation-denied selection
fails with exit 3; correcting eligibility belongs to the package/source owner.

## Validate JSON fields

Search, list, inspect and read use version 3 with a required `results` array.
Read results additionally contain exact `content`, `id`, `hash`, `path` and `base`.
Save output after checking the command's status, then parse required fields:

```sh
output=$(mktemp)
if skillwick --json read caveman > "$output"; then
  python3 - "$output" <<'PY'
import json, sys
with open(sys.argv[1]) as stream:
    value = json.load(stream)
assert value["version"] == 3 and isinstance(value["results"], list)
assert value["results"]
for row in value["results"]:
    assert all(isinstance(row[key], str) for key in ("id", "hash", "path", "base", "content"))
    print(row["id"], row["base"])
PY
else
  status=$?
  printf 'read failed: %s\n' "$status" >&2
fi
rm -f "$output"
```

Doctor uses a different version-3 envelope. Validate `healthy` as a Boolean,
`sources`, `diagnostics` and `required` as arrays, and `counts` as an object.
Each required entry has `name`, `resolved_id` and `diagnostic`; `resolved_id` is
null on failure. Do not iterate an invented field with an optional default:
a schema mismatch is a failed consumer, not evidence of an empty inventory.

## Scope

A default lookup discovers supported shared and host sources. Register a workspace
with `skillwick --cwd PATH init --yes --agent none --project` before using its
standard project skill directories. Custom sources use `--root` or `--project-root`.
Project scope includes descendants only; sibling projects do not leak through the
shared derived cache. Automatic discovery never sweeps the whole home directory.

For host-independent scripts, configure `--discovery explicit`. Otherwise Codex
plugin resolution requires a successful bounded installed-plugin listing. Claude
plugins come from their active installation registry and enablement settings.

Every lookup reconciles current sources. Failed complete-source checks preserve
cache state but fail the operation. Correct the reported source and retry; ordinary
package changes do not require a separate refresh.

See [reference](REFERENCE.md) for commands and formats and [operations](OPERATIONS.md)
for recovery, setup ownership, and uninstall.
