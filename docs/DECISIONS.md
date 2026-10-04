# Product decisions

## Discovery and ownership

Skillwick 0.4 discovers supported shared and host installations automatically.
Custom roots and project associations remain explicit. Source eligibility comes
from validated local installation metadata; Codex active plugin versions use its
bounded installed-plugin CLI query. Installed files remain content authority.
There is no whole-home sweep, agent-server inventory, daemon, or prompt hook.
See [ADR-0001](adr/0001-agent-owned-inventory-freshness.md).

Existing installers retain package ownership. Setup changes only explicitly owned
configuration and context references. Managed environments keep their current
owner and use `skillwick instructions` with `--agent none` setup.

## Retrieval and grouping

Search remains explicit, with local FTS5 candidate retrieval. Setup can select
optional local TinyBERT or hosted JEV reranking of at most 20 eligible grouped
candidates. Hosted requests contain only the query and candidate names/descriptions
with request-local labels. IDs, paths, provenance, and bodies remain local.
Exact names and IDs resolve deterministically.
Identical package copies group after scope/policy filtering; incomplete fingerprints
never collapse uncertainty. Origins and member IDs remain available. Different
same-name packages require explicit ID selection. Reading and inspection do not
execute package content.

## Freshness and errors

Lookups reconcile relevant sources before answering. A failed complete-source check
preserves the prior cache and fails the affected command. Valid search no-match is
success. All batch targets are validated before any instructions are printed.
The derived cache may be rebuilt; source and configuration failures cannot be
repaired by returning stale content.

## Interfaces and upgrades

Configuration schema 1 rejects unknown fields and obsolete configuration. JSON
version 3 preserves exact values and includes provenance and read content. Human
output handles terminal safety separately. Raw reads emit one validated body.
Old configuration requires explicit backup/re-setup; no legacy runtime or hidden
command aliases remain. Native Windows support is outside the 0.4 scope.

## Semantic retrieval

Lexical-only remains the default and needs no Python runtime or hosted account.
The opt-in reranker decision in [issue #33](https://github.com/danchurko/skillwick/issues/33)
supersedes the earlier decision to ship no reranker. Setup prepares pinned backends;
search never installs dependencies or downloads models. Backend failures preserve
lexical order with a sanitized diagnostic; inventory failures still fail closed.
No embedding retrieval ships. Relevance fixtures justify optional evaluation, not
claims of agent task success. New measurements separate independent
case count, provenance, positive ranking, negative false positives, and complete
CLI resource costs. See [reranking](RERANKING.md) and [research](RESEARCH.md).
