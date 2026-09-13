# Shared understanding and findings

Status: shared understanding confirmed. GitHub issue
[#1](https://github.com/churdaa/skillwick/issues/1) is the authoritative
[implementation specification](discovery-spec.md).
Updated 13 September 2026 from the user discussion in Codex task
`01a09a40-abcc-7bd0-9648-0da8b833bfe8`.

This is the single owner of the current design direction. It contains no
superseded proposals or interview transcript. Measured results retain their
original provenance in the benchmark evidence files.

## Product boundary

Skillwick enables reliable skill selection with less root-agent discovery
context. It owns search, inventory, inspection, and selected instruction reads.
The calling agent and usage prompts own model selection, prompt management,
adaptive searching, sub-agent invocation, judging, and run coordination.

Keep installed instructions short. When the calling workflow uses delegation,
one researcher may search from up to three distinct perspectives and return at
most five deduplicated IDs with descriptions and reasons. The root reads selected
instructions. Package inspection exposes bounded relative paths, counts, and
Markdown/non-Markdown file types without reading or executing supporting files.

Evaluation is a documented method with small offline helpers for inventory
snapshots, structural validation, token accounting, and scoring recorded results.
Do not build an evaluation framework, scheduler, or model-adapter layer.

## Evaluation method

- Internal runs use `gpt-5.6-terra` for the root and `gpt-5.6-luna` for the
  researcher. Use a separate Terra session for blind adjudication. Actual
  deployments may use different models; do not transfer measured percentages
  between model combinations.
- Run measured agents in a temporary clean Codex environment. Personal hooks,
  instructions, custom agents, plugins, memories, MCP configuration, and inherited
  task state must not silently affect results. Do not modify the installed setup.
  Export the full local skill inventory separately as the corpus input; isolation
  must not silently substitute an empty or incomplete corpus.
- Derive the default evaluation case count as `ceil(0.30 * discoverable_skills)`.
  Make the proportion adjustable and retain the full searchable corpus. Cases
  should span varied capabilities; there is no one-case-per-sampled-skill rule.
  This total includes no-skill cases. Repeated runs and workflow comparisons reuse
  the cases rather than increasing the population count.
- Author realistic held-out requests independently of target skill descriptions;
  use separate relevance review. Freeze labels before retrieval. No-skill cases
  must be independently judged, never tuned until search returns nothing.
- Compare lexical, lexical plus semantic, and lexical plus semantic with reranking,
  each with direct and delegated discovery. Use the same adaptive search budget
  and cases. Keep future backends unmeasured until implemented. Fixed-query replay
  can remain a diagnostic; it is not evidence of adaptive research.
- Add native Codex discovery as a reference using the same installed inventory
  and root model. Pin and disclose its catalogue configuration within the clean
  profile. Report rendered coverage; do not invent a synthetic native catalogue
  to equalize candidate counts. Uncapped native discovery is an optional diagnostic.
- Measure discovery through final selection. Account separately for loading
  selected instructions. Downstream task success requires a separate evaluation.
- Report recall, precision, no-skill abstention, root discovery context, total
  input/output/cache usage, latency, failures, and corpus/model provenance. Keep
  token volume distinct from monetary estimates; prices and billing semantics
  must be verified before quoting monetary estimates.
- Recommend a workflow only with lower root discovery context and no observed
  decline in held-out quality against its relevant baseline. Small-sample passes
  remain provisional. Compare delegation with direct discovery on the same
  backend, and both with native discovery.
- Keep frozen-label scores separate from blind adjudication of useful alternatives
  or unnecessary additions. Hide workflow/model identity and cost from the judge,
  retain rationales, flag unresolved cases, and label results agent-reviewed.
- Use one run for routine checks and three independent repetitions for claims
  of stable comparative quality or cost. Start with a pilot. Expanded runs need
  a workload estimate and supplied token budget; the calling agent owns budget
  enforcement and checkpointing. Incomplete runs stay partial.

## Public documentation and releases

Lead the README with the user benefit, a working example, measured evidence,
and installation. Use professional project language. Keep command detail in
the usage guide and the reproducible evaluation method in its guide and copyable
agent prompt. Label results by their actual measurement scope and model pair.

Keep Homebrew and the repository-owned script installer. Future release uploads
contain both architecture archives and their per-file checksums; retain build
metadata in CI. Verify the Homebrew formula against published archive digests.

## Existing work and required alignment

| Surface | Required alignment |
| --- | --- |
| `assets/skillwick/SKILLWICK.md`, `src/package.rs`, `src/cli.rs`, `docs/USAGE.md` | Review existing bounded instructions and inspection against the product boundary |
| `scripts/evaluate_skills.py`, `tests/test_evaluate_skills.py` | Reuse measurement logic; remove agent orchestration from the planned evaluation surface |
| `scripts/measure-context.py` | Retain narrow token measurement and explicit exclusions |
| `docs/BENCHMARKS.md`, `docs/prompts/evaluate-skills.md`, `README.md` | Align the method, clean profile, Terra/Luna pairing, and case-count policy |
| `benchmarks/local-skills-v2.json`, `benchmarks/results/` | Preserve actual historical evidence; create new cases/results for the current method |
| `Makefile`, `.github/workflows/ci.yml` | Align checks with retained small measurement helpers |
| `dist-workspace.toml`, `.github/workflows/release.yml`, `Formula/skillwick.rb` | Retain and verify the existing release simplification |

## Remaining work

1. The user completes device login in the isolated profile. Repeat preflight checks
   after authentication, then run the bounded Terra/Luna pilot and inspect actual
   runtime behavior. No new model evaluation has been run yet.
2. Complete and verify the ready-for-agent child issues linked from the
   authoritative implementation specification. Do not recreate completed work.

No further product questions are open. The case population includes no-skill cases.

## Verified findings

The [saved no-model preflight](../../benchmarks/results/profile-preflight-2026-09-13.json)
records these checks against Codex 0.154.0:

- A fresh native inventory read found 555 skills and exactly matched the captured
  source manifest. The read used temporary Skillwick caches; installed Skillwick
  state and user configuration were not changed.
- Staging actual native skill directories and cached plugin packages preserved
  all 555 names, plugin identities, metadata and instruction hashes. Every staged
  source path is inside the temporary profile. Paths and derived IDs change on
  relocation, so comparisons require an explicit identity mapping rather than
  pretending the original path-dependent digest is portable.
- The staged configuration disables hooks, apps and memories. The hook inventory
  is empty, tested personal instruction markers are absent, and there is no
  global `AGENTS.md` or custom-agent directory in the clean Codex home.
- Copying the real plugin packages initially exposed an enabled Chrome DevTools
  MCP server. `features.apps=false` did not disable it. An explicit disabled MCP
  declaration with a valid transport fixed this: zero MCP servers are now enabled.
  A fresh inventory comparison after that change still matched all 555 records.
- The staged plugin configuration must be loaded. `--ignore-user-config` would
  discard its plugin activation and MCP disablement. Use the isolated Codex home
  with only audited evaluation settings; do not copy the user's configuration.
- The live profile is authenticated, but a fresh Codex home has no matching keyring
  credential. The fully isolated home cannot access the default keychain in the
  probe. Supported file-backed storage reports not logged in. The next step is
  a normal device login confined to the evaluation profile; no login was initiated
  and no credentials were copied.

See the [clean-profile preparation method](../evaluation-profile.md) for commands
and checks. A named profile layers settings over the base profile; it does not
provide this isolation. `--ignore-rules` excludes execpolicy rules rather than
`AGENTS.md`, and `--ephemeral` controls persistence rather than configuration.

These are offline preparation results, not an authenticated runtime or quality
benchmark. Recheck after login: account-dependent behavior and actual model-turn
execution have not yet been observed. Package hook/MCP files remain physically
present as resources, while the audited settings disable their activation.
Required host policy is retained and must be disclosed.

## Evidence and implementation boundaries

The Rust CLI has the intended discovery boundary. `scripts/evaluate_skills.py`
now retains only corpus export, structural validation, recorded-evidence
scoring, portable identity mapping, and pure usage aggregation. Model choice,
authentication, prompts, delegation, sequencing, and partial-run handling live
in the documented calling workflow. The existing Rust benchmark still provides
a deterministic lexical diagnostic.

The historical 177-case snapshot and Astra/Luna pilot do not implement the new
population or clean-profile method. Preserve their recorded facts and limitations.
The current dataset has 167 total cases, including ten no-skill cases. New runs
use that frozen dataset and their actual Terra/Luna identities. No monetary
savings or general quality advantage has been established.

Release work already matches the intended archive/checksum and installer surface.
Final release verification should include the uploaded artifact layout as well
as filenames. Nothing has been committed, published, or applied to the user's
installed configuration in this preparation work.
