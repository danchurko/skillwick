# Retrieval evidence

Generated rankings, receipts, audits, and request traces are local artifacts in
Git-ignored `benchmarks/results/` (or an external output directory). The measured
findings and limitations are summarized here; raw files are not distributed
with the repository. See the reproduction commands below to generate and validate new results.

Keep raw captures, logs, and run work directories under `benchmarks/results/` or
outside the checkout. Use fresh output paths to preserve previous runs. Frozen
profiles and contract fixtures remain tracked as reproducible test inputs.

These fixtures measure the shipped lexical product and keep optional model
experiments outside the Rust binary. `profile-v1.json` freezes 531 sanitized
model-discoverable records and 35 held-out cases (105 query perspectives).
Paths, instruction bodies, hashes, and local source locations are excluded.

The query labels predate these retrieval runs. They were taken from the
reviewed held-out split in `c0ea8d9^:benchmarks/local-skills-v2.json`; only
path-derived ID suffixes were replaced by the corresponding unique skill name.

The [synthetic acceptance record](../docs/research/synthetic-acceptance.md)
summarizes the current offline comparison and the post-merge real-test boundary.

## Current task evaluation

`profile-v2.json` is a separate frozen 12-record, 20-case corpus. One query is a
sanitized task from the current user request; the rest are labelled synthetic.
It includes four negatives, multiple relevant skills, vocabulary mismatches,
and two distinct packages called `deploy`. Stable fixture IDs preserve those
names without conflating labels. No session archives were read.

V2 reports positive Recall@5 and @20, MRR@5, standard nDCG@5 (ideal ranking uses
`min(5, relevant_count)`), and negative false-positive rate separately. It is a
small engineering regression corpus, not representative user traffic. Labels
and corpus identities are frozen before running both binaries. Historical V1
summaries retain their original metric definition; raw result files stay local.

```sh
python3 scripts/benchmark-lexical.py run --binary /path/to/installed/skillwick \
  --profile benchmarks/profile-v2.json --samples 3 --output /tmp/candidate.json
python3 scripts/benchmark-lexical.py validate --profile benchmarks/profile-v2.json \
  --result /tmp/candidate.json
```

No pre-commit gate performs model inference or downloads model artifacts.
Optional-model adoption still requires independent task outcomes and supported
machine budgets; improving a ranking score alone is insufficient.

## Hosted JEV experiment

JEV is an external, explicitly opted-in SDK experiment. Rust search and read
remain unchanged. Copy [`.env.example`](../.env.example) to `.env` and put
`TYPESAFE_API_KEY` in that ignored local file. Never put a real key in the example
or a command argument. `uv` loads `.env` for the benchmark process.

```sh
cargo build --release --locked
python3 scripts/benchmark-lexical.py run --binary target/release/skillwick \
  --profile benchmarks/profile-v1.json --pool-size 20 --samples 3 \
  --output /tmp/lexical-jev-v1.json

# Explicit live smoke: partial hosted metrics are suppressed.
UV_CACHE_DIR=/private/tmp/skillwick-uv-cache \
uv run --python 3.14 --env-file .env --with typesafe-sdk==0.7.2 \
  python scripts/benchmark-semantic.py jev --live --max-queries 1 \
  --profile benchmarks/profile-v1.json --candidates /tmp/lexical-jev-v1.json \
  --pool-size 20 --output /tmp/jev-smoke.json

# Complete held-out evaluation. Set --expected-model to the returned version
# when the service exposes a versioned model, to detect subsequent drift.
UV_CACHE_DIR=/private/tmp/skillwick-uv-cache \
uv run --python 3.14 --env-file .env --with typesafe-sdk==0.7.2 \
  python scripts/benchmark-semantic.py jev --live \
  --profile benchmarks/profile-v1.json --candidates /tmp/lexical-jev-v1.json \
  --pool-size 20 --output /tmp/jev-v1-pool20.json

# Local comparison uses the same candidates and pinned historical artifacts.
UV_CACHE_DIR=/private/tmp/skillwick-uv-cache \
uv run --python 3.14 --with fastembed==0.8.0 \
  python scripts/benchmark-semantic.py rerank \
  --profile benchmarks/profile-v1.json --candidates /tmp/lexical-jev-v1.json \
  --output /tmp/tinybert-jev-v1.json --cache /private/tmp/skillwick-models

# Deterministic checks: no SDK installation, account, or network required.
make test-benchmark

# Optional real SDK serialization/error check; HTTP transport stays mocked.
UV_CACHE_DIR=/private/tmp/skillwick-uv-cache \
uv run --python 3.14 --with typesafe-sdk==0.7.2 python tests/jev_sdk_contract.py
```

Run V2 separately by substituting `profile-v2.json` throughout, and compare JEV
pool 10 by changing `--pool-size` while retaining the same lexical candidate
file. The production CLI cap is 20; pool 30 is not supported by this experiment.
`--base-url`, `--model`, `--timeout`, and `--confidence-threshold` configure the
hosted decision boundary. Do not tune thresholds using held-out labels.

Outputs retain lexical fallback rankings with explicit failure categories and
the process fails on provider/contract failures. A successful HTTP response or a
fallback ranking is not proof of a successful rerank. See
[methodology, measured results, and recommendation](../docs/research/jev-evaluation.md).

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

- 0.3 V1 (`lexical-0.3.0-profile-v1.json`, local artifact) and 0.4 V1 (`lexical-0.4.0-profile-v1.json`, local artifact)
- 0.3 V2 (`lexical-0.3.0-profile-v2.json`, local artifact) and 0.4 V2 (`lexical-0.4.0-profile-v2.json`, local artifact)

## Reproduce historical experiments

```sh
mkdir -p benchmarks/results
cargo build --release --locked
python3 scripts/benchmark-lexical.py run \
  --binary target/release/skillwick \
  --profile benchmarks/profile-v1.json \
  --output /tmp/lexical-current.json

UV_CACHE_DIR=/private/tmp/skillwick-uv-cache \
uv run --with fastembed==0.8.0 python scripts/benchmark-semantic.py embedding \
  --profile benchmarks/profile-v1.json \
  --output benchmarks/results/embedding-arctic-xs-current.json \
  --cache /private/tmp/skillwick-models

UV_CACHE_DIR=/private/tmp/skillwick-uv-cache \
uv run --with fastembed==0.8.0 python scripts/benchmark-semantic.py rerank \
  --profile benchmarks/profile-v1.json \
  --candidates /tmp/lexical-current.json \
  --output benchmarks/results/reranker-tinybert-lexical-current.json \
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

### Reproduce the historical TinyBERT runtime control

First generate `/tmp/lexical-jev-v1.json` with the hosted experiment’s lexical
command above. The following control consumes that new candidate file.

The historical quality control uses Python 3.10 and ONNX Runtime 1.23.2; the
SDK experiment uses Python 3.14. Runtime sensitivity is recorded in the report.

```sh
UV_CACHE_DIR=/private/tmp/skillwick-uv-cache \
uv run --python 3.10 --with fastembed==0.8.0 --with onnxruntime==1.23.2 \
  python scripts/benchmark-semantic.py rerank \
  --profile benchmarks/profile-v1.json \
  --candidates /tmp/lexical-jev-v1.json \
  --output /tmp/tinybert-historical-runtime.json \
  --cache /private/tmp/skillwick-models
```

### Audit ranking and request processing

First run the complete hosted evaluation above to produce the lexical candidate
file and `/tmp/jev-v1-pool20.json`. The audit consumes those newly generated
files; historical receipts are not required.

Repeat hosted runs now store whitelisted request state/questions, their digest,
question-to-fixture mappings, raw provider order, and fallback-trigger candidates.
SDK debug logging stays disabled. Trace files contain public fixture text only;
credentials, HTTP headers, raw errors, and full SDK responses are excluded.

```sh
python3 scripts/audit_jev_evals.py \
  --profile benchmarks/profile-v1.json \
  --candidates /tmp/lexical-jev-v1.json \
  --result /tmp/jev-v1-pool20.json \
  --output /tmp/jev-v1-audit.json
```

The independent audit recomputes final metrics and raw probability ranking,
checks mappings and fallback policy, and reports candidate-oracle coverage and
dataset limitations. Repeat for V2 and pool 10. Raw ranking is diagnostic evidence
on reused fixtures, not independent acceptance of a replacement confidence policy.
See the October 4 correction in the [report](../docs/research/jev-evaluation.md).

### Setup-selected production verification

The [reranking guide](../docs/RERANKING.md) records installed CLI, library,
failure, and workstation validation. To repeat the frozen V1 live run explicitly:

```sh
cargo build --release --locked
uv run --python 3.14 --env-file .env python scripts/verify-reranker-live.py \
  --binary target/release/skillwick \
  --output /tmp/configured-reranker-live.json \
  --workdir /private/tmp/skillwick-live-new-run
```

Use new output and work-directory paths. The verifier prepares both real backends,
clears the key environment for ordinary search and library tests, and stores
sanitized provider/model/fixture/source identities with the measured rankings.
