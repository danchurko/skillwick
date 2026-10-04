"""Shared evaluation measures; historical profile contracts stay unchanged."""

from __future__ import annotations

import hashlib
import math
import statistics
import subprocess
from pathlib import Path


def ranking_metrics(rankings: list[tuple[list[str], set[str]]]) -> dict:
    recalls, reciprocals, ndcgs = [], [], []
    for ranked, relevant in rankings:
        ranked = ranked[:5]
        if not relevant:
            recalls.append(1.0 if not ranked else 0.0)
            reciprocals.append(1.0 if not ranked else 0.0)
            ndcgs.append(1.0 if not ranked else 0.0)
            continue
        hits = [1 if name in relevant else 0 for name in ranked]
        recalls.append(sum(hits) / len(relevant))
        reciprocals.append(next((1 / (i + 1) for i, hit in enumerate(hits) if hit), 0.0))
        dcg = sum(hit / math.log2(i + 2) for i, hit in enumerate(hits))
        ideal = sum(1 / math.log2(i + 2) for i in range(min(len(relevant), len(ranked))))
        ndcgs.append(dcg / ideal if ideal else 0.0)
    return {
        "queries": len(rankings),
        "recall_at_5": statistics.fmean(recalls),
        "mrr_at_5": statistics.fmean(reciprocals),
        "ndcg_at_5": statistics.fmean(ndcgs),
    }


def task_metrics(rankings: list[tuple[list[str], set[str]]]) -> dict:
    positives = [(ranked, relevant) for ranked, relevant in rankings if relevant]
    negatives = [(ranked, relevant) for ranked, relevant in rankings if not relevant]
    metrics = {"queries": len(rankings), "positive_queries": len(positives), "negative_queries": len(negatives)}
    for limit in (5, 20):
        metrics[f"positive_recall_at_{limit}"] = statistics.fmean(
            len(set(ranked[:limit]) & relevant) / len(relevant) for ranked, relevant in positives
        ) if positives else None
    metrics["positive_mrr_at_5"] = statistics.fmean(
        next((1 / (index + 1) for index, name in enumerate(ranked[:5]) if name in relevant), 0.0)
        for ranked, relevant in positives
    ) if positives else None
    metrics["positive_ndcg_at_5"] = statistics.fmean(
        sum((name in relevant) / math.log2(index + 2) for index, name in enumerate(ranked[:5])) /
        sum(1 / math.log2(index + 2) for index in range(min(5, len(relevant))))
        for ranked, relevant in positives
    ) if positives else None
    metrics["negative_false_positive_rate"] = statistics.fmean(bool(ranked) for ranked, _ in negatives) if negatives else None
    return metrics


def quality(profile_version: int, rankings: list[dict]) -> dict:
    pairs = [(item["ranked"], set(item["relevant"])) for item in rankings]
    return (task_metrics if profile_version == 2 else ranking_metrics)(pairs)


def retrieval_metrics(profile_version: int, rankings: list[dict]) -> dict:
    """Additional cutoffs without changing the stored historical quality field."""
    result = quality(profile_version, rankings)
    measured = [item for item in rankings if item["relevant"]] if profile_version == 2 else rankings
    prefix = "positive_" if profile_version == 2 else ""
    for cutoff in (1, 3, 5):
        values = []
        for item in measured:
            relevant, ranked = set(item["relevant"]), item["ranked"][:cutoff]
            values.append(len(set(ranked) & relevant) / len(relevant) if relevant else float(not ranked))
        result[f"{prefix}recall_at_{cutoff}"] = statistics.fmean(values) if values else None
    return result


def selection_metrics(before: list[dict], after: list[dict]) -> dict:
    """Compare label-aware top-five quality, retaining multi-relevant labels."""
    if [(r["id"], r["query"], r["relevant"]) for r in before] != [
        (r["id"], r["query"], r["relevant"]) for r in after
    ]:
        raise ValueError("selection comparisons require identical queries and labels")
    counts = dict(improved=0, worsened=0, unchanged_materially=0, ordering_changed=0,
                  relevant_displacement_queries=0, relevant_displacements=0,
                  relevant_ranked_first=0, negative_correct_abstentions=0)
    for original, reranked in zip(before, after):
        relevant = set(original["relevant"])
        old, new = original["ranked"][:5], reranked["ranked"][:5]

        def value(ranked: list[str]) -> tuple:
            if not relevant:
                return (float(not ranked), 0.0)
            # Discounted gain reflects relevance ordering, then relevant coverage.
            return (sum((name in relevant) / math.log2(i + 2) for i, name in enumerate(ranked)),
                    len(set(ranked) & relevant))

        old_value, new_value = value(old), value(new)
        counts["improved" if new_value > old_value else "worsened" if new_value < old_value else "unchanged_materially"] += 1
        counts["ordering_changed"] += old != new
        displaced = max(0, len(set(old) & relevant) - len(set(new) & relevant))
        counts["relevant_displacement_queries"] += displaced > 0
        counts["relevant_displacements"] += displaced
        counts["relevant_ranked_first"] += bool(new and new[0] in relevant)
        counts["negative_correct_abstentions"] += not relevant and not new
    positives = sum(bool(item["relevant"]) for item in after)
    return {
        "queries": len(after), "positive_queries": positives, **counts,
        "relevant_ranked_first_rate": counts["relevant_ranked_first"] / positives if positives else None,
        "material_change_measure": "top-five discounted relevant gain, then relevant coverage; negative cases use empty-result correctness",
        "selected_skill_success": None,
        "selection_limit": "labels describe relevance, not actual agent selection or task success; no unique best skill is labelled for multi-relevant cases",
    }


def confidence_metrics(rankings: list[dict]) -> dict:
    judgments = {"correct": [], "incorrect": []}
    top_ranked = {"relevant": [], "irrelevant": []}
    for row in rankings:
        relevant = set(row["relevant"])
        for judgment in row.get("judgments") or []:
            is_relevant = judgment["candidate"] in relevant
            correct = (judgment["choice"] == "relevant") == is_relevant
            judgments["correct" if correct else "incorrect"].append(judgment["confidence"])
            if row.get("outcome") == "reranked" and row["ranked"] and judgment["candidate"] == row["ranked"][0]:
                top_ranked["relevant" if is_relevant else "irrelevant"].append(judgment["probability_relevant"])

    def distribution(values: list[float]) -> dict:
        ordered = sorted(values)
        if not ordered:
            return {"count": 0, "min": None, "median": None, "p95": None, "max": None}
        return {"count": len(ordered), "min": ordered[0], "median": statistics.median(ordered),
                "p95": ordered[math.ceil(len(ordered) * .95) - 1], "max": ordered[-1]}

    return {
        "candidate_choice_confidence": {name: distribution(values) for name, values in judgments.items()},
        "reranked_first_probability_relevant": {name: distribution(values) for name, values in top_ranked.items()},
        "scope": "binary judgement correctness includes valid low-confidence responses; top-ranked probabilities include only accepted reranks",
        "limit": "confidence in a binary relevance judgement is not a calibrated probability of successful skill selection",
    }


def provenance() -> dict:
    root = Path(__file__).resolve().parents[1]

    def git(*args: str) -> str | None:
        try:
            return subprocess.check_output(["git", *args], cwd=root, text=True,
                                           stderr=subprocess.DEVNULL).strip()
        except (OSError, subprocess.CalledProcessError):
            # Staged-tree and source-archive checks have no Git metadata. Keep
            # source hashes and report unavailable provenance explicitly.
            return None

    status = git("status", "--porcelain")

    paths = ("scripts/benchmark_metrics.py", "scripts/benchmark_profiles.py", "scripts/benchmark-lexical.py",
             "scripts/benchmark-semantic.py", "scripts/benchmark_jev.py", "assets/skillwick/reranker_runtime.py")
    return {
        "skillwick_commit": git("rev-parse", "HEAD"),
        "working_tree_dirty": None if status is None else bool(status),
        "implementations_sha256": {
            path: hashlib.sha256((root / path).read_bytes()).hexdigest()
            for path in paths if (root / path).is_file()
        },
    }
