# Skillwick

Skillwick helps coding agents select relevant installed skills while keeping
unneeded skill information out of the root agent's context.

## Language

**Skill corpus**:
The full set of installed skills available for discovery in an evaluation.
_Avoid_: Dataset, evaluation sample

**Evaluation dataset**:
Task prompts and independently assigned relevance judgments used to evaluate
skill discovery against a skill corpus.
_Avoid_: Skill corpus

**Skill coverage**:
The proportion of distinct skills in the corpus represented by relevant
judgments in the evaluation dataset. It does not determine evaluation population
size or reduce the searchable corpus.
_Avoid_: Evaluation population, corpus size, query count

**Evaluation population**:
The set of distinct task cases used for evaluation. Its default size is 30% of
the full locally discoverable skill count, rounded up. Repeated runs of a case
and comparisons across workflows do not create additional distinct cases.
_Avoid_: Skill coverage, searchable corpus

**Retrieval backend**:
The method used to find and rank skill candidates: lexical, lexical plus
semantic, or lexical plus semantic with reranking.
_Avoid_: Discovery workflow

**Discovery workflow**:
The agent arrangement used to search for and select skills: direct discovery
by the root agent or discovery delegated to a sub-agent.
_Avoid_: Retrieval backend

**Root context savings**:
The reduction in skill-discovery information supplied to the root agent
relative to a stated baseline. This is distinct from total workflow token cost.
_Avoid_: Total token savings
