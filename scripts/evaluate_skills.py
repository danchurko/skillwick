#!/usr/bin/env python3
"""Export, validate, and score recorded Skillwick evaluation artifacts."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import random
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any, Iterable, Mapping, Sequence


CORPUS_VERSION = 1
DATASET_VERSION = 2
EVIDENCE_VERSION = 1
REPORT_VERSION = 1
PERSPECTIVES = ("outcome", "mechanism", "constraints")
WORKFLOWS = ("direct", "delegated", "native")
DEFAULT_COVERAGE = 0.30
DEFAULT_SEED = 0
PATH_FIELDS = frozenset({"id", "path", "canonical", "base"})
LEGACY_PATH_FIELDS = frozenset({"path", "canonical", "base"})


class EvaluationError(RuntimeError):
    """A recorded artifact does not satisfy the evaluation contract."""


def canonical_bytes(value: Any) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_json(value: Any) -> str:
    return sha256_bytes(canonical_bytes(value))


def read_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except OSError as error:
        raise EvaluationError(f"{path}: {error}") from error
    except json.JSONDecodeError as error:
        raise EvaluationError(f"{path}: invalid JSON: {error}") from error


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary: Path | None = None
    try:
        with tempfile.NamedTemporaryFile(
            "w", encoding="utf-8", dir=path.parent, prefix=f".{path.name}.", delete=False
        ) as handle:
            temporary = Path(handle.name)
            json.dump(value, handle, ensure_ascii=False, indent=2)
            handle.write("\n")
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def require_mapping(value: Any, label: str) -> Mapping[str, Any]:
    if not isinstance(value, Mapping):
        raise EvaluationError(f"{label} must be a JSON object")
    return value


def require_list(value: Any, label: str) -> list[Any]:
    if not isinstance(value, list):
        raise EvaluationError(f"{label} must be a JSON array")
    return value


def require_string(value: Any, label: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise EvaluationError(f"{label} must be a non-empty string")
    return value


def portable_record(record: Mapping[str, Any]) -> dict[str, Any]:
    """Return the identity-bearing fields that survive corpus relocation."""
    return {str(key): value for key, value in record.items() if str(key) not in PATH_FIELDS}


def portable_id(record: Mapping[str, Any]) -> str:
    return sha256_json(portable_record(record))


def corpus_identity(records: Sequence[Mapping[str, Any]]) -> str:
    return sha256_json(sorted((portable_record(record) for record in records), key=sha256_json))


def legacy_corpus_identity(records: Sequence[Mapping[str, Any]]) -> str:
    identities = [
        {str(key): value for key, value in record.items() if str(key) not in LEGACY_PATH_FIELDS}
        for record in records
    ]
    return sha256_json(sorted(identities, key=lambda record: str(record["id"])))


def identity_mapping(records: Sequence[Mapping[str, Any]]) -> dict[str, dict[str, str]]:
    source_to_portable = {str(record["id"]): portable_id(record) for record in records}
    if len(set(source_to_portable.values())) != len(source_to_portable):
        raise EvaluationError("corpus contains duplicate portable identities")
    return {
        "source_to_portable": source_to_portable,
        "portable_to_source": {value: key for key, value in source_to_portable.items()},
    }


def validate_records(value: Any) -> list[dict[str, Any]]:
    records = require_list(value, "corpus records")
    normalized: list[dict[str, Any]] = []
    seen: set[str] = set()
    for index, item in enumerate(records):
        record = dict(require_mapping(item, f"corpus records[{index}]"))
        identifier = require_string(record.get("id"), f"corpus records[{index}].id")
        if identifier in seen:
            raise EvaluationError(f"duplicate corpus ID: {identifier}")
        seen.add(identifier)
        for field in ("name", "description", "hash"):
            require_string(record.get(field), f"corpus records[{index}].{field}")
        normalized.append(record)
    return normalized


def family_for(record: Mapping[str, Any]) -> str:
    plugin = record.get("plugin_id")
    if isinstance(plugin, str) and plugin:
        return f"plugin:{plugin}"
    name = str(record.get("name", ""))
    return f"namespace:{name.split(':', 1)[0]}" if ":" in name else str(record.get("source_kind", "unscoped"))


def stratified_sample(records: Sequence[Mapping[str, Any]], coverage: float, seed: int) -> list[str]:
    if not math.isfinite(coverage) or not 0 < coverage <= 1:
        raise EvaluationError("coverage must be greater than 0 and at most 1")
    target = math.ceil(len(records) * coverage)
    groups: dict[str, list[Mapping[str, Any]]] = {}
    for record in records:
        groups.setdefault(family_for(record), []).append(record)
    rng = random.Random(seed)
    families = sorted(groups)
    for group in groups.values():
        group.sort(key=lambda record: str(record["id"]))
        rng.shuffle(group)
    selected: list[str] = []
    while len(selected) < target:
        available = [family for family in families if groups[family]]
        if not available:
            break
        rng.shuffle(available)
        for family in available:
            if len(selected) == target:
                break
            selected.append(str(groups[family].pop()["id"]))
    return sorted(selected)


def manifest_from_records(
    records_value: Any, *, command: Sequence[str], stderr: str, coverage: float, seed: int
) -> dict[str, Any]:
    records = validate_records(records_value)
    sample = stratified_sample(records, coverage, seed)
    return {
        "version": CORPUS_VERSION,
        "kind": "skillwick-corpus",
        "corpus": {"total": len(records), "sha256": corpus_identity(records), "records": records},
        "identity": identity_mapping(records),
        "sample": {
            "coverage": coverage,
            "seed": seed,
            "count": len(sample),
            "sha256": sha256_json(sample),
            "ids": sample,
        },
        "provenance": {"command": list(command), "stderr": stderr},
    }


def load_manifest(path: Path) -> dict[str, Any]:
    manifest = dict(require_mapping(read_json(path), "corpus manifest"))
    if manifest.get("version") != CORPUS_VERSION or manifest.get("kind") != "skillwick-corpus":
        raise EvaluationError("unsupported corpus manifest")
    corpus = require_mapping(manifest.get("corpus"), "manifest.corpus")
    records = validate_records(corpus.get("records"))
    valid_identities = {corpus_identity(records), legacy_corpus_identity(records)}
    if corpus.get("total") != len(records) or corpus.get("sha256") not in valid_identities:
        raise EvaluationError("corpus manifest identity does not match its records")
    if manifest.get("identity") is not None and manifest.get("identity") != identity_mapping(records):
        raise EvaluationError("corpus identity mapping does not match its records")
    sample = require_mapping(manifest.get("sample"), "manifest.sample")
    ids = [require_string(item, "sample ID") for item in require_list(sample.get("ids"), "sample.ids")]
    if len(ids) != len(set(ids)) or not set(ids) <= {str(record["id"]) for record in records}:
        raise EvaluationError("sample contains duplicate or unknown IDs")
    if sample.get("count") != len(ids) or sample.get("sha256") != sha256_json(sorted(ids)):
        raise EvaluationError("sample identity does not match its IDs")
    return manifest


def validate_dataset(path: Path, manifest: Mapping[str, Any]) -> tuple[dict[str, Any], str, list[dict[str, Any]]]:
    dataset = dict(require_mapping(read_json(path), "dataset"))
    if dataset.get("version") != DATASET_VERSION:
        raise EvaluationError("unsupported dataset version")
    if dataset.get("corpus_sha256") != manifest["corpus"]["sha256"]:
        raise EvaluationError("dataset corpus identity does not match the manifest")
    if dataset.get("sample_sha256") != manifest["sample"]["sha256"]:
        raise EvaluationError("dataset sample identity does not match the manifest")
    if dataset.get("perspectives") != list(PERSPECTIVES):
        raise EvaluationError("dataset perspectives must be outcome, mechanism, and constraints")
    labeling = require_mapping(dataset.get("labeling"), "dataset.labeling")
    if labeling.get("method") != "independent_review" or labeling.get("search_results_used") is not False or labeling.get("reviewed") is not True:
        raise EvaluationError("dataset labels must be independently reviewed and frozen before retrieval")
    if "labels_frozen" in labeling and labeling.get("labels_frozen") is not True:
        raise EvaluationError("dataset labels are not frozen")

    positive = require_list(dataset.get("cases"), "dataset.cases")
    negative = require_list(dataset.get("negative_cases"), "dataset.negative_cases")
    cases: list[dict[str, Any]] = []
    seen_cases: set[str] = set()
    corpus_ids = {str(record["id"]) for record in manifest["corpus"]["records"]}
    sample_ids = set(manifest["sample"]["ids"])
    covered: set[str] = set()
    split_by_skill: dict[str, str] = {}
    for index, value in enumerate([*positive, *negative]):
        case = dict(require_mapping(value, f"case[{index}]"))
        identifier = require_string(case.get("id"), f"case[{index}].id")
        if identifier in seen_cases:
            raise EvaluationError(f"duplicate case ID: {identifier}")
        seen_cases.add(identifier)
        kind = case.get("kind")
        expected_kind = "positive" if index < len(positive) else "negative"
        if kind != expected_kind or case.get("split") not in {"dev", "heldout"}:
            raise EvaluationError(f"{identifier}: invalid kind or split")
        require_string(case.get("task"), f"{identifier}.task")
        if case.get("reviewed") is not True:
            raise EvaluationError(f"{identifier}: case is not independently reviewed")
        queries = require_list(case.get("queries"), f"{identifier}.queries")
        if len(queries) != len(PERSPECTIVES):
            raise EvaluationError(f"{identifier}: exactly three queries are required")
        seen_perspectives = []
        for query in queries:
            query = require_mapping(query, f"{identifier}.query")
            seen_perspectives.append(require_string(query.get("perspective"), "query.perspective"))
            require_string(query.get("query"), "query.query")
        if tuple(seen_perspectives) != PERSPECTIVES:
            raise EvaluationError(f"{identifier}: query perspectives are invalid")
        relevant = [require_string(item, f"{identifier}.relevant") for item in require_list(case.get("relevant"), f"{identifier}.relevant")]
        if len(relevant) != len(set(relevant)) or not set(relevant) <= corpus_ids:
            raise EvaluationError(f"{identifier}: relevant IDs are duplicate or outside the corpus")
        if kind == "positive" and not relevant or kind == "negative" and relevant:
            raise EvaluationError(f"{identifier}: relevance does not match case kind")
        for skill_id in relevant:
            previous = split_by_skill.setdefault(skill_id, str(case["split"]))
            if previous != case["split"]:
                raise EvaluationError(f"{skill_id}: relevant skill appears in both splits")
        covered.update(relevant)
        cases.append(case)
    if not cases:
        raise EvaluationError("dataset has no cases")
    population = require_mapping(dataset.get("population"), "dataset.population")
    expected_cases = math.ceil(manifest["corpus"]["total"] * manifest["sample"]["coverage"])
    if (
        population.get("corpus_total") != manifest["corpus"]["total"]
        or population.get("coverage") != manifest["sample"]["coverage"]
        or population.get("total_cases") != expected_cases
        or population.get("positive_cases") != len(positive)
        or population.get("no_skill_cases") != len(negative)
        or len(cases) != expected_cases
    ):
        raise EvaluationError("dataset population does not match ceil(coverage * corpus_total), including no-skill cases")
    skill_coverage = require_mapping(dataset.get("skill_coverage"), "dataset.skill_coverage")
    if (
        skill_coverage.get("sampled_skill_count") != len(sample_ids)
        or skill_coverage.get("corpus_total") != manifest["corpus"]["total"]
    ):
        raise EvaluationError("dataset skill coverage does not match the manifest")
    missing = sample_ids - covered
    if missing:
        raise EvaluationError(f"sampled skills are missing positive judgments: {sorted(missing)[:3]}")
    return dataset, sha256_bytes(path.read_bytes()), cases


def _usage(value: Any, label: str) -> dict[str, int | None]:
    usage = require_mapping(value, label)
    result: dict[str, int | None] = {}
    for field in ("input_tokens", "output_tokens", "cached_input_tokens"):
        item = usage.get(field)
        if item is not None and (not isinstance(item, int) or isinstance(item, bool) or item < 0):
            raise EvaluationError(f"{label}.{field} must be a non-negative integer or null")
        result[field] = item
    return result


def validate_evidence(
    path: Path, manifest: Mapping[str, Any], dataset_sha256: str, cases: Sequence[Mapping[str, Any]]
) -> dict[str, Any]:
    evidence = dict(require_mapping(read_json(path), "evidence"))
    if evidence.get("version") != EVIDENCE_VERSION or evidence.get("kind") != "skillwick-evaluation-evidence":
        raise EvaluationError("unsupported evidence artifact")
    status = evidence.get("status", "complete")
    if status not in {"complete", "partial", "failed"}:
        raise EvaluationError("evidence status must be complete, partial, or failed")
    evidence["status"] = status
    failures = require_list(evidence.get("failures", []), "evidence.failures")
    for index, value in enumerate(failures):
        failure = require_mapping(value, f"evidence.failures[{index}]")
        require_string(failure.get("stage"), f"evidence.failures[{index}].stage")
        require_string(failure.get("reason"), f"evidence.failures[{index}].reason")
    for field, expected in (
        ("corpus_sha256", manifest["corpus"]["sha256"]),
        ("sample_sha256", manifest["sample"]["sha256"]),
        ("dataset_sha256", dataset_sha256),
    ):
        if evidence.get(field) != expected:
            raise EvaluationError(f"evidence {field} does not match")
    case_by_id = {str(case["id"]): case for case in cases}
    corpus_ids = {str(record["id"]) for record in manifest["corpus"]["records"]}
    search_budget = require_mapping(evidence.get("search_budget"), "evidence.search_budget")
    max_queries = search_budget.get("max_queries_per_case")
    if max_queries != len(PERSPECTIVES):
        raise EvaluationError("evidence must declare the shared three-query search budget")
    query_results = require_mapping(evidence.get("query_results"), "evidence.query_results")
    if set(query_results) != set(WORKFLOWS):
        raise EvaluationError("query evidence must contain direct, delegated, and native workflows")
    for workflow in WORKFLOWS:
        workflow_queries = require_mapping(query_results[workflow], f"query_results.{workflow}")
        if not set(workflow_queries) <= set(case_by_id):
            raise EvaluationError(f"{workflow} query evidence contains unknown cases")
        if status == "complete" and set(workflow_queries) != set(case_by_id):
            raise EvaluationError(f"{workflow} query evidence is missing from complete evidence")
        for case_id in workflow_queries:
            rows = require_list(workflow_queries[case_id], f"query_results.{workflow}.{case_id}")
            if not 1 <= len(rows) <= max_queries:
                raise EvaluationError(f"{workflow}/{case_id}: query evidence must contain one to three adaptive queries")
            seen_perspectives: set[str] = set()
            for value in rows:
                row = require_mapping(value, f"query_results.{workflow}.{case_id}[]")
                perspective = require_string(row.get("perspective"), "query perspective")
                require_string(row.get("query"), "recorded query")
                if perspective not in PERSPECTIVES or perspective in seen_perspectives:
                    raise EvaluationError(f"{workflow}/{case_id}: query perspectives are invalid")
                seen_perspectives.add(perspective)
                results = require_list(row.get("results"), f"query_results.{workflow}.{case_id}.results")
                result_ids = [require_string(require_mapping(item, "result").get("id"), "result.id") for item in results]
                if len(result_ids) > 5 or len(result_ids) != len(set(result_ids)) or not set(result_ids) <= corpus_ids:
                    raise EvaluationError(f"{workflow}/{case_id}: query results contain duplicate or unknown IDs")
                wall_ms = row.get("wall_ms")
                if wall_ms is not None and (
                    not isinstance(wall_ms, (int, float)) or isinstance(wall_ms, bool) or wall_ms < 0
                ):
                    raise EvaluationError(f"{workflow}/{case_id}: query wall_ms must be non-negative or null")
    workflows = require_mapping(evidence.get("workflows"), "evidence.workflows")
    for workflow in WORKFLOWS:
        records = require_mapping(workflows.get(workflow), f"workflows.{workflow}")
        if set(records) != set(query_results[workflow]):
            raise EvaluationError(f"{workflow} workflow evidence does not match its query cases")
        for case_id, value in records.items():
            record = require_mapping(value, f"workflows.{workflow}.{case_id}")
            selected = [require_string(item, "selected ID") for item in require_list(record.get("selected_ids"), "selected_ids")]
            if len(selected) > 5 or len(selected) != len(set(selected)) or not set(selected) <= corpus_ids:
                raise EvaluationError(f"{workflow}/{case_id}: selected IDs are invalid")
            candidates = {
                str(result["id"])
                for query in query_results[workflow][case_id]
                for result in query["results"]
            }
            if not set(selected) <= candidates:
                raise EvaluationError(f"{workflow}/{case_id}: selected IDs are outside recorded candidates")
            usage = require_mapping(record.get("usage"), "usage")
            wall_ms = record.get("wall_ms")
            if wall_ms is not None and (
                not isinstance(wall_ms, (int, float)) or isinstance(wall_ms, bool) or wall_ms < 0
            ):
                raise EvaluationError(f"{workflow}/{case_id}: workflow wall_ms must be non-negative or null")
            root_usage = _usage(usage.get("root"), "usage.root")
            total_usage = _usage(usage.get("total"), "usage.total")
            if any(
                root_usage[field] is not None
                and total_usage[field] is not None
                and total_usage[field] < root_usage[field]
                for field in root_usage
            ):
                raise EvaluationError(f"{workflow}/{case_id}: total usage is smaller than root usage")
    adjudication = require_mapping(evidence.get("adjudication"), "evidence.adjudication")
    for workflow in WORKFLOWS:
        records = require_mapping(adjudication.get(workflow), f"adjudication.{workflow}")
        if set(records) != set(query_results[workflow]):
            raise EvaluationError(f"{workflow} adjudication does not match its query cases")
        for case_id, value in records.items():
            judgment = require_mapping(value, f"adjudication.{workflow}.{case_id}")
            if judgment.get("reviewed") is not True:
                raise EvaluationError(f"{workflow}/{case_id}: adjudication is not reviewed")
            relevant = [require_string(item, "adjudicated ID") for item in require_list(judgment.get("relevant"), "adjudication.relevant")]
            if len(relevant) != len(set(relevant)) or not set(relevant) <= corpus_ids:
                raise EvaluationError(f"{workflow}/{case_id}: adjudicated IDs are invalid")
    evidence["_sha256"] = sha256_bytes(path.read_bytes())
    return evidence


def aggregate_usage(calls: Iterable[Mapping[str, Any]]) -> dict[str, int | None]:
    totals: dict[str, int | None] = {field: 0 for field in ("input_tokens", "output_tokens", "cached_input_tokens")}
    for call in calls:
        usage = _usage(call.get("usage"), "usage")
        for field, value in usage.items():
            if totals[field] is not None:
                totals[field] = None if value is None else totals[field] + value
    return totals


def _selection_metrics(
    cases: Sequence[Mapping[str, Any]], selected: Mapping[str, Sequence[str]], relevant: Mapping[str, Sequence[str]]
) -> dict[str, float | int | None]:
    recalls: list[float] = []
    precisions: list[float] = []
    reciprocal_ranks: list[float] = []
    ndcgs: list[float] = []
    negative_total = negative_correct = 0
    for case in cases:
        case_id = str(case["id"])
        ranking = list(selected[case_id][:5])
        chosen = set(ranking)
        expected = set(relevant[case_id])
        if expected:
            recalls.append(len(chosen & expected) / len(expected))
            precisions.append(len(chosen & expected) / len(chosen) if chosen else 0.0)
            reciprocal_ranks.append(
                next((1 / rank for rank, identifier in enumerate(ranking, 1) if identifier in expected), 0.0)
            )
            dcg = sum(1 / math.log2(rank + 1) for rank, identifier in enumerate(ranking, 1) if identifier in expected)
            ideal = sum(1 / math.log2(rank + 1) for rank in range(1, min(len(expected), 5) + 1))
            ndcgs.append(dcg / ideal)
        else:
            negative_total += 1
            negative_correct += not chosen
    return {
        "recall_at_5": sum(recalls) / len(recalls) if recalls else None,
        "precision_at_5": sum(precisions) / len(precisions) if precisions else None,
        "mrr_at_5": sum(reciprocal_ranks) / len(reciprocal_ranks) if reciprocal_ranks else None,
        "ndcg_at_5": sum(ndcgs) / len(ndcgs) if ndcgs else None,
        "no_skill_abstention": negative_correct / negative_total if negative_total else None,
        "positive_cases": len(recalls),
        "no_skill_cases": negative_total,
    }


def _retrieval_metrics(
    cases: Sequence[Mapping[str, Any]],
    query_results: Mapping[str, Sequence[Mapping[str, Any]]],
    relevant: Mapping[str, Sequence[str]],
) -> dict[str, float | int | None]:
    query_cases: list[dict[str, Any]] = []
    rankings: dict[str, list[str]] = {}
    query_relevant: dict[str, Sequence[str]] = {}
    for case in cases:
        case_id = str(case["id"])
        for index, query in enumerate(query_results[case_id]):
            query_id = f"{case_id}:{index}"
            query_cases.append({"id": query_id})
            rankings[query_id] = [str(result["id"]) for result in query["results"]]
            query_relevant[query_id] = relevant[case_id]
    metrics = _selection_metrics(query_cases, rankings, query_relevant)
    metrics["queries"] = len(query_cases)
    return metrics


def score_evidence(
    evidence: Mapping[str, Any], manifest: Mapping[str, Any], cases: Sequence[Mapping[str, Any]]
) -> dict[str, Any]:
    workflows = evidence["workflows"]
    frozen_relevant = {str(case["id"]): list(case["relevant"]) for case in cases}
    frozen: dict[str, Any] = {}
    adjudicated: dict[str, Any] = {}
    combined_root_calls = []
    combined_total_calls = []
    usage_by_workflow: dict[str, Any] = {}
    latency_by_workflow: dict[str, Any] = {}
    workflow_latency: dict[str, Any] = {}
    recorded_case_count: dict[str, int] = {}
    for workflow in WORKFLOWS:
        workflow_case_ids = set(workflows[workflow])
        workflow_cases = [case for case in cases if str(case["id"]) in workflow_case_ids]
        recorded_case_count[workflow] = len(workflow_cases)
        selected = {case_id: list(record["selected_ids"]) for case_id, record in workflows[workflow].items()}
        frozen[workflow] = {
            "retrieval": _retrieval_metrics(
                workflow_cases, evidence["query_results"][workflow], frozen_relevant
            ),
            "selection": _selection_metrics(workflow_cases, selected, frozen_relevant),
        } if workflow_cases else None
        reviewed = {case_id: list(value["relevant"]) for case_id, value in evidence["adjudication"][workflow].items()}
        adjudicated[workflow] = {
            "retrieval": _retrieval_metrics(
                workflow_cases, evidence["query_results"][workflow], reviewed
            ),
            "selection": _selection_metrics(workflow_cases, selected, reviewed),
        } if workflow_cases else None
        root_calls = [{"usage": record["usage"]["root"]} for record in workflows[workflow].values()]
        total_calls = [{"usage": record["usage"]["total"]} for record in workflows[workflow].values()]
        usage_by_workflow[workflow] = {
            "root": aggregate_usage(root_calls),
            "total": aggregate_usage(total_calls),
        } if workflow_cases else None
        latencies = [
            query["wall_ms"]
            for rows in evidence["query_results"][workflow].values()
            for query in rows
            if query.get("wall_ms") is not None
        ]
        latency_by_workflow[workflow] = {
            "known_queries": len(latencies),
            "total_ms": sum(latencies) if latencies else None,
        } if workflow_cases else None
        call_latencies = [record["wall_ms"] for record in workflows[workflow].values() if record.get("wall_ms") is not None]
        workflow_latency[workflow] = {
            "known_calls": len(call_latencies),
            "total_ms": sum(call_latencies) if call_latencies else None,
        } if workflow_cases else None
        combined_root_calls.extend(root_calls)
        combined_total_calls.extend(total_calls)
    usage_by_workflow["combined"] = {
        "root": aggregate_usage(combined_root_calls),
        "total": aggregate_usage(combined_total_calls),
    }
    return {
        "version": REPORT_VERSION,
        "kind": "skillwick-evaluation-report",
        "status": evidence["status"],
        "bounded_pilot_status": evidence.get("bounded_pilot_status"),
        "comparison_token_fields": evidence.get("comparison_token_fields"),
        "provenance": evidence.get("provenance"),
        "case_count": len(cases),
        "recorded_case_count": recorded_case_count,
        "failures": evidence.get("failures", []),
        "corpus_sha256": manifest["corpus"]["sha256"],
        "sample_sha256": manifest["sample"]["sha256"],
        "evidence_sha256": evidence.get("_sha256"),
        "frozen": frozen,
        "adjudicated": adjudicated,
        "usage": usage_by_workflow,
        "latency": latency_by_workflow,
        "workflow_latency": workflow_latency,
    }


def export_corpus(args: argparse.Namespace) -> None:
    command = [args.skillwick, "--json", "list", "--all"]
    try:
        completed = subprocess.run(command, check=False, capture_output=True, text=True, timeout=30)
    except (OSError, subprocess.TimeoutExpired) as error:
        raise EvaluationError(f"could not export Skillwick corpus: {error}") from error
    if completed.returncode:
        raise EvaluationError(f"Skillwick corpus export failed: {completed.stderr.strip()}")
    document = require_mapping(json.loads(completed.stdout), "Skillwick output")
    if document.get("total") != len(require_list(document.get("results"), "Skillwick results")):
        raise EvaluationError("Skillwick output total does not match its results")
    manifest = manifest_from_records(
        document["results"], command=command, stderr=completed.stderr, coverage=args.coverage, seed=args.seed
    )
    write_json(args.output, manifest)
    print(json.dumps({"corpus": manifest["corpus"]["total"], "sample": manifest["sample"]["count"]}))


def validate_command(args: argparse.Namespace) -> None:
    manifest = load_manifest(args.corpus)
    _, _, cases = validate_dataset(args.dataset, manifest)
    print(json.dumps({"status": "valid", "corpus": manifest["corpus"]["total"], "cases": len(cases)}))


def score_command(args: argparse.Namespace) -> None:
    manifest = load_manifest(args.corpus)
    _, dataset_sha256, cases = validate_dataset(args.dataset, manifest)
    evidence = validate_evidence(args.evidence, manifest, dataset_sha256, cases)
    report = score_evidence(evidence, manifest, cases)
    write_json(args.output, report)
    print(json.dumps(report))


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser(description=__doc__)
    commands = root.add_subparsers(dest="command", required=True)
    export = commands.add_parser("export", help="export the complete Skillwick corpus")
    export.add_argument("--skillwick", default="skillwick")
    export.add_argument("--output", type=Path, required=True)
    export.add_argument("--coverage", type=float, default=DEFAULT_COVERAGE)
    export.add_argument("--seed", type=int, default=DEFAULT_SEED)
    export.set_defaults(handler=export_corpus)
    validate = commands.add_parser("validate", help="validate a frozen corpus and dataset")
    validate.add_argument("--corpus", type=Path, required=True)
    validate.add_argument("--dataset", type=Path, required=True)
    validate.set_defaults(handler=validate_command)
    score = commands.add_parser("score", help="validate and score recorded evidence")
    score.add_argument("--corpus", type=Path, required=True)
    score.add_argument("--dataset", type=Path, required=True)
    score.add_argument("--evidence", type=Path, required=True)
    score.add_argument("--output", type=Path, required=True)
    score.set_defaults(handler=score_command)
    return root


def main() -> int:
    try:
        args = parser().parse_args()
        args.handler(args)
        return 0
    except (EvaluationError, json.JSONDecodeError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
