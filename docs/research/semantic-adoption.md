# Semantic retrieval adoption decision

Status: decided 14 September 2026. Skillwick remains lexical-only.

This decision uses the frozen 531-record profile and 105 held-out query
perspectives in [`benchmarks/profile-v1.json`](../../benchmarks/profile-v1.json).
The labels were frozen before these runs. They are reviewable
metadata-derived cases, not sampled user traffic.

## Findings

| Retrieval path | Recall@5 | MRR@5 | nDCG@5 | Median incremental/model query | Peak RSS | Stored model/index |
|---|---:|---:|---:|---:|---:|---:|
| Lexical FTS5 | 0.886 | 0.837 | 0.850 | 59.5 ms full CLI | 12.6 MiB | 1.14 MiB index |
| Arctic XS embedding | 0.743 | 0.630 | 0.659 | 4.0 ms loaded model | 812 MiB | 86.2 MiB model |
| Lexical top 20 + TinyBERT | 0.952 | 0.929 | 0.935 | 45.6 ms reranking | 184 MiB | 4.31 MiB model |
| Arctic top 20 + TinyBERT | 0.905 | 0.895 | 0.898 | 13.9 ms reranking | 189 MiB | both models |

The timing columns are not interchangeable. Lexical timing includes a fresh
covered CLI process and filesystem scan; model timing is inference inside an
already loaded Python/ONNX process. Arctic corpus embedding took 24.9 seconds
and its cold load took 140 ms. TinyBERT cold load took 196 ms for the lexical
candidate experiment. Measurements are from one Apple Silicon machine and do
not establish other-platform performance.

- Arctic XS loses substantial quality on every recorded metric. Its artifact,
  memory, indexing, and runtime costs therefore have no compensating benefit.
- TinyBERT improves the frozen lexical ranking by 0.067 Recall@5, 0.092 MRR@5,
  and 0.085 nDCG@5. That is promising research evidence, but not sufficient to
  add a Python/ONNX lifecycle, a model artifact, roughly 46 ms median loaded
  inference, and roughly 184 MiB peak process memory to the shipped Rust CLI.
- Reranking Arctic candidates recovers much of the embedding loss but remains
  worse than reranking lexical candidates. There is no evidence for the
  combined path.
- Both runners verify pinned ONNX checksums before offline loading. Missing or
  corrupt artifacts fail deterministically in the research runner, and neither
  experiment changes or participates in lexical search.

## Decision

FTS5 remains the only production retrieval path. Do not add an embedding or
reranking runtime, model download, vector store, fallback branch, or product
configuration in this release.

Arctic XS is rejected for this profile. TinyBERT merits a separately approved
optional-product specification only after validation on independently sourced
user tasks and supported-machine measurements. Such a proposal must keep the
model local and optional, pin and verify every artifact, document CPU, memory,
storage, and platform requirements, bound the candidate pool, and leave
lexical search deterministic when the model is absent or corrupt.

## Evidence

- [Lexical baseline](../../benchmarks/results/lexical-baseline-2026-09-13.json)
- [Arctic XS](../../benchmarks/results/embedding-arctic-xs-2026-09-13.json)
- [TinyBERT over lexical candidates](../../benchmarks/results/reranker-tinybert-lexical-2026-09-13.json)
- [TinyBERT over Arctic candidates](../../benchmarks/results/reranker-tinybert-embedding-2026-09-13.json)
- [Reproduction commands and pre-run shortlist](../../benchmarks/README.md)
