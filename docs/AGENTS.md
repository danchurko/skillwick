# Agent guide

The installed [Skillwick context](../assets/skillwick/SKILLWICK.md) is the
canonical short guide for agents. It explains the invocation contract; this
page records the boundary around it.

Use deliberate discovery when a task would benefit from specialist guidance:

```sh
skillwick search "task and important technologies"
skillwick inspect ID --files
skillwick read ID
```

Review candidates before selecting one. A candidate is not an instruction to
load. Read only selected live instructions, resolve their relative references
from the printed package base, and keep package scripts and supporting files
non-executing until independently authorized.

Search is explicit and local. No match is valid. The calling workflow owns
relevance, selection, orchestration, permissions, and whether to delegate.
Skillwick does not require a search on every turn or a subagent for every task.

After installed skills, plugins, native enablement, or configured roots change,
run `skillwick refresh`. Use `skillwick doctor --strict` when a health result
must be actionable. A failed or incomplete inventory is not an empty complete
catalogue.
