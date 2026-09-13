# Clean Codex evaluation profile

Verified against Codex CLI 0.154.0 on macOS. This is a manual preparation method,
not an evaluation runner. The calling agent owns the measured workflow.

## Create an isolated environment

Run these commands in one shell. Keep the printed profile path for the duration
of the evaluation; changing `CODEX_HOME` creates a different authentication scope.

```sh
evaluation_root=$(mktemp -d /private/tmp/skillwick-evaluation.XXXXXX)
for evaluation_dir in home codex config data cache state tmp work; do
  mkdir -p "$evaluation_root/$evaluation_dir"
done
chmod 700 "$evaluation_root" "$evaluation_root/codex"
printf '%s\n' "$evaluation_root"

clean_codex() {
  env -i \
    HOME="$evaluation_root/home" \
    CODEX_HOME="$evaluation_root/codex" \
    XDG_CONFIG_HOME="$evaluation_root/config" \
    XDG_DATA_HOME="$evaluation_root/data" \
    XDG_CACHE_HOME="$evaluation_root/cache" \
    XDG_STATE_HOME="$evaluation_root/state" \
    TMPDIR="$evaluation_root/tmp" \
    PATH=/opt/homebrew/bin:/usr/bin:/bin \
    LANG=en_US.UTF-8 \
    /opt/homebrew/bin/codex "$@"
}
```

The absolute Codex path above matches the verified installation. On another
machine, use that installation's executable and required runtime path explicitly.
Do not inherit the entire parent environment to make a failed probe pass.

## Authenticate this profile

The user must complete Codex's supported device login:

```sh
clean_codex login --device-auth -c cli_auth_credentials_store=file
clean_codex login status -c cli_auth_credentials_store=file
```

This lets Codex create credentials in the evaluation profile. Do not copy the
live profile's credentials or configuration. A fresh Codex home had no matching
keyring login in the probes; the fully isolated home could not access the default
keychain. File-backed authentication is the supported route for this environment.
Keep credentials and raw profile state out of repository artifacts.

## Verify before measuring

Use Codex's no-model prompt rendering and native inventory diagnostics to check
the final staged environment. The preparation must pass all of these checks:

- No personal global/project instruction markers, active hooks, personal custom
  agent settings, enabled MCP servers, memories, or inherited task state.
- Only explicit evaluation settings and the frozen skill corpus are present.
- The staged inventory matches the source corpus by name, instruction hash and
  recorded provenance mapping, with bundled skills and duplicates accounted for.
- Authentication succeeds in this exact environment.
- Runtime evidence confirms the intended model, settings and allowed actions.

The local inventory export is a separate read of the real installed environment.
Do it before isolating measured agents. Do not replace real skill instructions
with synthetic files assembled from descriptions. Do not silently accept a
reduced corpus after disabling plugins or changing homes.

For measured executions, use the same `clean_codex` environment with an empty
working directory, `--ignore-rules`, `--ephemeral`,
`--skip-git-repo-check`, `--sandbox read-only`,
`-c cli_auth_credentials_store=file`, and `-c project_doc_max_bytes=0`.
Set the model explicitly: Terra for the root, Luna for the researcher. Native
catalogue inclusion and Skillwick usage instructions are explicit differences
between evaluation arms, not inherited machine settings.

Load the audited `config.toml` from the clean Codex home. Do not use
`--ignore-user-config` when it would discard the staged plugin declarations and
MCP disablement. Configure only the copied skill packages, required plugin
identities, and evaluation settings. Do not copy the user's configuration.

Set `features.hooks=false` and verify the hook inventory. `features.apps=false`
does not disable plugin MCP servers: after staging, enumerate them and explicitly
disable each server, retaining a valid transport declaration. The installed CLI
rejected an `enabled=false` override without transport details. For the server
exposed by the tested corpus, this worked:

```toml
[mcp_servers."chrome-devtools"]
command = "npx"
args = ["-y", "chrome-devtools-mcp@latest"]
enabled = false
```

This is a disabled declaration, not an instruction to install or run that server.
Recheck that no server remains enabled and that the full skill inventory still
matches. Plugin hook/MCP files may remain as inert package resources; their
presence must not turn into runtime activation. Preserve required machine policy
and disclose it rather than attempting to bypass it.

`--ignore-rules` excludes execpolicy rules, not `AGENTS.md` instructions.
`project_doc_max_bytes=0` did not suppress `CODEX_HOME/AGENTS.md` in the probes,
so a clean Codex home is still necessary. A named Codex profile layers settings
over base configuration; it is not isolation. `--ephemeral` controls persistence.

The method has passed no-model isolation probes. A successful authenticated
Terra/Luna evaluation has not yet been run with this profile. See the
[current findings and remaining checks](design/current-direction.md).
