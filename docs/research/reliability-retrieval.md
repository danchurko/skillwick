# Managed retrieval qualification

Generated rankings, receipts, audits, and request traces are local artifacts in
Git-ignored `benchmarks/results/` (or an external output directory). The measured
findings and limitations are summarized here; raw files are not distributed
with the repository. See [benchmark instructions](../../benchmarks/README.md) to generate and validate new results.

**Historical real-corpus status: partial and blocked.** #46 now covers
[synthetic pre-merge acceptance](synthetic-acceptance.md), following the user’s
sequencing decision. [#49](https://github.com/danchurko/skillwick/issues/49)
preserves this unfinished real-corpus work. The replay qualifies
only the current explicit eligible subset. The intended automatic corpus is
still blocked by the native Ponytail inventory/manifest version disagreement, so absent
targets cannot be distinguished from ranking failures yet.

## Frozen scope

The profile contains 171 eligible records from the configured managed explicit
Codex and Claude roots. Its sanitized records retain names, descriptions,
source-family labels, content fingerprints, scope, enabled/degraded state,
source kind, and grouped-origin count; source paths, package bodies, and source
identifiers are omitted. Every row was global-scope, enabled, non-degraded, and
filesystem-backed. There were no duplicate names: 154 records represented two
grouped origins and 17 represented one. The live CLI inventory matched the
supplied snapshot across the frozen fields.

The inventory does not contain `amazon-bedrock`, `aws-containers`, or
`aws-deployment`. These are eligibility gaps in this snapshot. Their absence is
not evidence that lexical ranking placed an eligible target too low.

The frozen v2 profile has six synthetic positive cases covering GKE inference,
Agent Platform inference, developer documentation, Codex release
orchestration, GKE readiness orchestration, and Google Cloud multi-agent
design. It also has two synthetic negatives: an unrelated general-knowledge
question and a translation request with lexical overlap. Labels were fixed from
task intent and skill metadata before the lexical run.

## Scored lexical run

The existing lexical benchmark ran the installed Skillwick 0.4.0 executable
with candidate pool 20 and one sample per query. All six labelled positives
ranked their relevant skill first: positive Recall@1/@3/@5/@20, MRR@5, and
nDCG@5 were 1.0. Both negatives returned candidates, for a false-positive rate
of 2/2 (1.0) on this small fixture. This is a bounded result, not a traffic-wide
rate or task-success measure.

| Case | Frozen relevance | First relevant rank | Top result |
|---|---|---:|---|
| GKE inference | `gke-inference` | 1 | `gke-inference` |
| Agent Platform inference | `agent-platform-inference` | 1 | `agent-platform-inference` |
| Developer documentation | `find-docs` | 1 | `find-docs` |
| Codex release orchestration | `astra-orchestrator` | 1 | `astra-orchestrator` |
| GKE readiness orchestration | `gke-productionize` | 1 | `gke-productionize` |
| Google multi-agent design | `google-cloud-solution-build-deploy-agents` | 1 | `google-cloud-solution-build-deploy-agents` |

The two negative results remain open ranking defects:

| Negative query | Returned candidates | Bounded follow-up |
|---|---|---|
| `What is the capital of Portugal?` | `cloud-databases-onboarding`, `code-review` | Keep this query and a second unrelated general-knowledge control in the next retrieval change. Validate abstention or a lower false-positive rate without lowering positive Recall@5 on this profile. |
| `Translate the phrase "find docs" into French.` | `find-docs`, `bigquery-ai-ml`, `research` | Assess task-intent handling for exact name and token overlap. Retain this negative beside the developer-documentation positive and require the latter to stay in the top five. |

Neither follow-up was implemented here. No change to retrieval behavior is
recommended from these eight synthetic cases alone.

## Historical and diagnostic replays

Both historical Bedrock queries were run through the current explicit CLI. The
target `amazon-bedrock` is absent, so both are recorded as eligibility-blocked
and unscored. The returned candidates below are observations only.

| Query | CLI results | First five candidates | Status |
|---|---:|---|---|
| `AWS Bedrock model availability pricing direct inference invoke-model CLI eu-west-1` | 6 | `gke-inference`, `agent-platform-inference`, `google-cloud-solution-guided-gke-ai-migration`, `gke-manifest-generation`, `google-cloud-waf-sustainability` | Eligibility-blocked; unscored |
| `AWS Bedrock direct inference TypeScript tool use evaluation` | 20 | `developing-genkit-js`, `detection-engineering-coverage-evaluation`, `agent-platform-inference`, `datalineage-bigquery-asset-impact-analysis`, `gke-inference` | Eligibility-blocked; unscored |

The historical broad query `orchestrate delegated implementation from a release
spec` returned five candidates, led by `gke-productionize`, `astra-orchestrator`,
and `gke-upgrades`. Plausible alternatives remain unscored; the audit does not
establish that only Astra is relevant. The historical compound-name misuse
`grilling domain-modeling astra-orchestrator` returned zero candidates and is
also unscored.

The additional targets `aws-containers` and `aws-deployment` are also absent.
The compound-name diagnostic `astra-orchestrator find-docs
agent-platform-inference` returned no results, but it lists known names without
describing a task. It is kept unscored and is not counted as a successful
negative or a retrieval defect.

## Evidence and limits

The [frozen profile](../../benchmarks/reliability-profile.json) records the
eligible corpus, content fingerprints, labels, source policy, inventory digest,
CLI identity, and evaluator source hashes. The
lexical result (`reliability-lexical-2026-10-04.json`, local artifact)
records per-query rankings, metrics, the eligibility-blocked replays, and the
unfixed negative cases. The installed CLI identity is Skillwick 0.4.0 with SHA-256
`b6e4b9532d44ed561527a4676c92c87c9438d2da62f412dfc3c223f2d249b754`.

The evaluator records repository revision `8c01ffc7f6389ce23d2803d76c12f35adbf2632b`
and a dirty working tree, and fingerprints its benchmark implementations. The
binary fingerprint is recorded independently; this evidence does not claim that
the executable was built from that checkout. No hosted or additional backend
was run.

These measurements cover labelled relevance and candidate ranking only. They
do not measure agent selection, complete instruction reads, instruction use, or
task outcomes. The profile is small, synthetic, and limited to an explicit
subset; its positive scores do not establish production reliability.

Completion remains blocked on establishing the intended eligible sources. Once
the managed Ponytail manifest mismatch is resolved by its owner, freeze the
resulting eligible inventory, check Bedrock and AWS container/deployment target
eligibility, and replay the historical queries before deciding whether any
remaining misses are ranking failures. Keep the two negative cases as
regression controls for any separately proposed retrieval change.

## Reproduction

```sh
mkdir -p benchmarks/results
python3 scripts/benchmark-lexical.py run \
  --binary /path/to/installed/skillwick \
  --profile benchmarks/reliability-profile.json \
  --pool-size 20 --samples 1 \
  --output benchmarks/results/reliability-lexical-current.json

python3 scripts/benchmark-lexical.py validate \
  --profile benchmarks/reliability-profile.json \
  --result benchmarks/results/reliability-lexical-current.json
```
