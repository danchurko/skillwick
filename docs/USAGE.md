# Usage and configuration

Skillwick is a deliberate local lookup tool. Search is always an explicit
command; a bare query is not interpreted as a search. Discovery reads the
configured local snapshot and never executes a skill package.

## Search and read

```sh
skillwick search "deploy an AgentCore MCP server with TypeScript"
skillwick search "SQLite full text ranking" --limit 3
skillwick read ID
skillwick inspect ID
skillwick inspect ID --files
```

Use an ID from search or list output. Search returns up to five candidates by
default. `--limit N` accepts 1 through 20; choosing none is valid. Read
selected instructions before following them.

Text search output bounds each record to 2,000 bytes. A long description is
truncated with `[truncated]`; later requested candidates remain visible. JSON
search output has no presentation-byte limit and returns every selected result
up to the requested limit.

`read` prints the live `SKILL.md` with its package base directory. Resolve
relative references against that directory, not the shell working directory.

`inspect` prints indexed metadata. Add `--files` for a bounded live listing of
relative package paths and file types. The listing distinguishes Markdown from
other files, reports truncation, and does not follow symbolic links. File
counts distinguish regular files from directories and symlinks, including how
many files are additional to `SKILL.md`. File extensions describe contents;
they do not establish whether a file is safe to execute.

Inspection does not print reference contents or execute scripts. Read only the
references needed for the task. Skill content does not authorize execution,
installation, permission changes, or actions outside the user's request.

## Inventory and refresh

```sh
skillwick list
skillwick --json list
skillwick refresh
skillwick doctor --strict
```

`list` always reports every current-scope model-discoverable record and its
total; it has no pagination or compatibility flags. Filesystem discovery
covers `$HOME/.agents/skills` and applicable `.agents/skills` directories from
`--cwd` through its ancestors. Add authorized roots with repeatable `--root
PATH` during setup. Symlink escapes are rejected.

The local index and Codex inventory are durable snapshots of the configured
scope. With Codex inventory enabled, ordinary queries use the published native
snapshot without starting Codex. Run `skillwick refresh` after installed skills,
plugins, native enablement, or configured roots change. A missing or incomplete
snapshot is reported as a diagnostic rather than treated as an empty library;
`read` still rechecks the selected live file and its content hash.

## Codex setup

Preview and apply ordinary setup as separate steps:

```sh
skillwick init --dry-run --yes --agent codex --catalog native
skillwick init --yes --agent codex --catalog native
skillwick refresh
skillwick doctor --strict
```

For a filesystem-only setup, use `--agent none --inventory filesystem`.

An external configuration manager can own the agent reference and Codex setting
itself. Use `skillwick instructions` as the canonical content source, then run
`skillwick init --yes --agent none --inventory codex` to configure and refresh
the local snapshot without editing those managed files.

Setup writes the owned `$CODEX_HOME/SKILLWICK.md` and one absolute reference in
the selected AGENTS file. When the detected Codex version supports it, native
setup indexes the inventory before setting
`skills.include_instructions = false`. Existing installers retain package
ownership. `--dry-run` performs no persistent writes, and repeat setup preserves
modified or borrowed files rather than overwriting them.

`skillwick uninstall` removes only Skillwick's owned integration. Add
`--purge-cache` to remove the disposable index. Conditional rollback preserves
unrelated edits and reports drift; installed skills, third-party configuration,
and the binary remain in place.

## Paths and output

Skillwick honors `HOME`, `CODEX_HOME`, and XDG overrides:

```text
$XDG_CONFIG_HOME/skillwick/config.toml
$XDG_CACHE_HOME/skillwick/index-v3.sqlite
$XDG_STATE_HOME/skillwick/integration.json
$CODEX_HOME/SKILLWICK.md
$CODEX_HOME/AGENTS.md
$CODEX_HOME/config.toml
```

`--json` is supported by `search`, `list`, `inspect`, and `doctor`. It emits a
version-2 contract. Search and inspection results use this envelope:

```json
{"version":2,"results":[{"id":"name@abcdef","name":"name","description":"..."}]}
```

`list` adds `total` and returns the complete inventory:

```json
{"version":2,"total":1,"results":[...]}
```

`inspect ID --files` keeps the result envelope and adds the bounded `package`
object. `doctor --json` returns its version-2 diagnostic object. Other commands
print text and reject `--json` with exit code 2 rather than silently ignoring
the option. Run `skillwick --help` and each subcommand's `--help` for the
complete interface, defaults, ranges, output, side effects, and failures.

Exit codes:

- `0`: successful command, including a valid search with no matches.
- `1`: operational, cache, configuration, or local database failure.
- `2`: unknown command or option, invalid argument, unsupported output mode,
  or non-interactive setup without `--yes`.
- `3`: selected skill missing, stale, disabled, unavailable, or blocked by
  native/provider state; the message identifies the failed boundary. Some
  executable or database failures use code `1` and retain their specific text.

Refresh may publish a new disposable snapshot and can call the configured
native provider. Ordinary covered searches, list, read, and inspect operations
use the cache and revalidate only the selected source for read/inspect.
