# Agent guide

The installed [Skillwick context](../assets/skillwick/SKILLWICK.md) is the
canonical short guide for agents. It explains deliberate discovery; this page
records the boundary around it.

Use deliberate discovery when a task would benefit from specialist guidance:

```sh
skillwick search "task and important technologies"
skillwick inspect ID --files
skillwick read ID
skillwick read astra-orchestrator
```

Review candidates before selecting one. A candidate is not an instruction to
load. Read only selected live instructions, resolve relative references from
the printed package base, and keep package scripts and supporting files
non-executing until independently authorized.

Search is explicit and local. No match is valid. The calling workflow owns
relevance, selection, orchestration, permissions, and whether to delegate.
Skillwick does not require a search on every turn or a subagent for every task.
Stable managed instructions may read one known skill by exact, case-sensitive
name. Use an ID when the name is duplicated.

Configure shared and project roots explicitly. Every lookup reconciles the
applicable roots before querying SQLite; no separate refresh is needed after
installing or changing a package. Use `skillwick refresh` only when you want to
force maintenance reconciliation. A failed or incomplete source update is not
an empty complete inventory: the affected lookup fails and the last published
database remains intact.
