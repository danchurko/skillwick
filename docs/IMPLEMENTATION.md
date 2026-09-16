# Implementation and verification history

This page records dated implementation and release evidence. Version numbers,
test counts, toolchains, and incidents describe their respective runs, not
automatically the current working tree. Use the [getting-started guide](GETTING_STARTED.md),
[operations guide](OPERATIONS.md), and [reference](REFERENCE.md) for the
current supported surface.

## Current filesystem-authority implementation

The 0.4 implementation resolves automatic provider roots and explicit filesystem
roots. Shared roots apply globally; project roots apply to their configured
workspace and descendants. Each lookup reconciles its applicable scope through
the same bounded scan and atomic SQLite/FTS5 publication path used by
`refresh`.

The current invariants are:

- Installed package directories and updates remain owned by their existing
  installer or user; Skillwick stores derived metadata and integration state.
- Canonical instruction paths and byte-verified package copies are grouped for
  public results while all origins and member IDs remain available.
- Complete scans are required before publication. Missing roots, malformed
  metadata, unreadable files, and unauthorized symlink escapes fail closed and
  preserve the previous complete cache.
- Freshness includes root configuration, canonical instruction identity and
  content, and adjacent invocation-policy identity, presence, and content.
- Reads validate the selected live path, size, encoding, and content hash.
- Search, listing, reading, and package inspection never execute package
  content. Automatic discovery consults bounded provider eligibility metadata.

## Shipped surface

The binary provides:

- Explicit root setup with `--root` and `--project-root`, optional reversible
  agent context/reference setup, and conditional uninstall.
- Bounded Agent Skills frontmatter parsing with Unicode, duplicate-key,
  size-limit, invalid-YAML, policy, and degraded-description handling.
- Transactional SQLite/FTS5 reconciliation, scoped cache locking, atomic
  publication, deterministic lexical ranking, and technical-token aliases.
- `search`, `read`, `inspect`, `list`, `refresh`, `init`, `doctor`,
  `uninstall`, lossless JSON v3 output, and Bash, Fish, and Zsh completions.
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

## 0.4 source verification — 2026-09-16

Verified locally on macOS ARM64, with an offline source-installed candidate in a
temporary prefix. Executable SHA-256:
`6fcc7aa023a02b929f7f983322166e30e35c970910d35bd0868b3b8cfeeb8ca7`.

- Formatting, Clippy, 51 Rust tests, and CLI, setup, provider, invocation,
  filesystem, distribution, documentation, benchmark and trust contracts passed.
- The staged-tree pre-commit hook passed with deliberately invalid unstaged Rust;
  staging that invalid Rust then repairing only the working file correctly failed.
- Automatic local discovery verified 812 canonical records, 773 discoverable
  packages and 543 groups. All five required managed skills resolved. Explicit
  configured roots were checked separately; source/configuration hashes stayed
  unchanged. These counts describe this host, not product expectations.
- The installed three-skill read chain reached its following `sed` and `rg`
  commands. No live agent instructions or installed binary were replaced.
- Both frozen benchmark profiles retain 0.3 ranking quality. See the
  [comparison](../benchmarks/README.md); this is not a semantic-quality gain.
- Managed-owner source changes passed isolated mac-state provisioning checks.
  Linux and Intel macOS runtime verification remains for the configured native CI
  runners. No CI, publication, or live workstation-apply success is claimed here.

## Historical release evidence

The following entries describe releases made before the filesystem-authority
change. They remain useful provenance, but their native-inventory behavior is
superseded by the current provider-resolution and filesystem-content contract.

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

- Release targets are macOS and Linux on ARM64 and x86_64. Runtime evidence
  is recorded separately in the compatibility page and CI; target configuration
  alone does not prove platform execution.
- Signing and notarization are not claimed unless a release verifies them.
- Historical benchmark and model-research measurements are not production
  quality claims; current research boundaries are recorded in [research](RESEARCH.md).

Detailed host, path, and release-run metadata belongs in the corresponding CI
or release records. This document keeps only evidence needed to understand the
product history and its limits.
