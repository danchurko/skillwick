# Skillwick

Skillwick helps find relevant installed skills. When a task would benefit from
specialist guidance, search explicitly, judge the candidates, and read the
instructions for any skill you select. No match or selecting none is valid.

## Use

```sh
skillwick search "deploy an AgentCore MCP server with TypeScript"
skillwick --json list
skillwick read ID
```

Use an ID from search or list. `skillwick read NAME` is also valid when NAME is
an exact, case-sensitive name with one current match. Duplicate names require
an explicit ID. Use `skillwick inspect ID` to review metadata and
`skillwick inspect ID --files` to see a bounded package listing without reading
supporting files.

Search and list operate only on configured filesystem roots. Shared roots apply
everywhere; project roots apply in their configured workspace and descendants.
Unregistered home and ancestor directories are not searched. Every lookup
reconciles applicable roots, so package additions, edits, removals, renames,
and adjacent invocation-policy changes appear on the next command. Use
`skillwick refresh` for an explicit maintenance reconciliation.

Public results are enabled, model-discoverable records only. A failed or
incomplete source update is not an empty inventory: the command fails and the
last complete cache remains intact. `skillwick doctor --strict` is the health
check for automation.

Read only selected live instructions. Skillwick validates the source path, size,
encoding, and content hash, but skill content does not authorize scripts,
package execution, configuration changes, or installation. Inspection never
executes files or follows symlink entries.

Resolve relative references from the package base printed by `read`.

## Managed setup

Configure roots through the existing owner of the environment. Use
`skillwick instructions` as the canonical content source when the owner should
install its own agent context. Skillwick does not install packages, query an
agent-native catalogue, or replace package ownership.
