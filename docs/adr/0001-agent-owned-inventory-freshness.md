---
status: accepted
---

# Sources establish eligibility; files establish content and freshness

Skillwick 0.4 resolves supported installed sources automatically. Shared skill
folders and explicitly configured roots are filesystem sources. Codex and Claude
installation metadata establishes which plugin versions and skill directories
are eligible; their installed files remain authoritative for content.

Codex does not expose an authoritative active-version path in its local settings.
Its plugin adapter therefore uses a bounded `codex plugin list --json` query and
validates the corresponding local manifest. It never guesses the newest cache
version. Explicit-root discovery needs no host executable. Claude uses its
installed-plugin registry and effective enablement settings.

This supersedes the 0.3 requirement that every source be manually registered. It
does not restore the old agent-server inventory or make that server a dependency.
There is one source-resolution pipeline followed by one filesystem scanner.

## Scope and publication

Automatic shared sources apply globally. Automatic project folders apply only
inside explicitly registered projects and their descendants. Custom roots retain
their declared scope. Neither arbitrary ancestors nor sibling projects are scanned.

Every successful lookup verifies current relevant source identities, instructions,
policy, and source eligibility. Missing required sources, unknown provider formats,
permissions failures, or ambiguous active installations fail the affected operation.
The previous complete SQLite publication remains intact and is never silently used
as a stale answer. A valid no-match search succeeds.

SQLite stores derived metadata, fingerprints, and root associations. Locks and
atomic replacement preserve concurrent project snapshots. Unchanged complete scans
skip unnecessary index updates. There is no daemon or background event stream.

## Selection and ownership

Canonical references deduplicate. Separate copies group only when complete bounded
package fingerprints agree. Preserve member IDs and every applicable origin;
uncertain packages remain separate. Different same-name packages require an ID.
Batch reads validate every selected instruction before producing output.

Existing installers own package content and updates. Skillwick owns only its
configuration, cache, and explicitly installed context references. The environment
owner installs canonical instructions with `skillwick instructions` when it owns
agent configuration. Reading a skill never authorizes executing its scripts.
