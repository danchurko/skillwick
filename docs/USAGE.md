# Usage and configuration

Skillwick is an explicit local lookup tool. Configured filesystem roots are
the complete discovery authority; the SQLite database is only a rebuildable
derived index. Lookup commands reconcile their applicable roots before reading
the index and never execute skill files.

## Configure roots

Use `init --agent none` when you only want to configure discovery:

```sh
skillwick init --yes --agent none --root "$HOME/.agents/skills"
skillwick --cwd "$PWD" init --yes --agent none \
  --project-root "$PWD/.agents/skills"
```

`--root` registers a shared root. It applies in every workspace. Each
`--project-root` is associated with the normalized `--cwd` and applies in that
directory and its descendants. Roots are canonicalized at setup and must be
directories. Unregistered home and ancestor directories are not searched.

The resulting configuration is conceptually:

```toml
roots = ["/Users/example/.agents/skills"]
agent = "none"

[[projects]]
path = "/Users/example/project"
roots = ["/Users/example/project/.agents/skills"]
```

The config file is owned by Skillwick. Installed package directories and their
updates remain owned by the package installer or the user who placed them.

## Search and select

Search takes one or more task words and returns up to five candidates by
default. `--limit` accepts 1 through 20:

```sh
skillwick search "deploy an AgentCore MCP server with TypeScript"
skillwick search "SQLite full text ranking" --limit 3
```

Search is lexical SQLite FTS5 retrieval. It is not an instruction to load a
candidate. Review the result, then use its exact ID:

```sh
skillwick inspect ID
skillwick inspect ID --files
skillwick read ID
```

`read` also accepts a unique, exact, case-sensitive name. Duplicate names are
reported with their source and scope so the caller can choose an ID. Reads
recheck the live canonical path, size, UTF-8 encoding, and content hash. A
changed or unavailable source fails closed instead of returning cached text.

## Inventory behavior

Each search, list, read, and inspect command reconciles the configured roots
applicable to its `--cwd`. A complete scan parses bounded `SKILL.md` metadata,
checks adjacent invocation policy, updates only the relevant root associations,
and publishes the derived SQLite/FTS5 snapshot atomically. Unchanged source
material leaves the durable cache untouched. `refresh` runs the same
reconciliation path explicitly.

Overlapping roots are deduplicated by canonical instruction path for public
results. Raw root associations remain in the index for diagnostics and
duplicate accounting. A project lookup cannot use a row associated only with
another project, even when both projects share a cache.

Additions, edits, renames, removals, and policy-file presence or content changes
are visible on the next lookup. Missing roots, unreadable files, malformed
metadata, and unauthorized symlink escapes fail the affected operation with
the previous complete cache preserved. They are never silently converted into
an empty inventory.

## Visibility and package safety

Only enabled, model-discoverable records under applicable roots appear in
search, list, counts, read, or inspect results. `disable-model-invocation:
true` in frontmatter and `policy.allow_implicit_invocation: false` in the
adjacent `agents/openai.yaml` deny discovery. Invalid recognized policy fails
closed and remains visible through `doctor` diagnostics.

`inspect --files` reports a bounded relative package listing. It does not read
supporting-file bodies, follow symlink entries, or execute scripts. A successful
`read` only returns the selected `SKILL.md`; it does not authorize running
anything in the package or changing configuration.

## Optional agent integration

The default setup target is Codex, but its integration is only Skillwick's
context/reference files. It does not query a native catalogue or change
installed packages. Preview and apply it separately:

```sh
skillwick init --dry-run --yes --root "$HOME/.agents/skills"
skillwick init --yes --root "$HOME/.agents/skills"
skillwick doctor --strict
```

Use `--agent none` to avoid integration files. Use `--instructions-file PATH`
when the managed agent instructions live somewhere other than the default
`$CODEX_HOME/AGENTS.md`. A managed configuration owner may instead consume
`skillwick instructions` and own the destination itself.

`--dry-run` performs no persistent writes. Setup and uninstall preserve
unrelated files and report ownership drift rather than overwriting another
writer's changes.

## Paths and output

Skillwick honors `HOME`, `CODEX_HOME`, and XDG overrides:

```text
$XDG_CONFIG_HOME/skillwick/config.toml
$XDG_CACHE_HOME/skillwick/index-v3.sqlite
$XDG_STATE_HOME/skillwick/integration.json
$CODEX_HOME/SKILLWICK.md
$CODEX_HOME/AGENTS.md
```

`--json` is supported by `search`, `list`, `inspect`, and `doctor` and emits a
version-2 envelope. Search and inspection results have this shape:

```json
{"version":2,"results":[{"id":"name@abcdef","name":"name","description":"..."}]}
```

`list` adds a complete `total` and `doctor` adds its health and count fields.
Human search output bounds each record independently and marks a truncated
description so later candidates remain visible.

## Exit codes

- `0`: successful command, including a valid search with no matches.
- `1`: operational, configuration, or database failure.
- `2`: invalid command or option, unsupported output mode, invalid limit, or
  non-interactive setup without `--yes`.
- `3`: incomplete source, stale or unavailable selected skill, policy-denied
  target, project-scope mismatch, or strict doctor failure.
