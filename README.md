# Skillwick

[![CI](https://github.com/danchurko/skillwick/actions/workflows/ci.yml/badge.svg)](https://github.com/danchurko/skillwick/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/danchurko/skillwick?display_name=tag)](https://github.com/danchurko/skillwick/releases)

**Find the skill. Load only what matters.**

Skillwick helps coding agents find relevant installed skills without loading an
entire catalogue. Configure the folders that contain your skills, search
deliberately, choose a candidate, and read its instructions.

It runs locally as one Rust binary, uses SQLite FTS5 for its derived index, and
leaves skill packages with their existing installers. No daemon, registry,
telemetry, agent server, or package execution.

Search is an explicit command. A bare query is not interpreted as a search;
this keeps normal shell composition and command errors predictable.

## Install

With Homebrew:

```sh
brew tap danchurko/skillwick https://github.com/danchurko/skillwick.git
brew install skillwick
```

Or install a specific version with a verified checksum:

```sh
curl -fsSLO https://raw.githubusercontent.com/danchurko/skillwick/v0.2.2/scripts/install.sh
sh install.sh --version 0.2.2 --prefix "$HOME/.local"
```

The installer installs only the executable. Release binaries are unsigned and
not notarized. To try unreleased changes, [build from source](CONTRIBUTING.md).

## Configure roots

Register shared skill roots explicitly. They apply in every project:

```sh
skillwick init --yes --root "$HOME/.agents/skills"
```

Register a project root with the working directory used for that project. It
applies in that directory and its descendants:

```sh
skillwick --cwd "$PWD" init --yes --root "$HOME/.agents/skills" \
  --project-root "$PWD/.agents/skills"
```

Unregistered home and ancestor directories are not searched. Installing a
package inside a registered root does not require registering that package.

## Search and read

```sh
skillwick search "deploy an AgentCore MCP server with TypeScript"
skillwick read ID
skillwick read astra-orchestrator
```

Each lookup reconciles the applicable roots automatically. Additions, edits,
removals, renames, policy changes, and root-configuration changes appear on the
next command. An unchanged inventory reuses SQLite. `refresh` remains available
as an explicit maintenance operation.

Search returns five candidates by default and accepts `--limit` values from 1
through 20. Text output bounds each record and marks long descriptions with
`[truncated]`; `--json` emits complete version-2 result envelopes.

`read` accepts exact IDs and unambiguous exact names. It validates the selected
live file and prints its package base so supporting references can be resolved.
`inspect ID --files` lists package references, scripts, and assets without
loading their contents. Neither command executes package files.

Use [usage and configuration](docs/USAGE.md) for workflows, [operations](docs/OPERATIONS.md)
for recovery and ownership, and the [command reference](docs/REFERENCE.md) for
stable scripting details. The [compatibility page](docs/COMPATIBILITY.md) lists
tested release boundaries.

Comparative retrieval quality, context savings, and task-success benefits have
not been established for the supported product and workflow.

## Contribute

Start with [CONTRIBUTING.md](CONTRIBUTING.md). See the [changelog](CHANGELOG.md)
for released changes; the [documentation map](docs/README.md) links user guides,
architecture, compatibility, and implementation history.

Licensed under either Apache License 2.0 or MIT, at your option.
