# Compatibility

Skillwick 0.4 targets macOS and Linux on ARM64 and x86_64. This is an unreleased
source revision; published 0.3 artifacts do not acquire these capabilities.
Windows is not a release target.

| Workflow | Contract |
|---|---|
| Shell, scripts and other agents | Explicit filesystem roots; no agent runtime required |
| Codex automatic discovery | Native skills plus active plugins selected by bounded `codex plugin list --json` |
| Claude automatic discovery | Native skills plus installed registry and effective enablement |
| Codex and Claude setup | Reversible owned context/reference writes; no prompt hooks |
| Workstation provisioning | Existing owner runs `instructions` and `init --agent none` |

The Codex plugin JSON and Claude registry fixtures capture the host schemas
observed on 2026-09-16. Unknown, incomplete or conflicting inputs fail visibly;
cache directories alone never establish an active plugin version.

Codex automatic discovery supports the exact Agent Plugins 1.0 root-manifest
schema before legacy `.codex-plugin/plugin.json`, verified against Codex
0.160.0. Native plugins use `skills/`; legacy overlay versions and paths do not
replace the root manifest's values. The native list's version selects the cache
directory and can differ from optional native manifest metadata. Legacy
manifest version equality and package-containment checks remain enforced.
See [real-source qualification](research/real-source-qualification.md).

| Platform | Release target | Runtime evidence |
|---|---|---|
| macOS ARM64 | `aarch64-apple-darwin` | Installed candidate gates passed 2026-09-16 |
| macOS x86_64 | `x86_64-apple-darwin` | Native CI configured; runtime not verified locally |
| Linux ARM64 | `aarch64-unknown-linux-musl` | CI build and installed fixture gate required |
| Linux x86_64 | `x86_64-unknown-linux-musl` | CI build and installed fixture gate required |

CI configuration is a requirement, not evidence that a particular run passed.
Older macOS releases, signing and notarization need separate release evidence.

Reading a skill does not authorize executing its scripts. Invocation policy
controls discoverability. See [reference](REFERENCE.md) for CLI contracts and
[operations](OPERATIONS.md) for ownership and recovery.
