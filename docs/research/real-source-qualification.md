# Real installed-source qualification

This is the post-merge qualification for
[#49](https://github.com/danchurko/skillwick/issues/49). Generated inventories,
profiles, rankings, receipts and traces belong in Git-ignored
`benchmarks/results/` or an external directory. The tracked
[case input](../../benchmarks/real-source-cases.json) contains task-intent labels
and queries, without installed package bodies or local source paths.

## Native manifest ownership

Codex 0.160.0 selects a root `plugin.json` with the exact Agent Plugins 1.0
schema before a legacy `.codex-plugin/plugin.json`. The root manifest's name
and conventional `skills/` directory define native skill discovery. Codex's
optional legacy overlay contributes apps, hooks, onboarding and interface;
its version and skill paths do not replace the root manifest's values.
See the pinned [manifest resolver](https://github.com/openai/codex/blob/rust-v0.160.0/codex-rs/utils/plugins/src/plugin_namespace.rs),
[native manifest](https://github.com/openai/codex/blob/rust-v0.160.0/codex-rs/core-plugins/src/agent_plugin_manifest.rs),
and [overlay loader](https://github.com/openai/codex/blob/rust-v0.160.0/codex-rs/core-plugins/src/manifest/manifest_cache.rs).

The native CLI's installed version identifies its active cache directory;
it is separate from an optional manifest-local version. See the pinned
[CLI serializer](https://github.com/openai/codex/blob/rust-v0.160.0/codex-rs/cli/src/plugin_cmd.rs)
and [store accessor](https://github.com/openai/codex/blob/rust-v0.160.0/codex-rs/core-plugins/src/store.rs).
For installed Ponytail, the native listing/cache identity was `1.0.0`, the
root manifest had no version, and the legacy overlay declared `4.10.3`.
The active cache and marketplace checkout both identified commit
`c982cd411abb53323c4baa1baa3c2f020b8d0b08`. This is a Skillwick manifest-selection
defect; the evidence does not identify a stale installation or installer defect.

The correction follows the supported native manifest while retaining installed
and enabled filtering, listed cache identity, legacy manifest version equality,
native metadata validation, package containment, invocation policy and atomic
fail-closed refresh.
It does not select a historical cache or alter provider trust or activation.

## Source policy and AWS routes

The intended local corpus uses automatic native discovery plus the existing
configured roots. The previous explicit generated-root subset omitted the
shared native AWS packages and the native Codex `codebase-memory` package.
For this qualification, a temporary copy of the config changes discovery to
`auto` and removes the hosted reranker section; other config fields remain
identical. Searches preserve the actual home and provider state and redirect
only the derived cache. No production setup/apply is performed.

AWS MCP skills and local installed skills are separate supported routes.
MCP search returned exact registry identifiers `amazon-bedrock`,
`aws-containers`, and `aws-deployment`; retrieving all three succeeded.
The local packages cover Bedrock, ECS/EKS/Fargate/ECR containers, and
CodePipeline/CodeBuild/CodeDeploy/CodeConnections/CodeArtifact CI/CD respectively.
Generic deployment requests may instead call for `aws-cdk` or another
service-specific skill. Missing local packages do not imply an MCP failure,
and this work does not mirror remote resources into the local index.
Replaying the two exact historical queries through MCP `agent_skills` search
returns `amazon-bedrock` at ranks one and three respectively (limit three).
Those remote search observations are separate from the local CLI ranking
comparison below; no cloud inference, account authentication or deployment ran.

The automatic inventory has 559 eligible global filesystem groups, all enabled
and non-degraded, with no grouping diagnostics. Origin counts are 311 groups
with one origin, 85 with two, six with three, and 157 with four. All labelled
targets resolve uniquely. This empty-workspace scope does not claim coverage
of live project-specific policy or every skill installed on other machines.

The merged agent-state selected-Codex verifier passes strict health and one
complete batch for `astra-orchestrator`, `codebase-memory`, `caveman`,
`ponytail:ponytail`, and `find-docs`. Every returned metadata field, distinct ID
and complete UTF-8 content hash agrees with the inventory. A separate complete
batch for the three local AWS targets passes the same metadata and content-hash
checks. This validates the read-only selected-source path, not production apply.

## Frozen labels

The scored input has ten positive queries and three negatives. Two Bedrock
queries are historical; the other queries are task-intent controls. Labels are
fixed before searching. Three underspecified or compound-known-name queries
are retained as unscored diagnostics. For known names, the supported route is
an exact batch read, not a compound-name search.

## Reviewed comparison, 4 October 2026

Both runs use the same 559-group corpus, thirteen scored queries, frozen labels,
and lexical pools of at most twenty exact source IDs. TinyBERT reorders each
recorded pool without adding or dropping candidates; the lexical and semantic
receipts validate together. Narrow target labels do not exhaust potentially
useful adjacent SDK or AI skills.

| Measure | Installed lexical CLI | Offline TinyBERT over that pool |
| --- | ---: | ---: |
| Positive Recall@5 / @20 | 0.9 / 0.9 | 0.9 / 0.9 |
| Positive MRR@5 | 0.825 | 0.9 |
| Positive nDCG@5 | 0.843068 | 0.9 |
| Negative false positives | 3/3 | 3/3 |

| Positive case | Relevant target | Lexical rank | TinyBERT rank |
| --- | --- | ---: | ---: |
| Historical Bedrock CLI query | `amazon-bedrock` | 4 | 1 |
| Historical Bedrock TypeScript query | `amazon-bedrock` | Absent from top 20 | Absent from the same pool |
| AWS containers | `aws-containers` | 1 | 1 |
| AWS CI/CD | `aws-deployment` | 1 | 1 |
| Domain model | `domain-modeling` | 1 | 1 |
| Developer documentation | `find-docs` | 1 | 1 |
| Codex orchestration | `astra-orchestrator` | 1 | 1 |
| GKE orchestration | `gke-productionize` | 1 | 1 |
| GKE inference | `gke-inference` | 1 | 1 |
| Agent Platform inference | `agent-platform-inference` | 1 | 1 |

The capital, quoted-name translation, and arithmetic negatives return sixteen,
seven and nineteen candidates respectively. Offline ordering changes all
thirteen scored rankings, but only one has improved labelled top-five gain;
twelve are materially unchanged and none worsens. It displaces no relevant
top-five targets and produces no correct negative abstentions. A reranker
cannot recover a target excluded by candidate generation.

The unscored broad orchestration query returns fourteen candidates, including
ECC orchestration workflows, GKE readiness/upgrades and Astra (rank six).
These are retained observations, not a unique-best-skill judgement. Both
compound-known-name diagnostics return zero candidates; neither counts as a
successful negative.

The candidate is source-installed Skillwick 0.4.0, SHA-256
`3350c9ab17ef86451bd97d6f47c40c4e79ce7f341a5f0a28e45fa399cc37e100`.
The full private inventory digest is
`fcd5e48af58b386e74fecb7d72734b1a74f3490a2110cd82943591228023a4f4`;
the frozen profile file digest is
`ba29543e50b536b67a239a5a813ca56d1104d977181589e8c75451d911d40c20`.
Receipts identify base revision `9d682b5de056400db8facdc5e0fd62e76c5416bd`,
a dirty tree containing this reviewed correction, and exact evaluator hashes.
The installed candidate was built from that tree; a clean published release
is not claimed.

TinyBERT uses the existing pinned ARM64 int8 model at revision
`81d1926f67cb8eee2c2be17ca9f793c7c3bd20cc`, with model and tokenizer checksums
verified before offline load. The runtime is Python 3.10.21, FastEmbed 0.8.0
and ONNX Runtime 1.23.2 on macOS ARM64. One sample per query measured median
installed CLI search at 400 ms (including native provider reconciliation),
median model scoring at 20 ms, and cold model load at 196 ms. These measure
different stages, are not end-to-end alternatives, and are not statistical
performance or task-success guarantees.

## Follow-up defects and validation at qualification time

[#50](https://github.com/danchurko/skillwick/issues/50) requires recovering the
eligible Bedrock target in the TypeScript query's lexical pool, preserving
the other Bedrock query and positive-regression controls.
[#51](https://github.com/danchurko/skillwick/issues/51) requires correcting
abstention for the three exact negatives and additional paraphrased controls,
while preserving the developer-documentation positive and real positive
Recall@5/MRR. Both require fresh installed-source replay, frozen source/policy
fingerprints, candidate identity conservation for optional comparisons,
independent review, and ignored raw outputs. Neither defect is repaired by this historical qualification. Subsequent
[retrieval corrections](retrieval-corrections.md) record their separate evidence.

No backend/default change is justified by this small relevance qualification.
The native manifest correction and reproducible qualification are the scoped
changes. Independent review found and corrected native malformed-metadata
acceptance and rejection of genuine semantic receipts. Focused discovery and
benchmark contracts pass, as do `make check` and locked dependency assurance
(Cargo.lock unchanged). Strict actual required-capability health and complete
batch reads pass separately from the isolated repository tests.

## Evidence boundaries

The retained [explicit-subset result](reliability-retrieval.md) had two negative
false positives out of two controls. The [synthetic V2 result](synthetic-acceptance.md)
covered twenty fixture cases with Recall@5 0.8125; it does not establish
real-corpus or production reliability. Those historical results remain intact.

Relevance rankings and offline model ordering do not establish agent selection,
instruction use or task outcomes. Required-capability batch verification proves
complete reads for the selected required names only. Hosted evaluation,
authentication, deployment, production configuration apply and a published
release remain outside this qualification.

## Reproduction

Use a source-installed candidate, an empty working directory, a temporary
config that preserves your configured roots and enables `discovery = "auto"`,
and a fresh run directory. Remove the temporary config's `[reranker]` section
for a lexical baseline; this avoids making a hosted request. Keep the real
`HOME`, `CODEX_HOME`, and Claude state unchanged. The runner redirects only
`XDG_CACHE_HOME`. Freeze fails if a labelled target is missing or ambiguous,
rather than deleting that case or scoring it as a negative.

```sh
task_run=benchmarks/results/real-source-current
mkdir -p "$task_run/workspace"
task_binary=/path/to/source-installed/skillwick
task_config=/path/to/temporary-auto-lexical-config.toml

python3 scripts/benchmark-lexical.py freeze-real \
  --binary "$task_binary" --config "$task_config" \
  --cwd "$task_run/workspace" --cache-dir "$task_run/freeze-cache" \
  --cases benchmarks/real-source-cases.json \
  --inventory-output "$task_run/inventory.json" \
  --output "$task_run/profile.json"

python3 scripts/benchmark-lexical.py replay \
  --binary "$task_binary" --config "$task_config" \
  --cwd "$task_run/workspace" --cache-dir "$task_run/replay-cache" \
  --profile "$task_run/profile.json" --pool-size 20 \
  --output "$task_run/lexical.json"

# Use an existing prepared TinyBERT model cache. Offline forbids model downloads.
task_model_cache=/path/to/prepared/tinybert/models
uv run --with fastembed==0.8.0 python scripts/benchmark-semantic.py rerank \
  --profile "$task_run/profile.json" --candidates "$task_run/lexical.json" \
  --pool-size 20 --samples 1 --cache "$task_model_cache" --offline \
  --output "$task_run/tinybert.json"

python3 scripts/benchmark-lexical.py validate \
  --profile "$task_run/profile.json" \
  --result "$task_run/lexical.json" --result "$task_run/tinybert.json"
```

The freezer binds the complete private inventory, content/grouping/eligibility
metadata, config, executable, case labels, and evaluator fingerprints. Replay
checks the inventory, config and executable before and after searches; it does
not initialize or materialize fixture skills. A changed source or policy
requires a new freeze and output directory. The semantic experiment consumes
the exact recorded candidate identities and labels, with the same pool limit.
It measures offline ordering rather than a configured CLI reranker invocation.
