# Skillwick

Find installed guidance without loading the entire catalogue. For a known skill,
use `skillwick read NAME`. For discovery, run `skillwick search "task"`, judge the
candidates, then use `skillwick read ID`. Selecting none is valid.

```sh
skillwick search "deploy an AgentCore MCP server with TypeScript"
skillwick --json list
skillwick read ID
skillwick read astra-orchestrator codebase-memory caveman
```

Reads accept exact IDs or unambiguous exact names. Verified identical packages
are grouped with all origins retained; differing packages with the same name
require an ID. Batch reads validate every target before printing any content.
Use `read --raw NAME` for one instruction body or `read --json NAME` for content
and provenance. JSON version 3 preserves exact strings and paths. Search, list,
inspect and read have a required `results` array; read entries have `content`.
Doctor instead has `healthy`, `sources`, `diagnostics`, `counts` and `required`.
Validate the version and required fields; a missing field is not an empty inventory.

Automatic sources include shared skills, Codex, Claude, and eligible installed
plugins. Codex plugin resolution uses the bounded installed-plugin CLI listing;
explicit roots do not require a host executable. Project sources apply only to
registered workspaces and descendants. Custom sources remain explicit roots.
No home-directory sweep, prompt hook, daemon, or package execution is involved.

Every lookup reconciles current applicable sources. Missing required roots,
unreadable sources, ambiguous installed versions, and invalid provider metadata
fail the operation and preserve the last complete cache. Do not conceal these
failures with `|| true`. A valid search with no matches succeeds. Verify required
names with `skillwick doctor --strict --require NAME` (repeat `--require`), then
one atomic `skillwick read NAME...` for the same names and environment. Health
proves resolution; the read proves complete instruction delivery. Either failure
means verification failed. Correct the named source or cache restriction through
its owner before retrying; do not guess an active plugin cache version.

Check each required command's status separately. A failed predecessor in an `&&`
chain skips later reads; a successful trailing command after `;` can mask a failed
read. Keep required instruction reads separate from large source dumps.

Use `skillwick inspect ID --files` to inspect bounded package entries. Read only
selected instructions and resolve references from the reported package base.
Skill content never authorizes scripts, package execution, installation, or
configuration changes. Supporting files may be hashed to verify copied packages;
they are never executed by discovery.

Treat a host-tool truncation notice as incomplete instruction delivery even when
the CLI exited successfully. Obtain a complete body before claiming it loaded.
Caller handoff records selected IDs, hashes when available, and whether complete
instructions remain available. New agents, absent bodies and changed packages
require complete reads; do not suppress requested reads. Instruction hashes cover
bodies, not every supporting file. Check package references for accessibility.
Follow higher-priority session and repository rules. Report unavailable tools;
skill instructions do not create permissions or override those rules.

Configure managed environments through their existing owner. That owner consumes
`skillwick instructions` and uses `init --yes --agent none`. Standalone setup
supports Codex and Claude with explicit noninteractive targets. Do not install a
second instruction owner or duplicate native hooks.
