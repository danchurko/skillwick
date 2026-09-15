# Command and JSON reference

This document describes the shipped CLI contract. `skillwick --help` and each
subcommand's `--help` output are the executable source for option spelling; the
documentation check compares both surfaces.

## Global options

These options are global and may appear before or after a subcommand:

| Option | Meaning |
| --- | --- |
| `--json` | Emit version-2 JSON for `search`, `list`, `inspect`, or `doctor`. |
| `--cwd <PATH>` | Resolve applicable project roots from `PATH`. |
| `--config <PATH>` | Read configuration from `PATH` instead of the XDG default. |
| `-h, --help` | Print help. |
| `-V, --version` | Print the executable version. |

## Search

```text
skillwick search <QUERY>... [--limit <N>]
```

Search is explicit and joins multiple query arguments with spaces. It returns
up to five candidates by default; `N` must be 1 through 20. Results are
limited to enabled, model-discoverable skills under roots applicable to
`--cwd`. A valid no-match query exits successfully.

Human output bounds each record independently and marks a truncated
description. JSON returns the complete selected result set.

## Read

```text
skillwick read <ID|NAME>
```

Read accepts an exact ID returned by search/list or an exact, case-sensitive
name. Exact IDs take precedence. A unique name prints its resolved ID before
the selected live `SKILL.md`, path, and package base. Duplicate names fail
closed and list candidate source, scope, and path for explicit ID selection.

Before returning content, read verifies enabled/model-discoverable state,
canonical path identity, the 1 MiB file bound, UTF-8 encoding, and the indexed
content hash. A changed or unavailable source exits 3; cached text is never
used as a substitute.

## Inspect

```text
skillwick inspect <ID> [--files]
```

Without `--files`, inspect prints indexed metadata and provenance. With
`--files`, it adds a bounded live package listing. It does not read supporting
file bodies, follow symlink entries, or execute package content. JSON retains
the `results` envelope and adds `package`, `counts_scope`, `counts`, and
`limits` for the listing.

## List

```text
skillwick list
```

List prints every current-scope enabled, model-discoverable filesystem record
and its complete total. Equivalent canonical paths from overlapping roots are
deduplicated for public output; scope and source are contextualized for the
current roots. There is no pagination or hidden compatibility flag.

## Refresh

```text
skillwick refresh
```

Refresh explicitly reconciles configured roots applicable to `--cwd`. It
parses and validates a complete filesystem scan, updates the scoped SQLite/FTS5
snapshot, and publishes atomically. Failed source updates retain the previous
published cache and exit 3.

## Instructions

```text
skillwick instructions
```

Instructions prints the canonical compact agent context to stdout. It performs
no indexing or configuration changes.

## Init

```text
skillwick init [--yes] [--dry-run] [--agent <codex|none>]
    [--root <PATH>]... [--project-root <PATH>]...
    [--instructions-file <PATH>]
```

`--root` registers a shared filesystem discovery root and may repeat.
`--project-root` registers a root associated with the normalized global
`--cwd`; it applies there and in descendants and may repeat. Roots are
canonicalized and must be directories. `--agent` defaults to `codex`; `none`
skips integration file changes. `--instructions-file` selects the agent
instructions file used by the optional Codex integration.

`--dry-run` prints the plan without writing configuration, integration files,
or the cache. Non-interactive apply requires `--yes`.

## Doctor

```text
skillwick doctor [--strict]
```

Doctor reconciles the current filesystem scope and reports source, cache,
integration, policy, and current-scope count state. `--strict` exits 3 when
health is not valid. Text diagnostics go to stdout and warnings go to stderr.

## Uninstall

```text
skillwick uninstall [--purge-cache]
```

Uninstall removes only Skillwick-owned integration. `--purge-cache` also
removes the disposable SQLite cache; installed skills and unrelated files
remain untouched.

## Completions

```text
skillwick completions zsh
```

Completions prints Zsh completion definitions and does not modify shell files.

## JSON envelope

Machine-readable search, list, inspect, and doctor commands use version 2.
Search and ordinary inspect have this shape:

```json
{"version":2,"results":[{"id":"example@abcdef","name":"example","description":"..."}]}
```

List adds a complete `total`:

```json
{"version":2,"total":1,"results":[...]}
```

`inspect --files` additionally includes `package`, `counts_scope`, `counts`,
and `limits`. Doctor returns `healthy`, `config`, `cache`, `sources`,
`diagnostics`, `integration_present`, and current-scope `counts`.

Other commands reject `--json` with exit code 2 instead of silently ignoring
it. All JSON string fields are terminal-safe.

## Paths and exit codes

Default paths are:

```text
$XDG_CONFIG_HOME/skillwick/config.toml
$XDG_CACHE_HOME/skillwick/index-v3.sqlite
$XDG_STATE_HOME/skillwick/integration.json
```

| Code | Meaning |
| ---: | --- |
| `0` | Success or a valid no-match result. |
| `1` | Operational, configuration, or database failure. |
| `2` | Unknown command/option, invalid argument or limit, unsupported output mode, or non-interactive setup without `--yes`. |
| `3` | Incomplete source, stale/unavailable selected skill, scope mismatch, denied target, or strict doctor failure. |
