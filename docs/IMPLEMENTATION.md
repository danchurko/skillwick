# Verification history

This is a dated implementation record, beginning with v0.1.0 and followed by
later evidence below. Version numbers, test counts, toolchains, and release
incidents describe their respective runs, not current project status. Use the
[usage guide](USAGE.md), [compatibility](COMPATIBILITY.md), and
[evaluation guide](BENCHMARKS.md) for the current supported surface.

The original v0.1.0 verification used synthetic skill packages and temporary
homes. No setup test changed live Codex configuration.

## Execution plan

1. Prove the lexical vertical slice: scoped source discovery, bounded metadata
   parsing, one SQLite/FTS5 index, deterministic search, and live `read`.
2. Add lifecycle safety: refresh semantics, short-lived Codex inventory,
   compatibility checks, reversible setup, diagnostics, and uninstall.
3. Prepare native macOS release artifacts, packaging, installer fixtures, lexical
   evaluation, and an evidence-backed release-candidate report.

## Fixed decisions

- Installed packages remain owned by their existing installers. Skillwick stores
  derived metadata only.
- Search stays lexical and local. No MCP server, Codex fork, daemon, embeddings,
  remote registry, package installer, or instruction execution exists in v1.
- Codex catalogue suppression is enabled only for explicitly supported versions,
  after the replacement instruction and an indexable source are verified.
- Setup tests use isolated `HOME`, `CODEX_HOME`, XDG directories, and fixture
  executables. No test targets live workstation configuration.
- Integration edits use marked ownership plus conditional rollback. Uninstall
  preserves installed and third-party skills.

## Implemented surface

- Scoped filesystem discovery for global, project, ancestor, and explicit roots;
  canonical deduplication; authorized-root symlink checks; cycle detection.
- Bounded Agent Skills frontmatter parsing with BOM, CRLF, Unicode, folded YAML,
  duplicate-key, size-limit, invalid-YAML, and degraded-description handling.
- One bundled SQLite database with transactional metadata/FTS5 updates, bounded
  lock waits, safe incomplete-scan behavior, technical-token aliases, weighted
  BM25, exact-name and term-coverage ranking, and deterministic ties.
- Search, `read`, `inspect`, `list`, `refresh`, `benchmark`, `init`, `doctor`,
  `uninstall`, JSON output, zsh completions, and a cached-only suggestion hook.
- Short-lived Codex `skills/list` inventory with bounded newline-delimited JSON,
  interleaved-notification handling, request IDs, stderr capture, timeout, EOF,
  and child termination. Native enabled state suppresses filesystem aliases.
- Reversible SKILLWICK.md context/reference and TOML integration with atomic
  writes, restrictive permissions, an ownership journal, override/collision
  checks, and conditional leaf rollback.
- cargo-dist configuration and generated GitHub release workflow, native Mac
  archives/checksums, a Homebrew formula template, and an explicit version/prefix
  installer that does not edit a home directory.

## Verification log

Host: Apple Silicon Mac, macOS 26.6.2. Temporary toolchain: Rust 1.98.1 in
`/private/tmp`. Installed Codex CLI: 0.154.0.

The locked dependency graph also passed `cargo +1.88.0 check --all-targets
--locked --offline`. Rust 1.85.0 correctly failed because current locked
dependencies require 1.88, so `package.rust-version` now states 1.88.

Core checks:

```text
cargo fmt --check
cargo clippy --all-targets --locked --offline -- -D warnings
cargo test --all-targets --locked --offline
```

Result: 13 unit tests and the 40-case lexical evaluation passed; one release
performance test is intentionally ignored by the normal suite. CLI acceptance,
Codex integration, and distribution smoke scripts passed separately.

The labelled evaluation contained 35 tasks with one or more relevant skills and
five no-skill-needed tasks. Result: Recall@5 1.000, zero irrelevant suggestions
on the no-skill set, and 2,779 aggregate result bytes. This small synthetic set
is transparent test evidence, not a general quality claim.

Release performance test, optimized build, in-memory bundled SQLite, 200 warm
queries per size:

```text
records=1000  refresh_ms=16.75  warm_query_p95_ms=1.72
records=10000 refresh_ms=128.37 warm_query_p95_ms=16.99
```

One hundred optimized arm64 `--version` starts took 0.19 seconds wall time,
about 1.9 ms per process on this host. This is an aggregate startup measure,
not a cold-start percentile.

`tests/cli_acceptance.sh` proved temporary-home setup, repeatability, global and
project scope, no sibling leakage, paths with spaces, C++/C#/.NET/Node.js query
handling, JSON, relative-base `read`, reserved-command escape, output bound,
exit codes, deletion, dry-run, and clean broken-pipe behavior.

`tests/codex_integration.sh` used the real 0.154.0 app server with a temporary
`HOME`, `CODEX_HOME`, and XDG tree. It proved the requested working-directory
scope, one native-precedence result, strict doctor health, inventory before
catalogue suppression, no managed writes after an inventory failure, the owned
context/reference config changes, prompt-input catalogue suppression, context
visibility, uninstall, and preservation of later unrelated edits. Prompt-input
size was 11,421 bytes with the catalogue hidden and 14,299 bytes with native
catalogue injection enabled. The hidden request had
one SKILLWICK.md reference, one user prompt, and no skills-catalogue marker.

Pinned cargo-dist 0.28.0 generated the GitHub workflow. `dist plan`
selected only `aarch64-apple-darwin` and `x86_64-apple-darwin`. `dist build`
created both archives and SHA-256 files. Both binaries were identified as the
expected Mach-O architecture; the Intel binary also ran through Rosetta on this
host. `otool -L` showed only system libraries, confirming bundled SQLite. Build
load commands target macOS 11.0 for arm64 and 10.12 for x86_64; runtime execution
was tested only on macOS 26.6.2. Installer and archive checksum smoke tests
passed. Ruby formula syntax and shell syntax checks passed.

GitHub Actions CI run
[`34687019561`](https://github.com/danchurko/skillwick/actions/runs/34687019561)
passed `make check` on commit `52c581d8b9d73e43cb02e39270d5dd61b2b273d7`.
The final main-branch run
[`34687599560`](https://github.com/danchurko/skillwick/actions/runs/34687599560)
also passed after the release workflow runner update.

The public
[`v0.1.0` release](https://github.com/danchurko/skillwick/releases/tag/v0.1.0)
contains arm64 and x86_64 archives plus separate SHA-256 files. GitHub reports
archive digests `86a24cd4d6653f752a44f00c5b4ff98081786404f372af539e3f1f3eec591506`
and `dcc272ae3d486dc6e5d6ec0d18903095ab5f09879f6271e2fd2e25c747e9196b`,
matching the formula and local build evidence. Fetching the installer from the
immutable tag into an isolated temporary home downloaded the public arm64
archive, verified its checksum, installed into a temporary prefix, and returned
`skillwick 0.1.0`.

## Compatibility and limits

- Codex 0.154.0 accepted `skills.include_instructions = false`; its real
  `skills/list` RPC returned current system/user inventory in under the adapter's
  eight-second bound. Other versions default to filesystem discovery only.
- `tests/inference_smoke.sh` ran a real `gpt-5.6-luna` turn. The model searched
  a temporary fixture library, copied the returned ID, read its live
  `SKILL.md`, found the expected marker, and returned
  `SKILLWICK_INFERENCE_OK`. Skillwick was not installed or integrated into the
  live Codex configuration.
- Signing, notarization, and execution on an actual Intel Mac or older macOS
  release remain unverified.
- The first tag-triggered cargo-dist run could not start because its generated
  workflow selected retired `ubuntu-20.04` runners. The run was cancelled and
  main now selects `ubuntu-24.04`; the v0.1.0 assets were published from the
  exact locally verified cargo-dist archives. A later tag must prove the fixed
  release workflow end to end.
- Non-full refresh currently hashes every discovered metadata document. This is
  simpler and correct; stat-based hash skipping can be added if real libraries
  show refresh cost is material.
- The optional suggestion-hook command is cached-only and failure-open. Codex
  still requires native review and trust before the installed handler runs.

New verification entries retain their run dates and distinguish historical
evidence from current behaviour.

## Post-v0.1 benchmark and integration evidence

On 12 September 2026, the production-path benchmark evaluated 65 labelled tasks
against a captured Codex inventory containing 418 enabled records. Native
`skills/list` supplied 327 ECC skills, 15 other plugin skills, and 76 records
without a plugin ID. Temporary Skillwick config, cache, and state kept the run
isolated without hiding installed plugins. The optimized lexical runner measured
Recall@5 0.869, MRR@5 0.812, nDCG@5 0.826, no-match accuracy 0.600, and warm
in-process query latency of 0.163 ms p50 and 0.380 ms p95. Dataset SHA-256 begins
`078bff04aa63`; corpus SHA-256 begins `6af61dadafa3`. Ten misses remain visible
in JSON for future ranking comparisons. See [benchmark evidence](BENCHMARKS.md).

The managed AGENTS block now routes every instruction to use, find, select, or
load a skill through Skillwick before reading the chosen live document. The
optional suggestion hook is one cached-only Codex `UserPromptSubmit` handler.
Temporary-home integration evidence preserved an existing `caveman` handler,
and Codex `hooks/list` returned both native definitions. The hook emitted valid
additional context, then uninstall removed only Skillwick's handler. Codex's
trust review remains unchanged. This matches the official
[Codex hooks contract](https://learn.chatgpt.com/docs/hooks): matching hook
sources coexist and non-managed hooks require review.

## v0.1.4 distribution verification

On 13 September 2026, both published macOS archives were downloaded to a
temporary directory. Their individual checksum files validated, both archives
contained the expected target directory, executable, README, and two licences,
and the arm64 archive passed the repository installer's real-archive smoke path.
GitHub reports archive digests beginning `fc51722e2622` (arm64) and
`0723cd48b632` (x86_64), matching `Formula/skillwick.rb`.

The release workflow now deletes every publication artifact except the two
architecture archives and their individual checksums before upload. This is a
future-release allowlist; historical v0.1.4 assets were not rewritten. A fresh
arm64 build from this worktree passed. The Homebrew Rust installation lacks the
x86_64 standard library, so the current x86_64 source build was not independently
repeated locally; the downloaded x86_64 archive and checksum were verified.
