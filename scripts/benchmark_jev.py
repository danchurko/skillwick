"""Opt-in TypeSafe JEV reranking over frozen Skillwick candidate rankings."""

from __future__ import annotations

import hashlib
import json
import math
import os
import platform
import re
import statistics
import sys
import time
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Callable, Mapping
from urllib.parse import urlsplit

from benchmark_metrics import confidence_metrics, provenance, quality, retrieval_metrics, selection_metrics
from benchmark_profiles import (
    profile_rows,
    validate_candidate_document as shared_validate_candidate_document,
    validate_profile,
)

ASSETS = Path(__file__).resolve().parents[1] / "assets"
if str(ASSETS) not in sys.path:
    sys.path.insert(0, str(ASSETS))
from skillwick.reranker_runtime import (  # noqa: E402 - shared asset path is installed above
    CHOICE_CRITERIA as CHOICE_CRITERIA,
    JEVInputError,
    MODEL_NAME as MODEL_NAME,
    SDK_VERSION as SDK_VERSION,
    _client_scope,
    _default_client_factory as _shared_client_factory,
    build_request,
    failure_category,
    parse_response,
    safe_model,
    stable_rerank,
)

DEFAULT_MODEL = "jev-latest"
DEFAULT_BASE_URL = "https://api.typesafe.ai"
API_KEY_ENV = "TYPESAFE_API_KEY"
INPUT_RATE_USD_PER_MILLION = 0.042
OUTPUT_RATE_USD_PER_MILLION = 0.0
PRICING_AS_OF = "2026-10-03"


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def digest(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def read_json(path: Path) -> dict:
    value = json.loads(path.read_text())
    if not isinstance(value, dict):
        raise JEVInputError("invalid_json_document")
    return value


def fixture_identity(record: dict) -> str:
    value = record.get("fixture_id", record.get("name"))
    if not isinstance(value, str) or not value.strip():
        raise JEVInputError("invalid_profile")
    return value


def profile_queries(profile: dict) -> list[dict]:
    """Adapt the shared frozen-profile validator to the runner's safe error type."""
    try:
        validate_profile(profile)
        return profile_rows(profile)
    except (ValueError, TypeError, KeyError):
        raise JEVInputError("invalid_profile") from None


def validate_candidate_document(profile_path: Path, profile: dict, profile_rows: list[dict], candidate_path: Path,
                                candidates: dict, pool_size: int | None,
                                *, allowed_kinds: tuple[str, ...] = ("skillwick-lexical-baseline",),
                                require_provenance: bool = True) -> tuple[list[dict], int]:
    """Adapt the shared candidate contract to the runner's safe error type."""
    try:
        return shared_validate_candidate_document(
            profile_path, profile, profile_rows, candidate_path, candidates, pool_size,
            allowed_kinds=allowed_kinds, require_provenance=require_provenance,
        )
    except ValueError as error:
        category = str(error)
        if not re.fullmatch(r"[a-z_]+", category):
            category = "invalid_candidates"
        raise JEVInputError(category) from None


def validate_base_url(value: str | None) -> str | None:
    if value is None:
        return None
    if not isinstance(value, str):
        raise JEVInputError("invalid_base_url")
    try:
        parsed = urlsplit(value)
        hostname = parsed.hostname
        _ = parsed.port
    except ValueError as error:
        raise JEVInputError("invalid_base_url") from error
    if (parsed.scheme not in {"https", "http"} or not hostname or parsed.username or parsed.password
            or parsed.query or parsed.fragment):
        raise JEVInputError("invalid_base_url")
    loopback = hostname.lower() in {"localhost", "127.0.0.1", "::1"}
    if parsed.scheme != "https" and not loopback:
        raise JEVInputError("invalid_base_url")
    return value


def _default_client_factory(api_key: str, model: str, base_url: str | None, timeout: float):
    return _shared_client_factory(api_key, model, base_url, timeout)


def _timing_summary(values: list[float]) -> dict | None:
    if not values:
        return None
    ordered = sorted(values)
    return {
        "samples": len(values), "min": ordered[0], "median": statistics.median(values),
        "p95": ordered[min(len(ordered) - 1, math.ceil(len(ordered) * 0.95) - 1)], "max": ordered[-1],
    }


def _count_summary(values: list[int]) -> dict:
    return {
        "queries": len(values),
        "mean": statistics.fmean(values) if values else None,
        "min": min(values) if values else None,
        "median": statistics.median(values) if values else None,
        "max": max(values) if values else None,
    }


def _expected_models(value: str | None) -> str | None:
    if value is None:
        return None
    return safe_model(value)


def _query_result(row: dict, ranked: list[str], outcome: str, fallback_category: str | None,
                  provider_status: str, source_candidates: int, scored_candidates: int,
                  payload_bytes: int, latency_ms: float | None = None, resolved_model: str | None = None,
                  usage: dict | None = None, judgments: list[dict] | None = None) -> dict:
    return {
        "id": row["id"], "query": row["query"], "relevant": row["relevant"], "ranked": ranked,
        "outcome": outcome, "fallback_category": fallback_category,
        "provider_status": provider_status, "source_candidate_count": source_candidates,
        "scored_candidate_count": scored_candidates, "payload_bytes": payload_bytes,
        "latency_ms": latency_ms, "resolved_model": resolved_model, "usage": usage,
        "judgments": judgments,
    }


def _record_cost(rankings: list[dict], resolved_models: Mapping[str, int], missing_input_usage: int,
                 attempted: int, valid_responses: int, partial_smoke: bool) -> dict:
    token_counts = [item["usage"]["input_tokens"] for item in rankings
                    if item.get("provider_status") == "valid_response" and item.get("usage")
                    and item["usage"].get("input_tokens") is not None]
    estimate = None
    reasons = []
    if not token_counts:
        reasons.append("No provider input-token usage was returned.")
    if missing_input_usage:
        reasons.append("Some valid responses omitted input-token usage.")
    if not resolved_models or set(resolved_models) != {"jev-1.13.0"}:
        reasons.append("Official pricing was verified only for resolved model jev-1.13.0.")
    if token_counts and not missing_input_usage and resolved_models and set(resolved_models) == {"jev-1.13.0"}:
        estimate = sum(token_counts) * INPUT_RATE_USD_PER_MILLION / 1_000_000
    complete = estimate is not None and attempted == valid_responses
    if estimate is not None and not complete:
        reasons.append("Estimate covers valid responses only; failed requests may also incur charges.")
    if estimate is not None and complete:
        reasons.append("This is a usage-based estimate, not a provider billing total.")
    if partial_smoke:
        reasons.append("Estimate covers this smoke prefix only; unrequested profile queries are excluded.")
    return {
        "actual_cost_usd": None,
        "actual_cost_reason": "Provider billing totals were not returned or independently verified.",
        "estimated_cost_usd": estimate,
        "estimated_cost_complete": complete,
        "estimated_cost_scope": "valid responses in this run",
        "estimate_reason": "; ".join(reasons) if reasons else None,
        "pricing": {
            "source": "https://docs.typesafe.ai/models; official TypeSafe model pricing verified 2026-10-03",
            "input_usd_per_million_tokens": INPUT_RATE_USD_PER_MILLION,
            "output_usd_per_million_tokens": OUTPUT_RATE_USD_PER_MILLION,
            "as_of": PRICING_AS_OF,
        },
    }


def run_experiment(profile_path: Path, candidates_path: Path, output_path: Path, *,
                   model: str = DEFAULT_MODEL, base_url: str | None = None, timeout: float = 15.0,
                   confidence_threshold: float = 0.55, expected_model: str | None = None,
                   pool_size: int | None = None, max_queries: int | None = None, live: bool = False,
                   environ: Mapping[str, str] | None = None,
                   client_factory: Callable[..., tuple[Any, Callable[..., Any], str | None]] | None = None) -> int:
    started_wall = time.perf_counter_ns()
    model = safe_model(model)
    expected_model = _expected_models(expected_model)
    base_url = validate_base_url(base_url)
    if not math.isfinite(timeout) or not 0 < timeout <= 300:
        raise JEVInputError("invalid_timeout")
    if not math.isfinite(confidence_threshold) or not 0 <= confidence_threshold <= 1:
        raise JEVInputError("invalid_confidence_threshold")

    profile = read_json(profile_path)
    rows = profile_queries(profile)
    source_candidates = read_json(candidates_path)
    lexical_rows, pool_size = validate_candidate_document(
        profile_path, profile, rows, candidates_path, source_candidates, pool_size
    )
    if max_queries is not None and (isinstance(max_queries, bool) or not 1 <= max_queries <= len(rows)):
        raise JEVInputError("invalid_max_queries")
    if not live:
        raise JEVInputError("live_opt_in_required")

    env = os.environ if environ is None else environ
    api_key = env.get(API_KEY_ENV)
    if not isinstance(api_key, str) or not api_key.strip():
        api_key = None
    elif api_key.strip() != api_key:
        api_key = api_key.strip()
    selected_count = len(rows) if max_queries is None else max_queries
    records = {fixture_identity(record): record for record in profile["corpus"]["records"]}
    if api_key is not None:
        payload_text = [model, expected_model or "", base_url or DEFAULT_BASE_URL]
        payload_text.extend(row["query"] for row in rows)
        for record in records.values():
            payload_text.extend((record["name"], record["description"]))
        if any(api_key in value for value in payload_text):
            raise JEVInputError("secret_in_payload")
    attempted = 0
    valid_responses = 0
    failed_requests = 0
    total_payload_bytes = 0
    total_input_tokens = 0
    total_output_tokens = 0
    missing_input_usage = 0
    missing_output_usage = 0
    latencies: list[float] = []
    failures: Counter[str] = Counter()
    all_result_rows: list[dict] = []
    resolved_models: Counter[str] = Counter()
    sdk_version = None
    client = None
    choice_type = None
    client_error = None

    needs_client = api_key is not None and any(lexical_rows[i]["ranked"][:pool_size] for i in range(selected_count))
    if needs_client:
        try:
            factory = _default_client_factory if client_factory is None else client_factory
            client, choice_type, sdk_version = factory(api_key, model, base_url, timeout)
        except Exception as error:
            client_error = failure_category(error)
            if isinstance(error, ImportError):
                client_error = "sdk_unavailable"

    active_context = _client_scope(client) if client is not None else None
    active_client = None
    if active_context is not None:
        try:
            active_client = active_context.__enter__()
        except Exception as error:
            client_error = failure_category(error)
            active_context = None
    try:
        for index, (row, lexical) in enumerate(zip(rows, lexical_rows)):
            original = lexical["ranked"]
            selected = original[:pool_size]
            tail = original[pool_size:]
            payload_bytes = 0
            request_trace = {}
            if index >= selected_count:
                result = _query_result(row, original, "not_run_smoke", None, "not_attempted",
                                       len(original), 0, 0)
            elif not selected:
                result = _query_result(row, original, "skipped_no_candidates", "no_candidates", "not_attempted",
                                       0, 0, 0)
            elif api_key is None:
                failures["missing_api_key"] += 1
                result = _query_result(row, original, "lexical_fallback_provider_failure", "missing_api_key",
                                       "not_attempted", len(original), len(selected), 0)
            elif client_error is not None:
                failures[client_error] += 1
                result = _query_result(row, original, "lexical_fallback_provider_failure", client_error,
                                       "client_initialization_failed", len(original), len(selected), 0)
            else:
                state, questions, reverse_mapping, payload_bytes = build_request(
                    row["query"], selected, records, choice_type, request_trace
                )
                total_payload_bytes += payload_bytes
                question_keys = list(questions)
                attempted += 1
                call_started = time.perf_counter_ns()
                try:
                    response = active_client.system_one(
                        state=state, questions=questions, model=model, timeout=timeout,
                    )
                    latency_ms = (time.perf_counter_ns() - call_started) / 1_000_000
                    judgments, resolved, usage = parse_response(response, question_keys)
                    if api_key in resolved:
                        raise JEVInputError("secret_in_response")
                except Exception as error:
                    latency_ms = (time.perf_counter_ns() - call_started) / 1_000_000
                    latencies.append(latency_ms)
                    category = failure_category(error)
                    failures[category] += 1
                    failed_requests += 1
                    result = _query_result(
                        row, original, "lexical_fallback_provider_failure", category, "failed",
                        len(original), len(selected), payload_bytes, latency_ms,
                    )
                else:
                    latencies.append(latency_ms)
                    valid_responses += 1
                    resolved_models[resolved] += 1
                    input_tokens = usage["input_tokens"]
                    output_tokens = usage["output_tokens"]
                    if input_tokens is None:
                        missing_input_usage += 1
                    else:
                        total_input_tokens += input_tokens
                    if output_tokens is not None:
                        total_output_tokens += output_tokens
                    else:
                        missing_output_usage += 1
                    if expected_model is not None and resolved != expected_model:
                        failures["model_mismatch"] += 1
                        result = _query_result(
                            row, original, "lexical_fallback_provider_failure", "model_mismatch", "valid_response",
                            len(original), len(selected), payload_bytes, latency_ms, resolved, usage,
                            _judgment_rows(selected, judgments, reverse_mapping),
                        )
                    elif any(value["confidence"] < confidence_threshold for value in judgments.values()):
                        result = _query_result(
                            row, original, "lexical_fallback_low_confidence", "low_confidence", "valid_response",
                            len(original), len(selected), payload_bytes, latency_ms, resolved, usage,
                            _judgment_rows(selected, judgments, reverse_mapping),
                        )
                    else:
                        reranked = stable_rerank(selected, judgments, reverse_mapping) + tail
                        result = _query_result(
                            row, reranked, "reranked", None, "valid_response", len(original), len(selected),
                            payload_bytes, latency_ms, resolved, usage,
                            _judgment_rows(selected, judgments, reverse_mapping),
                        )
            if request_trace:
                result["request_trace"] = request_trace
                result["raw_provider_ranked"] = (stable_rerank(selected, judgments, reverse_mapping) + tail
                                                 if result["provider_status"] == "valid_response" else None)
                result["low_confidence_candidates"] = [
                    item["candidate"] for item in result.get("judgments") or []
                    if item["confidence"] < confidence_threshold
                ]
            all_result_rows.append(result)
    finally:
        if active_context is not None:
            try:
                active_context.__exit__(None, None, None)
            except Exception as error:
                failures[failure_category(error)] += 1

    full_run = selected_count == len(rows)
    has_provider_failure = bool(failures)
    all_outcomes = Counter(item["outcome"] for item in all_result_rows)
    attempted_rows = [item for item in all_result_rows if item["provider_status"] in {"valid_response", "failed"}]
    selected_rows = all_result_rows[:selected_count]
    reranked_rows = [
        {"id": item["id"], "query": item["query"], "relevant": item["relevant"], "ranked": item["ranked"]}
        for item in all_result_rows
    ]
    baseline_quality = quality(profile["version"], lexical_rows)
    result = {
        "result_schema_version": 1,
        "version": profile["version"],
        "kind": "skillwick-jev-reranker-experiment",
        "measured_at": datetime.now(timezone.utc).isoformat(),
        "status": "completed_with_failures" if has_provider_failure else (
            "completed" if full_run else "partial_smoke"
        ),
        "profile_sha256": digest(profile_path),
        "candidate_source_sha256": digest(candidates_path),
        "corpus_sha256": profile["corpus"]["sha256"],
        "candidate_source": {
            "kind": source_candidates["kind"],
            "pool_size": source_candidates["candidate_pool_size"],
            "provenance": source_candidates["provenance"],
        },
        "pool_size": pool_size,
        "smoke": {"enabled": not full_run, "requested_queries": selected_count, "profile_queries": len(rows)},
        "endpoint": {
            "base_url": base_url or DEFAULT_BASE_URL,
            "method_path": "/v1/systemone",
        },
        "model": {
            "requested": model, "expected_resolved": expected_model,
            "resolved_counts": dict(sorted(resolved_models.items())),
            "changed_from_requested": any(name != model for name in resolved_models),
            "sdk_version": sdk_version,
        },
        "formulation": {
            "method": "one batched TypeSafe system_one request per query",
            "judgment": "independent binary Choice probability of relevant for each candidate",
            "choice_labels": ["relevant", "not_relevant"],
            "payload_fields": ["task", "candidate id", "candidate name", "candidate description"],
            "candidate_ids": "request-local c000 labels map back to fixture identities in memory",
            "timeouts_seconds": timeout,
            "retries": 0,
            "confidence_threshold": confidence_threshold,
            "low_confidence_behavior": "preserve complete lexical order for that query",
            "tie_breaking": "original lexical candidate order",
            "payload_bytes_basis": "compact UTF-8 JSON size of whitelisted state and question specs; excludes SDK envelope and HTTP headers",
        },
        "runtime": {"python": platform.python_version(), "platform": platform.platform()},
        "request_counts": {
            "attempted": attempted, "valid_responses": valid_responses, "failed_requests": failed_requests,
            "single_request_per_query": True,
        },
        "candidate_counts": {
            "provider_attempts": {
                "source_pool": _count_summary([item["source_candidate_count"] for item in attempted_rows]),
                "scored_pool": _count_summary([item["scored_candidate_count"] for item in attempted_rows]),
            },
            "all_selected_queries": {
                "source_pool": _count_summary([item["source_candidate_count"] for item in selected_rows]),
                "scored_pool": _count_summary([item["scored_candidate_count"] for item in selected_rows]),
            },
        },
        "usage": {
            "input_tokens": total_input_tokens if valid_responses and not missing_input_usage else None,
            "output_tokens": total_output_tokens if valid_responses and not missing_output_usage else None,
            "queries_missing_input_usage": missing_input_usage,
            "queries_missing_output_usage": missing_output_usage,
        },
        "payload_bytes": total_payload_bytes,
        "latency_ms": _timing_summary(latencies),
        "wall_time_ms": (time.perf_counter_ns() - started_wall) / 1_000_000,
        "outcomes": dict(sorted(all_outcomes.items())),
        "failure_categories": dict(sorted(failures.items())),
        "quality": quality(profile["version"], reranked_rows) if full_run else None,
        "retrieval_metrics": retrieval_metrics(profile["version"], reranked_rows) if full_run else None,
        "selection_metrics": selection_metrics(lexical_rows, reranked_rows) if full_run else None,
        "lexical_baseline_quality": baseline_quality,
        "aggregate_metrics_status": "complete" if full_run else "suppressed_partial_smoke",
        "cost": _record_cost(all_result_rows, resolved_models, missing_input_usage,
                              attempted, valid_responses, partial_smoke=not full_run),
        "confidence_metrics": confidence_metrics(all_result_rows),
        "provenance": provenance(),
        "rankings": all_result_rows,
    }
    output_bytes = json.dumps(result, ensure_ascii=False, indent=2) + "\n"
    if api_key is not None and api_key in output_bytes:
        raise JEVInputError("secret_in_output")
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(output_bytes)
    return 1 if has_provider_failure else 0


def _judgment_rows(names: list[str], judgments: Mapping[str, dict], reverse_mapping: Mapping[str, str]) -> list[dict]:
    identity_by_question = reverse_mapping
    rows = []
    for question_key, value in judgments.items():
        rows.append({
            "candidate": identity_by_question[question_key],
            "choice": value["choice"], "confidence": value["confidence"],
            "probability_relevant": value["probability_relevant"],
        })
    order = {identity: index for index, identity in enumerate(names)}
    rows.sort(key=lambda item: order[item["candidate"]])
    return rows


def run_from_args(args: Any) -> None:
    try:
        status = run_experiment(
            args.profile, args.candidates, args.output, model=args.model, base_url=args.base_url,
            timeout=args.timeout, confidence_threshold=args.confidence_threshold,
            expected_model=args.expected_model, pool_size=args.pool_size,
            max_queries=args.max_queries, live=args.live,
        )
    except JEVInputError as error:
        print(f"error: {error.category}", file=__import__("sys").stderr)
        raise SystemExit(2) from None
    except (OSError, json.JSONDecodeError):
        print("error: input_or_output_failure", file=__import__("sys").stderr)
        raise SystemExit(2) from None
    output = read_json(args.output)
    print(json.dumps({
        "status": output["status"], "quality": output["quality"],
        "request_counts": output["request_counts"], "outcomes": output["outcomes"],
        "failure_categories": output["failure_categories"],
    }, sort_keys=True))
    if status:
        raise SystemExit(status)
