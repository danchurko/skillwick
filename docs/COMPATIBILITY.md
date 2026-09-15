# Compatibility

Skillwick is a local filesystem tool. It does not require an agent server,
agent-specific inventory, plugin registry, or writable agent state. This page
records tested release boundaries; Rust source portability is not a release
promise.

## Calling workflows

| Workflow | Status | Evidence |
|---|---|---|
| Local shell and scripted CLI use | Supported | CLI, SQLite, trust-boundary, and source-install checks |
| Managed coding-agent integration | Supported where its owner can install the canonical context | Isolated integration checks and read-only local-corpus verification |
| Other agent products | Not separately supported | No product-specific setup or release test is claimed |

Skillwick does not decide which skills an agent may invoke. Invocation-policy
metadata controls model discoverability; reading a skill does not authorize
executing its scripts or changing user configuration.

## Operating systems

| Platform | Status | Evidence |
|---|---|---|
| macOS arm64 | Supported | Built and executed on the maintained local release host |
| macOS x86_64 | Built; hardware unverified | Archive build only |
| Linux | Not supported as a release artifact | No release target or runtime test |
| Windows | Not supported as a release artifact | No release target or runtime test |

Arm64 load commands target macOS 11.0. Intel load commands target macOS 10.12.
Those older releases were not runtime-tested, so they are build metadata rather
than a support claim.

For current commands and failure semantics, see the [reference](REFERENCE.md).
For installation, source ownership, and recovery, see [getting started](GETTING_STARTED.md)
and [operations](OPERATIONS.md).
