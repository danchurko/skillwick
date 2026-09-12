# Retrieval benchmark

Skillwick evaluates the production ranking path against explicit relevance
judgments. The runner uses standard information-retrieval metrics without a
benchmark framework dependency: Recall@k, hit rate, reciprocal rank, binary
nDCG, no-match accuracy, and per-query latency.

## Current result

Run date: 12 September 2026. Host: Apple Silicon Mac, macOS 26.6.2. Binary:
optimized Rust release build. Retriever: SQLite FTS5 lexical ranking. Corpus:
the maintainer's live Codex inventory loaded through `skills/list`, with
temporary Skillwick config, cache, and state.

```text
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

The inventory contains 327 enabled skills from `ecc@ecc`, six from
`ponytail@ponytail`, and nine from bundled/runtime plugins. The remaining 76
skills have no plugin ID. Queries are natural tasks rather than skill names.
Ten misses remain in JSON. The larger corpus introduced two false positives on
the five no-skill tasks and exposed lexical misses in six existing user skills
and two bundled plugin skills. These are concrete targets for later ranking
work.

This is one labelled local corpus, not a general search-quality claim. Latency
measures in-process warm queries and excludes process startup, Codex inventory,
and index refresh. The committed query judgments are reviewable at
[`benchmarks/local-skills-v1.json`](../benchmarks/local-skills-v1.json).

## Run and compare

Run against the current user's native and plugin skills with temporary
Skillwick config, cache, and state:

```sh
make benchmark
target/release/skillwick --json benchmark \
  --native-only --dataset benchmarks/local-skills-v1.json > benchmark.json
```

`make benchmark` reads the live Codex inventory but changes no live Codex or
Skillwick configuration. The direct JSON command uses the caller's configured
inventory and is intended for saved comparison runs.

The runner rejects judgments whose relevant skill is absent. Each report
includes SHA-256 identities for both the dataset and `name + content hash` of
every corpus record. Compare quality or latency only when those identities
match, or disclose the changed dataset or corpus.

Dataset format is deliberately small:

```json
{
  "version": 1,
  "name": "example",
  "cases": [
    {"query": "natural task", "relevant": ["skill-name"]},
    {"query": "task needing no skill", "relevant": []}
  ]
}
```

New retrieval implementations must use this same runner and judgments. Add a
new retriever name to the report, keep corpus and dataset hashes unchanged, and
compare Recall@5, MRR@5, nDCG@5, no-match accuracy, startup, warm latency, peak
RSS, and model storage. Retain lexical-only behavior unless measured quality
gains justify runtime and distribution cost. Model candidates and adoption gate
are recorded in [embedding model research](EMBEDDING_MODELS.md).
