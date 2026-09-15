---
status: accepted
---

# Agents maintain inventory freshness

Configured skill roots are authoritative for discovery. Skillwick discovers
instructions independently of Codex or another agent's enablement settings;
it does not query an agent server to establish which skills are available.
Installed packages remain owned by their existing installers.

Every successful lookup must establish inventory freshness under the applicable
roots, including changes made during the session. Skillwick and the
calling agent own synchronization and recovery through supported permission
mechanisms; routine refresh is not delegated to the human.

If recovery fails, preserve the last successfully published snapshot and stop
affected discovery. Do not present stale or incomplete inventory as current.
This chooses verified filesystem inventory over continuing discovery from a
potentially outdated snapshot. Skill metadata still controls invocation policy;
discovering or reading instructions does not authorize their execution.

## Scope and storage

Shared roots apply everywhere. Project roots apply only within their associated
project. Roots are explicit configuration; installing a new skill inside a root
does not require registering the skill individually. Query results are limited
to the roots applicable to the current working directory, even when projects
share a database.

SQLite remains the derived local index. It stores skill metadata, source paths,
scope associations, fingerprints, and the FTS5 search index. Transactions and the
existing atomic publication mechanism protect updates. Root configuration and
installed files remain authoritative; the database is rebuildable.

Filesystem reads discover changes and load selected instructions. SQLite serves
metadata queries, counts, filtering, and ranked search; it does not watch folders
or replace the installed instruction files.

## Lookup flow

1. Resolve the configured shared and project roots for the working directory.
2. Enumerate relevant files and compute content fingerprints, including
   `SKILL.md` and adjacent invocation-policy metadata. Additions, removals,
   renames, and changes to the applicable root set invalidate the prior inventory.
3. If the complete inventory is unchanged, reuse the SQLite index. Otherwise,
   update and publish the index atomically before answering the lookup.
4. Query SQLite for the requested results. For `read`, revalidate the selected
   live file before returning its instructions.

No background service or agent event stream is required. Explicit `refresh`
remains a maintenance operation rather than a prerequisite for normal discovery.
When permissions block an operation, the calling agent retries through supported
permission escalation when authorized. Only unsuccessful or unavailable recovery
requires human intervention; retries must be bounded.

## Implementation

The runtime, setup, diagnostics, tests, and agent instructions use this
filesystem authority and no longer retain a native inventory path. Managed
installations register their intended package directories explicitly through
their existing owner. File validation, atomic updates, and protection of the
previous snapshot and unrelated user state remain required invariants.
