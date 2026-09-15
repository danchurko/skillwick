# Implementation and verification history

This page records dated implementation and release evidence. Version numbers,
test counts, toolchains, and incidents describe their respective runs, not
automatically the current working tree. Use the [getting-started guide](GETTING_STARTED.md),
[operations guide](OPERATIONS.md), and [reference](REFERENCE.md) for the
current supported surface.

## Current filesystem-authority implementation

The current implementation discovers only explicitly configured filesystem
roots. Shared roots apply globally; project roots apply to their configured
workspace and descendants. Each lookup reconciles its applicable scope through
the same bounded scan and atomic SQLite/FTS5 publication path used by
`refresh`.

The current invariants are:

- Installed package directories and updates remain owned by their existing
  installer or user; Skillwick stores derived metadata and integration state.
- Canonical instruction paths are deduplicated for public results while raw
  root associations remain available for scope and duplicate accounting.
- Complete scans are required before publication. Missing roots, malformed
  metadata, unreadable files, and unauthorized symlink escapes fail closed and
  preserve the previous complete cache.
- Freshness includes root configuration, canonical instruction identity and
  content, and adjacent invocation-policy identity, presence, and content.
- Reads validate the selected live path, size, encoding, and content hash.
- Search, listing, reading, and package inspection never execute package
  content or query a second agent-native inventory.

## Shipped surface

The binary provides:

- Explicit root setup with `--root` and `--project-root`, optional reversible
  agent context/reference setup, and conditional uninstall.
- Bounded Agent Skills frontmatter parsing with Unicode, duplicate-key,
  size-limit, invalid-YAML, policy, and degraded-description handling.
- Transactional SQLite/FTS5 reconciliation, scoped cache locking, atomic
  publication, deterministic lexical ranking, and technical-token aliases.
- `search`, `read`, `inspect`, `list`, `refresh`, `init`, `doctor`,
  `uninstall`, versioned JSON output, and Zsh completions.
- Bounded package inspection that reports shape without following symlink
  entries, reading support files, or executing scripts.
- Reversible integration edits with atomic writes, restrictive permissions,
  ownership journaling, collision detection, and conditional rollback.

## Verification contract

Core source checks are:

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
```

Acceptance, trust-boundary, documentation, and integration checks run
separately so source tests are not mistaken for end-to-end evidence. The
source-install check must install the current source into a temporary prefix,
invoke that installed executable, exercise deterministic fixture workflows,
and run a read-only corpus pass over the maintained configured roots. It also
compares dynamic JSON/SQLite counts, verifies project isolation and concurrent
reconciliation, and hashes relevant `SKILL.md` and policy inputs before and
after the corpus pass.

Before a release tag, run the full gate from the exact source tree:

```sh
make release-preflight
```

The gate must pass with a fresh source-installed executable. Do not treat a
plan, a debug binary, a partial probe, or a test-only result as release
evidence. Published artifact verification additionally checks archive layout,
checksums, executable versions, installer behavior, and package metadata.

## Historical release evidence

The following entries describe releases made before the filesystem-authority
change. They remain useful provenance, but their native-inventory behavior is
superseded by the current root-only contract.

### 0.2.2 exact-name reads - 2026-09-14

Before the release tag was created, `make release-preflight` passed on commit
`485ad99`. The gate included formatting, Clippy, Rust tests, CLI,
documentation, trust-boundary, distribution, native-inventory integration,
live inference, dependency assurance, and a fresh source installation.
Independent published-release verification confirmed archive layout, checksums,
executable versions, and isolated installer behavior.

### 0.2.1 corrective release - 2026-09-14

Before the release tag was created, `make release-preflight` passed on the exact
0.2.1 source tree. The gate included formatting, Clippy, Rust tests, CLI and
trust-boundary acceptance, Codex integration, live inference, dependency
assurance, and a fresh source installation into a temporary prefix.

## Compatibility and limits

- The maintained release target is macOS arm64. macOS x86_64 archives are
  built, but hardware and older macOS runtime coverage remain limited.
- Signing and notarization are not claimed unless a release verifies them.
- Historical benchmark and model-research measurements are not production
  quality claims; current research boundaries are recorded in [research](RESEARCH.md).

Detailed host, path, and release-run metadata belongs in the corresponding CI
or release records. This document keeps only evidence needed to understand the
product history and its limits.
