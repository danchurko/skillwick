# Architecture

Skillwick is one local Rust binary with one rebuildable SQLite database.

```text
explicit shared/project roots + --cwd
                 │
                 ▼
      resolve applicable roots
                 │
                 ▼
      bounded scan, parse, validate
                 │
                 ▼
      complete atomic SQLite/FTS5 reconcile
                 │
                 ▼
      lexical search, counts, inspection
                 │
                 ▼
      selected live-file read
```

## Ownership

- Existing installers own skill packages and package updates.
- Skillwick owns its configuration, derived index, canonical context file,
  agent reference, and integration journal.
- Configured roots and installed files are discovery authority. The SQLite
  database is rebuildable derived state.
- Agents decide which returned instructions to select and follow.

## Modules

- `config` resolves XDG paths, explicit roots, project associations, and the
  normalized working directory.
- `sources` resolves applicable roots, scans bounded package trees, validates
  canonical paths, and rejects unauthorized symlink escapes.
- `metadata` parses bounded `SKILL.md` frontmatter and invocation policy.
- `package` lists bounded live package entries without following symlinks.
- `index` owns SQLite schema, root/scope associations, locking, and atomic
  publication.
- `inventory` reconciles complete source scans before a lookup and keeps the
  explicit maintenance refresh command on the same path.
- `search` owns scope-filtered SQLite queries, token normalization, and
  deterministic lexical ranking.
- `integration` owns reversible context/reference edits and managed setup.
- `doctor` reports source, scope, cache, and integration health.
- `cli` maps commands to those owning modules.

Every inventory-backed lookup reconciles its applicable roots. An unchanged
inventory reuses SQLite. A changed inventory is collected and validated fully
before publication; failed updates preserve the previous database and fail the
affected lookup. Selected instructions are read from their validated live file,
not from a second copy stored in SQLite.

The production path is local and lexical. It does not install packages, execute
instruction files, start an agent server, require a daemon, or use a remote
service. Search and inspection JSON uses version 2 envelopes. List adds a
complete `total` count, and human search output bounds each result independently.
