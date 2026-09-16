# Skillwick

[![CI](https://github.com/danchurko/skillwick/actions/workflows/ci.yml/badge.svg)](https://github.com/danchurko/skillwick/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/danchurko/skillwick?display_name=tag)](https://github.com/danchurko/skillwick/releases)

**Find the skill. Load only what matters.**

Skillwick finds installed skills and reads selected instructions. It discovers
shared, Codex, and Claude skill sources, groups verified package copies, and
keeps project skills within registered workspace boundaries. Existing installers
retain package ownership. No daemon, prompt hook, telemetry, or package execution.

This tree prepares **0.4.0**. Build unreleased changes from source:

```sh
cargo install --locked --path . --root "$HOME/.local"
"$HOME/.local/bin/skillwick" --version
```

Published macOS releases are also available through Homebrew:

```sh
brew tap danchurko/skillwick https://github.com/danchurko/skillwick.git
brew install skillwick
```

The release shell installer supports macOS and Linux, defaults to its packaged
release version, and accepts an explicit version and prefix. It verifies
checksums and executable version. Release artifacts are unsigned; see
[compatibility](docs/COMPATIBILITY.md) for tested boundaries.

## Use

```sh
skillwick search "deploy an AgentCore MCP server with TypeScript"
skillwick read ID
skillwick read astra-orchestrator codebase-memory caveman
skillwick --json list
skillwick read --raw caveman
```

Search returns up to five distinct groups by default (`--limit 1` through `20`).
No match is successful. Exact reads fail on missing or ambiguous names; batch
reads validate every target before printing any instructions. Shell chaining,
pipes, and redirection retain ordinary exit semantics. JSON version 3 preserves
exact values and paths, including read content and all grouped origins.

Every lookup reconciles applicable sources. Codex plugin discovery uses its
bounded installed-plugin CLI listing; shared and explicit roots work without a
host executable. Invalid required sources fail and retain the prior cache without
silently returning stale results. Production retrieval remains local SQLite FTS5.

## Setup

```sh
skillwick init --dry-run --yes --agent codex --agent claude
skillwick init --yes --agent codex --agent claude
skillwick --cwd "$PWD" init --yes --agent none --project
skillwick doctor --strict --require codebase-memory
```

Supported global directories are discovered automatically. `--project` registers
standard project folders for that workspace and descendants. `--root PATH` adds a
custom shared source; `--discovery explicit` restricts discovery to configured roots.
Managed owners consume `skillwick instructions` and use `--agent none`.

See [getting started](docs/GETTING_STARTED.md), [command reference](docs/REFERENCE.md),
[operations](docs/OPERATIONS.md), and [architecture](docs/ARCHITECTURE.md).
Historical semantic measurements are [research evidence](docs/RESEARCH.md), not
proof of current-release quality or task success.

Start contributing with [CONTRIBUTING.md](CONTRIBUTING.md). See the
[changelog](CHANGELOG.md) and [documentation map](docs/README.md).

Licensed under either Apache License 2.0 or MIT, at your option.
