# Local English embedding and reranking models

Reviewed 12 September 2026. This is a source-backed selection for Skillwick,
not a benchmark run on this Mac. Sources are model cards/configuration files
and the local repository specification; model metadata was checked on the
review date.

## Recommendation

Use `Snowflake/snowflake-arctic-embed-xs` as the embedding candidate. It is an
English retrieval-specific model with 22.6M parameters, 384-dimensional
vectors, Apache-2.0 licensing, and published ONNX/quantized variants. Its
card reports MTEB retrieval NDCG@10 of 50.15, versus 41.95 for
`all-MiniLM-L6-v2` at comparable size; those are self-reported card results,
not Skillwick measurements. Arctic XS requires the documented query prefix and
CLS pooling. [Arctic card](https://huggingface.co/Snowflake/snowflake-arctic-embed-xs),
[Arctic config](https://huggingface.co/Snowflake/snowflake-arctic-embed-xs/blob/main/config.json),
[Arctic ONNX files](https://huggingface.co/Snowflake/snowflake-arctic-embed-xs/tree/main/onnx)
(accessed 2026-09-12).

Use `cross-encoder/ms-marco-TinyBERT-L2-v2` as the reranker only if a
Skillwick benchmark proves reranking improves results. It is English,
Apache-2.0, ONNX-published, and only 4.39M parameters. The model card reports
TREC DL19 NDCG@10 69.84, MS MARCO MRR@10 32.56, and 9,000 documents/second on
a V100. Those benchmark and hardware numbers are self-reported and are not a
Mac CPU promise. [TinyBERT card](https://huggingface.co/cross-encoder/ms-marco-TinyBERT-L2-v2),
[TinyBERT ONNX files](https://huggingface.co/cross-encoder/ms-marco-TinyBERT-L2-v2/tree/main/onnx)
(accessed 2026-09-12).

Arctic XS accepts up to 512 tokens in its documented usage, which is adequate
for Skillwick's bounded metadata rather than whole instructions. Keep the
reranker limited to a small candidate pool because a cross-encoder scores each
query/document pair jointly. [Arctic usage](https://huggingface.co/Snowflake/snowflake-arctic-embed-xs#usage),
[Skillwick spec](SPEC.md#5-indexing-freshness-and-search) (accessed
2026-09-12).

## Shortlist

| Role | Model | HF safetensors parameter count | Output/config | Context | License | Assessment |
|---|---|---:|---|---:|---|---|
| Embedding | `Snowflake/snowflake-arctic-embed-xs` | 22.6M | 384-d vector; retrieval-specific Arctic/BERT | 512 tokens | Apache-2.0 | Recommended: retrieval-trained and smallest quality/footprint balance; requires query prefix and CLS pooling. |
| Embedding | `sentence-transformers/all-MiniLM-L6-v2` | 22.7M | 384-d vector; BERT, 6 layers | 512 positions; card training length 128 | Apache-2.0 | Simple baseline alternative; card-reported retrieval quality is below Arctic XS. |
| Embedding | `BAAI/bge-small-en-v1.5` | 33.4M | 384-d vector; BERT, 12 layers | 512 positions | MIT | Quality-oriented alternative with roughly 47% more parameters than MiniLM. It requires the documented retrieval query prefix for best use. |
| Embedding | `jinaai/jina-embeddings-v2-small-en` | 32.7M | 512-d vector; JinaBERT, 4 layers | 8,192 positions | Apache-2.0 | Attractive long-context/CPU option, but larger vectors and custom-code/runtime considerations are unnecessary for Skillwick metadata. |
| Reranker | `cross-encoder/ms-marco-TinyBERT-L2-v2` | 4.39M | scalar relevance logit; TinyBERT, 2 layers | card-defined pair limit | Apache-2.0 | Recommended only after a local quality gate; vastly smallest and card reports 9,000 docs/sec on V100. |
| Reranker | `cross-encoder/ms-marco-MiniLM-L2-v2` | 15.6M | scalar relevance logit; MiniLM, 2 layers | 512 positions | Apache-2.0 | Small alternative; card reports higher MS MARCO/TREC scores but 4,100 docs/sec on V100. |
| Reranker | `cross-encoder/ms-marco-MiniLM-L6-v2` | 22.7M | scalar relevance logit; BERT, 6 layers | 512 positions | Apache-2.0 | Quality-oriented alternative, not the smallest; use only if its measured gain matters. |
| Reranker | `jinaai/jina-reranker-v1-turbo-en` | 37.8M | scalar relevance score; English | model-defined | Apache-2.0 | Direct fastembed integration fallback when convenience matters more than minimum footprint. |
| Reranker | `BAAI/bge-reranker-base` | 278.0M | scalar relevance logit; XLM-R, 12 layers | 514 positions | MIT | More capable and English-capable, but about 12x the parameters; slower and needlessly multilingual for this requirement. |

Parameter counts and tags above come from the Hugging Face model API's
published safetensors metadata; architecture and position values come from
each model's published `config.json`. [MiniLM embedding API](https://huggingface.co/api/models/sentence-transformers/all-MiniLM-L6-v2),
[BGE API](https://huggingface.co/api/models/BAAI/bge-small-en-v1.5),
[Jina API](https://huggingface.co/api/models/jinaai/jina-embeddings-v2-small-en),
[Arctic API](https://huggingface.co/api/models/Snowflake/snowflake-arctic-embed-xs),
[TinyBERT reranker API](https://huggingface.co/api/models/cross-encoder/ms-marco-TinyBERT-L2-v2),
[MiniLM-L2 reranker API](https://huggingface.co/api/models/cross-encoder/ms-marco-MiniLM-L2-v2),
[MiniLM-L6 reranker API](https://huggingface.co/api/models/cross-encoder/ms-marco-MiniLM-L6-v2),
[Jina turbo API](https://huggingface.co/api/models/jinaai/jina-reranker-v1-turbo-en),
[BGE reranker API](https://huggingface.co/api/models/BAAI/bge-reranker-base),
[BGE config](https://huggingface.co/BAAI/bge-small-en-v1.5/blob/main/config.json),
[Jina config](https://huggingface.co/jinaai/jina-embeddings-v2-small-en/blob/main/config.json),
[BGE reranker config](https://huggingface.co/BAAI/bge-reranker-base/blob/main/config.json)
(accessed 2026-09-12).

Quality numbers should not be mixed across cards: they use different datasets,
metrics, and evaluation revisions. BGE's card explicitly positions the v1.5
family as retrieval embeddings and its reranker as more accurate but less
efficient; that supports keeping BGE-small as a quality fallback, not
proving it will improve Skillwick's skill-ranking precision. [BGE model list](https://huggingface.co/BAAI/bge-reranker-base#model-list),
[BGE evaluation records](https://huggingface.co/BAAI/bge-small-en-v1.5#evaluation)
(accessed 2026-09-12).

## Integration shape

Keep the existing SQLite FTS5 path authoritative and add semantic retrieval
only behind an explicit optional profile. At refresh, embed the compact
`name + description + keywords` text already owned by the index; store vectors
keyed by model revision, preprocessing version, dimension, and content hash.
At query time, retrieve a bounded semantic candidate set, fuse it with lexical
results using an evaluated rank-fusion method, then optionally rerank only the
shortlist. This follows the existing roadmap and avoids a mandatory daemon or
network client. [Hybrid-retrieval roadmap](SPEC.md#12-future-development--gated-by-evidence) (accessed
2026-09-12).

For a native Rust binary, `fastembed-rs` is the easiest embedding path: its
current README lists Arctic XS and quantized variants as supported models.
The README's reranker enum does not list TinyBERT-L2, so use a small custom
ONNX Runtime path or fastembed's user-defined local-model mechanism for that
model. If integration simplicity outweighs the 4.39M-parameter target,
`jinaai/jina-reranker-v1-turbo-en` is a directly supported fastembed fallback
(37.8M parameters), not the efficiency recommendation. Do not add a runtime
until a held-out Skillwick query set demonstrates semantic misses that FTS5
cannot recover. [fastembed-rs README](https://github.com/anush008/fastembed-rs#readme),
[Arctic ONNX files](https://huggingface.co/Snowflake/snowflake-arctic-embed-xs/tree/main/onnx),
[TinyBERT ONNX files](https://huggingface.co/cross-encoder/ms-marco-TinyBERT-L2-v2/tree/main/onnx),
[ort Rust binding](https://github.com/pykeio/ort) (accessed 2026-09-12).

## Decision gate

Before adoption, measure on representative English skill queries: Recall@5
for candidate retrieval, MRR/NDCG@5 for final ranking, cold startup, warm
query latency, peak RSS, model download/storage size, and behavior when the
model is unavailable. Compare Arctic XS against all-MiniLM; compare TinyBERT-L2
against MiniLM-L2/L6; retain lexical-only behavior if the quality gain does
not justify the startup and distribution cost. This is required because the
repository's current performance targets are goals, not measured embedding
results. [Performance and hybrid requirements](SPEC.md#12-future-development--gated-by-evidence)
(accessed 2026-09-12).
