# Contributing

Skillwick v1 supports Codex on macOS. Changes for other coding agents or
operating systems need an explicit compatibility design and evidence before
they enter the supported surface.

## Development

Install stable Rust, then run:

```sh
make check
```

`make check` formats, lints, runs Rust tests, and exercises the filesystem CLI
and installer with temporary homes. Codex-specific checks are separate because
they require an installed compatible Codex CLI:

```sh
make test-codex
make test-inference
```

Neither command installs Skillwick into the current home. The integration test
uses isolated `HOME`, `CODEX_HOME`, and XDG directories. The inference test uses
the caller's existing Codex authentication but gives the model only a temporary
fixture library.

## Design rules

- Keep installed skills in their existing locations.
- Keep search lexical and local. Do not add MCP, embeddings, a daemon, or a
  package manager without a new product decision.
- Preserve unrelated user configuration during setup and uninstall.
- Add compatibility fixtures before changing the supported Codex version.
- Keep source, metadata, and policy errors visible. Never treat an incomplete
  scan as proof that a skill was deleted.

See [the design contract](docs/SPEC.md), [architecture](docs/ARCHITECTURE.md),
and [verification record](docs/IMPLEMENTATION.md).

## Pull requests

Keep changes focused. Include the motivation, behavioral impact, tests run, and
remaining limits. Use Conventional Commits for commit and pull-request titles.
