# Retrieval evidence

These fixtures measure the shipped lexical product and keep optional model
experiments outside the Rust binary. `profile-v1.json` freezes 531 sanitized
model-discoverable records and 35 held-out cases (105 query perspectives).
Paths, instruction bodies, hashes, and local source locations are excluded.

The query labels predate these retrieval runs. They were taken from the
reviewed held-out split in `c0ea8d9^:benchmarks/local-skills-v2.json`; only
path-derived ID suffixes were replaced by the corresponding unique skill name.

## Current task evaluation

`profile-v2.json` is a separate frozen 12-record, 20-case corpus. One query is a
sanitized task from the current user request; the rest are labelled synthetic.
It includes four negatives, multiple relevant skills, vocabulary mismatches,
and two distinct packages called `deploy`. Stable fixture IDs preserve those
names without conflating labels. No session archives were read.

V2 reports positive Recall@5 and @20, MRR@5, standard nDCG@5 (ideal ranking uses
`min(5, relevant_count)`), and negative false-positive rate separately. It is a
small engineering regression corpus, not representative user traffic. Labels
and corpus identities are frozen before running both binaries. Historical v1
results retain their original metric definition and files.

```sh
python3 scripts/benchmark-lexical.py run --binary /path/to/installed/skillwick \
  --profile benchmarks/profile-v2.json --samples 3 --output /tmp/candidate.json
python3 scripts/benchmark-lexical.py validate --profile benchmarks/profile-v2.json \
  --result /tmp/candidate.json
```

No pre-commit gate performs model inference or downloads model artifacts.
Optional-model adoption still requires independent task outcomes and supported
machine budgets; improving a ranking score alone is insufficient.

## 0.3 versus 0.4 installed candidates

The recorded 2026-09-16 runs use the same profiles and three timed samples per
query. The 0.4 executable was installed into a temporary prefix from this source;
0.3 was the existing release installation. Result files record executable hashes.

| Profile | Metric | 0.3.0 | 0.4.0 |
|---|---|---:|---:|
| V1: 35 cases, 105 perspectives | Recall@5 | 0.8857 | 0.8857 |
| V1 | MRR@5 / historical nDCG@5 | 0.8373 / 0.8497 | 0.8373 / 0.8497 |
| V2: 16 positive cases | Recall@5 and @20 | 0.8125 | 0.8125 |
| V2 | MRR@5 / nDCG@5 | 0.8750 / 0.8281 | 0.8750 / 0.8281 |
| V2: 4 negative cases | False-positive rate | 0 | 0 |

Ranking quality is unchanged on both fixtures. This is regression evidence for
the discovery and interface changes, not evidence of improved semantic retrieval
or task success. Latency and memory are recorded in the JSON but single-machine
runs with concurrent development activity do not establish a performance claim.

- [0.3 V1](results/lexical-0.3.0-profile-v1.json) and [0.4 V1](results/lexical-0.4.0-profile-v1.json)
- [0.3 V2](results/lexical-0.3.0-profile-v2.json) and [0.4 V2](results/lexical-0.4.0-profile-v2.json)

## Reproduce historical experiments

```sh
cargo build --release --locked
python3 scripts/benchmark-lexical.py run \
  --binary target/release/skillwick \
  --profile benchmarks/profile-v1.json \
  --output /tmp/lexical-current.json

UV_CACHE_DIR=/private/tmp/skillwick-uv-cache \
uv run --with fastembed==0.8.0 python scripts/benchmark-semantic.py embedding \
  --profile benchmarks/profile-v1.json \
  --output benchmarks/results/embedding-arctic-xs-2026-09-13.json \
  --cache /private/tmp/skillwick-models

UV_CACHE_DIR=/private/tmp/skillwick-uv-cache \
uv run --with fastembed==0.8.0 python scripts/benchmark-semantic.py rerank \
  --profile benchmarks/profile-v1.json \
  --candidates benchmarks/results/lexical-baseline-2026-09-13.json \
  --output benchmarks/results/reranker-tinybert-lexical-2026-09-13.json \
  --cache /private/tmp/skillwick-models
```

`freeze` on the lexical runner accepts a complete `skillwick --json list`
capture and the historical query file when a deliberately new profile is
needed. Do not overwrite an existing profile to refresh ordinary results.

Cold means a fresh process; the runner does not claim to flush kernel caches.
Warm latency uses separate covered CLI processes and repeated samples. The
stored JSON includes machine, executable, corpus, cache, artifact, checksum,
latency, memory, and per-query ranking evidence.

## Candidates fixed before evaluation

- Embedding: `Snowflake/snowflake-arctic-embed-xs` at revision
  `d8c86521100d3556476a063fc2342036d45c106f`. It was the smallest
  retrieval-specific English candidate with a directly supported runtime,
  384 dimensions, documented query prefix, and Apache-2.0 licence. The runner
  requires SHA-256
  `cf2698d30ff05da02c70a088313bad56e5c2f401d734cb24a8390d446111936c`
  for the 90,387,631-byte ONNX artifact.
  The runner also records and verifies the pinned configuration, tokenizer,
  tokenizer configuration, and special-token map before loading that snapshot.
- Reranker: `cross-encoder/ms-marco-TinyBERT-L2-v2` at revision
  `81d1926f67cb8eee2c2be17ca9f793c7c3bd20cc`. It was selected as the
  smallest English cross-encoder in the reviewed shortlist. The experiment
  independently reranks a bounded lexical top-20 pool. The runner requires
  SHA-256
  `7497b40504d425ef6482693039690106dca4f1f8d88fb5c4aedd63e73ed6ef68`
  for the 4,518,071-byte arm64 quantized ONNX artifact.
  Its pinned tokenizer is recorded and verified before loading the ONNX session.

The source shortlist also considered MiniLM, BGE, and Jina alternatives.
Those larger candidates were not run because the smallest candidate already
answers whether each component can justify its lifecycle cost. Model-card
scores are selection context, never Skillwick evidence.

| Role | Candidate | Pre-run disposition |
|---|---|---|
| Embedding | Arctic XS, 22.6M parameters | Run: smallest retrieval-specific option with direct runtime support. |
| Embedding | all-MiniLM-L6-v2, 22.7M | Hold: similar footprint but lower card-reported retrieval score. |
| Embedding | BGE-small-en-v1.5, 33.4M | Hold: larger quality-oriented fallback. |
| Embedding | Jina embeddings v2 small, 32.7M | Hold: long context and larger vectors are unnecessary for bounded metadata. |
| Reranker | TinyBERT-L2-v2, 4.39M | Run: smallest English cross-encoder in the shortlist. |
| Reranker | MiniLM-L2/L6-v2, 15.6M/22.7M | Hold: larger quality fallbacks if TinyBERT misses the gate. |
| Reranker | Jina reranker v1 turbo, 37.8M | Hold: convenient but substantially larger. |
| Reranker | BGE reranker base, 278M | Reject for this experiment: multilingual footprint is disproportionate. |
