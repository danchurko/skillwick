# Source review — local skill discovery

Reviewed 11 September 2026. This is a targeted **static source review**, not a security audit or performance benchmark. The implementations were not installed or executed. The assessment distinguishes code behavior from design judgments and README claims. Main-branch links can change; pinned commit/release links and inspected blob hashes are recorded where available.

## Recommendation

Do not adopt any of the five projects unchanged for this particular workstation use case. Their useful ideas are real, but most solve remote skill discovery, hosted MCP delivery, or retrieval research rather than local installed-package discovery with reversible Codex integration. Build the thin missing layer on maintained libraries and native Codex mechanisms. Do not fork a large retrieval stack merely to avoid writing a small CLI.

The most important correction to the earlier direction is native catalogue suppression. `skills.include_instructions = false` exists in the inspected **0.154.0 release**, so relocating installed files is unnecessary on a compatible client. Native listing/reading is also further along than a simple startup metadata list, although the inspected list tool is not ranked search. See the release-tagged sources in `SPEC.md`.

## 1. Jignesh-Ponamwar/skills-mcp

**Language and components:** Python; MCP/FastMCP; Qdrant client; Pydantic; frontmatter/YAML parsing; Cloudflare embedding/deployment components. The manifest separates seed/local-server dependencies rather than making every package part of the base install.

**Inspected:** [manifest](https://github.com/Jignesh-Ponamwar/skills-mcp/blob/main/pyproject.toml), [retrieval implementation](https://github.com/Jignesh-Ponamwar/skills-mcp/blob/main/skill_mcp/tools/find_skills.py), tool directory and architecture documentation. Retrieval file blob: `b5d2e287147912720dbe1b91eb57c26b0d510dca`.

**Good:** retrieval returns discovery metadata and directs the caller to load the body separately. The function bounds query length and result count, handles empty queries, and has an explicit no-strong-match path. Separating catalogue metadata from instruction bodies is the right interface principle to borrow.

**Concrete problem:** the cache key contains only the first 200 query characters plus top-k, even though the complete query is embedded. Two different queries with an identical prefix can therefore return the same cached response. That is a correctness issue visible directly in the function, not a benchmark inference. An index-generation component is also absent from this key, so cache invalidation needs consideration after corpus updates.

**Trade-offs:** default score-based usage labels are model/corpus-specific, not universal confidence estimates. The hosted/vector-store setup and separate script-execution tools broaden operational and security scope well beyond local read-only discovery. This is a good architecture reference, not a dependency to embed wholesale.

**Borrow:** search/body/reference separation and explicit bounds. **Avoid:** prefix-only cache keys, mandatory service infrastructure, or executing skill scripts from the search layer.

## 2. yya007/SkillFinder

**Language and components:** Python; FAISS; NumPy; requests; local Ollama and a Qwen3 embedding model. The agent's `SKILL.md` workflow adds interpretation/reranking beyond the Python candidate retrieval function.

**Inspected:** [search runtime](https://github.com/yya007/SkillFinder/blob/main/scripts/search.py) and project documentation. The inspected range includes embedding, index loading, filters, formatting, and a GitHub fallback helper; the complete end-to-end CLI fallback behavior was not executed.

**Good:** query vectors are normalized, index/metadata record counts are checked, unavailable embedding service errors are explicit, and remote candidates can be distinguished from indexed sources. These are practical numerical/data-integrity checks.

**Trade-offs:** the runtime can start `ollama serve` and wait for readiness. It loads the FAISS index and metadata JSONL, then applies attribute filters after retrieving a bounded candidate pool. Those choices create cold-start costs and can underfill valid results. The `propose` parameter is not simply the number emitted: the search layer can return a multiple of it for agent-side selection. A vector-count check cannot detect a same-length metadata reorder.

**Fit:** useful for semantic discovery over a prebuilt public catalogue; unnecessarily heavy for a native local lexical CLI. The README's latency claims were not independently validated and should not be transplanted to cold starts on a Mac.

**Borrow:** normalization, alignment checks, separate candidate selection. **Avoid:** requiring a model service before ordinary search works, or silently expanding local discovery to remote services. The existence of a fallback helper alone does not prove when it is invoked.

## 3. jo-inc/safe-skill-search

**Language and components:** Rust; clap; Tokio; reqwest/rustls; rusqlite with bundled SQLite; Tantivy; serde/serde_json; YAML parsing; tracing. Default branch is `master`.

**Inspected source at commit `49cc0bdb4e68d3639a3c5729ac3a9b471dbb73a6`:** [Cargo.toml](https://github.com/jo-inc/safe-skill-search/blob/49cc0bdb4e68d3639a3c5729ac3a9b471dbb73a6/Cargo.toml), [search index](https://github.com/jo-inc/safe-skill-search/blob/49cc0bdb4e68d3639a3c5729ac3a9b471dbb73a6/src/index.rs), [CLI](https://github.com/jo-inc/safe-skill-search/blob/49cc0bdb4e68d3639a3c5729ac3a9b471dbb73a6/src/bin/safe-skill-search.rs).

**Good:** native binary shape; real lexical retrieval; optional JSON rather than JSON-only output; source metadata; inline tests for creation, rebuild/search, registry filtering, limits, and body matches. These tests were inspected, not run.

**Concrete behavior to avoid:** missing quality scores become zero while the default minimum is 80. Unscored new/private skills disappear by default. This is separate from retrieval relevance or security approval. Search retrieves `limit * 4` candidates and then applies additional quality/trust filters, so valid later results may never be considered. Database hydration errors are converted through `.ok().flatten()`, which can hide operational faults as missing results.

**Operational mismatch:** first launch can synchronize remote registries before dispatching the requested command. It is not a strictly offline local-file search tool. SQLite holds metadata while a separate Tantivy index holds search state; rebuilding both adds consistency/maintenance work that one FTS5 database avoids at this scale. Raw query-parser input also needs punctuation/syntax tests before it is used as a natural-language interface. The primary text index includes full instruction bodies, so it does not follow a metadata-only design.

**Borrow:** Rust CLI ergonomics and retrieval fixtures. **Avoid:** a mandatory remote first-run sync, quality-as-relevance gating, swallowed database errors, and a second index without a demonstrated need.

## 4. andreas-timm/skills

**Language and components:** TypeScript/Bun, `bun:sqlite`, cac, gray-matter/YAML, Zod, TOML helpers, and several maintainer-owned CLI/config/logging packages. Hugging Face Transformers is optional rather than mandatory for text search.

**Inspected:** [package manifest](https://github.com/andreas-timm/skills/blob/main/package.json), [query implementation at commit `1b18e29eec903afde1bcd278eb7f09546162126a`](https://github.com/andreas-timm/skills/blob/1b18e29eec903afde1bcd278eb7f09546162126a/src/features/search/query.ts), and project documentation. The query file review covered FTS, LIKE fallback, occurrence/approval/tag handling and ordering; it was not an exhaustive audit of the entire repository.

**Good:** closest to the local-inventory problem. It uses weighted FTS5, parameter bindings, deterministic ordering, occurrence/provenance data, and an escaped LIKE fallback for certain FTS syntax errors. Optional embeddings preserve a simpler text path. The project calls itself a research prototype, which is a useful honest maturity signal.

**Trade-offs visible in code:** approved status sorts ahead of relevance. That may suit an approval-oriented registry, but a weak approved match can outrank a much more relevant unapproved one. Its LIKE fallback requires all whitespace-separated terms, which is restrictive for natural sentences with filler words. The function loads occurrence/status/tag maps across the database even when returning a small result set. That is straightforward code, but not necessarily the lightest query path.

**Fit:** useful reference for SQLite search and provenance. Adopting the whole application also adopts its Bun-specific database layer, approval/product model, and helper-package ecosystem. That is more commitment than this tool needs; it is not proof that Bun itself is unsuitable for native distribution.

**Borrow:** one-database approach, safe fallback, provenance. **Avoid:** copying approval-first ranking into a relevance tool or building an approval registry that the user did not request.

## 5. EverMind-AI/SkillCorpus

**Language and components:** Python; PyTorch; Hugging Face Transformers; a bi-encoder and cross-encoder; corpus/training/evaluation infrastructure. The reviewed serving component uses Python's `ThreadingHTTPServer`, not FastAPI.

**Inspected at commit `c82ca38dfd49a74bba656305915b80b56e9f17fc`:** [model server](https://github.com/EverMind-AI/SkillCorpus/blob/c82ca38dfd49a74bba656305915b80b56e9f17fc/skillcorpus/match/serve.py), retrieval/evaluation source search, manifest and documentation.

**Good:** real engineering attention to model behavior: batched calls, normalized embeddings, explicit GPU-call locking, a request-body limit, and warnings about model/config compatibility affecting retrieval. Keeping evaluation and model artifacts explicit is a useful roadmap pattern.

**Trade-offs:** two loaded models and an HTTP service are disproportionate for the first local lexical version. Default device selection is CUDA when available, otherwise CPU; it does not automatically select Apple's MPS backend. The server is a model-serving building block, not an installed-skill lifecycle manager. Its body-size check is not equivalent to complete input/authentication/security hardening.

**Correction:** the reviewed README distinguishes a downloadable 1,000-skill demo from the full hosted catalogue. Do not conflate a claimed corpus size with a complete locally released dataset, or model scores with proof of end-user task success.

**Borrow:** evaluation discipline and versioned embedding behavior later. **Avoid:** training/serving infrastructure in v1 or loading a reranker merely because another project has one.

## Installer and native integration findings

The [OpenAI installer](https://github.com/openai/skills/blob/main/skills/.system/skill-installer/scripts/install-skill-from-github.py) has `--dest` for the destination; `--path` selects paths inside the source repository. It refuses an existing destination instead of being a general in-place updater. A migration plan must not confuse these two flags.

[Vercel skills](https://github.com/vercel-labs/skills) documents global/agent selection and copy behavior. Its [custom destination request](https://github.com/vercel-labs/skills/issues/222) was still shown open in the retrieved page. This review did not establish a supported universal `--path` destination flag; documentation, not an assumed flag, must drive adapters. Native plugins are distinct from standalone skill-copy installation and may own tools, hooks, credentials, and dependencies.

Codex's [native inventory schema](https://github.com/openai/codex/blob/rust-v0.154.0/codex-rs/app-server-protocol/src/protocol/v2/plugin.rs) offers a better boundary than reconstructing every plugin manager's cache layout. Use it for inventory, not to replace the user's harness. Failure and stale state must remain visible.

## Hooks and installation conclusions

The [official hook documentation](https://developers.openai.com/codex/hooks/) supports prompt-submit context, but that is not a universal installer event and does not establish startup ordering for file relocation. Hook definitions also have their own trust lifecycle. Prefer an optional, bounded, cached-only suggestion hook. Keep direct CLI discovery as the reliable path.

A nice wizard should configure **Skillwick's integration**, not take ownership of every installer. Installation means binary + explicit setup. For Homebrew, do not hide user-home edits in formula installation. Reuse [dist](https://github.com/axodotdev/cargo-dist) for binary/release scaffolding instead of writing an entire packaging system.

## Name screening

Working choice: **Skillwick** — a small thing that makes the relevant skill available when needed. Repository and command use the same spelling. This is a proposed name, not a registered brand.

A GitHub repository-name search for `skillwick` returned no matches during this review; an exact web search surfaced no clear competing tool. That is a limited collision screen, not a domain/package reservation or trademark clearance. `Skillotron` already has public use, and `Skiln`/`SkillsPool` also surfaced existing projects/products, so they were not selected. Do not spend another implementation cycle optimizing the name before validating the tool.

## What was not verified

No project was installed, benchmarked, fuzzed, or tested on the user's Mac. No supply-chain audit, maintainer-response study, issue-triage benchmark, or exhaustive security audit was performed. No claim of “most production-ready” is justified from this inspection alone. The proposed dependencies and acceptance tests are a build recommendation; only the implementing agent can report which tests and platform builds actually pass.
