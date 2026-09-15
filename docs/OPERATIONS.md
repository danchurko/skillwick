# Operations

Skillwick keeps a disposable SQLite/FTS5 index of installed skills. The
configured filesystem roots and their files remain authoritative. This guide
covers ownership, reconciliation, diagnosis, recovery, and removal; see the
[command reference](REFERENCE.md) for exact syntax.

## Ownership and setup

Existing package installers or users own skill directories and package updates.
Skillwick owns its configuration, derived cache, optional context file, the
reference it adds to an agent instructions file, and its integration journal.
It does not install, update, move, or execute package files.

Register shared roots explicitly:

```sh
skillwick init --yes --agent none --root "$HOME/.agents/skills"
```

Associate project roots with a workspace and its descendants:

```sh
skillwick --cwd "$PWD" init --yes --agent none \
  --project-root "$PWD/.agents/skills"
```

Use `--dry-run --yes` to preview a plan. The default agent target is Codex and
only adds Skillwick's context/reference integration; `--agent none` configures
roots without writing integration files. A managed owner can consume
`skillwick instructions` and retain ownership of its own destination.

## Reconciliation and freshness

Search, list, read, and inspect reconcile the roots applicable to their
normalized `--cwd`. The scan is bounded and complete: it parses each
`SKILL.md`, checks its adjacent `agents/openai.yaml` policy input, validates
canonical paths, and rejects unauthorized symlink escapes. Overlapping roots
share one canonical public record while raw root associations remain available
for diagnostics and counts.

An unchanged source and root configuration leave the durable cache untouched.
When a source changes, Skillwick builds a complete scoped update and publishes
it atomically. `refresh` requests that same reconciliation explicitly. A
failure—such as a missing root, malformed metadata, unreadable file, or
publication error—fails the affected command and retains the previous complete
cache. It is not treated as an empty inventory.

Concurrent reconciliations use the cache lock and preserve unrelated project
associations. A lookup in one project cannot read a record associated only with
another project, even when both use the same cache.

## Diagnose state

Use the regular report while investigating:

```sh
skillwick doctor
```

Use strict mode in automation:

```sh
skillwick doctor --strict
```

The report includes the normalized source count and current-scope counts for
filesystem rows, raw root associations, canonical duplicates, and
model-discoverable rows. Policy diagnostics remain visible even when a record
is hidden from public search. `--json` returns the version-2 diagnostic object.

The default state paths are:

```text
$XDG_CONFIG_HOME/skillwick/config.toml
$XDG_CACHE_HOME/skillwick/index-v3.sqlite
$XDG_STATE_HOME/skillwick/integration.json
```

`HOME`, `CODEX_HOME`, and the XDG variables can isolate a run for tests or
recovery. The cache is derived state and may be rebuilt; it is not package
ownership or a second instruction store.

## Recover an incomplete source update

If a lookup reports an incomplete or invalid source, correct the root or file
and repeat the same command. An explicit refresh is also available:

```sh
skillwick refresh
skillwick search "your task"
```

The prior cache remains available to other successful contexts while the
affected operation fails. After a refresh, select a newly returned ID rather
than assuming an old ID is still current. `read` and `inspect` independently
revalidate the selected live path, canonical identity, size, encoding, and
content hash.

## Inspect safely

Inspect package shape before selecting a skill:

```sh
skillwick inspect ID --files
```

The listing is bounded by entry count, depth, and relative-path size. It
reports file types and classifications, does not read reference bodies, does
not follow symlink entries, and never executes scripts. Read only the selected
`SKILL.md`, then apply the calling workflow's own trust review.

## Remove integration

Uninstall removes only Skillwick-owned integration changes:

```sh
skillwick uninstall
```

Add `--purge-cache` to remove the disposable local index as well:

```sh
skillwick uninstall --purge-cache
```

Installed skills, unrelated instructions, unrelated agent settings, and
unrelated hooks remain in place. If an owned file has drifted, uninstall
reports the conflict instead of deleting another writer's changes.
