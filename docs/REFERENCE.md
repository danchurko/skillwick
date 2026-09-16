# Command and JSON reference

`skillwick --help` and subcommand help define option spelling. No arguments print
help successfully. Unknown commands and invalid argument combinations exit 2.

## Global options

`--json`, `--cwd PATH`, and `--config PATH` work before or after a subcommand.
`--help` and `--version` print executable information. Search, list, inspect,
read, and doctor support JSON. Other commands reject `--json` with exit 2.

## Search and list

```text
skillwick search <QUERY>... [--limit N]
skillwick list
```

Query arguments join with spaces; use `--` before a query starting with a hyphen.
Search returns five groups by default; limits range from 1 through 20. Exact
names, token coverage, weighted FTS5 BM25, and deterministic ties establish rank.
Grouping precedes result limits. A valid query with no matches exits 0.
List returns every applicable model-discoverable group and a complete total.
Human search descriptions are bounded per result and marked when truncated.

## Read and inspect

```text
skillwick read <ID|NAME>... [--raw]
skillwick --json read <ID|NAME>...
skillwick inspect ID [--files]
```

Read resolves exact IDs first, then exact case-sensitive names. Verified copies
count as one name match. Different packages with the same name fail closed with
candidate IDs and provenance. Member IDs select their particular installed copy.
All batch targets are resolved and live-validated before output. A missing,
denied, ambiguous, changed, or inaccessible target exits 3 with no partial body.

Default reads include resolved identity, path, base, and instruction content.
`--raw` emits exactly one UTF-8 instruction body without a prefix; it conflicts
with JSON and multiple targets. JSON read results include `content` and metadata.
Live validation covers path identity, the 1 MiB instruction bound, UTF-8, and hash.

Inspect returns metadata and origins. `--files` adds a bounded package listing
without following symlink entries or executing files. Fingerprint grouping can
read supporting files to establish package equality; inspection output never
contains their bodies. Incomplete grouping is reported and copies remain separate.

## Setup

```text
skillwick init [--yes] [--dry-run] [--agent codex|claude|none]...
    [--discovery auto|explicit] [--project]
    [--root PATH]... [--project-root PATH]... [--instructions-file PATH]
```

Default discovery uses supported global sources. Global explicit mode disables
provider adapters and uses configured roots. Project discovery controls its standard
skill folders; enabled global providers still honor registered project policy. Supplying `--root` selects explicit shared discovery unless paired with
`--discovery auto`. `--root` adds a shared root. `--project-root` associates a root with normalized
`--cwd`; `--project` registers standard project sources there. Project applicability
includes descendants only. Registration is stored in central configuration.

Noninteractive setup requires `--yes` and explicit integration targets. Repeat
`--agent` for Codex and Claude; `none` cannot be combined with another target.
Interactive setup offers detected hosts. A custom instructions file must identify
one target. Dry-run validates and displays proposed paths without persistent writes.

## Health and maintenance

```text
skillwick doctor [--strict] [--require NAME]...
skillwick refresh
skillwick instructions
skillwick uninstall [--purge-cache]
skillwick completions bash|zsh|fish
```

Doctor reconciles sources and reports resolved sources, diagnostics, required names,
package/group counts, and integration health. `--strict` or a failed `--require`
returns exit 3. Configuration errors return exit 1. Counts distinguish canonical
filesystem records, root associations, visible package records, groups, and copies.
Refresh uses the same complete reconciliation path as lookup. Instructions prints
the canonical agent context. Completions prints shell definitions without editing
shell files. Uninstall removes verified owned integration; purge also removes cache.

## JSON version 3

Search, list, inspect, and read use `{"version":3,"results":[...]}`. List adds
`total` (groups). Results preserve original strings and paths through JSON escaping
and include `origins` with member IDs, paths, source, scope, and plugin identity.
Read adds `content`. Inspect with files adds `package`, its counts and limits.
Doctor returns `healthy`, `config`, `cache`, resolved `sources`, `diagnostics`,
`integration_present`, `counts`, and `required`.

Successful machine results go to stdout; errors and scan warnings go to stderr.
Human metadata escapes controls. Instruction bodies and JSON values are not
silently sanitized. UTF-8 paths support spaces, Unicode, tabs, and newlines;
non-UTF-8 paths fail explicitly. Early-closing output pipes are successful.

## Paths and exit codes

```text
$XDG_CONFIG_HOME/skillwick/config.toml
$XDG_CACHE_HOME/skillwick/index-v4.sqlite
$XDG_STATE_HOME/skillwick/integration.json
```

Without XDG overrides these resolve under `~/.config`, `~/.cache`, and
`~/.local/state`. Configuration schema is `version = 1`. Obsolete configurations
must be backed up and explicitly recreated; they are not silently converted.

| Code | Meaning |
| --- | --- |
| 0 | Success or valid search no-match |
| 1 | Operational, configuration, or database failure |
| 2 | Invalid invocation, unsupported format, or noninteractive setup without required choices |
| 3 | Incomplete source, unavailable/ambiguous/denied selection, or failed health requirement |
