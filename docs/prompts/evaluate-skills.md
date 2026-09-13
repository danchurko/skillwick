# Evaluation prompt

Give this prompt to the coding agent that owns the evaluation run. The agent,
not Skillwick's offline helper, owns coordination and model execution.

```text
Evaluate Skillwick skill discovery on this machine.

Read docs/BENCHMARKS.md and docs/evaluation-profile.md. Export the complete
installed corpus with:

  uv run --script scripts/evaluate_skills.py export \
    --skillwick <SKILLWICK> \
    --output <CORPUS_JSON> \
    --coverage 0.30 \
    --seed <SEED>

The total evaluation population is ceil(coverage * corpus total), including
no-skill cases. Skill coverage is separate: every sampled skill needs a frozen
positive judgment, but multiple relevant skills may share a realistic case.
Author requests independently of retrieval output, review relevance separately,
freeze labels before searching, and validate them with:

  uv run --script scripts/evaluate_skills.py validate \
    --corpus <CORPUS_JSON> \
    --dataset <DATASET_JSON>

Prepare the isolated Codex profile exactly as documented. Pause for the user to
complete device login and supply a token budget. Recheck the full staged corpus,
configuration, disabled personal integrations, authentication, and actual model
identities after login. Use gpt-5.6-terra for root and independent judge sessions
and gpt-5.6-luna for researcher sessions, unless an explicit override is recorded.

Run a bounded pilot first. Compare direct, delegated, and native discovery on
identical frozen cases and equal search budgets. For delegated discovery, one
Luna researcher may use up to three adaptive perspectives and return at most
five deduplicated IDs with descriptions and reasons; the Terra root makes the
final selection. Do not load selected instructions during discovery.

Record each query, candidate list, final selection, failure, latency, and provider
usage. Keep root and total input, output, and cached-input counts separate. Mark
unfinished runs partial; do not fill missing cells. Keep semantic and reranking
backends unmeasured. Use three independent repetitions only for stability claims.

Blind adjudication uses a separate Terra session with workflow, model, cost, and
original labels hidden. Preserve its rationale and unresolved judgments without
rewriting frozen labels. Keep raw transcripts and credentials out of repository
artifacts. Map relocated corpus IDs through the exported portable identities.

When the evidence artifact is complete, validate and score it offline:

  uv run --script scripts/evaluate_skills.py score \
    --corpus <CORPUS_JSON> \
    --dataset <DATASET_JSON> \
    --evidence <EVIDENCE_JSON> \
    --output <REPORT_JSON>

Report frozen and adjudicated quality separately, plus no-skill abstention,
root context, total input/output/cache usage, latency, failures, corpus and model
provenance, and selected-instruction loading. Do not claim monetary savings
without verified dated prices and billing semantics, or general quality from a
small pilot.
```
