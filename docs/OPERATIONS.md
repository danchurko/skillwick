# Operations

Installed skill packages remain owned by their installers. Skillwick owns only
its configuration, disposable cache, and explicitly installed context references.
It never executes, updates, moves, or deletes skill packages.

## Setup and state

Use `init --dry-run --yes --agent codex` to preview standalone integration.
Repeat `--agent claude` to select both. Managed owners use `--agent none` and
consume `skillwick instructions` themselves. No prompt or tool hooks are installed.

Default paths are:

```text
$XDG_CONFIG_HOME/skillwick/config.toml
$XDG_CACHE_HOME/skillwick/index-v4.sqlite
$XDG_STATE_HOME/skillwick/integration.json
```

`HOME`, `CODEX_HOME`, `CLAUDE_CONFIG_DIR`, and XDG overrides support isolated
operation. Configuration has strict `version = 1` and automatic or explicit
source discovery. Unknown fields and obsolete configuration are errors.

## Re-setup after an upgrade

Preserve a copy of existing configuration before recreating it. Inspect custom
roots and register them explicitly in the new setup; do not discard user-owned
roots. Retire obsolete product-owned integration using its owning version before
installing new integration. A changed context or unrecognized journal requires
manual review; Skillwick never guesses ownership or overwrites it.

For an obsolete configuration, move it aside rather than leaving an old schema
at the active path. Review its roots before recreating them:

```sh
config_dir="${XDG_CONFIG_HOME:-$HOME/.config}/skillwick"
backup_dir="$(mktemp -d "$config_dir/backup.XXXXXX")"
mv "$config_dir/config.toml" "$backup_dir/config.toml"
# Standalone example; repeat --root for each reviewed custom root.
skillwick init --yes --agent codex --discovery auto --root "$HOME/my-skills"
```

Retire product-owned integration with its owning binary before moving its journal;
backing up a receipt alone does not remove the files it owns.

In managed environments, agent-state remains the owner of persistent setup.
Do not run standalone integration over its instruction files. Its
`services/skillwick/install.sh` owns executable provisioning and source/release
provenance; `services/skillwick/config.sh` owns staged discovery configuration,
canonical contexts and their references. The selected workflow owns required
capability names and checks their health followed by one complete batch read
under the same configuration, workspace and environment. Generic healthy output
is insufficient. Retire competing product-owned integration through its matching
binary before managed setup.

CLI root registration is additive. Desired-state retirement of obsolete managed
roots belongs to the environment owner, which preserves custom registrations,
modified contexts and rollback material. Validate staged configuration before
the separately authorized managed apply. Source changes and fixture success do
not themselves upgrade the installed binary or prove a published release.

## Coverage and failures

```sh
skillwick doctor --strict --require codebase-memory
skillwick --json doctor
```

Health includes resolved sources, policy/eligibility diagnostics, grouped and raw
counts, required names, and owned integration. Require the specific skills your
workflow needs; a cache that works is not proof that a missing custom root was
intended to be absent.

Automatic optional directories can be absent. Missing explicit roots, unreadable
present sources, unknown provider schemas, ambiguous active versions, or failed
Codex plugin queries fail the affected operation. Explicit discovery avoids host
executable dependencies. Correct the source and repeat the failed command.

Complete scans publish atomically. Failures preserve the last complete publication
but never use it as a stale successful answer. Concurrent project lookups remain
scope-filtered. `refresh` uses the same reconciliation path; it is not a substitute
for correcting source or configuration errors.

## Identity and safe reads

Canonical aliases deduplicate. Copies group only when complete bounded package
fingerprints agree; every origin/member ID is retained. Symlinks, unreadable
support files, special files, and exceeded fingerprint limits leave copies separate.
Use an exact ID when a name resolves to different packages.

`read` validates all selected files before printing bodies. `read --raw` emits one
body. `--json` preserves exact strings and paths. `inspect ID --files` reports
bounded package shape without executing files or outputting supporting bodies.

## Removal

```sh
skillwick uninstall
skillwick uninstall --purge-cache
```

Removal is restricted to verified owned integration. Modified or unmanaged files,
other tools' references/hooks, and all skill packages remain intact. Setup journals
support recovery from partial writes and detect concurrent modifications.
