# Implementation and verification history

This is a dated record of implementation and release evidence. Version numbers,
test counts, toolchains, and release incidents describe their respective runs,
not current project status. Use the [getting-started guide](GETTING_STARTED.md),
[operations guide](OPERATIONS.md), and [reference](REFERENCE.md) for the
current supported surface.

## Initial implementation record

The first implementation established scoped source discovery, bounded metadata
parsing, one SQLite/FTS5 index, deterministic search, live `read`, short-lived
Codex inventory, compatibility checks, reversible setup, diagnostics,
uninstall, and macOS release artifacts.

The following decisions remain part of the product history:

- Existing package owners retain installed skills and updates. Skillwick stores
  derived metadata and integration state only.
- Search is explicit, local, and lexical. The production path has no MCP
  server, daemon, remote registry, package installer, or instruction execution.
- Native catalogue suppression is limited to explicitly supported Codex
  versions after replacement discovery succeeds.
- Setup and installer tests use isolated homes and fixture executables; they do
  not modify persistent user configuration.
- Integration edits use marked ownership and conditional rollback. Uninstall
  preserves installed skills and unrelated configuration.

## Retained implementation evidence

The shipped surface includes:

- Scoped filesystem discovery for global, project, ancestor, and explicit roots;
  canonical deduplication; authorized-root symlink checks; and cycle detection.
- Bounded Agent Skills frontmatter parsing with Unicode, duplicate-key,
  size-limit, invalid-YAML, and degraded-description handling.
- Transactional SQLite/FTS5 refreshes, bounded lock waits, safe incomplete-scan
  behavior, deterministic ranking, and technical-token aliases.
- Search, `read`, `inspect`, `list`, `refresh`, `init`, `doctor`, `uninstall`,
  versioned JSON output, and Zsh completions.
- A short-lived Codex `skills/list` inventory with bounded newline-delimited
  JSON, interleaved-notification handling, request IDs, timeout, EOF, and child
  termination.
- Reversible Skillwick context/reference and TOML integration with atomic writes,
  restrictive permissions, ownership journaling, collision checks, and
  conditional rollback.
- cargo-dist macOS archives, per-file SHA-256 checksums, a Homebrew formula
  template, and an explicit-prefix installer.

## Verification log

The release checks use isolated temporary homes and fixture packages. Codex
integration uses the supported CLI contract and does not modify installed skills
or persistent user configuration.

Core source checks are:

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked
make check
```

The retained CLI, Codex, distribution, documentation, and trust-boundary checks
are run separately so source tests are not mistaken for release or integration
evidence. Published release verification additionally checks archive layout,
checksums, executable versions, installer behavior, and package metadata.

### 0.2.1 corrective release - 2026-09-14

Before the release tag was created, `make release-preflight` passed from an
unrestricted local terminal on the exact 0.2.1 source tree. The gate included
formatting, Clippy, 49 Rust tests, CLI and trust-boundary acceptance, Codex
integration, a live coding-agent search/read inference check, dependency
assurance, and a fresh `cargo install --path` into a temporary prefix. The
source-installed executable passed filesystem discovery and real Codex-native
refresh, search, read, and strict doctor checks across two workspace contexts,
including first-query refresh for a workspace with no cached native snapshot.

## Compatibility and limits

- Codex 0.154.0 accepts `skills.include_instructions = false`; its native
  `skills/list` contract is covered by the integration fixture. Other versions
  default to filesystem discovery only.
- macOS arm64 is the supported runtime. macOS x86_64 archives are built, but
  hardware and older macOS runtime coverage remain limited.
- Signing and notarization are not claimed unless a release verifies them.
- Historical benchmark and model-research measurements are not production
  quality claims. Current research boundaries are recorded in [research](RESEARCH.md).

The detailed host, path, and release-run metadata remains in the corresponding
CI or release records. This document keeps only evidence needed to understand
the product history and its limits.
