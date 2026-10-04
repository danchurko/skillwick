# Hosted JEV reranking experiment

Status: experimental. Production search and read remain local and unchanged.

## Hypothesis and boundary

Hosted System One relevance judgements might improve skill ranking within a
bounded FTS5 candidate pool enough to justify an optional reranker. TinyBERT over
lexical candidates is the quality comparison to beat; a working API call alone
does not justify adoption.

The experiment extends the existing external Python semantic benchmark through
the official [TypeSafe Python SDK](https://github.com/typesafe-ai/typesafe-sdk-python).
The experiment pins `typesafe-sdk==0.7.2` and uses Python 3.14.
The SDK owns HTTP transport and typed questions. Skillwick still owns discovery,
applicability, identity, freshness, lexical candidates, and live reads. Nothing
loads a model or contacts TypeSafe from the Rust CLI.

Remote state contains only a frozen benchmark query and candidate labels, names,
and descriptions. Labels map locally to known benchmark identities. No skill
bodies, absolute source paths, package files, relevance labels, local
configuration, or credentials are included in decision state. The API key is
used only for authentication. Full provider responses, error messages, and HTTP
headers are not written to results.

## Decision formulation

Each candidate receives an independent typed `relevant` / `not_relevant` choice,
batched in one System One request per nonempty query. Rank by the probability of
`relevant`, with original lexical order breaking ties. This preserves multiple
relevant candidates and ranking information. One exclusive choice over the
whole pool would assume a single winner and its probabilities would describe
mutually exclusive alternatives rather than independent relevance.

Request formulation and thresholds are fixed before observing held-out results.
The default confidence threshold is 0.55. Ambiguous judgements retain lexical
ordering and are counted as fallback, rather than successful reranking.
Provider confidence is stored separately from choice probabilities. It is not
assumed to equal the probability of the selected choice. An initial smoke
exposed that incorrect parser assumption; the parser and its offline regression
fixture were corrected before the full evaluation, without changing thresholds.
Authentication, network, timeout, rate-limit, server, malformed response,
mapping, and model-mismatch failures remain visible and retain lexical ordering.
Provider failures make the experimental command fail after storing evidence.

## Methodology

V1 is the unchanged 531-record corpus with 35 held-out cases and 105 query
perspectives. It provides direct comparison to the historical lexical and
TinyBERT results. V2 is evaluated separately: 12 records, 20 cases, four
negatives, and multiple relevant skills. Neither profile is an independently
sampled task-success dataset.

The current release binary generates a fresh lexical top-20 candidate file for
each profile. Lexical top five, pinned TinyBERT over that same pool, and JEV over
that same pool are compared. The pool-10 JEV experiment uses only the first ten
candidates for decisions; candidates below that pool cannot enter its top five.
Pool 30 is not run: the shipped search interface caps candidates at 20, and this
experiment does not change the product retrieval contract.

Both frozen profile files were verified against their exact Git blob hashes in
the public `danchurko/skillwick` repository before the live requests. Only that
already-published query and candidate metadata crosses the hosted boundary.

Historical `quality` fields keep their existing definitions. Additional measures
report Recall@1, @3, @5, MRR@5, and nDCG@5. V2 positive retrieval metrics exclude
negative cases, whose false-positive rate is reported separately. Multi-relevant
labels stay intact. Selection comparisons measure relevant-first frequency,
top-five discounted relevance changes, and relevant coverage displaced by
irrelevant candidates. They cannot prove which skill an agent actually selected,
identify a unique best skill where labels do not specify one, or establish task
success.

Hosted latency is measured independently of lexical CLI work. Requests, failures,
fallbacks, candidate counts, payload bytes, usage, resolved model identity, and
wall time are retained. Missing usage or cost evidence stays unknown. Results
bind to the profile, candidate input, commit, and evaluator source hashes;
concurrent workstation activity and one machine limit latency conclusions.

The [published model pricing](https://docs.typesafe.ai/models), checked on
3 October 2026, is USD 0.042 per million input tokens and zero for output tokens.
Reported usage can therefore support an estimated cost at that recorded rate.
It is not an account billing receipt; unavailable usage or unreturned charges
cannot establish actual cost.

## Results and recommendation

Keep JEV experimental and retain lexical-only production retrieval. The original
comparison conflated model quality with an overly broad confidence fallback:
raw JEV pool-20 ranking beats TinyBERT on V1, while the gated integration loses.
The October 4 audit and repeat runs below supersede the original model-quality
conclusion. Network, privacy, model drift, and latency costs still require
independent task evidence before adoption. The historical
[semantic adoption decision](semantic-adoption.md) remains unchanged; these
results do not recommend shipping TinyBERT either.

### V1 ranking quality

| Path | Recall@1 | Recall@3 | Recall@5 | MRR@5 | nDCG@5 |
|---|---:|---:|---:|---:|---:|
| Lexical | .800000 | .876190 | .885714 | .837302 | .849679 |
| TinyBERT, reproduced runtime | .914286 | .942857 | .952381 | .928889 | .934750 |
| JEV pool 20 | .866667 | .914286 | .914286 | .890476 | .896711 |
| JEV pool 10 | .866667 | .914286 | .914286 | .888889 | .895464 |

Relevant-first counts were 84/105 for lexical, 96/105 for TinyBERT, and 91/105
for either JEV pool. TinyBERT improved 16 cases and worsened one; JEV improved
eight and worsened one. Neither JEV run displaced relevant top-five coverage.
These label-based comparisons are selection proxies, not observed agent task
success.

The primary TinyBERT control reproduces historical quality exactly with Python
3.10.21 and ONNX Runtime 1.23.2. A second run using Python 3.14.8 and ONNX Runtime
1.30.0 produced Recall@5 .942857, MRR@5 .926984, and nDCG@5 .931065 with the same
verified model, tokenizer, and lexical candidate order. Original NumPy sorting
was preserved. Runtime and workstation differences prevent attributing the
change to one dependency; the stronger reproduced control is used above.

### V2 ranking quality and limits

| Paths | Positive Recall@1 | Positive Recall@3/@5/@20 | MRR@5 | nDCG@5 | Negative false-positive rate |
|---|---:|---:|---:|---:|---:|
| Lexical, TinyBERT, JEV 10, JEV 20 | .765625 | .812500 | .875000 | .828114 | 0 |

All paths ranked a relevant candidate first for 14/16 positive cases. JEV changed
two orderings but improved no labelled outcome. Six pools were empty, including
all four negatives and two positives: there is no hosted negative-case rejection
evidence. Nonempty pools contained only one or two candidates, so pool 10 and
pool 20 effectively supplied identical state. This small fixture cannot settle
quality on realistic distractor-heavy or independent tasks.

### Hosted operations

All final requests resolved to `jev-1.13.0` through SDK 0.7.2. There were zero
failed requests. A valid response can still trigger lexical fallback.

| Profile / pool | Requests | Accepted / low-confidence fallback / empty | Median / p95 ms | Wall s | Input / output tokens | Estimated USD |
|---|---:|---|---|---:|---|---:|
| V1 / 20 | 104 | 37 / 67 / 1 | 252.807 / 356.359 | 27.717 | 389690 / 61944 | .01636698 |
| V1 / 10 | 104 | 53 / 51 / 1 | 243.428 / 329.997 | 26.678 | 237280 / 35979 | .00996576 |
| V2 / 20 | 14 | 14 / 0 / 6 | 228.023 / 280.133 | 3.480 | 6695 / 746 | .00028119 |
| V2 / 10 | 14 | 14 / 0 / 6 | 218.985 / 326.319 | 3.472 | 6695 / 746 | .00028119 |

Mean scored candidates per request were 14.8654 and 8.6154 for V1 pools 20 and
10, and 1.2857 for V2. Compact state/questions payload totals were 1,381,388,
800,570, and 11,000 bytes respectively; these exclude SDK envelopes and headers.
Pool 10 reduced usage without improving quality. Actual billing was unavailable.

V1 lexical full-CLI latency was median 58.650 ms / p95 61.855 ms. Reproduced
TinyBERT loaded inference was 17.362 / 24.763 ms, with 5.136 s wall time and
227760 KiB peak RSS. These timing surfaces differ from hosted request latency;
they are not directly comparable end-to-end measurements.

Confidence is not correctness evidence: V1 pool-20 incorrect binary judgements
had median confidence .535 and p95 .95, with maximum .99. Two accepted irrelevant
first candidates had relevance probabilities .96 and .97. The fixed .55 policy
was not tuned after observing these labels.

### Evidence and verification

The initial smoke falsely classified independent confidence as malformed. Its
[failed receipt](../../benchmarks/results/jev-smoke-profile-v1-2026-10-03.json),
[sanitized diagnostic](../../benchmarks/results/jev-diagnostic-profile-v1-2026-10-03.json),
and [corrected smoke](../../benchmarks/results/jev-smoke-corrected-profile-v1-2026-10-03.json)
are retained as non-comparable preflight evidence. This was a parser defect,
not evidence of provider downtime. The corrected parser has offline regression
coverage; partial smoke metrics are suppressed.

Final artifacts:

- V1: [lexical](../../benchmarks/results/lexical-jev-profile-v1-pool20-2026-10-03.json), [TinyBERT historical runtime](../../benchmarks/results/tinybert-jev-profile-v1-pool20-legacy-runtime-2026-10-03.json), [TinyBERT latest runtime](../../benchmarks/results/tinybert-jev-profile-v1-pool20-2026-10-03.json), [JEV 20](../../benchmarks/results/jev-profile-v1-pool20-2026-10-03.json), [JEV 10](../../benchmarks/results/jev-profile-v1-pool10-2026-10-03.json).
- V2: [lexical](../../benchmarks/results/lexical-jev-profile-v2-pool20-2026-10-03.json), [TinyBERT](../../benchmarks/results/tinybert-jev-profile-v2-pool20-legacy-runtime-2026-10-03.json), [JEV 20](../../benchmarks/results/jev-profile-v2-pool20-2026-10-03.json), [JEV 10](../../benchmarks/results/jev-profile-v2-pool10-2026-10-03.json).

Rust formatting, Clippy, 52 unit tests, CLI, documentation, trust, filesystem,
benchmark contracts, SDK mock transport contracts, inference tests, and dependency
assurance passed. Source-install built and passed installed-binary fixtures,
but its live inventory phase failed on an existing Ponytail plugin manifest
version mismatch. Persistent plugin state was left untouched. This prevents an
unqualified claim that every test passed.

The conclusion applies to this batched binary-Choice formulation and frozen
confidence policy. Future experiments need independently sampled tasks and
predeclared formulations and budgets; tuning on these held-out labels would
weaken the evidence.

## October 4 correctness audit and repeat evaluation

The earlier claim that JEV itself loses to TinyBERT was too broad. The stored
quality field measures the complete integration, including a veto that falls
back to lexical order whenever **any candidate** has confidence below .55.
This applies even when an uncertain distractor cannot affect the winning skill.
TinyBERT has no equivalent veto. The policy is preserved for reproducibility;
raw provider ranking is now recorded separately, without selecting a new
threshold using held-out labels.

An independent review found no identity reversal, probability inversion, or
cross-model candidate mismatch. Ten pool-20 first-place rescues in the October 3
raw replay had winner confidence .94–1.0; every veto came from other candidates.

Fresh lexical and historical-runtime TinyBERT comparisons and all four JEV runs
were repeated on October 4 using the same frozen profiles. All 236 hosted
requests returned valid responses from `jev-1.13.0`, with no provider failures.

| Fresh V1 path | Recall@1 | Recall@5 | MRR@5 | nDCG@5 |
|---|---:|---:|---:|---:|
| Lexical | .800000 | .885714 | .837302 | .849679 |
| TinyBERT historical runtime | .914286 | .952381 | .928889 | .934750 |
| JEV 20, raw diagnostic ranking | .961905 | .971429 | .966667 | .967914 |
| JEV 20, confidence-gated integration | .866667 | .914286 | .890476 | .896711 |
| JEV 10, raw diagnostic ranking | .923810 | .933333 | .928571 | .929818 |

Pool-20 raw quality matches the previous day's replay. The number of
confidence-gated fallbacks changed from 67 to 64, so repeat responses are not
assumed deterministic. Pool 10 has only 98/105 labelled answers available,
versus 102/105 for pool 20. Raw JEV reaches both pools' maximum attainable
Recall@5. Three pool-20 misses are upstream retrieval misses. V2 remains
unchanged across all paths and is insufficient to test rejection.

### Dataset validity

V1 tasks were agent-authored from skill descriptions; relevance labels are
non-exhaustive. The 105 perspectives are 35 correlated task groups, not 105
independent trials. An apparently wrong first result for the latency/design
query plausibly matches another skill's description. Therefore unlabelled
candidates are not necessarily adjudicated irrelevant, and confidence error
counts must be interpreted as disagreement with fixture labels. Provider
training contamination cannot be established or excluded from these receipts.

A stronger acceptance dataset needs independently collected tasks, exhaustive
relevance adjudication, negatives with plausible lexical distractors, and
uncertainty estimates grouped by underlying task. The existing fixtures remain
useful for regression and debugging, but cannot establish production task success.

### Added diagnostic proof

Each attempted query now retains the exact whitelisted state/questions document,
a SHA-256 digest, local question-to-identity mapping, raw probability ranking,
and candidates that triggered fallback. Credentials, HTTP headers, raw errors,
and full SDK responses remain excluded. Actual SDK mock-transport tests verify
that the logged state, question instructions, and criteria equal the serialized
wire body. These traces cover application payload fields, not the SDK envelope.

`audit_jev_evals.py` independently recomputes metrics and ranking policy, validates
payload/mapping consistency, and reports dataset and candidate coverage. Audits
are stored beside the corresponding results. Historical receipts are preserved.
The initial October 4 sandbox-network run is retained as a failed preflight;
its 104 transport failures are not included as comparable model-quality evidence.

Repeat artifacts and audit receipts live in `benchmarks/results/` with date
`2026-10-04`; successful hosted filenames include `live`. Raw ranking remains a
post-hoc diagnostic on reused fixtures, even though the fresh calls reproduce it.
This is evidence that the gate caused the original quality loss, not independent
validation of a replacement deployment policy.

Stored independent audit receipts: [V1 pool 20](../../benchmarks/results/jev-audit-v1-pool20-2026-10-04.json),
[V1 pool 10](../../benchmarks/results/jev-audit-v1-pool10-2026-10-04.json),
[V2 pool 20](../../benchmarks/results/jev-audit-v2-pool20-2026-10-04.json),
and [V2 pool 10](../../benchmarks/results/jev-audit-v2-pool10-2026-10-04.json).
The [repeat verification index](../../benchmarks/results/jev-repeat-verification-2026-10-04.json)
records result hashes, explicit failed preflight status, and verification limits.
