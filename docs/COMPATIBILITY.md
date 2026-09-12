# Compatibility

Skillwick v1 has one supported integration and one supported operating-system
family. Other environments are not implied by portable Rust source.

## Coding agents

| Agent | Status | Evidence |
|---|---|---|
| Codex CLI 0.154.0 | Supported | Real `skills/list`, `debug prompt-input`, setup, doctor, and uninstall tests |
| Other Codex CLI versions | Discovery only | Native catalogue changes refused until a version fixture is added |
| Codex desktop app | Not separately supported | Desktop and CLI builds may differ |
| Claude Code | Not supported | No inventory or integration adapter |
| OpenCode | Not supported | No inventory or integration adapter |
| Cursor and other agents | Not supported | No inventory or integration adapter |

## Operating systems

| Platform | Status | Evidence |
|---|---|---|
| macOS arm64 | Supported | Built and executed on macOS 26.6.2 Apple Silicon |
| macOS x86_64 | Built; hardware unverified | Mach-O archive executed through Rosetta |
| Linux | Not supported | No release target or runtime tests |
| Windows | Not supported | No release target or runtime tests |

Arm64 load commands target macOS 11.0. Intel load commands target macOS 10.12.
Those older releases were not runtime-tested, so they are build metadata rather
than a support claim.

## Codex contract

The supported 0.154.0 adapter uses newline-delimited JSON-RPC over stdio:
`initialize`, `initialized`, then `skills/list` with `forceReload: true`. It
ignores notifications until the matching response ID arrives and always
terminates the short-lived child.

Compatibility is based on the detected CLI executable. New Codex versions need
a release-tagged fixture, source review, and real integration proof before
Skillwick changes native catalogue policy.

Codex 0.154.0 hook integration is also covered with a temporary `hooks.json`.
The test keeps an existing `UserPromptSubmit` handler, adds Skillwick as a
second handler, verifies Codex `hooks/list` returns both, validates Skillwick's
JSON context, and removes only Skillwick on uninstall. Users still review and
trust the installed handler through Codex.
