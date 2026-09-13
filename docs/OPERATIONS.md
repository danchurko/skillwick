# Operations

This guide covers routine state changes and recovery. For command syntax, see
the [command reference](REFERENCE.md).

## Choose an inventory

Filesystem inventory is useful for a standalone local setup:

```sh
skillwick init --yes --agent none --inventory filesystem
```

Codex inventory uses the supported native `skills/list` contract and keeps
records scoped to the normalized working directory and Codex home:

```sh
skillwick init --yes --agent codex --catalog native
```

Existing skill installers own package files. Skillwick owns only its derived
index, integration context/reference, configured Codex setting when explicitly
requested, and integration journal.

## Refresh after changes

The index is a durable snapshot, not a live filesystem query. Refresh after
installing, removing, enabling, disabling, or changing a skill, plugin, native
catalogue setting, or configured root:

```sh
skillwick refresh
```

A complete refresh publishes a new snapshot atomically. A failed native refresh
retains the previous valid context partition and reports the provider error.
Ordinary queries use compatible cached native coverage without starting Codex.

## Diagnose state

Run the non-strict report while investigating. It exits successfully and shows
diagnostic fields even when health is incomplete:

```sh
skillwick doctor
```

Use strict mode in automation:

```sh
skillwick doctor --strict
```

The report distinguishes filesystem, native, raw, duplicate, and
model-discoverable counts. A native report is scoped to the current workspace
and Codex home; counts from another context are not substitutes.

## Recover a stale or unavailable source

`read` and `inspect` validate the selected source against its indexed canonical
path and content hash. If validation fails, run:

```sh
skillwick refresh
skillwick search "your task"
```

Do not use an old ID after refresh unless it is returned again. Missing,
changed, disabled, policy-denied, malformed, and provider-failed states have
different diagnostics and exit behavior; do not treat an empty result as proof
that an incomplete scan deleted a skill.

If the configured cache is unavailable, Skillwick uses an empty in-memory
database for that invocation and tells you to refresh. The durable cache is
disposable; it contains derived metadata, not package ownership.

## Inspect safely

Use `inspect --files` to review package shape before selecting a skill:

```sh
skillwick inspect ID --files
```

The listing is bounded by entry count, depth, and relative-path size. It does
not follow symlink entries, read reference bodies, execute scripts, or establish
that a listed file is safe to run. Read only the selected `SKILL.md` and apply
your normal trust review before taking any action.

## Remove the integration

Uninstall removes only Skillwick-owned integration changes:

```sh
skillwick uninstall
```

Add `--purge-cache` to remove the disposable local index as well:

```sh
skillwick uninstall --purge-cache
```

Uninstall preserves installed skills, unrelated AGENTS content, unrelated
Codex settings, and unrelated hooks. If an owned file has drifted, it reports
the conflict instead of deleting another writer's changes.

## Configuration paths

Skillwick honors `HOME`, `CODEX_HOME`, and XDG overrides:

```text
$XDG_CONFIG_HOME/skillwick/config.toml
$XDG_CACHE_HOME/skillwick/index-v3.sqlite
$XDG_STATE_HOME/skillwick/integration.json
$CODEX_HOME/SKILLWICK.md
$CODEX_HOME/AGENTS.md
$CODEX_HOME/config.toml
```

Keep these paths under the owner that configured them. A managed environment
should consume `skillwick instructions` rather than copy a second instruction
source.
