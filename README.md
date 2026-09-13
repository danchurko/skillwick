# Skillwick

[![CI](https://github.com/danchurko/skillwick/actions/workflows/ci.yml/badge.svg)](https://github.com/danchurko/skillwick/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/danchurko/skillwick?display_name=tag)](https://github.com/danchurko/skillwick/releases)

**Find the skill. Load only what matters.**

Skillwick helps coding agents find relevant skills in large installed libraries
without carrying the entire skill catalogue in context. Describe the task, get
up to five candidates, and load the instructions you need.

```sh
skillwick "deploy an AgentCore MCP server with TypeScript"
skillwick read ID   # use an ID returned by the search
```

It runs locally as one Rust binary, searches with SQLite FTS5, and leaves skill
packages with their existing installers. No daemon, registry, or telemetry.

**Supported:** Codex CLI 0.154.0 on macOS. See [compatibility](docs/COMPATIBILITY.md)
for tested versions and platform limits.

## Less discovery context

On a 555-skill inventory, Skillwick used an estimated **658 discovery tokens
per task**, compared with **13,303 tokens** for the complete native catalogue:
about **95% less discovery text** across 65 single-search tasks.

The estimate includes Skillwick's instructions, query, and returned candidates.
Codex rendered 530 of the 555 indexed skills in that catalogue. Against a
smaller, configured catalogue containing 172 entries, the reduction was about
84%. These are tokenizer estimates, not total workflow or billing savings.
They exclude selected skill bodies and model reasoning; delegation has a
separate comparison below.
See [method, limitations, and saved results](docs/BENCHMARKS.md).

## Retrieval quality

The recorded lexical benchmark evaluated 65 tasks against a 418-skill Codex
inventory, including native and plugin skills:

| Metric | Result |
| --- | ---: |
| Recall@5 | 0.869 |
| MRR@5 | 0.812 |
| nDCG@5 | 0.826 |
| No-match accuracy | 0.600 |
| Warm query p95 | 0.380 ms |

This is a dated evaluation corpus, not a universal accuracy claim. Warm latency
excludes process startup and agent reasoning. The dataset, misses, and comparison
rules are documented in [benchmarks](docs/BENCHMARKS.md).

## Install

With Homebrew:

```sh
brew tap danchurko/skillwick https://github.com/danchurko/skillwick.git
brew install skillwick
```

Or install a specific version with a verified checksum:

```sh
curl -fsSLO https://raw.githubusercontent.com/danchurko/skillwick/v0.1.4/scripts/install.sh
sh install.sh --version 0.1.4 --prefix "$HOME/.local"
```

The installer installs only the executable. Release binaries are unsigned and
not notarized. To try unreleased changes, [build from source](CONTRIBUTING.md).

## Connect Codex

Preview the integration, then apply it:

```sh
skillwick init --dry-run --yes --agent codex --catalog native --hooks off
skillwick init --yes --agent codex --catalog native --hooks off
skillwick doctor --strict
```

Skillwick indexes Codex's native inventory before hiding its automatic catalogue.
It adds one short instruction file and preserves installed skills, plugin hooks,
and unrelated configuration. Optional prompt suggestions are available through
`--hooks suggest`.

The source version's instructions support bounded delegated discovery when your
agent already uses sub-agents. It also adds `skillwick inspect ID --files` to
list package references, scripts, and assets without loading their contents.
These additions are not in v0.1.4.

See [usage and configuration](docs/USAGE.md) for package boundaries, refresh,
JSON output, and reversible uninstall.

## Evaluate your own library

Use the [evaluation guide](docs/BENCHMARKS.md) and its
[copyable coding-agent prompt](docs/prompts/evaluate-skills.md) to prepare a
dataset covering 30% of your distinct skills by default. Coverage is adjustable;
the full library stays searchable.

The comparison matrix separates retrieval backends from direct and delegated
discovery. Semantic retrieval and reranking remain unimplemented and unmeasured.

A 12-task pilot reduced root input by 9.5% through delegation, but used 74% more
total input and took 2.26× the model-call time. Both workflows found every
labelled relevant skill, with additional unlabelled selections. See the
[pilot results and limits](docs/BENCHMARKS.md#direct-versus-delegated-pilot).
That historical run did not verify isolation from local Codex customizations.

## Contribute

Start with [CONTRIBUTING.md](CONTRIBUTING.md). The [documentation map](docs/README.md)
links user guides, evaluation evidence, architecture, and research.

Licensed under either Apache License 2.0 or MIT, at your option.
