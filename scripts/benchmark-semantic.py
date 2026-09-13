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

ARCTIC = {
    "name": "snowflake/snowflake-arctic-embed-xs",
    "revision": "d8c86521100d3556476a063fc2342036d45c106f",
    "file": "onnx/model.onnx",
    "sha256": "cf2698d30ff05da02c70a088313bad56e5c2f401d734cb24a8390d446111936c",
    "bytes": 90_387_631,
    "license": "Apache-2.0",
}
TINYBERT = {
    "name": "cross-encoder/ms-marco-TinyBERT-L2-v2",
    "revision": "81d1926f67cb8eee2c2be17ca9f793c7c3bd20cc",
    "file": "onnx/model_qint8_arm64.onnx",
    "sha256": "7497b40504d425ef6482693039690106dca4f1f8d88fb5c4aedd63e73ed6ef68",
    "bytes": 4_518_071,
    "license": "Apache-2.0",
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


def locate(cache: Path, filename: str, size: int) -> Path | None:
    matches = [path for path in cache.rglob(Path(filename).name) if path.is_file() and path.stat().st_size == size]
    return matches[0] if matches else None


def pinned_path(cache: Path, model: dict) -> Path:
    repository = model["name"].replace("/", "--")
    return cache / f"models--{repository}" / "snapshots" / model["revision"] / model["file"]


def rss_kib() -> int:
    maximum = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    return maximum // 1024 if sys.platform == "darwin" else maximum


def metric(rankings: list[dict]) -> dict:
    recalls, reciprocals, ndcgs = [], [], []
    for item in rankings:
        ranked, relevant = item["ranked"][:5], set(item["relevant"])
        if not relevant:
            recalls.append(1.0 if not ranked else 0.0)
            reciprocals.append(1.0 if not ranked else 0.0)
            ndcgs.append(1.0 if not ranked else 0.0)
            continue
        hits = [name in relevant for name in ranked]
        recalls.append(sum(hits) / len(relevant))
        reciprocals.append(next((1 / (i + 1) for i, hit in enumerate(hits) if hit), 0.0))
        dcg = sum(hit / math.log2(i + 2) for i, hit in enumerate(hits))
        ideal = sum(1 / math.log2(i + 2) for i in range(min(len(relevant), 5)))
        ndcgs.append(dcg / ideal if ideal else 0.0)
    return {
        "queries": len(rankings), "recall_at_5": statistics.fmean(recalls),
        "mrr_at_5": statistics.fmean(reciprocals), "ndcg_at_5": statistics.fmean(ndcgs),
    }


def timings(values: list[float]) -> dict:
    values = sorted(values)
    return {
        "samples": len(values), "min": values[0], "median": statistics.median(values),
        "p95": values[min(len(values) - 1, math.ceil(len(values) * 0.95) - 1)], "max": values[-1],
    }


def common_result(kind: str, profile_path: Path, model: dict, load_ms: float, rankings: list[dict], warm: list[float], build_ms: float) -> dict:
    with tempfile.TemporaryDirectory() as temporary:
        corrupt = Path(temporary) / "model.onnx"
        corrupt.write_bytes(b"corrupt")
        failures = {}
        for name, path in (("missing", Path(temporary) / "missing.onnx"), ("corrupt", corrupt)):
            try:
                verify_artifact(path, model)
            except ValueError as error:
                failures[name] = str(error).split(":", 1)[0]
    return {
        "version": 1,
        "kind": kind,
        "measured_at": datetime.now(timezone.utc).isoformat(),
        "status": "research-only; no production behavior changed",
        "profile_sha256": digest(profile_path),
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
        "quality": metric(rankings),
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

    profile = read_json(args.profile)
    records = profile["corpus"]["records"]
    texts = [f"{record['name']}: {record['description']}" for record in records]
    if args.offline:
        verify_artifact(pinned_path(args.cache, ARCTIC), ARCTIC)
    started = time.perf_counter_ns()
    model = TextEmbedding(
        model_name=ARCTIC["name"], cache_dir=str(args.cache), threads=args.threads,
        local_files_only=args.offline,
    )
    load_ms = (time.perf_counter_ns() - started) / 1_000_000
    artifact = locate(args.cache, ARCTIC["file"], ARCTIC["bytes"])
    verify_artifact(artifact or args.cache / ARCTIC["file"], ARCTIC)
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
                "ranked": [records[position]["name"] for position in order],
            })
    result = common_result("skillwick-embedding-experiment", args.profile, ARCTIC, load_ms, rankings, warm, build_ms)
    result["design"] = "Arctic XS query prefix and pooling are owned by fastembed 0.8.0; top-20 cosine retrieval"
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result["quality"], sort_keys=True))


def tinybert_model(cache: Path, offline: bool):
    import numpy as np
    import onnxruntime as ort
    from huggingface_hub import hf_hub_download
    from tokenizers import Tokenizer

    if offline:
        verify_artifact(pinned_path(cache, TINYBERT), TINYBERT)
    files = {
        filename: Path(hf_hub_download(
            TINYBERT["name"], filename, revision=TINYBERT["revision"], cache_dir=cache,
            local_files_only=offline,
        ))
        for filename in (TINYBERT["file"], "tokenizer.json")
    }
    verify_artifact(files[TINYBERT["file"]], TINYBERT)
    tokenizer = Tokenizer.from_file(str(files["tokenizer.json"]))
    tokenizer.enable_truncation(max_length=512)
    tokenizer.enable_padding()
    session = ort.InferenceSession(str(files[TINYBERT["file"]]), providers=["CPUExecutionProvider"])

    def score(query: str, documents: list[str]) -> np.ndarray:
        encodings = tokenizer.encode_batch([(query, document) for document in documents])
        available = {value.name for value in session.get_inputs()}
        inputs = {
            "input_ids": np.asarray([value.ids for value in encodings], dtype=np.int64),
            "attention_mask": np.asarray([value.attention_mask for value in encodings], dtype=np.int64),
        }
        if "token_type_ids" in available:
            inputs["token_type_ids"] = np.asarray(
                [value.type_ids or [0] * len(value.ids) for value in encodings], dtype=np.int64
            )
        return np.asarray(session.run(None, inputs)[0]).reshape(-1)

    return score


def rerank(args: argparse.Namespace) -> None:
    import numpy as np

    profile = read_json(args.profile)
    candidates = read_json(args.candidates)
    descriptions = {row["name"]: f"{row['name']}: {row['description']}" for row in profile["corpus"]["records"]}
    started = time.perf_counter_ns()
    score = tinybert_model(args.cache, args.offline)
    load_ms = (time.perf_counter_ns() - started) / 1_000_000
    rankings, warm = [], []
    started_build = time.perf_counter_ns()
    for item in candidates["rankings"]:
        names = item["ranked"][:20]
        if not names:
            rankings.append({**item, "ranked": []})
            continue
        documents = [descriptions[name] for name in names]
        scores = None
        for _ in range(args.samples):
            started = time.perf_counter_ns()
            scores = score(item["query"], documents)
            warm.append((time.perf_counter_ns() - started) / 1_000_000)
        order = np.argsort(scores)[::-1]
        rankings.append({**item, "ranked": [names[position] for position in order]})
    build_ms = (time.perf_counter_ns() - started_build) / 1_000_000
    result = common_result("skillwick-reranker-experiment", args.profile, TINYBERT, load_ms, rankings, warm, build_ms)
    result["design"] = f"TinyBERT independently reranks the bounded top-20 pool from {candidates['kind']}"
    result["candidate_source_sha256"] = digest(args.candidates)
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result["quality"], sort_keys=True))


def main() -> None:
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(required=True)
    embed = commands.add_parser("embedding")
    embed.set_defaults(handler=embedding)
    reranker = commands.add_parser("rerank")
    reranker.add_argument("--candidates", type=Path, required=True)
    reranker.set_defaults(handler=rerank)
    for command in (embed, reranker):
        command.add_argument("--profile", type=Path, required=True)
        command.add_argument("--output", type=Path, required=True)
        command.add_argument("--cache", type=Path, required=True)
        command.add_argument("--samples", type=int, default=3)
        command.add_argument("--offline", action="store_true")
    embed.add_argument("--threads", type=int, default=4)
    args = parser.parse_args()
    args.handler(args)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, RuntimeError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1)
