# Product decisions

This page records decisions that shape the shipped product. Proposed changes
belong in GitHub issues until implemented.

## Deliberate local discovery

Skillwick provides explicit lexical search over configured filesystem roots. A
bare query is not a command. SQLite FTS5 remains the derived query index because
it keeps ordinary operation local, deterministic, and independent of model
artifacts.

## Filesystem authority and scope

Configured roots are the only discovery authority. Shared roots apply in every
project. A project association applies in its configured directory and all
descendants. Unregistered home and ancestor directories are not searched.
Installing a package inside a registered root does not require registering the
package itself.

The applicable root set is resolved from explicit configuration and the
normalized `--cwd`. Results are filtered to that set even when projects share a
SQLite database. Overlapping authorized references are canonically deduplicated;
duplicate names remain explicit ambiguity errors.

## Freshness and publication

Every inventory-backed lookup reconciles the applicable roots. Fingerprints cover
relevant file identities, instruction contents, adjacent invocation-policy
metadata, its presence or absence, and the root set. Directory timestamps alone
are not freshness proof.

An unchanged inventory reuses the published SQLite database. A changed inventory
is collected and validated completely before atomic publication. Missing or
unreadable required roots, invalid input, and publication failures preserve the
previous database and fail the affected lookup rather than returning partial or
stale results. `refresh` remains an explicit maintenance operation, not a normal
workflow prerequisite.

## Visibility and policy

Public inventory exposes only records under applicable roots that pass
invocation-policy checks. Malformed policy fails closed for the affected skill
and remains visible through diagnostics. Skillwick does not infer permission to
execute a package from model discoverability or from a successful read.

## Bounded and read-only discovery

Metadata parsing, package inspection, and file reads have explicit bounds.
Discovery and inspection never execute instruction or supporting files. Selected
reads validate live path identity, file size, encoding, and content hash before
returning content. Existing package owners retain package files and updates.

## Integration ownership

Setup is previewable and reversible. Managed environments register intended roots
through their existing configuration owner and consume the canonical context
from `skillwick instructions`. Setup preserves modified or unrelated files and
settings; it does not copy, install, update, or remove skill packages.

## Future semantic retrieval

No embedding or reranker is part of the production path. Any future semantic
work must remain optional and local, preserve lexical fallback, and pass a
reviewable quality, latency, memory, artifact-size, and failure-behavior gate.
The [research record](RESEARCH.md) contains candidates and historical evidence;
it is not a production recommendation.
