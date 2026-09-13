# Retrieval evidence

These fixtures measure the shipped lexical product and keep optional model
experiments outside the Rust binary. `profile-v1.json` freezes 531 sanitized
model-discoverable records and 35 held-out cases (105 query perspectives).
Paths, instruction bodies, hashes, and local source locations are excluded.

The query labels predate these retrieval runs. They were taken from the
reviewed held-out split in `c0ea8d9^:benchmarks/local-skills-v2.json`; only
path-derived ID suffixes were replaced by the corresponding unique skill name.

## Reproduce

```sh
cargo build --release --locked
python3 scripts/benchmark-lexical.py run \
  --binary target/release/skillwick \
  --profile benchmarks/profile-v1.json \
  --output benchmarks/results/lexical-baseline-2026-09-13.json

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
- Reranker: `cross-encoder/ms-marco-TinyBERT-L2-v2` at revision
  `81d1926f67cb8eee2c2be17ca9f793c7c3bd20cc`. It was selected as the
  smallest English cross-encoder in the reviewed shortlist. The experiment
  independently reranks a bounded lexical top-20 pool. The runner requires
  SHA-256
  `7497b40504d425ef6482693039690106dca4f1f8d88fb5c4aedd63e73ed6ef68`
  for the 4,518,071-byte arm64 quantized ONNX artifact.

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
