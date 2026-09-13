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

**Integration**:
The configuration and usage instructions that make Skillwick available to a
coding agent. It does not determine the agent's general workflow.
_Avoid_: Orchestration
