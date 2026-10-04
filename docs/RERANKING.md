# Optional search reranking

Setup saves the backend choice. Ordinary searches use it automatically:

```sh
skillwick search "review database migrations"
```

Agent search examples and `SKILLWICK.md` do not need backend flags. Existing
configurations default to lexical-only retrieval, which needs no Python runtime,
model download, or hosted account.

## Choose a backend

Use the current source build; this capability is not in the older released CLI.
`uv` must be available when preparing either optional backend.

```sh
skillwick init --yes --agent none --reranker tinybert
```

Setup prepares Python 3.14 and pinned inference dependencies in private
Skillwick-owned runtime state. It downloads and verifies the TinyBERT tokenizer
and ONNX artifact at the recorded revision, then runs local inference. Searches
load those verified artifacts locally; they never download models or dependencies.
The model is `cross-encoder/ms-marco-TinyBERT-L2-v2`, revision
`81d1926f67cb8eee2c2be17ca9f793c7c3bd20cc`, using ONNX Runtime 1.30.0.

For JEV, supply `TYPESAFE_API_KEY` to setup. With the ignored `.env` file:

```sh
uv run --env-file .env skillwick init --yes --agent none --reranker jev
```

The official TypeSafe SDK 0.7.2 authenticates and validates a smoke request to
`jev-1.13.0` before setup publishes the choice. A successful setup stores the
credential in its private machine-local runtime directory with mode 0600;
configuration and agent instructions contain no key. Later searches do not need
the environment variable or `.env` file. An explicitly supplied environment key
takes precedence over the saved key. SDK debug logging stays disabled.

Selecting JEV authorizes sending the current query and up to 20 candidate names
and descriptions to TypeSafe. Skill bodies, filesystem paths, inventory
provenance, and public candidate IDs stay local. Requests use opaque local labels.
JEV has a 15-second request timeout and zero automatic retries.

To switch backends, repeat setup with the other choice. To disable reranking:

```sh
skillwick init --yes --agent none --reranker none
```

Omitting `--reranker` in non-interactive setup preserves the existing choice.
Interactive setup offers a selection. `--dry-run` prints the plan without
runtime preparation, downloads, provider calls, or writes. Failed preparation
preserves the previous configuration and integration. Prepared runtime material can remain after a failed attempt; it does not enable
reranking by itself. Each JEV setup stages a fresh runtime and credential before
the configuration transaction selects it. Setup can reuse the currently selected
JEV credential when the environment key is absent. Disabling reranking retains
prepared runtimes, but selecting JEV again requires supplying the key.

Ordinary search remains available while a backend prepares. Setup checks that
configuration and integration have not changed before publishing its choice.
Configuration reads recover an interrupted publication before loading the
backend, preserving the old choice when the transaction did not commit.

## Ranking and failures

Skillwick first reconciles inventory freshness and gets up to 20 eligible,
grouped lexical candidates. The selected backend only changes their order.
Original IDs, metadata, origins, applicability, and live-read checks remain
unchanged. The requested result limit is applied after reranking. Empty pools
skip backend work.

TinyBERT ranks finite model scores. JEV makes independent typed relevance
judgements and ranks the probability of relevance. Both preserve lexical order
for equal scores. Production does not use the benchmark's whole-pool confidence
veto, which discarded useful rankings when an unrelated distractor was uncertain.
This is an explicitly selected ranking policy, not a claim of validated task
success; the [research audit](research/jev-evaluation.md) records its limitations.

Missing runtimes or credentials, corrupted artifacts, timeouts, provider errors,
model drift, and invalid response mappings retain lexical results. A sanitized
stderr diagnostic states that lexical order was used. Valid fallback searches
still exit successfully and retain the existing JSON/text interfaces. Operational
inventory failures continue to fail; reranking cannot mask stale or unavailable
sources. The subprocess has a separate 25-second search deadline and bounded I/O.

The public library boundary is `skillwick::reranker::search`. It returns original
result rows plus an optional categorical fallback diagnostic. Its caller must
first reconcile inventory, as the CLI does. The lexical primitive remains
available for callers deliberately requesting the baseline.

## Verification

The existing CLI, setup transaction, trust, and filesystem contracts remain the
primary seams. New contracts exercise ordinary search, setup failure and dry-run,
configured library search, candidate mapping, SDK requests, and fallback. The
live verification runner installs neither agent instructions nor fixture skills
into real workstation roots. It initializes both real backends against frozen
public metadata, then runs ordinary CLI searches and an explicit live library
test. Result receipts distinguish successful reranking from fallback and record
backend, model, binary, profile, and source identities.

The behavior spec is [issue #33](https://github.com/danchurko/skillwick/issues/33).

### October 4 validation

The [installed-CLI and library receipt](../benchmarks/results/configured-reranker-live-2026-10-04.json)
records 105 ordinary searches for each backend on frozen V1 metadata. All three
runs completed without fallback. Both optional backends passed the explicit live
library test with the key environment cleared.

| Backend | Recall@5 | MRR@5 | Median CLI latency | p95 CLI latency |
| --- | ---: | ---: | ---: | ---: |
| Lexical | 0.8857 | 0.8373 | 44.7 ms | 52.4 ms |
| TinyBERT | 0.9429 | 0.9270 | 282.1 ms | 547.9 ms |
| JEV | 0.9714 | 0.9667 | 500.3 ms | 586.7 ms |

CLI latency includes reconciliation and starting the isolated Python runtime on
each search. These reused, correlated relevance fixtures do not establish agent
task success or independent generalization. Production TinyBERT uses the newer
pinned runtime; historical benchmark timings and scores are separate controls.

The [live failure receipt](../benchmarks/results/configured-reranker-failures-2026-10-04.json)
records lexical fallback for missing credentials, missing model artifacts, and
provider rejection, followed by successful searches after restoration. Offline
contracts additionally cover bounded subprocess timeouts, invalid mappings,
model mismatches, failed preparation, dry-run, and transaction recovery.

The [workstation receipt](../benchmarks/results/configured-reranker-workstation-2026-10-04.json)
records initialization and ordinary searches for both backends, with JEV left
selected. The existing roots and canonical agent contexts were preserved, legacy
configuration and executable were backed up, and strict doctor passed. Installation
uses the managed agent-state source-checkout override; this capability has not
been published as a release. Full agent-state apply was not needed or run.

The [initial validation summary](../benchmarks/results/configured-reranker-validation-2026-10-04.json)
records the earlier checks. The broader source-install automatic native-inventory
acceptance remains failed on a pre-existing Ponytail plugin manifest version
mismatch; the reranker tests and workstation explicit-root health checks passed.

The [reviewed source-installed receipt](../benchmarks/results/configured-reranker-live-reviewed-2026-10-04.json)
extends the library proof to all 105 queries for each optional backend, with
complete result-row preservation checks. The
[independent proof](../benchmarks/results/configured-reranker-reviewed-proof-2026-10-04.json)
checks receipt identities, mappings, recomputed CLI/library metrics, and restored
searches after controlled missing-credential, missing-artifact, and provider
rejection failures. The
[reviewed validation summary](../benchmarks/results/configured-reranker-reviewed-validation-2026-10-04.json)
records the final code reviews and checks, including setup concurrency and
interrupted-publication recovery. Source-install validation with private provider
homes uses the real configured skill roots; the broader native-inventory failure
remains separately recorded.
