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
