# Skillwick

[![CI](https://github.com/danchurko/skillwick/actions/workflows/ci.yml/badge.svg)](https://github.com/danchurko/skillwick/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/danchurko/skillwick?display_name=tag)](https://github.com/danchurko/skillwick/releases)

Find the skill. Load only what matters.

> [!IMPORTANT]
> Skillwick v1 supports Codex CLI 0.154.0 on macOS only. Claude Code,
> OpenCode, Cursor, other coding agents, Linux, and Windows are not supported.
> See [compatibility](docs/COMPATIBILITY.md) for exact test coverage.

Skillwick is a native CLI for ranked lexical search over installed Agent Skills.
It indexes bounded `SKILL.md` metadata in SQLite FTS5, returns at most five
compact candidates, and reads selected instructions from their live package
directory. Existing installers keep ownership of every skill package.

Skillwick does not install, relocate, execute, or rewrite third-party skills.
V1 has no MCP server, Codex fork, daemon, embeddings, remote registry, or
telemetry.

## Measured on a real skill library

The checked-in benchmark uses 65 natural-language tasks against 418 enabled
skills from the maintainer's live Codex inventory. The corpus includes 327 ECC
skills, 15 other plugin skills, 76 native/user skills, and five prompts that
should return no skill.

```text
Recall@5             0.869
MRR@5                0.812
nDCG@5               0.826
No-match accuracy    0.600
Warm query p95       0.380 ms
```

These are measured lexical results, including ten published misses. This is not
a synthetic perfection claim. Dataset and corpus hashes make later embedding or
reranker comparisons detect drift. Run the same evaluation with `make
benchmark`; use `--json` for machine-readable results. See [benchmark method and
full evidence](docs/BENCHMARKS.md).

## Install

Build from source with stable Rust:

```sh
git clone https://github.com/danchurko/skillwick.git
cd skillwick
cargo build --release --locked
./target/release/skillwick --version
```

After a GitHub release exists, use Homebrew:

```sh
brew tap danchurko/skillwick https://github.com/danchurko/skillwick.git
brew install skillwick
```

Or install an explicit version and prefix:

```sh
curl -fsSLO https://raw.githubusercontent.com/danchurko/skillwick/v0.1.0/scripts/install.sh
sh install.sh --version 0.1.0 --prefix "$HOME/.local"
```

The installer verifies the release checksum and installs only the executable.
It does not edit your home directory or Codex configuration. Release binaries
are unsigned and not notarized.

## Use

Search works before integration:

```sh
skillwick "deploy an AgentCore MCP server with TypeScript"
skillwick search "SQLite full text ranking" --limit 3
skillwick read aws-agentcore@7d92ac
skillwick inspect aws-agentcore@7d92ac
skillwick list --limit 20
skillwick refresh
```

Default discovery covers `$HOME/.agents/skills` and applicable
`.agents/skills` directories from `--cwd` through its ancestors. Add other
authorized roots with repeatable `--root PATH` during setup. Symlink escapes
are rejected.

## Connect Codex

Review the plan without writing files:

```sh
skillwick init --dry-run --yes --agent codex --catalog native --hooks off
```

Apply setup when ready:

```sh
skillwick init --yes --agent codex --catalog native --hooks suggest
skillwick doctor --strict
```

Setup installs Skillwick's router skill, adds one marked global instruction
block, indexes native inventory, then sets `skills.include_instructions = false`.
Catalogue suppression occurs only after replacement discovery succeeds.

`--hooks suggest` adds one bounded, cached-only `UserPromptSubmit` handler to
Codex's existing `hooks.json`. Codex still runs every caveman, ponytail, plugin,
project, and managed hook through its native lifecycle. Skillwick does not
disable hooks or bypass Codex trust review. Use `/hooks` to review the new
handler. Keep `--hooks off` when automatic suggestions are not wanted.

`skillwick uninstall` removes only owned integration. `--purge-cache` also
removes the disposable index. Conditional rollback preserves unrelated edits
and reports drift.

Skillwick honors `HOME`, `CODEX_HOME`, and XDG overrides:

```text
$XDG_CONFIG_HOME/skillwick/config.toml
$XDG_CACHE_HOME/skillwick/index.sqlite
$XDG_STATE_HOME/skillwick/integration.json
$HOME/.agents/skills/skillwick/SKILL.md
$CODEX_HOME/AGENTS.md
$CODEX_HOME/config.toml
```

## Commands

Run `skillwick --help` for the complete interface. Key behaviors:

- Default search returns zero to five compact results.
- `--json` emits a versioned machine-readable envelope.
- `skillwick -- init hooks` searches those literal words.
- Exit codes: `0` success/no matches, `1` operational failure, `2` usage or
  configuration error, `3` stale, disabled, conflicting, or incomplete state.

## Develop

```sh
make check
make benchmark        # requires the labelled skills in current Codex inventory
make test-codex       # requires Codex CLI 0.154.0
make test-inference   # opt-in real gpt-5.6-luna turn by default
```

All setup and installer tests use temporary homes. The inference test uses your
existing Codex authentication but only a temporary fixture skill; it does not
install Skillwick or change Codex configuration.

Read [CONTRIBUTING.md](CONTRIBUTING.md), [architecture](docs/ARCHITECTURE.md),
[design contract](docs/SPEC.md), and [verification evidence](docs/IMPLEMENTATION.md)
before changing lifecycle or integration behavior.

## License

Licensed under either Apache License 2.0 or MIT, at your option.
