"""Frozen benchmark identities and labels shared by lexical and model runners."""

from __future__ import annotations

import hashlib
import json
import re
from pathlib import Path

from benchmark_metrics import quality


def canonical_hash(value: object) -> str:
    payload = json.dumps(value, ensure_ascii=False, separators=(",", ":"), sort_keys=True)
    return hashlib.sha256(payload.encode()).hexdigest()


def validate_profile(profile: dict) -> None:
    if not isinstance(profile, dict) or profile.get("version") not in (1, 2) or profile.get("kind") != "skillwick-lexical-profile":
        raise ValueError("unsupported benchmark profile")
    corpus, heldout = profile.get("corpus"), profile.get("heldout")
    if not isinstance(corpus, dict) or not isinstance(heldout, dict):
        raise ValueError("invalid benchmark corpus or held-out set")
    records, cases = corpus.get("records"), heldout.get("cases")
    if not isinstance(records, list) or not records or not isinstance(cases, list) or not cases:
        raise ValueError("benchmark corpus and held-out set must be nonempty")
    if corpus.get("total") != len(records) or corpus.get("sha256") != canonical_hash(records):
        raise ValueError("corpus identity does not match its records")
    names = []
    for record in records:
        if not isinstance(record, dict) or not isinstance(record.get("name"), str) or not record["name"].strip() or not isinstance(record.get("description"), str):
            raise ValueError("invalid benchmark skill metadata")
        identity = record.get("fixture_id", record["name"])
        if not isinstance(identity, str) or not identity.strip():
            raise ValueError("invalid benchmark fixture identity")
        names.append(identity)
    if len(set(names)) != len(records):
        raise ValueError("corpus fixture identities are not unique")
    if heldout.get("case_count") != len(cases):
        raise ValueError("held-out case count is incorrect")
    if profile["version"] == 2 and heldout.get("sha256") != canonical_hash(cases):
        raise ValueError("held-out labels differ from frozen identity")
    identities, case_ids = set(names), set()
    query_count = 0
    for case in cases:
        if not isinstance(case, dict):
            raise ValueError("invalid held-out case")
        case_id, queries, relevant = case.get("id"), case.get("queries"), case.get("relevant")
        if not isinstance(case_id, str) or not case_id.strip() or case_id in case_ids:
            raise ValueError("held-out case identities must be nonempty and unique")
        if not isinstance(queries, list) or not queries or any(not isinstance(q, str) or not q.strip() for q in queries):
            raise ValueError("held-out queries must be nonempty strings")
        if not isinstance(relevant, list) or any(not isinstance(label, str) for label in relevant):
            raise ValueError("invalid relevance labels")
        if len(set(relevant)) != len(relevant) or not set(relevant) <= identities:
            raise ValueError(f"invalid labels for {case_id}")
        if (case.get("kind") == "negative") != (not relevant) or case.get("kind") not in {"positive", "negative"}:
            raise ValueError(f"case kind and relevance disagree: {case_id}")
        query_count += len(queries)
        case_ids.add(case_id)
    if heldout.get("query_count") != query_count:
        raise ValueError("held-out query count is incorrect")


def profile_rows(profile: dict) -> list[dict]:
    validate_profile(profile)
    return [
        {"id": f"{case['id']}:{index}", "query": query,
         "relevant": sorted(case["relevant"]), "kind": case["kind"]}
        for case in profile["heldout"]["cases"]
        for index, query in enumerate(case["queries"], 1)
    ]


def validate_candidate_document(profile_path: Path, profile: dict, expected_rows: list[dict],
                                candidate_path: Path, candidates: dict, pool_size: int | None,
                                *, allowed_kinds: tuple[str, ...] = ("skillwick-lexical-baseline",),
                                require_provenance: bool = True) -> tuple[list[dict], int]:
    """Validate a shared candidate input; archived local experiments may lack provenance."""
    if not isinstance(candidates, dict) or candidates.get("kind") not in allowed_kinds or candidates.get("version") != profile["version"]:
        raise ValueError("invalid_candidates")
    if candidates.get("profile_sha256") != hashlib.sha256(profile_path.read_bytes()).hexdigest():
        raise ValueError("candidate_profile_mismatch")
    for field, expected in (("corpus_sha256", profile["corpus"]["sha256"]), ("corpus_total", profile["corpus"]["total"])):
        if (require_provenance or field in candidates) and candidates.get(field) != expected:
            raise ValueError("candidate_corpus_mismatch")
    recorded_pool = candidates.get("candidate_pool_size", None if require_provenance else 20)
    if isinstance(recorded_pool, bool) or not isinstance(recorded_pool, int):
        raise ValueError("invalid_candidates")
    if not 1 <= recorded_pool <= 20:
        raise ValueError("candidate_pool_exceeds_cli_limit")
    if pool_size is None:
        pool_size = recorded_pool
    if isinstance(pool_size, bool) or not isinstance(pool_size, int) or not 1 <= pool_size <= recorded_pool:
        raise ValueError("requested_pool_exceeds_candidate_source")
    if require_provenance:
        provenance = candidates.get("provenance")
        if not isinstance(provenance, dict):
            raise ValueError("missing_candidate_provenance")
        commit, hashes = provenance.get("skillwick_commit"), provenance.get("implementations_sha256")
        if (not isinstance(commit, str) or not re.fullmatch(r"[0-9a-f]{40,64}", commit)
                or not isinstance(provenance.get("working_tree_dirty"), bool) or not isinstance(hashes, dict)):
            raise ValueError("missing_candidate_provenance")
        lexical_hash = hashes.get("scripts/benchmark-lexical.py")
        if not isinstance(lexical_hash, str) or not re.fullmatch(r"[0-9a-f]{64}", lexical_hash):
            raise ValueError("missing_candidate_provenance")
    rankings = candidates.get("rankings")
    if not isinstance(rankings, list) or len(rankings) != len(expected_rows):
        raise ValueError("candidate_query_coverage_mismatch")
    identities = {record.get("fixture_id", record["name"]) for record in profile["corpus"]["records"]}
    normalized = []
    for expected, item in zip(expected_rows, rankings):
        if not isinstance(item, dict):
            raise ValueError("invalid_candidates")
        if (item.get("id") != expected["id"] or item.get("query") != expected["query"]
                or item.get("relevant") != expected["relevant"]):
            raise ValueError("candidate_query_or_label_mismatch")
        ranked = item.get("ranked")
        if (not isinstance(ranked, list) or any(not isinstance(name, str) for name in ranked)
                or len(ranked) > recorded_pool or len(ranked) != len(set(ranked)) or not set(ranked) <= identities):
            raise ValueError("invalid_candidate_identities")
        normalized.append({"id": expected["id"], "query": expected["query"],
                           "relevant": expected["relevant"], "ranked": ranked})
    if candidates.get("quality") != quality(profile["version"], normalized):
        raise ValueError("candidate_quality_mismatch")
    return normalized, pool_size
