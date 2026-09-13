# Usage and configuration

This guide describes the current source version. Package file inspection and
delegated discovery instructions are unreleased additions after v0.1.4.

## Search and load

```sh
skillwick "deploy an AgentCore MCP server with TypeScript"
skillwick search "SQLite full text ranking" --limit 3
skillwick read ID
skillwick inspect ID
skillwick inspect ID --files
```

Use an ID from search output. Search returns zero to five compact candidates.
Selecting no skill is valid even when candidates are returned. Read selected
instructions before following them.

When your normal agent workflow supports delegation, assign one discovery pass
with up to three perspectives: the desired outcome, technical mechanism, and
constraints. Ask for at most five deduplicated IDs, descriptions, and short
relevance reasons. Give the researcher enough task context to distinguish nearby
skills. The root agent reads the selected instructions; the researcher need not
copy full skill bodies into its response.

## Package contents

`read` prints the live `SKILL.md` with its package base directory. Resolve
relative references against that directory, not the shell working directory.

`inspect` prints indexed metadata. Add `--files` for a bounded live listing of
relative package paths and file types. The listing distinguishes Markdown from
other files, reports truncation, and does not follow symbolic links. File
counts distinguish regular files from directories and symlinks, including how
many files are additional to `SKILL.md`. Truncated counts cover only the shown
subset. File
extensions describe contents; they do not establish whether a file is safe to
execute. Selected skills are checked for enablement, path changes, and content
changes before their packages are inspected.

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

`list` reports the current-scope inventory and its total. With Codex inventory
enabled, queries use the published native snapshot without starting Codex.
Refresh after installed skills or native enablement change. An empty native
cache asks you to refresh; `read` still checks the selected live file's hash.

Filesystem discovery covers `$HOME/.agents/skills` and applicable
`.agents/skills` directories from `--cwd` through its ancestors. Add authorized
roots with repeatable `--root PATH` during setup. Symlink escapes are rejected.

## Codex integration

```sh
skillwick init --dry-run --yes --agent codex --catalog native --hooks off
skillwick init --yes --agent codex --catalog native --hooks off
skillwick doctor --strict
```

Setup writes an owned `$CODEX_HOME/SKILLWICK.md` and one absolute reference in
the selected AGENTS file. It indexes native inventory before setting
`skills.include_instructions = false`. Existing installers retain package ownership.

For optional prompt suggestions, use `--hooks suggest`. This adds one bounded,
cached-only `UserPromptSubmit` handler to the existing `hooks.json`. Codex
continues to run its other hooks and enforce native trust review. Use `/hooks`
to review the handler. `--hooks off` removes only Skillwick's suggestion hook.

`skillwick uninstall` removes owned integration. Add `--purge-cache` to remove
the disposable index. Conditional rollback preserves unrelated edits and reports
drift. Re-running `init` updates an unchanged owned instruction file; modified
or borrowed files retain their ownership protections.

## Paths and output

Skillwick honors `HOME`, `CODEX_HOME`, and XDG overrides:

```text
$XDG_CONFIG_HOME/skillwick/config.toml
$XDG_CACHE_HOME/skillwick/index.sqlite
$XDG_STATE_HOME/skillwick/integration.json
$CODEX_HOME/SKILLWICK.md
$CODEX_HOME/AGENTS.md
$CODEX_HOME/config.toml
```

`--json` emits a versioned envelope. Use `skillwick -- init hooks` to search
literal words that would otherwise be interpreted as commands.

Exit codes: `0` success or no matches; `1` operational failure; `2` usage or
configuration error; `3` stale, disabled, conflicting, or incomplete state.
Run `skillwick --help` for the complete command interface.
