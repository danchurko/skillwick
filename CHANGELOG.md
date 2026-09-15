# Changelog

User-visible changes are recorded here. Release artifacts and verification
evidence live in [implementation history](docs/IMPLEMENTATION.md).

## 0.3.0 - 2026-09-15

- Make explicitly configured shared and project skill roots the sole discovery
  authority, with automatic per-command freshness and isolated project scope.
- Remove the Codex native-inventory runtime and its provider-specific options;
  discovery now works without a Codex executable or writable Codex state.
- Preserve complete SQLite snapshots across failed scans while reflecting file,
  invocation-policy, and root-configuration changes on the next lookup.

## 0.2.2 - 2026-09-14

- Allow `skillwick read` to resolve a unique exact, case-sensitive skill name
  while preserving exact-ID reads and live source validation.
- Reject unknown or duplicate names with search guidance or explicit candidate
  provenance instead of choosing a result.

## 0.2.1 - 2026-09-14

- Explain uncovered-workspace native refresh failures with the real Codex
  provider detail and an unrestricted-terminal recovery command while keeping
  unknown enablement fail-closed and the previous cache intact.
- Require a local source installation to pass filesystem and real native Codex
  workflows in two workspace contexts before a release tag starts CI.

## 0.2.0 - 2026-09-14

- Partition native snapshots by normalized workspace and Codex home, refresh
  missing coverage safely, and report truthful inventory diagnostics.
- Respect model-invocation policy across search, list, counts, reads, and
  inspection while retaining denied records for diagnostics.
- Ship the breaking version-2 JSON contract, complete bounded human output,
  search limits from 1 through 20 with a default of 5, and comprehensive help.
- Reconstruct user, contributor, agent, decision, research, history, and
  changelog documentation with deterministic drift checks.
- Bound metadata, protocol, path, symlink, and package-inspection trust
  boundaries; add dependency assurance and end-to-end release verification.
- Freeze the lexical baseline, evaluate local embedding and reranking
  candidates, and retain lexical-only production behavior from the recorded
  adoption decision.

## 0.1.5

- Adapt the Codex integration when the selected skill invocation is
  unavailable and publish the corrected Homebrew formula.

## 0.1.4

- Improve Codex context installation and preserve existing integration state.
- Verify published macOS archives and per-file checksums.

## 0.1.0

- Initial local lexical discovery release for Codex on macOS.
