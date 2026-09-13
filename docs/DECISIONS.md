# Product decisions

This page records decisions that shape the shipped product. Proposed changes
belong in GitHub issues until implemented.

## Deliberate local discovery

Skillwick provides explicit lexical search over a local snapshot. A bare query
is not a command. SQLite FTS5 remains authoritative because it keeps ordinary
operation local, deterministic, and independent of model artifacts.

## Ownership

Existing package owners retain installed skills and updates. Codex retains
native discovery, plugins, enablement, and permissions. Skillwick owns only its
derived index, integration context/reference, explicitly managed setting, and
diagnostics. The calling agent decides which candidate to select and read.

## Bounded and read-only discovery

Metadata parsing, package inspection, and protocol input have explicit bounds.
Discovery and inspection never execute instruction or supporting files. Selected
reads validate live path identity, file size, encoding, and content hash before
returning content.

## Native context and snapshots

Native inventory is partitioned by normalized workspace and Codex home. A
covered query uses only its matching snapshot. Missing or incompatible coverage
refreshes explicitly; a failed refresh retains every previous valid partition.
The cache is disposable derived state, not package ownership.

## Visibility and policy

Public inventory exposes only enabled, model-discoverable records in the
requested context. A valid invocation deny wins. `user-invocable: false` is a
separate user-menu setting and does not deny model discovery. Malformed policy
fails closed for the affected skill and remains visible through diagnostics.

## Compatibility and integration

The supported native adapter targets Codex CLI 0.154.0 on macOS. Setup is
previewable and reversible. Managed environments can own the equivalent agent
files and Codex settings while consuming the same canonical instruction
content.

## Future semantic retrieval

No embedding or reranker is part of the production path. Any future semantic
work must remain optional and local, preserve lexical fallback, and pass a
reviewable quality, latency, memory, artifact-size, and failure-behavior gate.
The [research record](RESEARCH.md) contains candidates and historical evidence;
it is not a production recommendation.
