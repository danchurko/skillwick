# Contributing

Skillwick v1 supports Codex on macOS. Changes for other coding agents or
operating systems need an explicit compatibility design and evidence before
they enter the supported surface.

## Development

Install stable Rust, then run:

```sh
git clone https://github.com/churdaa/skillwick.git
cd skillwick
cargo build --release --locked
./target/release/skillwick --version
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

See [the implementation specification](docs/design/discovery-spec.md), [architecture](docs/ARCHITECTURE.md),
and [verification record](docs/IMPLEMENTATION.md).

Keep the CLI's search path explicit and local. Tests should use temporary homes
and fixture skill packages for filesystem and Codex integration coverage; do not
modify a developer's installed skills or persistent configuration.

## Pull requests

Keep changes focused. Include the motivation, behavioral impact, tests run, and
remaining limits. Use Conventional Commits for commit and pull-request titles.

## Release maintenance

Publish the arm64 and x86_64 archives with their per-file SHA-256 checksums.
Keep build manifests in CI and use `scripts/install.sh` as the script installer.
Update the Homebrew formula from verified published archive digests.

The generated release workflow has one deliberate customization: the publication
allowlist. `allow-dirty = ["ci"]` preserves it, so `dist generate --check` alone
does not verify workflow consistency. After changing cargo-dist configuration,
regenerate CI in a temporary copy without that exemption and verify that only
the publication cleanup step differs.
