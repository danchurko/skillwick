# Getting started

Skillwick is a local catalogue for installed Agent Skills. It indexes bounded
metadata, returns deliberate candidates, and reads only the instruction file
you select. It does not install packages or execute package content.

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

## First search

Skillwick discovers `$HOME/.agents/skills` and applicable `.agents/skills`
directories from the selected working directory through its ancestors. Create
or install a skill with its existing package owner, then refresh the local
snapshot:

```sh
skillwick refresh
skillwick search "deploy an AgentCore MCP server with TypeScript"
```

Search returns compact IDs. Copy one ID into `read`:

```sh
skillwick read ID
```

The `read` command rechecks the live path, canonical identity, size, and content
hash. Treat the selected file as instructions to review, not as permission to
run scripts or change configuration.

## Inspect before reading

Use metadata inspection when you need package provenance. Add `--files` for a
bounded relative listing:

```sh
skillwick inspect ID
skillwick inspect ID --files
```

Inspection does not print reference contents and does not execute scripts.
Symlink entries are reported without being followed. A truncated listing is a
partial view; it is not proof that omitted files do not exist.

## Check and refresh

Use `list` to see the complete current model-discoverable inventory and
`doctor` to inspect coverage and integration state:

```sh
skillwick list
skillwick doctor --strict
```

Run `skillwick refresh` after installed skills, plugins, native enablement, or
configured roots change. With Codex inventory enabled, a missing or incompatible
workspace snapshot triggers one observable automatic refresh; covered searches
stay local and cache-only.

## Optional Codex integration

Preview setup before writing files:

```sh
skillwick init --dry-run --yes --agent codex --catalog native
```

Apply setup only when you want Skillwick to own its integration files:

```sh
skillwick init --yes --agent codex --catalog native
skillwick doctor --strict
```

The setup writes Skillwick's owned context/reference and native snapshot. It
does not move installed skills or take ownership of Codex plugins. Managed
environments can provide the same context with `skillwick instructions` and
own their AGENTS and Codex settings themselves.

## Next steps

Read the [operations guide](OPERATIONS.md) for recovery and uninstall, or the
[command reference](REFERENCE.md) for stable scripting details.
