#!/usr/bin/env python3
"""External-only embedding and reranking experiments for Skillwick."""

from __future__ import annotations

import argparse
import hashlib
import importlib.metadata
import json
import math
import platform
import resource
import statistics
import sys
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path

from benchmark_metrics import provenance, quality, retrieval_metrics, selection_metrics

ASSETS = Path(__file__).resolve().parents[1] / "assets"
if str(ASSETS) not in sys.path:
    sys.path.insert(0, str(ASSETS))
from skillwick.reranker_runtime import (
    RuntimeErrorCategory,
    TINYBERT,
    load_tinybert_scorer,
    pinned_model as shared_pinned_model,
)

ARCTIC = {
    "name": "snowflake/snowflake-arctic-embed-xs",
    "revision": "d8c86521100d3556476a063fc2342036d45c106f",
    "file": "onnx/model.onnx",
    "sha256": "cf2698d30ff05da02c70a088313bad56e5c2f401d734cb24a8390d446111936c",
    "bytes": 90_387_631,
    "license": "Apache-2.0",
    "required_files": {
        "config.json": {"sha256": "d7d071046ab952af96b7abad788db7ab3fc997b465e1b9914ff39707092254ec", "bytes": 737},
        "tokenizer.json": {"sha256": "91f1def9b9391fdabe028cd3f3fcc4efd34e5d1f08c3bf2de513ebb5911a1854", "bytes": 711_649},
        "tokenizer_config.json": {"sha256": "9ca59277519f6e3692c8685e26b94d4afca2d5438deff66483db495e48735810", "bytes": 1_433},
        "special_tokens_map.json": {"sha256": "5d5b662e421ea9fac075174bb0688ee0d9431699900b90662acd44b2a350503a", "bytes": 695},
        "onnx/model.onnx": {"sha256": "cf2698d30ff05da02c70a088313bad56e5c2f401d734cb24a8390d446111936c", "bytes": 90_387_631},
    },
}


def read_json(path: Path) -> dict:
    return json.loads(path.read_text())


def digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def verify_artifact(path: Path, expected: dict) -> None:
    if not path.is_file():
        raise ValueError(f"model artifact missing: {path}")
    if path.stat().st_size != expected["bytes"] or digest(path) != expected["sha256"]:
        raise ValueError(f"model artifact checksum mismatch: {path}")


def pinned_path(cache: Path, model: dict) -> Path:
    repository = model["name"].replace("/", "--")
    return cache / f"models--{repository}" / "snapshots" / model["revision"] / model["file"]


def pinned_model(cache: Path, model: dict, offline: bool) -> Path:
    if model.get("name") == TINYBERT["name"] and model.get("revision") == TINYBERT["revision"]:
        try:
            return shared_pinned_model(cache, model, allow_download=not offline)
        except RuntimeErrorCategory as error:
            labels = {
                "model_artifact_missing": "model artifact missing",
                "model_artifact_checksum": "model artifact checksum mismatch",
            }
            raise ValueError(f"{labels.get(error.category, 'model artifact resolution failed')}: {error.category}") from None
    from huggingface_hub import hf_hub_download

    root = pinned_path(cache, model).parents[len(Path(model["file"]).parts) - 1]
    for filename, expected in model["required_files"].items():
        path = Path(hf_hub_download(
            model["name"], filename, revision=model["revision"], cache_dir=cache,
            local_files_only=offline,
        ))
        verify_artifact(path, expected)
        if not path.absolute().is_relative_to(root.absolute()):
            raise ValueError(f"model artifact resolved outside pinned revision: {path}")
    return root


def rss_kib() -> int:
    maximum = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    return maximum // 1024 if sys.platform == "darwin" else maximum


def timings(values: list[float]) -> dict:
    values = sorted(values)
    return {
        "samples": len(values), "min": values[0], "median": statistics.median(values),
        "p95": values[min(len(values) - 1, math.ceil(len(values) * 0.95) - 1)], "max": values[-1],
    }


def common_result(kind: str, profile_path: Path, model: dict, load_ms: float, rankings: list[dict], warm: list[float],
                  build_ms: float, candidate_pool_size: int = 20) -> dict:
    with tempfile.TemporaryDirectory() as temporary:
        corrupt = Path(temporary) / "model.onnx"
        corrupt.write_bytes(b"corrupt")
        failures = {}
        for name, path in (("missing", Path(temporary) / "missing.onnx"), ("corrupt", corrupt)):
            try:
                verify_artifact(path, model)
            except ValueError as error:
                failures[name] = str(error).split(":", 1)[0]
    profile = read_json(profile_path)
    profile_version = profile["version"]
    return {
        "version": profile_version,
        "kind": kind,
        "measured_at": datetime.now(timezone.utc).isoformat(),
        "status": "research-only; no production behavior changed",
        "profile_sha256": digest(profile_path),
        "corpus_sha256": profile["corpus"]["sha256"],
        "corpus_total": profile["corpus"]["total"],
        "candidate_pool_size": candidate_pool_size,
        "provenance": provenance(),
        "model": model,
        "runtime": {
            "python": platform.python_version(), "platform": platform.platform(),
            "fastembed": importlib.metadata.version("fastembed"),
            "onnxruntime": importlib.metadata.version("onnxruntime"),
        },
        "measurements": {
            "cold_model_load_ms": load_ms, "index_or_candidate_build_ms": build_ms,
            "warm_query_ms": timings(warm), "peak_rss_kib": rss_kib(), "artifact_bytes": model["bytes"],
        },
        "quality": quality(profile_version, rankings),
        "retrieval_metrics": retrieval_metrics(profile_version, rankings),
        "selection_metrics": selection_metrics(rankings, rankings),
        "failure_behavior": {
            **failures,
            "verified_before_offline_model_load": True,
            "lexical_search_affected": False,
        },
        "rankings": rankings,
    }


def embedding(args: argparse.Namespace) -> None:
    import numpy as np
    from fastembed import TextEmbedding

    started_wall = time.perf_counter_ns()
    profile = read_json(args.profile)
    records = profile["corpus"]["records"]
    texts = [f"{record['name']}: {record['description']}" for record in records]
    model_dir = pinned_model(args.cache, ARCTIC, args.offline)
    started = time.perf_counter_ns()
    model = TextEmbedding(
        model_name=ARCTIC["name"], cache_dir=str(args.cache), threads=args.threads,
        local_files_only=True, specific_model_path=str(model_dir),
    )
    load_ms = (time.perf_counter_ns() - started) / 1_000_000
    started = time.perf_counter_ns()
    corpus = np.asarray(list(model.embed(texts, batch_size=64)), dtype=np.float32)
    corpus /= np.linalg.norm(corpus, axis=1, keepdims=True).clip(min=1e-12)
    build_ms = (time.perf_counter_ns() - started) / 1_000_000
    rankings, warm = [], []
    for case in profile["heldout"]["cases"]:
        for index, query in enumerate(case["queries"], 1):
            vector = None
            for _ in range(args.samples):
                started = time.perf_counter_ns()
                vector = np.asarray(next(iter(model.query_embed(query))), dtype=np.float32)
                warm.append((time.perf_counter_ns() - started) / 1_000_000)
            vector /= max(float(np.linalg.norm(vector)), 1e-12)
            scores = np.sum(corpus * vector, axis=1, dtype=np.float64)
            order = np.argsort(scores)[::-1][:20]
            rankings.append({
                "id": f"{case['id']}:{index}", "query": query, "relevant": case["relevant"],
                "ranked": [records[position].get("fixture_id", records[position]["name"]) for position in order],
            })
    result = common_result("skillwick-embedding-experiment", args.profile, ARCTIC, load_ms, rankings, warm, build_ms)
    result["wall_time_ms"] = (time.perf_counter_ns() - started_wall) / 1_000_000
    result["design"] = "Arctic XS query prefix and pooling are owned by fastembed 0.8.0; top-20 cosine retrieval"
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result["quality"], sort_keys=True))


def tinybert_model(cache: Path, offline: bool):
    try:
        return load_tinybert_scorer(cache, allow_download=not offline, check_versions=False)
    except RuntimeErrorCategory as error:
        labels = {
            "model_artifact_missing": "model artifact missing",
            "model_artifact_checksum": "model artifact checksum mismatch",
        }
        raise ValueError(f"{labels.get(error.category, 'model load failed')}: {error.category}") from None


def rerank(args: argparse.Namespace) -> None:
    import numpy as np

    started_wall = time.perf_counter_ns()
    profile = read_json(args.profile)
    candidates = read_json(args.candidates)
    from benchmark_profiles import profile_rows, validate_candidate_document

    try:
        expected_rows = profile_rows(profile)
        source_rows, selected_pool_size = validate_candidate_document(
            args.profile, profile, expected_rows, args.candidates, candidates, args.pool_size,
            allowed_kinds=("skillwick-lexical-baseline", "skillwick-embedding-experiment"),
            require_provenance=False,
        )
    except (ValueError, KeyError, TypeError):
        raise ValueError("candidate profile, provenance, query, label or identity contract failed") from None
    requested_pool_size = selected_pool_size
    identities = {
        row.get("fixture_id", row["name"]): f"{row['name']}: {row['description']}"
        for row in profile["corpus"]["records"]
    }
    started = time.perf_counter_ns()
    score = tinybert_model(args.cache, args.offline)
    load_ms = (time.perf_counter_ns() - started) / 1_000_000
    rankings, warm = [], []
    started_build = time.perf_counter_ns()
    for item in source_rows:
        lexical_names = item["ranked"]
        names = lexical_names[:requested_pool_size]
        tail = lexical_names[requested_pool_size:]
        if not names:
            rankings.append({**item, "ranked": lexical_names})
            continue
        documents = [identities[name] for name in names]
        scores = None
        for _ in range(args.samples):
            started = time.perf_counter_ns()
            scores = score(item["query"], documents)
            warm.append((time.perf_counter_ns() - started) / 1_000_000)
        # Preserve the historical TinyBERT runner's ordering for comparison.
        order = np.argsort(scores)[::-1]
        rankings.append({**item, "ranked": [names[position] for position in order] + tail})
    build_ms = (time.perf_counter_ns() - started_build) / 1_000_000
    result = common_result(
        "skillwick-reranker-experiment", args.profile, TINYBERT, load_ms, rankings, warm, build_ms,
        candidate_pool_size=requested_pool_size,
    )
    result["selection_metrics"] = selection_metrics(source_rows, rankings)
    result["source_candidate_pool_size"] = candidates.get("candidate_pool_size", 20)
    result["candidate_source_kind"] = candidates["kind"]
    result["design"] = f"TinyBERT reranks the bounded top-{requested_pool_size} pool from {candidates['kind']}"
    result["candidate_source_sha256"] = digest(args.candidates)
    result["candidate_source_provenance"] = candidates.get("provenance")
    result["wall_time_ms"] = (time.perf_counter_ns() - started_wall) / 1_000_000
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result["quality"], sort_keys=True))


def main() -> None:
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(required=True)
    embed = commands.add_parser("embedding")
    embed.set_defaults(handler=embedding)
    reranker = commands.add_parser("rerank")
    reranker.add_argument("--candidates", type=Path, required=True)
    reranker.add_argument("--pool-size", type=int, choices=range(1, 21))
    reranker.set_defaults(handler=rerank)
    jev = commands.add_parser("jev", help="opt-in TypeSafe JEV relevance reranking experiment")
    jev.add_argument("--candidates", type=Path, required=True)
    jev.add_argument("--profile", type=Path, required=True)
    jev.add_argument("--output", type=Path, required=True)
    jev.add_argument("--pool-size", type=int, choices=range(1, 21))
    jev.add_argument("--base-url")
    jev.add_argument("--model", default="jev-latest")
    jev.add_argument("--timeout", type=float, default=15.0)
    jev.add_argument("--confidence-threshold", type=float, default=0.55)
    jev.add_argument("--expected-model")
    jev.add_argument("--live", action="store_true", help="allow requests to the configured TypeSafe endpoint")
    jev.add_argument("--max-queries", type=int, help="run a prefix smoke sample; aggregate reranking metrics are omitted")
    jev.set_defaults(handler=jev_command)
    for command in (embed, reranker):
        command.add_argument("--profile", type=Path, required=True)
        command.add_argument("--output", type=Path, required=True)
        command.add_argument("--cache", type=Path, required=True)
        command.add_argument("--samples", type=int, default=3)
        command.add_argument("--offline", action="store_true")
    embed.add_argument("--threads", type=int, default=4)
    args = parser.parse_args()
    if hasattr(args, "samples") and args.samples < 1:
        parser.error("samples must be positive")
    args.handler(args)


def jev_command(args: argparse.Namespace) -> None:
    from benchmark_jev import run_from_args

    run_from_args(args)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, RuntimeError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1)
