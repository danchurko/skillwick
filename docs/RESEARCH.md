# Source review — local skill discovery

Reviewed 11 September 2026. This is a targeted static source review, not a
runtime or security audit. Main-branch links can change; pinned release links
and the implementation specification record the source boundaries used here.

## Native integration

Codex 0.154.0 supports `skills.include_instructions = false`, which hides the
automatic catalogue without moving or disabling installed skills. Its native
`skills/list` interface is an inventory, not task-ranked search. Skillwick uses
that narrow, version-bound boundary for setup and refresh rather than guessing
private plugin cache layouts.

The [Codex configuration source](https://github.com/openai/codex/blob/rust-v0.154.0/codex-rs/config/src/skills_config.rs),
[skills extension](https://github.com/openai/codex/blob/rust-v0.154.0/codex-rs/ext/skills/src/extension.rs),
[catalogue rendering](https://github.com/openai/codex/blob/rust-v0.154.0/codex-rs/ext/skills/src/world_state_catalogs.rs),
and [native inventory schema](https://github.com/openai/codex/blob/rust-v0.154.0/codex-rs/app-server-protocol/src/protocol/v2/plugin.rs)
are the relevant dated references. New Codex versions need source review,
fixtures, and integration proof before native catalogue policy changes.

## Installer and ownership boundaries

The [OpenAI skill installer](https://github.com/openai/skills/blob/main/skills/.system/skill-installer/scripts/install-skill-from-github.py)
uses `--dest` for the destination and `--path` for a path inside the source
repository. These flags are not interchangeable, and the installer refuses an
existing destination instead of acting as a general in-place updater.

[Vercel skills](https://github.com/vercel-labs/skills) documents global, agent,
and copy selection, but this review did not establish a universal custom
destination flag. Native plugins may also own tools, configuration, credentials,
and package state. Skillwick therefore owns only its derived index, integration
files, and diagnostics; existing installers and Codex retain package and plugin
ownership.

## Safety conclusions

Search is local and metadata-only. It does not execute instruction files, move
installed packages, install dependencies, or grant permission to run tools.
Symlink targets and package references must remain within explicitly authorized
roots. Setup and uninstall preserve unrelated configuration and remove only
owned changes. Native inventory failures remain visible, and a failed refresh
must retain the previous successful snapshot.

Release packaging uses [cargo-dist](https://github.com/axodotdev/cargo-dist) for
macOS archives and per-file SHA-256 checksums. The repository installer verifies
the selected archive before installing it into an explicit prefix; Homebrew
installation does not edit a user's home directory. Signing and notarization
are not claimed unless a release verifies them.

This source review did not execute the inspected external implementations or
establish production readiness. Current behavior and supported versions belong
in [usage](USAGE.md) and [compatibility](COMPATIBILITY.md).
