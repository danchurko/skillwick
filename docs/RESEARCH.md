# Research record

This page records dated source review and experiments that informed the shipped
local lookup boundary. It is not a promise of current model quality, a
security audit, or a production dependency list. The supported path is
configured filesystem discovery plus local SQLite FTS5.

## Filesystem authority review

The current product resolves automatic and explicit roots, then reads installed
files as content authority. Provider metadata selects eligible active plugins.
Project roots apply only in their configured workspace and descendants.
The cache is rebuildable derived state.

The implementation review established these boundaries:

- A complete scoped scan is required before publishing a derived snapshot.
- Root configuration, canonical instruction identity, bounded file contents,
  and adjacent invocation-policy identity/presence/content participate in
  freshness.
- Overlapping roots deduplicate by canonical instruction path for public
  output while retaining raw root associations for diagnostics.
- Symlink escapes, malformed metadata, missing roots, and unreadable inputs
  fail closed and preserve the last complete cache.
- Selected reads revalidate the live file. Discovery and package inspection do
  not execute instruction or supporting files.

These are implementation boundaries, not claims about package ownership.
Existing installers and users continue to own skill directories and updates.

## Historical agent-inventory review

Earlier releases evaluated a Codex-native inventory contract. Codex CLI 0.154.0
exposed `skills.include_instructions = false` and a newline-delimited
`skills/list` app-server contract. That work is retained as historical release
evidence only; the current product does not use that app-server catalogue. Codex plugin
eligibility instead uses a bounded `codex plugin list --json` call.

The dated source review used these pinned references:

- [Codex skills configuration](https://github.com/openai/codex/blob/rust-v0.154.0/codex-rs/config/src/skills_config.rs)
- [Codex skills extension](https://github.com/openai/codex/blob/rust-v0.154.0/codex-rs/ext/skills/src/extension.rs)
- [Codex catalogue rendering](https://github.com/openai/codex/blob/rust-v0.154.0/codex-rs/ext/skills/src/world_state_catalogs.rs)
- [Codex inventory schema](https://github.com/openai/codex/blob/rust-v0.154.0/codex-rs/app-server-protocol/src/protocol/v2/plugin.rs)

No current compatibility claim follows from that historical review. A future
agent integration would need a separate product decision, ownership boundary,
compatibility fixture, and live evidence.

## Historical semantic evaluation

On 13 September 2026, an offline benchmark evaluated a frozen profile of 531
model-discoverable records and 105 held-out query perspectives. The artifacts
remain in `benchmarks/` so the experiment can be reproduced. They measure
retrieval only—not task success, selection quality, instruction following, or
safety.

| Path | Recall@5 | MRR@5 | nDCG@5 | Resource observation |
| --- | ---: | ---: | ---: | --- |
| [Lexical](../benchmarks/results/lexical-baseline-2026-09-13.json) | 0.886 | 0.837 | 0.850 | warm p95 116.2 ms; index 1.20 MB |
| [Arctic XS](../benchmarks/results/embedding-arctic-xs-2026-09-13.json) | 0.743 | 0.630 | 0.659 | 90.4 MB artifact; peak RSS 812 MiB |
| [TinyBERT over lexical](../benchmarks/results/reranker-tinybert-lexical-2026-09-13.json) | 0.952 | 0.929 | 0.935 | 4.52 MB artifact; warm p95 136.6 ms; peak RSS 184 MiB |

The embedding candidate was `Snowflake/snowflake-arctic-embed-xs`, revision
`d8c86521100d3556476a063fc2342036d45c106f`, with the measured ONNX SHA-256
`cf2698d30ff05da02c70a088313bad56e5c2f401d734cb24a8390d446111936c`. The
reranker was `cross-encoder/ms-marco-TinyBERT-L2-v2`, revision
`81d1926f67cb8eee2cbe17ca9f793c7c3bd20cc`, with ONNX SHA-256
`7497b40504d425ef6482693039690106dca4f1f8d88fb5c4aedd63e73ed6ef68`.
Neither artifact is a Skillwick runtime dependency.

The decision remains lexical-only production retrieval. Any future model
proposal must preserve deterministic lexical behavior when unavailable, pin
and verify artifacts, and report quality, selected-instruction loading,
candidate-pool size, latency, memory, storage, and failure behavior on a
reviewable corpus.

The full comparison and adoption rationale are in the
[semantic adoption decision](research/semantic-adoption.md).

## Durable lessons

- Metadata search and instruction-body loading remain separate.
- A count check does not prove metadata identity, ordering, or policy coverage.
- Operational errors remain visible instead of becoming empty search results.
- One local FTS5 index is easier to keep consistent than separate metadata and
  text stores at this scale.
- Package ownership, agent configuration, and Skillwick's derived state stay
  separate.

These lessons are design context, not endorsements of compared projects or
claims that unexecuted source was production-ready. Current behavior belongs
in the [architecture](ARCHITECTURE.md), [decisions](DECISIONS.md), and
[security policy](../SECURITY.md).
