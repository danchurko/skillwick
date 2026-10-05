# Retrieval corrections for 0.5.0

This follows the [historical installed-source qualification](real-source-qualification.md)
and addresses [#50](https://github.com/danchurko/skillwick/issues/50) and
[#51](https://github.com/danchurko/skillwick/issues/51). The historical measurements
remain unchanged. Generated inventories, profiles, rankings, receipts and traces
stay in ignored `benchmarks/results/` or an external directory.

## Inputs and behavior

[The release regression input](../../benchmarks/retrieval-regression-cases.json)
preserves all thirteen original scored cases and three diagnostics. It adds five
unrelated paraphrases or quoted-name controls, one positive quoted-SDK task, and
three positive quoted lookup/selection tasks: twenty-two queries in total.
The input was frozen before candidate replay, with SHA-256
`5dac4f489459d5783d5041d4542dea9c641f1856627b46e1a4e5012713bdb42f`.
Labels are relevance judgements, not evidence of agent selection or task success.

FTS5 owns matching and coverage. The unindexed ID receives zero weight; name,
description and keywords receive weights eight, three and one. Exact names rank
first, then native BM25, coverage and deterministic ties. Repeated query terms
count once, and substrings do not contribute token coverage. Native package-name
evidence can retain a specific service in a longer task whose other words are
absent from the package metadata. Source, policy and project filters remain in
force; complete package grouping precedes limits.

Common function words and contextual action verbs are not object evidence.
Action verbs retain secondary ranking and coverage context.
Balanced quoted text cannot independently qualify a candidate when the query
contains unquoted task text. Quote-only searches and complete exact names remain
supported. Explicit retrieval requests without transformation operators promote
their quoted targets to task evidence and preserve exact-name priority; translation or mixed transformation requests do not. Unmatched
quotes remain literal. This is a lexical query
policy; it does not provide general semantic task classification.

## Reviewed installed-source results

On 2026-10-05 the fresh source-installed 0.5.0 candidate replayed the same 559
complete eligible package groups as the historical qualification, inventory
SHA-256 `fcd5e48af58b386e74fecb7d72734b1a74f3490a2110cd82943591228023a4f4`.
Independent standards and specification reviews found no unresolved material
finding after the quoted exact-name correction. Receipts record the dirty
reviewed source candidate and executable hash; these measurements are not
published-archive proof.

| Profile | Recall@5 | MRR@5 | Negative false positives |
| --- | ---: | ---: | ---: |
| Original ten real positives, lexical | 1.0000 | 0.8533 | 0 / 3 original controls |
| Expanded fourteen real positives, lexical | 1.0000 | 0.8952 | 0 / 8 |
| Expanded real profile, offline TinyBERT comparison | 0.9286 | 0.8750 | 0 / 8 |
| V1, 105 synthetic queries | 0.9333 | 0.9016 | Not a negative profile |
| V2, sixteen positive and four negative queries | 0.859375 | 0.8750 | 0 / 4 |

The historical Bedrock CLI and TypeScript targets rank third and fifth. All
added positive lexical controls appear within five, including SDK documentation,
C++, Bedrock lookup and exact Ponytail selection. The offline TinyBERT comparison
conserves the candidate pool but can change top-five recall; its expanded result
is reported separately rather than presented as a lexical improvement.
The configured local TinyBERT runtime also executed all twenty-two actual-source
queries without fallback, with identical candidate identities and empty negative
results. Hosted JEV and real AWS inference were not exercised.

Ignored receipts are under `benchmarks/results/release-0.5.0/issue51-exact-final/`.
Earlier failed candidates remain in separate directories. Keep the frozen inputs
and this summary in Git, and keep regenerated profiles, raw rankings and traces
outside Git.

## Published release after workstation apply

The published 0.5.0 ARM64 archive was installed by agent-state's normal
unpinned `/latest` path after `make apply AI_AGENT=all` from the canonical
`~/.local/share/agent-state` checkout. The installed executable matches the
published archive bytes; executable SHA-256 is
`487108d3774567e0f94658e562833c19f686e6d074d7f171a20268d6284b2a66`.
The source-build override was retired with a private rollback backup. A second
latest-release installer run completed without replacing the verified binary.

Apply refreshed upstream skills, so a new actual-source profile was frozen
instead of reusing the reviewed candidate receipt. The unrestricted native
terminal replay, repeated from an external empty workspace, contains 579 eligible
complete groups, inventory SHA-256
`903101664eb5e6994bcfa6b77fd72b01d2bc6d300385ba49bda689bc3caac8e6`.
All fourteen positives remain within five (MRR@5 0.8952); all eight negatives
abstain. All 105 V1 and twenty V2 queries also replayed and their receipts
validated. Installed CLI, provider, batch invocation, reranker, documentation,
trust and filesystem contracts passed against the published executable.
An independent sandboxed applied-state check passed Codex and Claude integration health,
the complete three-skill AWS metadata/body/hash batch, and all twenty-two search
controls. Configured local TinyBERT ran all twenty-two queries without fallback;
candidate identities were conserved and negative results remained empty.
The sandboxed native inventory exposes 560 groups: nineteen connected-plugin
skills available to the unrestricted terminal are absent. The same config and
external workspace reproduce this difference with fresh caches; it is a provider
visibility boundary, not a changed label or a stale receipt. Both environments
pass the twenty-two search controls.

Generated receipts remain ignored under
`benchmarks/results/release-0.5.0/published-applied/` and the external-workspace
repeat in `published-applied-global/`. The live reranker choice
was preserved; these lexical replays use a temporary AUTO config without that
backend and make no hosted JEV or AWS inference requests.

## Reproduction

Use the [installed-source reproduction commands](real-source-qualification.md#reproduction)
with the released executable and substitute
`benchmarks/retrieval-regression-cases.json` for the historical case input.
Use an unrestricted native terminal, preserve actual home/provider state, use
an external empty workspace, and copy the live
config to a temporary file with automatic discovery and its reranker removed.
This measures lexical behavior without a hosted request. Freeze again after any
source, policy, config or executable change; never reuse an incompatible receipt.

Retain the synthetic and historical fixtures too:

```sh
task_binary=/path/to/installed/skillwick
task_run=benchmarks/results/release-regression-current
mkdir -p "$task_run"
for task_profile in v1 v2; do
  python3 scripts/benchmark-lexical.py run --binary "$task_binary" \
    --profile "benchmarks/profile-$task_profile.json" --samples 1 \
    --output "$task_run/lexical-$task_profile.json"
  python3 scripts/benchmark-lexical.py validate \
    --profile "benchmarks/profile-$task_profile.json" \
    --result "$task_run/lexical-$task_profile.json"
done
```

The optional offline TinyBERT comparison must consume the exact new lexical
candidate file and conserve candidate identities. Source tests, materialized
fixtures, actual installed-source replays, configured backend execution and
published archive verification are separate evidence boundaries.
