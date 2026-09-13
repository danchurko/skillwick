# Shared understanding

Status: shared understanding confirmed by the user on 13 September 2026.
This direction supersedes the previous direction.
Implementation has not started.
The [implementation specification](https://github.com/churdaa/skillwick/issues/8)
is published with the `ready-for-agent` label and is authoritative for delivery.
It supersedes issue #1; this document records the confirmed decisions and findings.

## Requirements stated by the user

- Skillwick should be a simple tool that fits into a user's working environment.
- Agents should invoke it deliberately for relevant tasks. Automatic prompt
  suggestions must not stand in for a working CLI.
- Remove benchmarking and evaluation machinery, artifacts, procedures, and
  comparative claims from the maintained repository. Retain only a documented
  gap: comparative retrieval quality, context savings, and task-success benefits
  have not been established for the supported product and workflow.
- Fix defects at their responsible boundary and retain correctness checks.
- Keep portable product behavior independent of mac-state installation choices
  and personal agent orchestration policy.

## Decisions confirmed in the interview

- Support Codex on macOS first, independently of mac-state.
- Retain explicit reversible setup for ordinary users and externally managed
  configuration for managed environments. Each file or setting has one owner.
- Remove the automatic prompt-suggestion hook from the product and mac-state
  integration, preserving unrelated hooks and user-owned configuration.
- Require explicit `skillwick search "task"`. Remove shorthand interpretation
  rather than adding special handling for unknown command words.
- Keep delegation outside shipped Skillwick instructions. The calling workflow
  chooses which agent searches; searching alone does not require a subagent.
- Retain native catalogue suppression in previewed, reversible setup. Managed
  environments own the equivalent Codex setting themselves.
- Keep searches local with explicit inventory refresh. Setup and mac-state run
  refresh after installation changes. Changes to native enablement require
  refresh too; the snapshot is not a claim of live native state.
- Report missing or incompatible inventory clearly. Do not silently represent
  missing workspace coverage as a complete no-match result.

## Findings verified in the current source

- mac-state installs its base `AGENTS.md`, then Skillwick appends an owned
  reference to that same file. Managed setup should assign that reference to
  mac-state rather than requiring two configuration writers.
- mac-state owns release selection, binary verification, installed capability
  roots, and apply ordering. Skillwick has no need to know those local paths.
- `src/doctor.rs` requires nonempty filesystem discovery even when Codex native
  inventory is selected. This source-level coupling can reject native-only
  installations; a native-only fixture should verify the corrected contract.
- Installed product instructions prescribe a discovery subagent, up to three
  search perspectives, and a root-agent instruction read. This is workflow policy
  in `assets/skillwick/SKILLWICK.md`, not a requirement of the search implementation.
- `src/cli.rs` runs prompt suggestions through a separate hidden hook command.
  It searches submitted prompt text and emits Codex additional context.
- mac-state `agents/apply.sh` explicitly enables suggestions to run discovery
  before a failed command joined with `&&` can prevent it. This changes invocation
  policy; it does not fix shell output. Independent discovery belongs in an
  independent agent tool call.
- In an isolated filesystem fixture, direct search, a successful shell chain,
  and the same successful chain through RTK produced identical candidate text
  without hooks. A failing left command correctly prevented the search.
- Shorthand `probe --limit 5` returned no matches where explicit
  `search probe --limit 5` returned the fixture's exact match. The shorthand
  parser captures the option as query text.
- A first candidate whose rendered line exceeds 2,000 bytes produced empty text
  output with exit code zero. JSON still contained the candidate. The early break
  in `src/output.rs` owns this defect; its connection to the historical shell
  report has not been established.

These are bounded local reproductions, not a complete release certification.
The structural index is stale; findings use direct source reads. The linked
Codex task was readable. Initial GitHub access failed; issue state was subsequently
verified during specification publication.

## Agreed responsibility boundary

- Skillwick owns its search and read contract, derived inventory, compact usage
  instructions, explicit refresh, diagnostics, and portable reversible setup.
- Codex owns native discovery, enablement, plugin state, and permissions.
  Existing package installers continue to own installed skills.
- The calling agent owns relevance decisions, query formulation, selection, and
  whether to delegate. The instructions explain when discovery is useful and
  how to call it; they do not require a search on every turn.
- mac-state owns release selection, managed files and Codex settings, installed
  capabilities, local paths, and lifecycle ordering. It consumes the same public
  Skillwick contracts as any other managed environment. Managed integration
  consumes the product's canonical usage content rather than maintaining a
  separate local version of its instructions.

## Agreed implementation order and proof

1. Establish the public CLI and output contract. Remove shorthand and obsolete
   compatibility paths; fix oversized-result output at the formatter. Verify
   unknown commands/options, option placement, successful/no-match/error output,
   JSON, bounded text, and normal shell composition at the executable boundary.
2. Remove the suggestion hook and ship neutral usage instructions. Retire only
   the owned installed hook during the coordinated transition; preserve unrelated
   hook configuration. Do not retain a second supported discovery flow.
3. Consolidate setup and refresh around the owning inventory implementation.
   Separate ordinary setup from externally managed configuration ownership.
   Verify native-only and filesystem inventory, workspace scope, failed refresh
   retention, repeat setup, modified user files, and uninstall in temporary homes.
4. Align mac-state with the public contract and chosen refresh policy. Check
   desired-state diffs, verify install/configuration/capability ordering, and run
   one real clean-environment setup/search/read check with the built artifact.
   Stubbed installer tests alone do not establish usable integration.
5. Remove out-of-scope tooling and its commands, dependencies, fixtures, CI jobs,
   docs, and claims. Retain ordinary correctness regressions and the documented
   evidence gap. Reconcile the implementation issue and public documentation
   with this direction; do not keep parallel authoritative specifications.

For each defect, first reproduce the observable failure, trace its responsible
layer and callers, then add the smallest regression that fails before the fix.
Use unit tests for isolated invariants and executable/integration tests for
contracts that cross module or process boundaries. Formatting and lint checks
catch source problems; `doctor` diagnoses a current environment. Neither proves
that actual commands return the correct output. Run relevant checks during the
change and the complete retained CI suite before release. Verify the packaged
artifact and a supported Codex integration when those boundaries change.

Resolved terms live in `CONTEXT.md`. Implementation tasks belong in the issue
tracker after agreement. No ADR is needed for this reversible cleanup.
