# Synthetic pre-merge acceptance

The user chose synthetic acceptance before merge and real installed-source
qualification afterward. [Follow-up #49](https://github.com/danchurko/skillwick/issues/49)
preserves the original real-corpus requirements from #46, native installation
checks, historical replays, and unresolved ranking findings. This change in
sequencing does not establish production installation or real-task success.

## Frozen comparison

The existing `benchmarks/profile-v2.json` freezes 12 sanitized records and
20 cases: 16 positive cases and four negatives, including vocabulary mismatches,
multiple relevant labels and distinct same-name packages. Most cases are
synthetic; one is a sanitized task. Labels predate both runs. Profile SHA-256:
`1b070dc155c6adcc17f223616d001ec409939bffe3d5d22ca54e7bb75ddb5bba`.
Corpus SHA-256:
`67a3ce6b805e04bd854ca7b48e56c457aa3d379cc2c966eae43b531b87972e70`.

Both runs used candidate pool 20. Lexical search used the controlled installed
Skillwick 0.4.0 executable with SHA-256
`b6e4b9532d44ed561527a4676c92c87c9438d2da62f412dfc3c223f2d249b754`.
TinyBERT reranked those exact lexical candidate identities offline with the
existing pinned model and tokenizer. It used FastEmbed 0.8.0, ONNX Runtime
1.30.0 and Python 3.14.8 on arm64 macOS. Both receipts bind evaluator revision
`a29c4506c50d0bd8100eb19a63edad746caab2b7` and implementation hashes. The TinyBERT
receipt reports a dirty tree because the runtime emitted a local telemetry
artifact; that artifact was moved under the ignored result directory. This is
source fingerprint evidence, not a claim that the executable was built at that
revision.

| Measure | Lexical | Offline TinyBERT |
| --- | ---: | ---: |
| Positive Recall@5 / @20 | 0.8125 | 0.8125 |
| Positive MRR@5 | 0.875 | 0.875 |
| Positive nDCG@5 | 0.828114 | 0.828114 |
| Negative false-positive rate | 0/4 | 0/4 |
| Relevant first, positive cases | 14/16 | 14/16 |

TinyBERT changed one ordering, with zero material improvements, regressions,
or relevant displacements on these reused cases. This supports keeping current
behavior. It does not imply perfect coverage or justify a new default. A
reranker cannot recover relevant candidates omitted by lexical retrieval.
The separate 171-record explicit-subset run still has two negative false
positives; those findings remain in #49.

The existing profile/result validator accepted both 20-query receipts against
the same profile. `make test-benchmark` and the controlled installation,
selection, freshness, body delivery, policy, owner provisioning, and actual
sandbox root/child contracts are recorded in [managed reliability](managed-reliability.md).
No hosted model evaluation, package script, authentication, production apply,
or release was run for this comparison.

## Native installation audit boundary

The managed owner installs native capabilities before initializing and
verifying Skillwick. Ponytail is registered, installed and enabled in Codex,
but native inventory reports version `1.0.0` while the active package's Codex
and Claude manifests declare `4.10.3`. Skillwick automatic discovery rejects
that disagreement. The active cache and marketplace checkout match commit
`c982cd411abb53323c4baa1baa3c2f020b8d0b08`; their manifests agree. The root
agent-plugin manifest has a schema and name but no version. This is evidence
of a provider/discovery metadata disagreement, not stale package contents or
a demonstrated installer failure. Determine the authoritative version semantics
and correct the responsible layer in #49; no guessed cache was selected.

The AWS MCP server is configured and connected in the current session. Its
skill search returned `amazon-bedrock` and `aws-containers`, and complete skill
retrieval succeeded for both. AWS explicitly supports MCP-delivered skills
whose reference files are retrieved remotely, as well as locally installed
packages. Local `amazon-bedrock`, `aws-containers`, and `aws-deployment`
packages also exist in the native shared skill root, and `codebase-memory`
exists in the native Codex root. The restricted two-generated-root explicit
configuration omits them. Their absence from that selected index does not
establish failed installation or failed MCP setup.
This proof covers those read paths only; it does not verify all AWS skills,
account permissions, or deployment. Intended source coverage and real queries
remain follow-up work.

## Reproduce

Use a fresh output directory under ignored `benchmarks/results/`, an installed
candidate binary, and the existing prepared TinyBERT model cache. The model
artifacts must already match the pinned checksums; `--offline` prevents model
downloads. The benchmark environment requires the dependencies documented in
[benchmark instructions](../../benchmarks/README.md).

```sh
mkdir -p benchmarks/results/synthetic-new-run
python3 scripts/benchmark-lexical.py run \
  --binary /path/to/installed/skillwick \
  --profile benchmarks/profile-v2.json --pool-size 20 --samples 1 \
  --output benchmarks/results/synthetic-new-run/lexical.json

# Set this to the models directory of an existing prepared TinyBERT runtime.
task_model_cache=/path/to/prepared/tinybert/models
UV_OFFLINE=1 uv run --python 3.14 --with fastembed==0.8.0 \
  python scripts/benchmark-semantic.py rerank \
  --profile benchmarks/profile-v2.json \
  --candidates benchmarks/results/synthetic-new-run/lexical.json \
  --output benchmarks/results/synthetic-new-run/tinybert.json \
  --cache "$task_model_cache" --offline

python3 scripts/benchmark-lexical.py validate \
  --profile benchmarks/profile-v2.json \
  --result benchmarks/results/synthetic-new-run/lexical.json \
  --result benchmarks/results/synthetic-new-run/tinybert.json
```

Raw rankings, request traces, runtime artifacts and receipts remain local and
ignored. Summaries, reproducible inputs and evaluator code remain tracked.
