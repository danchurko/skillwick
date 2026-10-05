# Compatibility

Skillwick 0.5.0 targets macOS and Linux on ARM64 and x86_64. Older published
artifacts do not acquire the current discovery and retrieval corrections.
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
| macOS ARM64 | `aarch64-apple-darwin` | Published 0.5.0 native CI and local installer passed 2026-10-05 |
| macOS x86_64 | `x86_64-apple-darwin` | Published 0.5.0 native Intel CI passed; no local Rosetta |
| Linux ARM64 | `aarch64-unknown-linux-musl` | Published 0.5.0 native CI and installer passed 2026-10-05 |
| Linux x86_64 | `x86_64-unknown-linux-musl` | Published 0.5.0 native CI and installer passed 2026-10-05 |

The [0.5.0 release run](https://github.com/danchurko/skillwick/actions/runs/37278751116)
verified both staged and published archives on all four native platforms. Local
macOS ARM64 verification additionally checked every archive digest/layout and
the isolated installer. This host cannot execute Intel binaries without Rosetta;
the native Intel CI result supplies that platform proof.
CI configuration alone is not evidence that a particular run passed.
Older macOS releases, signing and notarization need separate release evidence.

Reading a skill does not authorize executing its scripts. Invocation policy
controls discoverability. See [reference](REFERENCE.md) for CLI contracts and
[operations](OPERATIONS.md) for ownership and recovery.
