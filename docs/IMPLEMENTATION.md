# Skillwick v1 implementation record

Status: release candidate. Verification used synthetic skill packages and
temporary homes only. No test changed live Codex configuration.

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
- Search, `read`, `inspect`, `list`, `refresh`, `init`, `doctor`, `uninstall`,
  JSON output, zsh completions, and a cached-only internal hook command. Hook
  installation remains off because no compatible hook fixture was accepted.
- Short-lived Codex `skills/list` inventory with bounded newline-delimited JSON,
  interleaved-notification handling, request IDs, stderr capture, timeout, EOF,
  and child termination. Native enabled state suppresses filesystem aliases.
- Reversible instruction/router/TOML integration with atomic writes, restrictive
  permissions, an ownership journal, override/collision checks, and conditional
  leaf rollback.
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
instruction/config changes, prompt-input catalogue suppression, router
visibility, uninstall, and preservation of later unrelated edits. Prompt-input
size was 11,421 bytes with the catalogue hidden and 14,299 bytes with native
catalogue injection enabled. The hidden request had
one router marker, one user prompt, and no skills-catalogue marker.

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
- Non-full refresh currently hashes every discovered metadata document. This is
  simpler and correct; stat-based hash skipping can be added if real libraries
  show refresh cost is material.
- The optional suggestion-hook command is cached-only and failure-open, but setup
  refuses `--hooks suggest` until a release-tagged hook configuration fixture is
  added. Normal instruction-plus-CLI integration is complete.

Commands, results, hardware, compatibility evidence, evaluation metrics, and
remaining limits will be recorded here after each milestone passes.
