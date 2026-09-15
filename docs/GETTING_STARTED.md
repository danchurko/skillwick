# Getting started

Skillwick is a local catalogue for installed Agent Skills. It discovers only
the filesystem roots you configure, indexes bounded metadata in a disposable
SQLite database, and reads only the instruction file you select. It never
installs packages or executes package content.

## Install

For a published macOS release, use Homebrew:

```sh
brew tap danchurko/skillwick https://github.com/danchurko/skillwick.git
brew install skillwick
```

To build from source, install stable Rust and run:

```sh
git clone https://github.com/danchurko/skillwick.git
cd skillwick
cargo build --release --locked
```

Check the executable before using it:

```sh
skillwick --version
skillwick --help
```

## Configure discovery roots

Register a shared root when its packages should be available in every project:

```sh
skillwick init --yes --agent none --root "$HOME/.agents/skills"
```

Register a project root with the workspace it belongs to. It applies in that
workspace and all descendants, not in sibling projects:

```sh
skillwick --cwd "$PWD" init --yes --agent none \
  --root "$HOME/.agents/skills" \
  --project-root "$PWD/.agents/skills"
```

Roots are explicit. Unregistered home, ancestor, and agent-specific folders
are not searched. Existing package owners continue to own package files and
updates; registering a parent directory is enough to discover packages below
it.

## Search, inspect, and read

Every lookup reconciles the applicable roots before querying the local index.
An unchanged inventory reuses SQLite, so a separate refresh is not required
after ordinary package changes. Search is explicit and accepts one or more
task words:

```sh
skillwick search "deploy an AgentCore MCP server with TypeScript"
skillwick search "SQLite full text ranking" --limit 3
```

Select an ID from the results and inspect it before reading when provenance or
package shape matters:

```sh
skillwick inspect ID
skillwick inspect ID --files
skillwick read ID
```

`read` also accepts a unique, exact, case-sensitive skill name. It validates
the live canonical path, size, encoding, and content hash before printing the
file. Treat the file as instructions to review; a successful read does not
authorize running scripts or changing configuration.

## Check the inventory

`list` is the complete current-scope model-discoverable inventory. `doctor`
reports source, cache, policy, and integration health:

```sh
skillwick list
skillwick doctor --strict
```

Use `--json` with `search`, `list`, `inspect`, or `doctor` for version-2
machine-readable output. `refresh` is available when a caller wants an
explicit maintenance reconciliation:

```sh
skillwick refresh
```

Failed or incomplete source scans fail the affected operation and preserve the
last complete published cache. They are not reported as an empty inventory.

## Optional agent integration

Setup defaults to the Codex integration target. Apply it only when Skillwick
should manage its own context file and one reference in the selected agent
instructions file:

```sh
skillwick init --dry-run --yes --root "$HOME/.agents/skills"
skillwick init --yes --root "$HOME/.agents/skills"
skillwick doctor --strict
```

Use `--agent none` for filesystem discovery without integration files. A
managed configuration owner can instead consume the canonical content from
`skillwick instructions` and keep ownership of its own agent files. Skillwick
does not query an agent-native catalogue or change installed packages.

## Next steps

Read the [usage guide](USAGE.md) for workflows, [operations guide](OPERATIONS.md)
for recovery and ownership, or the [command reference](REFERENCE.md) for
stable scripting details.
