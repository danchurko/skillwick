# Changelog

User-visible changes are recorded here. Release artifacts and verification
evidence live in [implementation history](IMPLEMENTATION.md).

## Unreleased

- Clarify the user-first documentation map, command reference, operations
  guide, agent boundary, security contract, decisions, research, and history.
- Keep search explicit, local, bounded, and lexical; document version-2 JSON
  envelopes and current-context inventory behavior.
- Add deterministic documentation and trust-boundary checks to normal
  validation.

## 0.1.5

- Keep native inventory snapshots partitioned by normalized workspace and Codex
  home.
- Distinguish filesystem, native, raw, duplicate, and model-discoverable
  inventory diagnostics.
- Normalize invocation visibility at ingestion and retain previous valid state
  when native refresh fails.

## 0.1.4

- Improve Codex context installation and preserve existing integration state.
- Verify published macOS archives and per-file checksums.

## 0.1.0

- Initial local lexical discovery release for Codex on macOS.
