# Getting started

This tree prepares Skillwick 0.4.0. Build it from source until that version is
published; an existing Homebrew installation may still contain an older release.

```sh
cargo install --locked --path . --root "$HOME/.local"
"$HOME/.local/bin/skillwick" --version
```

Published macOS releases use Homebrew:

```sh
brew tap danchurko/skillwick https://github.com/danchurko/skillwick.git
brew install skillwick
```

The release shell installer supports macOS and Linux, defaults to its packaged
release version, and accepts `--version VERSION --prefix PATH` for a pinned
installation. It verifies checksums and executable version before replacement.
It does not edit your shell startup files. Ensure the installed bin directory is
on PATH. See [compatibility](COMPATIBILITY.md) for verification boundaries.

## Discover and read

A fresh executable discovers supported shared sources without writing agent
instructions. Shared sources include `~/.agents/skills`, `$CODEX_HOME/skills`,
and `${CLAUDE_CONFIG_DIR:-~/.claude}/skills`, plus eligible installed plugins.

```sh
skillwick search "SQLite full text ranking"
skillwick read ID
skillwick read astra-orchestrator codebase-memory caveman
skillwick --json list
```

Codex plugin discovery uses a bounded installed-plugin query. Claude uses its
installation registry and enablement settings. Staging directories, inactive
versions, and marketplace checkouts are not skill installations. Discovery does
not execute packages or activate all instructions in the agent context.

## Setup instructions and project sources

Preview and configure one or both supported agents:

```sh
skillwick init --dry-run --yes --agent codex --agent claude
skillwick init --yes --agent codex --agent claude
```

Register project sources at the workspace boundary:

```sh
skillwick --cwd "$PWD" init --yes --agent none --project
```

This includes that project's `.agents/skills`, `.codex/skills`, and
`.claude/skills`; it does not include sibling projects or arbitrary ancestors.
Custom roots remain available for any installer:

```sh
skillwick init --yes --agent none --root "$HOME/my-skills"
```

For a host-independent explicit-only setup:

```sh
skillwick init --yes --agent none --discovery explicit --root "$HOME/my-skills"
```

A managed owner consumes `skillwick instructions` and uses `--agent none` so that
only one owner writes agent instructions. Existing installers retain skills and
updates. Repeated setup is idempotent and refuses unmanaged file collisions.

## Verify coverage

```sh
skillwick doctor --strict --require codebase-memory
skillwick read --raw codebase-memory
skillwick --json read codebase-memory
```

A valid search no-match succeeds. Missing required skills fail, so `&&` correctly
stops a dependent command chain. Every lookup checks current sources; no separate
refresh is needed after ordinary package changes. Failed complete-source checks
retain the previous cache but do not return stale results.

Old configuration requires explicit backup and re-setup. Read [operations](OPERATIONS.md)
for ownership, recovery, and uninstall, and [reference](REFERENCE.md) for the full
command/output contract.
