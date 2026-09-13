# Command and JSON reference

This document describes the shipped CLI contract. `skillwick --help` and each
subcommand's `--help` output are the executable source for option spelling; the
documentation checks compare both surfaces.

## Global options

These options can appear before or after the subcommand:

| Option | Meaning |
| --- | --- |
| `--json` | Emit a version-2 JSON envelope instead of human-readable output. |
| `--cwd <PATH>` | Resolve project/ancestor discovery from `PATH`. |
| `--config <PATH>` | Read configuration from `PATH` instead of the XDG default. |
| `-h, --help` | Print help. |
| `-V, --version` | Print the executable version. |

## Search

```text
skillwick search <QUERY>... [--limit <N>]
```

Search is explicit. It accepts one or more query words, defaults to five
results, and accepts `N` from 1 through 20. Search returns zero to `N` current,
enabled, model-discoverable candidates from the requested context. Exit code 0
also represents a valid no-match result.

Human output bounds each record independently and marks truncated descriptions;
it does not silently omit later requested candidates. JSON output returns the
complete requested result set.

## Read

```text
skillwick read <ID>
```

Read prints the selected live `SKILL.md`, its path, and package base. It first
checks enabled/model-discoverable state, canonical path identity, file size,
UTF-8, and content hash. A changed or unavailable source fails with exit code
3; it is never served as trusted cached content.

## Inspect

```text
skillwick inspect <ID> [--files]
```

Without `--files`, inspect prints indexed metadata and provenance. With
`--files`, it adds a bounded relative package listing. It does not read
supporting-file contents, follow symlinks, or execute package files. JSON adds
the package report and explicit truncation/count limits.

## List

```text
skillwick list
```

List prints the complete current-scope model-discoverable inventory and total.
It deduplicates equivalent filesystem/native paths for public output while
retaining raw provenance for diagnostics. `list` has no pagination or hidden
compatibility flags.

## Refresh

```text
skillwick refresh
```

Refresh rescans configured filesystem roots and, when configured, requests a
native Codex inventory. It publishes only after selected sources complete. A
native/provider failure preserves the previous valid cache and exits 3.

## Instructions

```text
skillwick instructions
```

Instructions prints the canonical compact agent context. It performs no setup
and writes no files.

## Init

```text
skillwick init [--yes] [--dry-run] [--agent <codex|none>]
    [--inventory <codex|filesystem>] [--catalog <auto|native|unchanged>]
    [--root <PATH>]... [--codex-home <PATH>] [--codex-bin <PATH>]
    [--instructions-file <PATH>]
```

Init configures Skillwick's own integration and inventory selection. `--dry-run`
previews without persistent writes. `--yes` accepts the setup confirmation.
`--root` adds an explicitly authorized discovery root and may repeat. Native
catalogue suppression is applied only after compatible inventory succeeds.

## Doctor

```text
skillwick doctor [--strict]
```

Doctor reports cache, integration, source, coverage, and compatibility state.
`--strict` returns exit code 3 when health is not complete.

## Uninstall

```text
skillwick uninstall [--purge-cache]
```

Uninstall removes Skillwick-owned integration. `--purge-cache` also removes the
disposable cache. Unrelated user-owned files remain untouched.

## Completions

```text
skillwick completions zsh
```

Completions prints Zsh completion definitions and does not modify shell files.

## JSON envelope

Every machine-readable command uses version 2. Search, list, and inspect return
a top-level object with a `version` field and command-specific fields. Search
and inspect include `results`; list includes `total` and `results`. `inspect
--files` additionally includes `package`, `counts_scope`, `counts`, and
`limits`. String fields are terminal-safe and JSON remains valid when metadata
contains control characters.

Example shape:

```json
{"version":2,"results":[{"id":"example@abcdef","name":"example"}]}
```

## Paths and exit codes

The default paths are documented in [operations](OPERATIONS.md#configuration-paths).

| Code | Meaning |
| ---: | --- |
| `0` | Success or valid no-match result. |
| `1` | Operational/database failure. |
| `2` | Invalid command, option, configuration, or limit. |
| `3` | Stale, disabled, denied, conflicting, incomplete, or provider state. |
