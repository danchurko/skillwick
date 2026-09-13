# Skillwick

[![CI](https://github.com/churdaa/skillwick/actions/workflows/ci.yml/badge.svg)](https://github.com/churdaa/skillwick/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/churdaa/skillwick?display_name=tag)](https://github.com/churdaa/skillwick/releases)

**Find the skill. Load only what matters.**

Skillwick helps coding agents find relevant skills in large installed libraries
without loading the entire catalogue. Search deliberately, choose a candidate,
and read the instructions you need.

```sh
skillwick search "deploy an AgentCore MCP server with TypeScript"
skillwick read ID   # use an ID returned by search
```

It runs locally as one Rust binary, searches with SQLite FTS5, and leaves skill
packages with their existing installers. No daemon, registry, or telemetry.

Search is an explicit command. A bare query is not interpreted as a search;
this keeps normal shell composition and command errors predictable.

Search returns five candidates by default and accepts `--limit` values from 1
through 20. Text output bounds each record and marks long descriptions with
`[truncated]`; `--json` emits complete version-2 result envelopes.

**Supported:** Codex CLI 0.154.0 on macOS. See [compatibility](docs/COMPATIBILITY.md)
for tested versions and platform limits.

## Install

With Homebrew:

```sh
brew tap churdaa/skillwick https://github.com/churdaa/skillwick.git
brew install skillwick
```

Or install a specific version with a verified checksum:

```sh
curl -fsSLO https://raw.githubusercontent.com/churdaa/skillwick/v0.1.5/scripts/install.sh
sh install.sh --version 0.1.5 --prefix "$HOME/.local"
```

The installer installs only the executable. Release binaries are unsigned and
not notarized. To try unreleased changes, [build from source](CONTRIBUTING.md).

## Connect Codex

Preview the integration, then apply it:

```sh
skillwick init --dry-run --yes --agent codex --catalog native
skillwick init --yes --agent codex --catalog native
skillwick refresh
skillwick doctor --strict
```

Skillwick indexes Codex's native inventory before hiding its automatic catalogue
when the installed version supports that contract. It adds one short
instruction file and preserves installed skills and unrelated configuration.
Run `skillwick refresh` after installed skills, plugins, or native enablement
change; the local index and native inventory are snapshots, not live state.

`skillwick inspect ID --files` lists package references, scripts, and assets
without loading their contents. See [usage and configuration](docs/USAGE.md)
for the complete command and ownership details.

Configuration managers can obtain the canonical agent guidance with
`skillwick instructions` and keep ownership of its destination and Codex
settings.

Comparative retrieval quality, context savings, and task-success benefits have
not been established for the supported product and workflow.

## Contribute

Start with [CONTRIBUTING.md](CONTRIBUTING.md). The [documentation map](docs/README.md)
links user guides, architecture, compatibility, and implementation history.

Licensed under either Apache License 2.0 or MIT, at your option.
