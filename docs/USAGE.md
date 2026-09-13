# Usage and configuration

Skillwick is a deliberate local lookup tool. Search is always an explicit
command; a bare query is not interpreted as a search.

## Search and read

```sh
skillwick search "deploy an AgentCore MCP server with TypeScript"
skillwick search "SQLite full text ranking" --limit 3
skillwick read ID
skillwick inspect ID
skillwick inspect ID --files
```

Use an ID from search output. Search returns up to five compact candidates, and
choosing none is valid. Read selected instructions before following them.

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

`list` reports the current-scope inventory and its total. Filesystem discovery
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
$XDG_CACHE_HOME/skillwick/index-v2.sqlite
$XDG_STATE_HOME/skillwick/integration.json
$CODEX_HOME/SKILLWICK.md
$CODEX_HOME/AGENTS.md
$CODEX_HOME/config.toml
```

`--json` emits a versioned envelope for machine consumers. Run
`skillwick --help` for the complete command interface.

Exit codes: `0` success or no matches; `1` operational failure; `2` usage or
configuration error; `3` stale, disabled, conflicting, or incomplete state.
