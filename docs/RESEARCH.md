# Research record

This page records dated source review and experiments that informed the shipped
local lexical boundary. It is not a promise of current model quality, a
security audit, or a production dependency list. The production path remains
local SQLite FTS5 and the version-bound Codex native inventory documented in
the [architecture](ARCHITECTURE.md) and [compatibility](COMPATIBILITY.md)
pages.

## Native boundary

Codex CLI 0.154.0 exposes `skills.include_instructions = false` and the
newline-delimited `skills/list` app-server contract. The latter is an
inventory, not task-ranked search, so Skillwick uses it only to obtain the
native records that Codex already owns. It does not reconstruct private plugin
directories, move installed files, or replace Codex's enablement and
permission decisions.

The dated source review used these pinned references:

- [Codex skills configuration](https://github.com/openai/codex/blob/rust-v0.154.0/codex-rs/config/src/skills_config.rs)
- [Codex skills extension](https://github.com/openai/codex/blob/rust-v0.154.0/codex-rs/ext/skills/src/extension.rs)
- [Codex catalogue rendering](https://github.com/openai/codex/blob/rust-v0.154.0/codex-rs/ext/skills/src/world_state_catalogs.rs)
- [Codex native inventory schema](https://github.com/openai/codex/blob/rust-v0.154.0/codex-rs/app-server-protocol/src/protocol/v2/plugin.rs)

The source boundaries were reviewed on 11 September 2026. A new Codex
version requires a fresh source review, compatibility fixture, and integration
run before native catalogue policy changes.

## Historical semantic evaluation

On 13 September 2026, the benchmark harness evaluated the same frozen profile
of 531 model-discoverable native records and 105 held-out query perspectives.
These are offline retrieval measurements; they do not measure agent task
success, selection quality, instruction following, or safety. The artifacts
remain in `benchmarks/` so a future experiment can reproduce the inputs and
report retrieval, resource, and artifact measurements separately.

| Path | Recall@5 | MRR@5 | nDCG@5 | Resource observation |
| --- | ---: | ---: | ---: | --- |
| [Lexical](../benchmarks/results/lexical-baseline-2026-09-13.json) | 0.886 | 0.837 | 0.850 | warm p95 93.8 ms; index 1.20 MB |
| [Arctic XS](../benchmarks/results/embedding-arctic-xs-2026-09-13.json) | 0.743 | 0.630 | 0.659 | 90.4 MB artifact; peak RSS 821 MiB |
| [TinyBERT over lexical](../benchmarks/results/reranker-tinybert-lexical-2026-09-13.json) | 0.952 | 0.929 | 0.935 | 4.52 MB artifact; warm p95 53.5 ms; peak RSS 206 MiB |

The embedding candidate was `Snowflake/snowflake-arctic-embed-xs`, revision
`d8c86521100d3556476a063fc2342036d45c106f`, with the measured ONNX SHA-256
`cf2698d30ff05da02c70a088313bad56e5c2f401d734cb24a8390d446111936c`. The
reranker was `cross-encoder/ms-marco-TinyBERT-L2-v2`, revision
`81d1926f67cb8eee2c2be17ca9f793c7c3bd20cc`, with ONNX SHA-256
`7497b40504d425ef6482693039690106dca4f1f8d88fb5c4aedd63e73ed6ef68`.
Both artifacts were downloaded and executed locally for this experiment, but
neither is a Skillwick runtime dependency. The reranker result is a useful
candidate-pool experiment, not evidence that adding model startup, memory,
or artifact supply-chain cost improves end-user work.

The current decision is therefore to keep lexical retrieval authoritative and
to leave semantic retrieval optional and research-only. Any proposal to add a
model must preserve lexical fallback and report, on the same held-out profile,
retrieval quality, final selection, selected-instruction loading, root-context
size, input/output/cache tokens, latency, memory, artifact size, and unavailable
model behavior. Vectors would also need a key containing model revision,
preprocessing, dimension, and content hash; reranking would be limited to a
small lexical candidate pool.

The complete comparison, including the embedding-plus-reranker run and the
adoption rationale, is recorded in the
[semantic adoption decision](research/semantic-adoption.md).

## Source-review lessons

The earlier comparison of local and hosted skill-search projects produced a
few durable boundaries:

- Metadata search and instruction-body loading should remain separate. A
  prefix-only cache key or a remote first-run sync is not sufficient for a
  correctness-preserving local cache.
- A vector-count check does not detect metadata reordering, and a quality
  threshold is not the same thing as relevance or trust. Operational errors
  must remain visible instead of becoming empty search results.
- A single FTS5 database is easier to keep consistent than separate metadata
  and text indexes at this scale. Optional semantic work must not make a model
  service a prerequisite for ordinary search.
- Native plugin ownership includes tools, hooks, configuration, credentials,
  and package state. Existing installers and Codex retain that ownership;
  Skillwick owns only its derived index, integration files, and diagnostics.

These lessons are design context, not endorsements of the compared projects
or claims that their unexecuted source was production-ready. Current behavior
and limits belong in the [implementation history](IMPLEMENTATION.md) and
[security policy](../SECURITY.md).
