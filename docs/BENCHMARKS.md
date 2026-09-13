# Evaluation

Skillwick evaluation has two separate questions: can the retrieval backend
surface relevant skills, and can an agent select useful IDs from those results?
Reports keep those metrics and their token costs separate.

## Current lexical result

The production Rust benchmark uses SQLite FTS5 lexical ranking and explicit
relevance judgments. It reports Recall@5, hit rate, reciprocal rank, binary
nDCG, no-match accuracy, and warm query latency.

```text
run date: 12 September 2026
dataset: local-skills-v1-2026-09-12 (078bff04aa63)
corpus: 418 skills (6af61dadafa3)
retriever: lexical
queries: 65 (60 judged, 5 no-match)
Recall@5: 0.869
HitRate@5: 0.867
MRR@5: 0.812
nDCG@5: 0.826
no-match accuracy: 0.600
query latency: p50 0.163 ms, p95 0.380 ms
misses: 10
```

This is one labelled local corpus, not a general search-quality claim. Latency
is in-process warm query time and excludes process startup, Codex inventory,
and index refresh. The reviewed judgments are in
[`benchmarks/local-skills-v1.json`](../benchmarks/local-skills-v1.json), and the
Rust-only runner remains available through `make benchmark`.
The reviewed version-2 task set is
[`benchmarks/local-skills-v2.json`](../benchmarks/local-skills-v2.json).

## Direct versus delegated pilot

The 13 September 2026 pilot completed 36 model calls across 12 tasks from a
pre-reconciliation 177-case v2 dataset draft,
with all 555 corpus records searchable. It included eight positive development
cases, two positive held-out cases, and one no-skill case in each split. Both
workflows received the same three fixed queries per task. The root used
`gpt-6-astra`; the researcher used `gpt-5.6-luna`, both with medium reasoning.

| Metric across 12 tasks | Direct | Delegated |
| --- | ---: | ---: |
| Root input tokens | 202,950 | 183,630 |
| Total input tokens | 202,950 | 352,980 |
| Total output tokens | 334 | 1,763 |
| Model-call wall time, mean per task | 5.16 s | 11.64 s |
| Labelled positive Recall@5 | 1.00 | 1.00 |
| No-skill abstentions | 2 / 2 | 2 / 2 |
| Additional unlabelled selections | 3 | 5 |
| Cases matching the exact label set | 9 / 12 | 8 / 12 |

Delegation reduced provider-reported root input by **9.52%**, while increasing
total input by **73.92%** and model-call wall time by **2.26×**. Provider input
includes common Codex context; cached input is already included in those totals.
Wall time includes CLI startup and excludes retrieval and corpus export. These
are token counts, not a monetary cost comparison.

Both workflows retrieved every labelled relevant skill in both splits, but
additional unlabelled choices prevent a claim of perfect selection. That draft
used agent-authored requests and metadata judgments which were not exhaustive
human judgments or a sample of real user traffic. This small
pilot measures fixed-query selection and condensation, not adaptive research
or downstream task success. It supports keeping delegation conditional; it does
not establish a general quality or cost advantage.

The [saved pilot evidence](../benchmarks/results/discovery-pilot-2026-09-13.json)
contains split metrics, case decisions, usage, and provenance. Its historical
runner recorded executable versions but not binary hashes. All 36 transcripts
were audited, and all 36 searches were replayed against the source build with
identical result metadata; that does not establish the historical binary hash.
The replacement offline helper performs no replay; new calling workflows must
record executable identities and corpus checks explicitly. Raw transcripts
remain local.

This historical pilot also retained the existing `CODEX_HOME`; clean-profile
isolation was not verified. `--ignore-rules` excludes execpolicy rules, not
`AGENTS.md` instructions, and hiding the skill catalogue does not remove those
instructions. Do not treat these results as evidence from a clean Codex profile.
The [current evaluation direction](design/current-direction.md) requires that
isolation for future comparisons.

## Reproducible corpus and dataset

The local helper is a stdlib Python script. It exports and validates artifacts;
it does not invoke models or coordinate agents. Run it with the repository's
current Python through `uv`:

```sh
uv run --script scripts/evaluate_skills.py export \
  --skillwick target/release/skillwick \
  --output target/evaluation/corpus.json \
  --coverage 0.30 \
  --seed 20260913
```

`export` executes `skillwick --json list --all`, stores every returned record,
and writes a manifest containing the command provenance, stable record metadata,
and corpus identity. The sample is a deterministic seeded
stratified selection of `ceil(coverage * corpus_total)` distinct skill IDs. The
full corpus remains available to search. The sample defines the skill-coverage
target; it does not add task cases or require one case per sampled skill.

The evaluation population is exactly `ceil(coverage * corpus_total)` distinct
task cases, including independently reviewed no-skill cases. For the captured
555-record corpus at 30% coverage, the current dataset contains 167 cases: 157
positive cases and 10 no-skill cases. Multiple sampled skills may share one
positive case, while labels may also name other full-corpus skills. Author
realistic requests independently of retrieval results, then review relevance
and freeze labels before any search. The dataset format requires exactly three
query perspectives for every case: `outcome`, `mechanism`, and `constraints`.
Positive cases have explicit reviewed full IDs; no-skill cases have an empty
relevance list and live in `negative_cases`. A sampled ID must appear in a
positive judgment, and a skill must not be labelled in both `dev` and `heldout`.
The `labeling`, `population`, and `skill_coverage` fields record these distinct
contracts. The validator checks their declarations and structural safeguards,
but cannot independently prove how a label was derived. Keep the review record
and source notes with the dataset when publishing results.

```json
{
  "version": 2,
  "name": "skills-eval-v1",
  "corpus_sha256": "<manifest corpus.sha256>",
  "sample_sha256": "<manifest sample.sha256>",
  "population": {
    "coverage": 0.30,
    "corpus_total": 555,
    "total_cases": 167,
    "positive_cases": 157,
    "no_skill_cases": 10
  },
  "skill_coverage": {
    "sampled_skill_count": 167,
    "corpus_total": 555,
    "sampled_proportion": 0.30
  },
  "perspectives": ["outcome", "mechanism", "constraints"],
  "labeling": {
    "method": "independent_review",
    "search_results_used": false,
    "reviewed": true
  },
  "cases": [
    {
      "id": "case-0001",
      "split": "dev",
      "kind": "positive",
      "task": "A natural language task",
      "queries": [
        {"perspective": "outcome", "query": "the desired result"},
        {"perspective": "mechanism", "query": "the implementation mechanism"},
        {"perspective": "constraints", "query": "the important constraints"}
      ],
      "relevant": ["skill-id@hash"],
      "reviewed": true
    }
  ],
  "negative_cases": [
    {
      "id": "negative-0001",
      "split": "heldout",
      "kind": "negative",
      "task": "A task that needs no installed skill",
      "queries": [
        {"perspective": "outcome", "query": "..."},
        {"perspective": "mechanism", "query": "..."},
        {"perspective": "constraints", "query": "..."}
      ],
      "relevant": [],
      "reviewed": true
    }
  ]
}
```

Validate the manifest and dataset before spending model tokens:

```sh
uv run --script scripts/evaluate_skills.py validate \
  --corpus target/evaluation/corpus.json \
  --dataset target/evaluation/dataset.json
```

Validation rejects changed corpus or sample identities, a population count that
does not include the no-skill cases, missing or duplicate IDs, labels outside
the corpus, a dataset that declares search-derived or unfrozen labels,
incomplete perspectives, split leakage, and unreviewed negatives. It cannot
observe the author's labelling process, so preserve the review record when
sharing a dataset.

## Caller-owned comparison and offline scoring

The calling agent owns clean-profile preparation, model choice, adaptive
queries, delegation, budgets, transcripts, partial-run handling, and blind
adjudication. Follow the [copyable evaluation prompt](prompts/evaluate-skills.md)
and [clean-profile procedure](evaluation-profile.md). Use the same frozen cases
and search budget for direct, delegated, and native comparisons. Internal runs
use Terra roots and judges and Luna researchers; record the actual identities.

Record query results, final selections, provider usage, failures, and reviewed
adjudication in a versioned evidence JSON file. Keep input, output, and cached
input counts separate under both `usage.root` and `usage.total`. Relocated
corpora use the manifest's portable identity mapping instead of path-derived IDs.
Raw transcripts stay outside repository evidence when they contain private data.

The helper accepts complete, partial, and failed recorded artifacts:

```sh
uv run --script scripts/evaluate_skills.py score \
  --corpus target/evaluation/corpus.json \
  --dataset target/evaluation/dataset.json \
  --evidence target/evaluation/evidence.json \
  --output target/evaluation/report.json
```

`score` validates exact corpus, sample, dataset, query-budget, case, selection,
usage, and adjudication identities across the recorded direct, delegated, and
native cells. A `complete` artifact must contain every case in every arm;
`partial` and `failed` artifacts preserve missing cells instead of fabricating
measurements. It
reports per-query position-sensitive retrieval metrics and workflow-level final
selection metrics against frozen and adjudicated judgments separately. It
aggregates root and total usage and known query latency per workflow (plus a
combined usage total) without invoking a model,
searching, authenticating, building prompts, sequencing agents, or resuming runs.
Semantic and reranking backends remain unmeasured until implemented.

The 13 September 2026 authenticated observational pilot records four cases for
the direct, delegated lexical, and native paths using Luna throughout. It reused
the live authentication scope, so the bounded pilot is complete but the dataset
artifact remains partial and is not clean-profile evidence. Every path selected
all frozen relevant skills and abstained on both no-skill cases. Delegation
reduced root input from 151,994 to 48,736 tokens, but increased total input to
226,678 tokens after the Luna researcher was included. Native discovery used
72,913 input tokens. Linear extrapolation to all 167 cases is about 6.35 million
direct, 2.03 million delegated-root, 9.46 million delegated-total, and 3.04
million native input tokens. Corresponding output estimates are about 61,289,
7,557, 77,989, and 13,819 tokens. Cached-input counts are retained only as raw
provider provenance and are not added to or subtracted from these comparisons.
Codex shortened native descriptions to fit its context budget; that arm saw the
530-record rendered catalogue rather than a full-description baseline. Four
cases are too few for a general savings claim, and no currency estimate is made.
See the portable [evidence](../benchmarks/results/observational-pilot-2026-09-13.evidence.json)
and [offline report](../benchmarks/results/observational-pilot-2026-09-13.report.json).

## Context estimate

Context accounting is separate from retrieval and model replay. A 13 September
2026 capture of the local Codex catalogue found 555 indexed records, 530
rendered by native discovery, and 172 rendered under the configured catalogue
cap. With `tiktoken` 0.14.0 and `o200k_base`, the captured discovery block was
estimated at 13,303 tokens without the cap and 4,020 with the cap; the
Skillwick context file was 350 tokens. These are prompt-content estimates, not
end-to-end billing or quality measurements. The raw capture and calculation
are recorded in
[`benchmarks/results/context-estimate-2026-09-13.json`](../benchmarks/results/context-estimate-2026-09-13.json).

To reproduce the context measurement on a compatible Codex installation, first
capture the three prompt variants and the corpus that they describe. The
default native catalogue is the configured capture; the larger token cap is the
full capture. Use the existing 65-case lexical dataset for the one-query-per-
case discovery estimate:

```sh
codex debug prompt-input --config skills.include_instructions=true \
  "find specialist guidance" > target/evaluation/prompt-native.json
codex debug prompt-input --config skills.include_instructions=true \
  --config skills.max_context_tokens=100000 \
  "find specialist guidance" > target/evaluation/prompt-native-full.json
codex debug prompt-input --config skills.include_instructions=false \
  "find specialist guidance" > target/evaluation/prompt-hidden.json
skillwick --json list --all > target/evaluation/corpus-live.json
uv run --script scripts/measure-context.py \
  --native target/evaluation/prompt-native-full.json \
  --configured-native target/evaluation/prompt-native.json \
  --hidden target/evaluation/prompt-hidden.json \
  --corpus target/evaluation/corpus-live.json \
  --dataset benchmarks/local-skills-v1.json \
  --binary skillwick \
  --instructions assets/skillwick/SKILLWICK.md \
  --output target/evaluation/context-estimate.json
```

The capture files are measurement inputs. The result JSON stores their hashes
and aggregate estimates, rather than embedding the raw prompt captures.

## Comparing retrieval implementations

Keep the corpus and dataset identities unchanged when comparing backends. Report
Recall@5, MRR@5, nDCG@5, no-match accuracy, startup and warm latency, peak RSS,
model storage, root context tokens, total workflow tokens, and failures. A
changed identity is a new evaluation, not a directly comparable result.

The optional model candidates and adoption gate are documented in
[embedding model research](EMBEDDING_MODELS.md). Do not claim semantic or
reranking quality until those cells have an implemented backend and measured
results on the heldout split.
