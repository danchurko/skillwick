# Architecture

Skillwick is one Rust binary with one disposable SQLite database.

```text
installed SKILL.md files ── scan and parse ── SQLite FTS5
Codex skills/list ───────── native policy ───────┘
                                                │
task text ── lexical rank ── 0–5 candidates ── read selected file
```

## Ownership

- Existing installers own skill packages and updates.
- Codex owns native discovery, enablement, plugins, and permissions.
- Skillwick owns its derived index, config, SKILLWICK.md context file, AGENTS
  reference, and integration journal.
- Users and agents decide which returned instructions to load and execute.

## Modules

- `sources` discovers applicable filesystem roots and enforces path boundaries.
- `metadata` parses bounded `SKILL.md` frontmatter.
- `package` lists bounded live package entries without following symlinks.
- `index` owns SQLite schema and transactional refreshes.
- `search` owns token normalization and deterministic lexical ranking.
- `native` is the version-bound Codex `skills/list` adapter.
- `integration` owns reversible instruction and configuration edits.
- `doctor` reports coverage, compatibility, and drift.
- `cli` maps commands to those owning modules.

The Codex adapter stays narrow. Supporting another agent requires a separate
inventory and integration contract; it must not weaken Codex or filesystem
semantics.

Evaluation tooling lives in `scripts/`, outside the production search path.
It freezes a corpus and relevance judgments, then compares direct and delegated
selection over identical retrieval inputs. Optional semantic retrieval and
reranking remain research candidates; the shipped search backend is lexical.
