# Skillwick

Skillwick helps coding agents find relevant installed skills and read selected
instructions. Agents decide when discovery is needed and which skills to use.

## Language

**Installed skill**:
A package of instructions and optional supporting files installed by its existing
package owner. Discovering a skill does not transfer ownership to Skillwick.
_Avoid_: Skillwick-managed package

**Skill inventory**:
The installed skills known to Skillwick for a working environment, including
their source and enablement information.
_Avoid_: Evaluation dataset

**Candidate**:
A skill returned by a search for the calling agent to assess. A candidate is not
an instruction to load or execute that skill.
_Avoid_: Selected skill

**Selected skill**:
An installed skill the calling agent chooses to read and use for the current task.
_Avoid_: Automatically activated skill

**Filesystem record**:

A metadata record discovered under an authorized filesystem root.
_Avoid_: Native inventory

**Native inventory**:

Records returned by the supported agent-native inventory contract, scoped to a
normalized workspace and agent home.
_Avoid_: Filesystem scan

**Raw record**:

One source record before public canonical deduplication.
_Avoid_: Public candidate

**Model-discoverable**:

Enabled inventory state that passes invocation-policy checks and may appear in
public search, list, count, read, or inspect results.
_Avoid_: User-invocable

**Workspace coverage**:

A valid native snapshot for the exact normalized workspace and Codex home.
_Avoid_: Global native cache

**Invocation policy**:

The normalized rule that decides whether an installed skill may be suggested to
the model.
_Avoid_: Permission to execute

**Package inspection**:

A bounded listing of package paths and types without reading or executing their
contents.
_Avoid_: Safety approval

**Integration**:
The configuration and usage instructions that make Skillwick available to a
coding agent. It does not determine the agent's general workflow.
_Avoid_: Orchestration
