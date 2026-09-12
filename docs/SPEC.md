# Skillwick — implementation specification

Status: implemented v1 design contract.
Research date: 11 September 2026. Target: lexical v1, macOS only, Codex only.
Repository and binary working name: `skillwick`. Tagline: **Find the skill. Load only what matters.**

## 1. Outcome and boundaries

Build a small native CLI that searches an existing local skill library and returns a few relevant descriptions. An agent then deliberately reads selected instructions. Keep skills and complete packages in the locations their installers own.

The normal interaction is:

```sh
skillwick "deploy an AgentCore MCP server with TypeScript"
skillwick read aws-agentcore@7d92ac
```

The design must save catalogue tokens without turning into a plugin manager, agent harness, permissions engine, or hosted service. No Codex fork, MCP server, embedding service, permanent watcher, background daemon, automatic skill installation, or script execution in v1. Do not rewrite third-party `SKILL.md`, `agents/openai.yaml`, plugin manifests, or installer lockfiles. Do not promise that a few hundred lines can cover the entire installation and recovery contract.

Only derived metadata is copied into the index. Original instructions, scripts, references, and assets remain together. Search does not execute instructions or grant permission to run tools.

## 2. Verified native integration: do not move the library

The reviewed Codex **release tag `rust-v0.154.0`**, not merely its development branch, contains:

```toml
[skills]
include_instructions = false
```

Its config type, thread-context implementation, and world-state rendering distinguish hiding the automatic catalogue from disabling installed skills. Explicit mentions remain a separate path. Native `skills.list`/`skills.read` tools also exist in some runtime contexts, but the inspected list schema is paginated enumeration, not task-ranked retrieval. These facts are the reason for building a small search layer, not replacing native installation. [S1–S5]

Use this setting on verified compatible Codex versions. Place a tiny search instruction in the active global instructions file so discovery does not depend on the hidden catalogue. Install one router skill for explicit invocation and fuller guidance; its metadata need not be injected because the global instruction already names the command.

Important limits:

- This hides the skills catalogue, not every plugin/tool schema or all startup context. Local discovery work may still happen inside Codex.
- Existing sessions may retain already-injected text. Verify the result in a new session.
- Do not substitute `enabled = false` for catalogue suppression. Disabled skills must stay disabled.
- `allow_implicit_invocation` belongs in a skill's `agents/openai.yaml`; repeatedly patching installer-owned copies is not the default strategy. [S6]
- Version 0.154.0 is a verified reference, not a claim about the earliest supported release or the user's installed desktop build. CLI and desktop compatibility must be reported separately.

### Compatibility contract

Ship a small, documented compatibility table and release-tagged protocol fixtures. Detect the actual Codex executable and version. Confirm known configuration semantics; accepting an unknown TOML key without an error is not proof of support. Unknown builds default to **discovery-only** mode, leaving catalogue settings untouched and reporting the limitation. `--catalog native` must fail clearly when unsupported rather than silently degrade. Never download or patch Codex to make integration work.

Before enabling native suppression, verify that the router instruction is in the effective instructions file and that at least one configured source can be indexed. Install guidance first and change catalogue policy last. A failed setup must not leave the user without either normal discovery or the replacement route.

## 3. Architecture and ownership

```text
Existing installers / native plugins
             │ own packages and updates
             ▼
Existing local skill locations ──► source inventory
                                      │
                              parse bounded metadata
                                      ▼
                               SQLite + FTS5
                                      │
Global AGENTS instruction ──► skillwick "task"
                                      │
                              0–5 compact candidates
                                      ▼
                           skillwick read SELECTED_ID
                                      │
                      live instruction file + base directory
```

Ownership is explicit:

| Owner | Owns |
|---|---|
| Existing installers / workstation manager | Skill packages, symlinks, versions, plugin state, installer locks |
| Codex | Native discovery, plugin enablement, tool availability, permissions, explicit skill invocation |
| Skillwick | Search index, its config, its router, its marked instruction block, and recorded integration edits |
| The agent | Relevance judgment and execution within existing authorization |

Use a single Rust package with small modules: `cli`, `sources`, `metadata`, `index`, `search`, `output`, `integration`, `doctor`. Add a narrowly scoped Codex inventory adapter; do not construct a general RPC framework or plugin framework.

### Implementation libraries

Use `clap` for CLI parsing; `rusqlite` with bundled SQLite for persistence/FTS5; `serde`/`serde_json` for internal data; `toml_edit` to preserve unrelated TOML; `inquire` for the wizard; a bounded YAML parser such as `serde-saphyr` for frontmatter; and standard filesystem utilities plus a hash library. Verify dependency maintenance/licensing and commit `Cargo.lock`. No async runtime is required solely for local SQLite or file scanning. Do not implement YAML, SQL ranking, terminal prompts, or release packaging from scratch. [S10–S13]

Use `dist` (the `cargo-dist` project) to generate release packaging and CI where it fits. Keep setup/instruction editing in the application, not in a package-manager post-install hook. [S14]

## 4. Source discovery and plugin inventory

### Filesystem sources

Support the standard global `.agents/skills` tree and current-project/ancestor `.agents/skills` locations according to the supported Codex discovery rules. Additional local roots are explicit, repeatable setup options. Respect `CODEX_HOME`, XDG overrides for Skillwick's own files, and paths containing spaces. Do not assume every harness discovers the same paths.

Do not recursively scan the entire home directory, every repository, or every version in a plugin cache. Search results must include only global sources and sources applicable to the current working directory. A skill from project A must not leak into project B's results.

Follow legitimate installer-managed skill-directory symlinks, preserve the visible path and canonical target, detect cycles, and deduplicate identical canonical documents. Explicitly authorize external symlink target roots during setup or through native inventory; a symlink is not permission to scan an arbitrary directory. Reject unexpected escapes and revalidate targets before reading.

### Native Codex inventory

For Codex-owned/plugin-owned sources, prefer the existing app-server `skills/list` RPC over guessing private cache layouts. Its reviewed schema accepts `cwds` and `forceReload`; metadata includes `path`, `scope`, `enabled`, `pluginId`, and dependencies. [S7–S8]

Implement a **short-lived local inventory subprocess** during `init`/`refresh`: initialize the installed app server over its documented stdio transport, request inventory for the relevant cwd with `forceReload: true`, consume the response, and terminate cleanly. Do not start a model turn, launch an agent session, request installation/reconciliation, or expose this RPC to the model. This is neither an MCP server nor a permanent service. Handle interleaved notifications, bounded messages, request IDs, stderr, timeout, EOF, and child cleanup.

Record snapshots by executable/version, Codex home, and cwd scope. Native enablement is authoritative for native records; never resurrect a native-disabled record through the filesystem adapter. Do not load an old cached plugin version merely because its directory still exists.

Steady-state search must not launch Codex on every query. Refresh native inventory after installation/update, during explicit `refresh`, and when a previously unseen workspace needs its native scope. When runtime permissions prevent reconciliation, return an explicit coverage/staleness diagnostic rather than silently broadening discovery. A complete native inventory failure must not destroy the previous successful snapshot.

Ordinary `.agents/skills` changes are detected by local scanning. A newly added native plugin with a new root becomes searchable after native refresh; the workstation pipeline must call it after installs. Out-of-band plugin installs therefore have a documented one-command recovery: `skillwick refresh`. Before `read` of a native/plugin record, revalidate current enablement and the active path through a fresh native inventory or a proven equivalent supported adapter; do not silently load stale policy state.

Core search has no network client. The optional inventory child is the user's installed Codex and remains subject to Codex's own settings; do not claim the entire subprocess tree is network-free without testing it. A filesystem-only profile must work completely offline without Codex.

### Format and identity

Primary file format is Agent Skills `SKILL.md` with YAML frontmatter. Use native resolved metadata where available, including its current short-description/interface fields. Do not invent the schema of newer manifests; unsupported native formats must be counted and diagnosed, not silently included as successful coverage.

Identity is independent of instruction contents: stable source identity plus package-relative location, with a short collision-checked display ID such as `aws-agentcore@7d92ac`. A content edit must not change the identity. Versioned native cache paths may require plugin identity + package-relative path. Canonical paths deduplicate symlink aliases; identical text alone does not prove two packages have identical assets. Preserve native precedence and show genuinely distinct variants instead of merging by name.

## 5. Indexing, freshness, and search

Store metadata, provenance, file identity, hashes, source scope, policy state, and FTS data in **one SQLite database**. Keep source/integration configuration outside this disposable cache. Use transactions to update metadata and FTS together. An external-content FTS table must have correctly tested synchronization; a simple ordinary FTS table is acceptable at this scale.

Index names, descriptions, optional declared keywords, and native short descriptions. Do not rewrite upstream descriptions. Do not embed or index entire instruction bodies in the primary v1 retrieval field. Missing/invalid descriptions are diagnostics; an explicit filename/name fallback can be searchable but must be marked degraded. Optional heading/body fallback belongs to the measured roadmap.

Parse bounded metadata: proposed limits are 64 KiB frontmatter, 8 KiB description, and 1 MiB instruction file. Handle BOM, CRLF, Unicode, folded YAML, invalid YAML, duplicate keys, and alias/depth limits deliberately. Limits are product choices, not upstream format guarantees.

On search, perform a lightweight scan of configured filesystem roots. Use path, file identity, size and nanosecond timestamps to detect changes, then hash changed documents. `refresh --full` rehashes everything, including files whose timestamps were preserved. Directory timestamps alone are insufficient. Only remove records after a successful complete scan of their source; a permission error or temporarily missing root during an update is not proof of deletion. Do not serve a deleted/unreadable selected file as current.

A read-only sandbox must still be usable. Prefer a readable existing cache; allow a bounded in-memory refresh when the durable cache is not writable. Do not request a broad home-directory write grant just to maintain derived metadata. Bound SQLite lock waits and handle corruption with a rebuild recommendation or safe disposable-cache recovery, never deletion of source files.

### Lexical algorithm

1. Normalize the query and tokenize safely; preserve useful technical identifiers and tested aliases (`C++`, `C#`, `.NET`, `Node.js`, hyphenated names, acronyms).
2. Construct escaped FTS expressions from tokens. Parameterized SQL alone does not make raw FTS syntax safe or user-friendly. Natural language is not an advanced query language.
3. Retrieve with weighted BM25 over the metadata fields. Start with higher name weight than description; record the actual values in config and tests. SQLite's better BM25 matches have **lower** scores. [S10]
4. Include explicit exact-name and useful term-coverage signals with deterministic tie-breaking. Avoid requiring every filler word in a natural-language question to match. Apply source/enablement filters before the final result limit.
5. Return at most five candidates. No fabricated percentages or universal relevance thresholds. The agent selects zero, one, or a few skills. A generic low-quality hit is not an instruction to load it.

Use an internal candidate pool larger than the output limit, but do not hide policy filtering behind a fixed oversampling heuristic that can exclude valid matches. No query-result cache is necessary in v1. If added later, keys must include the complete normalized query, source scope, index generation, filters, and ranking version—not a truncated prefix.

## 6. CLI and output contract

All examples below describe the **intended implemented interface**, not a currently published binary.

| Command | Behavior |
|---|---|
| `skillwick "query"` | Default lexical search; limit 5, compact text |
| `skillwick search "query"` | Explicit alias; useful when the query begins with a reserved command |
| `skillwick read ID` | Read the current selected instruction file, with source/base path |
| `skillwick inspect ID` | Metadata, origin, enablement, dependencies, hashes, and paths; no full body |
| `skillwick list` | Exhaustive current-scope inventory and total count; hidden `--limit N` and `--all` compatibility flags remain accepted |
| `skillwick refresh` | Refresh local index and configured native inventory; never update/install packages |
| `skillwick refresh --full` | Hidden compatibility alias for `refresh` |
| `skillwick benchmark --dataset PATH` | Evaluate current production ranking with labelled relevance judgments |
| `skillwick init` | Interactive setup wizard; repeatable and idempotent |
| `skillwick doctor` | Read-only health, compatibility, source coverage, staleness, integration drift |
| `skillwick uninstall` | Remove owned integration; preserve skills, unrelated config, and binary |
| `skillwick completions zsh` | Emit shell completion definitions |

Support `--help`, `--version`, `--config PATH`, `--cwd PATH`, and explicit `--json` for machine consumers. Search also supports `--limit N`. Treat multiple positional words as one query. `skillwick -- init hooks` must search literally rather than invoke setup. No arguments show short help and never mutate anything. Unknown flags fail with a useful message.

Default stdout, using illustrative records:

```text
aws-agentcore@7d92ac [global] Deploy and debug AgentCore runtimes and MCP endpoints.
mcp-typescript@51b408 [plugin:mcp] Build MCP servers using the TypeScript SDK.
```

Search has no banner, echoed query, pretty JSON, confidence number, install command, repeated full paths, or decorative table. Use one bounded description line per candidate; target a default output cap of 2,000 UTF-8 bytes, with complete IDs and valid UTF-8. Do not truncate an ID or print half a record. Result count may be below five. Empty search output is `No matching skills.` List reports the authoritative current-scope count and prints every compact ID and scope; its hidden `--limit N` compatibility option may disclose a bounded result set.

`read` prints a small header containing the live document path and base directory, then the full instruction document. Relative references resolve against that base, not the shell cwd. Reading does not `cd`, execute a script, auto-install dependencies, or activate plugin tools. For oversized documents, fail clearly or require explicit line-range reading; never silently present a truncated body as complete.

Errors and operational warnings go to stderr. JSON mode uses a small versioned schema and preserves provenance/staleness fields; JSON is an opt-in interface, not the default token cost. Strip terminal control sequences from metadata presentation. Document exit codes: 0 successful operation (including a search with no matches); 1 operational failure; 2 usage/configuration error; 3 stale, disabled, or conflicting requested resource. `doctor --strict` fails on incomplete required integration/coverage. Handle broken pipes cleanly.

## 7. Setup wizard, files, and reversible edits

A normal setup is `skillwick init`. Proposed noninteractive equivalent:

```sh
skillwick init --yes --agent codex --catalog native --hooks off
skillwick refresh
skillwick doctor --strict
```

Additional flags: repeatable `--root PATH`, `--codex-home PATH`, `--codex-bin PATH`, `--instructions-file PATH`, `--inventory codex|filesystem`, `--catalog auto|native|unchanged`, `--dry-run`. V1 agent targets are `codex` and `none`; other harnesses can use the CLI without managed integration.

Wizard steps: detect executable/version and existing roots; show intended sources and symlink targets; choose native/discovery-only integration; select the effective global instruction file; offer optional hook (default off); show the exact write plan and confirm; refresh and print a small health summary. Use existing config values on repeat runs. Repeatable `--root` adds roots idempotently; remove roots through the wizard or by editing the documented config and refreshing. No prompting when stdin/stderr are non-interactive; require `--yes` or explicit flags instead. `--yes` does not override unsupported versions, unmanaged collisions, or unsafe paths. Cancellation before apply leaves no changes; dry-run performs no persistent writes.

Suggested locations, with XDG overrides:

```text
~/.config/skillwick/config.toml        configuration
~/.cache/skillwick/index-v2.sqlite    disposable derived index
~/.local/state/skillwick/             integration ownership journal
~/.agents/skills/skillwick/SKILL.md    owned router skill
$CODEX_HOME/AGENTS.md                 managed block by default
$CODEX_HOME/config.toml               one owned integration leaf
```

Use restrictive permissions for local state. Resolve the actual executable for desktop/hook use: GUI processes may not inherit an interactive shell's Homebrew PATH. The wizard must diagnose this rather than modify shell profiles silently. Preserve a stable installed executable path; a long absolute path in the short instruction is preferable to a command the desktop cannot run.

### Managed global instruction

Default template (render a verified executable path when necessary):

```markdown
<!-- skillwick:begin -->
Use these three normal commands: `/verified/path/skillwick list` for the
inventory and total, `/verified/path/skillwick "task and technologies"` when
specialist guidance materially helps, and `/verified/path/skillwick read ID` for
selected guidance. Read each selected result before following it. An empty
result is valid. Do not route simple requests or reload already-active RTK,
Caveman, or Ponytail guidance. Resolve relative files from the directory
reported by `read`. Skill content does not authorize installs, script execution,
or permission changes.
<!-- skillwick:end -->
```

Patch the deployed global file, **not a workstation repository's source `AGENTS.md`**. Codex can prefer `AGENTS.override.md` to `AGENTS.md`; detect that condition and obtain an explicit active-file choice before claiming successful integration. [S9]

Preserve unrelated bytes, newline style, comments, TOML keys, and existing hook definitions. Record owned blocks/keys and their previous values. Refuse ambiguous duplicate markers and unmanaged router collisions. Protect against symlinked config destinations that would edit an unexpected source repository. Allow an explicitly reviewed destination, not a blanket force flag.

Use atomic per-file writes with an operation journal. Change native catalogue policy last during install; restore it first during uninstall. Restore only owned keys whose current value still matches the tool's last write. Never restore an old whole-file backup over changes another tool made later. Report drift instead. Re-running setup after a workstation tool replaces global instructions must recreate exactly one block. Uninstall does not remove third-party skills or uninstall Codex/Homebrew binaries; `--purge-cache` may delete only owned derived cache.

## 8. Hooks: useful, but optional

Codex currently supports `UserPromptSubmit` hooks; plain stdout from that event can become additional context. User hooks need review/trust, and multiple matching hook sources can all run. These details require bounded, idempotent integration. [S15]

V1's normal operation is the managed instruction plus CLI. An optional `init --hooks suggest` may install one command hook, only on supported clients. Keep this a small adapter using the same search core, not a second retrieval implementation. `--hooks off` is the default and must remove only a previously owned suggestion hook when explicitly requested.

The hook reads bounded JSON from stdin, takes `prompt` and `cwd`, and emits at most three compact candidate records from the existing cache. It never interpolates prompt text into a shell command, launches Codex, refreshes a library, reads full instructions, persists the prompt, changes permissions, or blocks a turn. Treat returned metadata as retrieval data, not higher-priority instructions. Empty results, invalid input, missing cache, timeout, or internal errors yield no context and exit 0. Set a short outer timeout and an internal deadline that exits sooner. Preserve Codex's trust review; never set bypass-trust flags.

Do not use SessionStart to move skills or rewrite configuration: it is unnecessary and ordering relative to catalogue construction must not be assumed. Do not enforce “search before every tool” through PreToolUse. PostToolUse cannot observe installs performed outside that agent session. There is no assumed universal plugin-install hook; the workstation's explicit post-install `refresh` is the update boundary.

## 9. Installation, distribution, and workstation integration

Ship native macOS arm64 and x86_64 binaries with bundled SQLite; users should not need Rust, Node, Python, Ollama, or Docker to run search. Linux can be added when CI supports it. State the actual minimum tested macOS version. Do not claim Apple signing/notarization unless implemented.

Prepare a public GitHub release workflow with versioned archives, SHA-256 checksums, a pinned release-tool version, and least-privilege CI. Prefer generated packaging from `dist`; inspect its output. Prepare a Homebrew tap/formula and an explicit version-selectable shell installer. Formula install must not edit a user's home directory. The documented installation flow is binary installation **followed by `skillwick init`**, whose default integration includes the global instruction block. [S14]

Homebrew interface from this repository:

```sh
brew tap danchurko/skillwick https://github.com/danchurko/skillwick.git
brew install skillwick
skillwick init
```

The formula uses versioned GitHub release archives. Update its digests from the
exact published artifacts before tagging a release.

For an existing workstation pipeline, integrate after base instructions have been copied, all skills/plugins installed, and the final baseline config imported:

```sh
skillwick init --yes --agent codex --catalog native --hooks off
skillwick refresh
skillwick doctor --strict
```

Run setup on every apply, not only when the binary is first installed. The sequence repairs a replaced instruction block and indexes the latest packages. Preserve inspect/dry-run behavior. Existing RTK and other integrations must survive unchanged. Keep one owner for each configuration key; do not have the workstation baseline and Skillwick fight over the same value.

Installer compatibility evidence: OpenAI's skill installer uses `--dest` for destination and `--path` for repository-internal source paths. Vercel skills documents agent/global/copy selection; custom destination flags must not be assumed. Existing isolated-HOME staging can remain in its established adapter, but do not apply that trick blindly to native plugin managers that also use HOME for credentials/state. With native catalogue suppression, destination redirection is normally unnecessary. [S16–S17]

## 10. Verification and acceptance

Use synthetic fixture skill packages, temporary homes, mocked native RPC streams, and snapshot output tests. Never run setup tests against the developer's real home. Static source review is not execution evidence.

Required acceptance coverage:

| Area | Required cases |
|---|---|
| Discovery | Global/project scope, ancestor rules, custom roots, legitimate symlinks, cycles, aliases, duplicate names, no cross-project leakage |
| Native state | Enabled/disabled skill, inactive plugin/cache version, inventory errors, scope changes, policy changed before read, unsupported format/version |
| Update lifecycle | New/changed/deleted files; timestamp-preserved edit with `--full`; package replacement; new native plugin after refresh; interrupted/missing source without destructive purge |
| Search | Exact names, acronyms, paraphrases with lexical overlap, punctuation, quotes, C++/C#/.NET, Unicode, no matches, stable ties, filters before limit |
| Presentation | Default query syntax, reserved-command escape, bounded text, optional JSON, no scores-as-probabilities, stderr separation, full body/base-directory output |
| Setup/recovery | Fresh init, repeated init, cancellation, dry-run, CI `--yes`, inactive AGENTS.md due to override, unknown Codex, conflicting marker/symlink, uninstall after unrelated edits |
| Workstation | Global instructions overwritten then reinitialized, RTK block retained, final baseline merge, existing skill ownership/locks unchanged |
| Runtime | Read-only cache/home, concurrent readers/refresh, busy database, corrupted cache, missing executable, desktop PATH, native child timeout/notification/EOF cleanup |
| Hooks | Disabled by default, no duplicate entry, trust retained, stdin injection attempts, failure-open, bounded cached-only output |
| Distribution | Both Mac architectures build; archive/checksum smoke tests; installer prefix/version; no source files touched; Homebrew install plus explicit init |

Prove catalogue suppression and explicit skill availability with supported Codex integration tests/request fixtures on a new session. Where a real model run would incur usage or require credentials, keep it opt-in and report it as not run. Do not claim token savings from a generic multiplier; compare actual prompt/request content with and without the integration in the tested client.

Create a labelled lexical evaluation set of at least 40 representative tasks, including exact technologies, ambiguous/general tasks, and no-skill-needed cases. Report Recall@5 on tasks with labelled relevant skills, false-positive/irrelevant suggestions, and result/output sizes. Keep held-out cases separate from tuned examples.

Performance goals, not measured claims: warm search p95 under 150 ms at 1,000 metadata records on a documented Mac; benchmark 10,000 records too. Measure executable startup, scan/refresh, query, and native inventory separately. A slow native refresh must not be concealed inside a “search latency” claim. Record actual hardware, corpus, run count, cold/warm conditions, and failures.

## 11. Milestones and definition of done

**M0 — compatibility and fixtures:** confirm the installed/release Codex contract; write source/ownership/format fixtures and a smoke-test plan. This is a bounded check, not an excuse to redesign the project.

**M1 — lexical vertical slice:** scan → metadata → SQLite FTS5 → default search → read. Test real package-relative references. Deliver a useful binary before polishing integrations.

**M2 — reliable lifecycle:** native inventory adapter, freshness, policy checks, init wizard, managed instruction/config edits, doctor/uninstall, workstation simulation. Add the optional cached suggestion hook only after the normal path passes.

**M3 — distributable v1:** native Mac artifacts, generated release packaging/tap recipe, installation docs, evaluation results, and complete verification report. Stop at a reviewable release candidate; publishing is a separate authorization.

Definition of done: all implemented commands documented; core acceptance tests pass; both targeted architectures have honest build/test status; known limitations recorded; no MCP/fork/daemon/embeddings; no third-party file relocation; no real workstation modification from tests; no unapproved publication.

## 12. Future development — gated by evidence

**Lexical refinement:** improve metadata aliases and query normalization from observed misses; consider bounded low-weight heading/body fallback for poor descriptions. Measure precision before making it default. Do not require editing upstream skill files.

**Hybrid retrieval:** add only when held-out tasks show meaningful semantic misses. Keep the lexical engine independently usable. Embed compact discovery metadata; key vectors by model/version, preprocessing, dimension, and content hash. Re-embed only changed documents. Prefer optional local inference and no mandatory daemon. Fuse lexical and semantic ranked lists with an evaluated method such as reciprocal-rank fusion; do not add raw BM25 and cosine with arbitrary percentages. Compare quality, cold startup, memory, latency, and download size against lexical v1.

**Reranking:** first let the existing agent choose from compact candidates. A separate cross-encoder is justified only if it improves end-to-end selection enough to cover its cost. A hosted or LLM selector must never silently receive local prompts.

**Adaptive hooks:** evaluate candidate usefulness and additional prompt bytes across real sessions; retain manual discovery for follow-up turns, delegated subtasks, and domain changes. Do not infer hooks understand a whole conversation from the latest prompt alone.

**Other harnesses:** add small native inventory/instruction adapters for OpenCode/Claude only when requested; preserve the same CLI and core index. A new harness should not require a new storage system.

**Maintenance:** add compatibility fixtures for new Codex releases. If Codex gains suitable native ranked search, reuse it or retire redundant integration. Signing/notarization, additional package managers, and optional filesystem watching need specific demand. Do not add a marketplace, approval workflow, telemetry service, or scheduling subsystem by default.

## Sources

Reviewed source is evidence of the dated implementation, not permission to copy code or mutate a workstation. Further source-level findings are in `RESEARCH.md`.

- [S1 — Codex 0.154.0 skill config](https://github.com/openai/codex/blob/rust-v0.154.0/codex-rs/config/src/skills_config.rs)
- [S2 — Thread-context integration](https://github.com/openai/codex/blob/rust-v0.154.0/codex-rs/ext/skills/src/extension.rs)
- [S3 — World-state catalogue rendering](https://github.com/openai/codex/blob/rust-v0.154.0/codex-rs/ext/skills/src/world_state_catalogs.rs)
- [S4 — Hidden catalogue / explicit mentions](https://github.com/openai/codex/blob/rust-v0.154.0/codex-rs/ext/skills/src/world_state.rs)
- [S5 — Native list schema](https://github.com/openai/codex/blob/rust-v0.154.0/codex-rs/ext/skills/src/tools/list.rs)
- [S6 — Per-skill OpenAI metadata](https://github.com/openai/codex/blob/c62d191c4c8c0cab7045fca6efc399197334bb6c/codex-rs/skills/src/assets/samples/skill-creator/references/openai_yaml.md)
- [S7 — Native inventory RPC](https://github.com/openai/codex/blob/rust-v0.154.0/codex-rs/app-server-protocol/src/protocol/v2/plugin.rs)
- [S8 — Native skill metadata](https://github.com/openai/codex/blob/rust-v0.154.0/codex-rs/app-server-protocol/schema/typescript/v2/SkillMetadata.ts)
- [S9 — Global instruction precedence](https://developers.openai.com/codex/agent-configuration/agents-md)
- [S10 — SQLite FTS5](https://www.sqlite.org/fts5.html)
- [S11 — rusqlite bundled SQLite](https://docs.rs/crate/rusqlite/latest/source/Cargo.toml.orig)
- [S12 — inquire prompts](https://docs.rs/inquire/latest/inquire/)
- [S13 — Bounded YAML parsing](https://docs.rs/serde-saphyr/latest/serde_saphyr/fn.from_reader_with_options.html)
- [S14 — dist release tooling](https://github.com/axodotdev/cargo-dist)
- [S15 — Codex hook contract](https://developers.openai.com/codex/hooks/)
- [S16 — OpenAI installer destination/source flags](https://github.com/openai/skills/blob/main/skills/.system/skill-installer/scripts/install-skill-from-github.py)
- [S17 — Vercel skills CLI](https://github.com/vercel-labs/skills)
