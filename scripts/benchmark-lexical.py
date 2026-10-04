#!/usr/bin/env python3
"""Freeze and benchmark Skillwick's public lexical retrieval profile."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import platform
import re
import resource
import statistics
import subprocess
import sys
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path

from benchmark_metrics import ranking_metrics, task_metrics, retrieval_metrics, selection_metrics, provenance
from benchmark_profiles import canonical_hash, validate_profile


def read_json(path: Path) -> dict:
    return json.loads(path.read_text())


def result_rows(value: dict) -> list[dict]:
    rows = value.get("results")
    if not isinstance(rows, list):
        raise ValueError("Skillwick JSON has no results array")
    return rows


def freeze(args: argparse.Namespace) -> None:
    inventory = read_json(args.inventory)
    old_profile = read_json(args.queries)
    records = sorted(
        (
            {
                "name": row["name"],
                "description": row["description"],
                "source_family": row.get("plugin_id") or "native",
            }
            for row in result_rows(inventory)
        ),
        key=lambda row: row["name"],
    )
    names = {record["name"] for record in records}
    if len(names) != len(records):
        raise ValueError("benchmark corpus requires unique skill names")
    cases = []
    for case in old_profile["cases"]:
        if case["split"] != "heldout":
            continue
        relevant = sorted(
            name for identifier in case["relevant"] if (name := identifier.rsplit("@", 1)[0]) in names
        )
        if case["kind"] == "positive" and not relevant:
            continue
        cases.append(
            {
                "id": case["id"],
                "kind": case["kind"],
                "queries": [query["query"] for query in case["queries"]],
                "relevant": relevant,
            }
        )
    corpus_hash = canonical_hash(records)
    profile = {
        "version": 1,
        "kind": "skillwick-lexical-profile",
        "frozen_at": datetime.now(timezone.utc).isoformat(),
        "provenance": {
            "inventory_command": "skillwick --json list",
            "inventory_total": inventory.get("total"),
            "inventory_sha256": hashlib.sha256(args.inventory.read_bytes()).hexdigest(),
            "query_source": "benchmarks/local-skills-v2.json from git c0ea8d9^",
            "labels": "held-out labels frozen before this retrieval benchmark; path-derived ID suffixes mapped to unique names",
            "privacy": "paths, hashes, local source locations, and instruction bodies omitted",
        },
        "corpus": {"total": len(records), "sha256": corpus_hash, "records": records},
        "heldout": {"case_count": len(cases), "query_count": sum(len(case["queries"]) for case in cases), "cases": cases},
    }
    args.output.write_text(json.dumps(profile, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"corpus": len(records), "cases": len(cases), "sha256": corpus_hash}))


def validate(args: argparse.Namespace) -> None:
    profile = read_json(args.profile)
    validate_profile(profile)
    profile_hash = hashlib.sha256(args.profile.read_bytes()).hexdigest()
    query_count = profile["heldout"]["query_count"]
    for path in args.result:
        result = read_json(path)
        if result.get("profile_sha256") != profile_hash or result.get("version") != profile["version"]:
            raise ValueError(f"result profile identity mismatch: {path}")
        rankings = result.get("rankings", [])
        expected = [(f"{case['id']}:{index}", query, sorted(case["relevant"]))
                    for case in profile["heldout"]["cases"]
                    for index, query in enumerate(case["queries"], 1)]
        actual = [(item["id"], item["query"], sorted(item["relevant"])) for item in rankings]
        if actual != expected:
            raise ValueError(f"result queries or labels differ from frozen profile: {path}")
        if len(rankings) != query_count or result.get("quality") != (task_metrics if result.get("version") == 2 else ranking_metrics)(
            [(item["ranked"], set(item["relevant"])) for item in rankings]
        ):
            raise ValueError(f"result rankings or metrics are inconsistent: {path}")
        identities = {record.get("fixture_id", record["name"]) for record in profile["corpus"]["records"]}
        ranking_limit = result.get("source_candidate_pool_size", result.get("candidate_pool_size", 20))
        if isinstance(ranking_limit, bool) or not isinstance(ranking_limit, int) or not 1 <= ranking_limit <= 20:
            raise ValueError(f"result has an invalid candidate limit: {path}")
        for item in rankings:
            ranked = item["ranked"]
            if len(ranked) > ranking_limit or len(ranked) != len(set(ranked)) or not set(ranked) <= identities:
                raise ValueError(f"result has invalid candidate identities: {path}")
        executable = result.get("executable")
        if executable and Path(executable["path"]).is_absolute():
            raise ValueError(f"result exposes an absolute executable path: {path}")
    print(json.dumps({"profile": "valid", "results": len(args.result), "queries": query_count}))


def percentile(values: list[float], fraction: float) -> float:
    ordered = sorted(values)
    return ordered[min(len(ordered) - 1, math.ceil(len(ordered) * fraction) - 1)]


def summary(values: list[float]) -> dict:
    return {
        "samples": len(values),
        "min": min(values),
        "median": statistics.median(values),
        "p95": percentile(values, 0.95),
        "max": max(values),
    }


def run_command(command: list[str], env: dict[str, str], check: bool = True) -> tuple[subprocess.CompletedProcess[str], float]:
    started = time.perf_counter_ns()
    completed = subprocess.run(command, env=env, text=True, capture_output=True, timeout=30)
    elapsed_ms = (time.perf_counter_ns() - started) / 1_000_000
    if check and completed.returncode:
        raise RuntimeError(f"command failed ({completed.returncode}): {' '.join(command)}\n{completed.stderr}")
    return completed, elapsed_ms


def maximum_rss_kib(command: list[str], env: dict[str, str]) -> int | None:
    if sys.platform != "darwin" or not Path("/usr/bin/time").exists():
        return None
    completed = subprocess.run(["/usr/bin/time", "-l", *command], env=env, text=True, capture_output=True)
    match = re.search(r"(\d+)\s+maximum resident set size", completed.stderr)
    if match:
        return int(match.group(1)) // 1024
    maximum = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss
    return maximum // 1024 if sys.platform == "darwin" else maximum


def materialize(records: list[dict], root: Path) -> None:
    for index, record in enumerate(records):
        directory = root / f"skill-{index:04d}"
        directory.mkdir(parents=True)
        body = "---\nname: {}\ndescription: {}\n---\n".format(
            json.dumps(record["name"], ensure_ascii=False),
            json.dumps(record["description"], ensure_ascii=False),
        )
        (directory / "SKILL.md").write_text(body)


def benchmark(args: argparse.Namespace) -> None:
    profile = read_json(args.profile)
    validate_profile(profile)
    binary_label = str(args.binary)
    binary = args.binary.resolve()
    if not binary.exists():
        raise ValueError(f"binary not found: {binary}")
    with tempfile.TemporaryDirectory(prefix="skillwick-lexical-") as temporary_name:
        temporary = Path(temporary_name)
        root, workspace = temporary / "skills", temporary / "workspace"
        workspace.mkdir()
        materialize(profile["corpus"]["records"], root)
        env = os.environ.copy()
        env.update(
            HOME=str(temporary / "home"),
            CODEX_HOME=str(temporary / "codex"),
            CLAUDE_CONFIG_DIR=str(temporary / "claude"),
            XDG_CONFIG_HOME=str(temporary / "config"),
            XDG_CACHE_HOME=str(temporary / "cache"),
            XDG_STATE_HOME=str(temporary / "state"),
        )
        config = temporary / "config.toml"
        common = [str(binary), "--cwd", str(workspace), "--config", str(config)]
        startup = [run_command([str(binary), "--version"], env)[1] for _ in range(args.samples)]
        version = run_command([str(binary), "--version"], env)[0].stdout.strip()
        init = [*common, "init", "--yes", "--agent", "none", "--root", str(root)]
        # Historical baseline binaries retain their original setup contract.
        if tuple(int(part) for part in version.split()[-1].split(".")[:2]) >= (0, 4):
            init += ["--discovery", "explicit"]
        _, refresh_ms = run_command(init, env)
        fixture_ids = {f"skill-{index:04d}": record.get("fixture_id", record["name"])
                       for index, record in enumerate(profile["corpus"]["records"])}
        queries = [
            (f"{case['id']}:{index}", query, set(case["relevant"]))
            for case in profile["heldout"]["cases"]
            for index, query in enumerate(case["queries"], 1)
        ]
        cold_command = [*common, "--json", "search", queries[0][1], "--limit", str(args.pool_size)]
        _, cold_search_ms = run_command(cold_command, env)
        warm_ms, rankings, ranking_records = [], [], []
        for query_id, query, relevant in queries:
            command = [*common, "--json", "search", query, "--limit", str(args.pool_size)]
            for _ in range(args.samples):
                completed, elapsed = run_command(command, env)
                warm_ms.append(elapsed)
            ranked = [fixture_ids[Path(row["canonical"]).parent.name] if profile["version"] == 2 else row["name"]
                      for row in result_rows(json.loads(completed.stdout))]
            rankings.append((ranked, relevant))
            ranking_records.append({"id": query_id, "query": query, "relevant": sorted(relevant), "ranked": ranked})
        cache = next((temporary / "cache" / "skillwick").glob("index-*.sqlite"))
        version = run_command([str(binary), "--version"], env)[0].stdout.strip()
        rss = maximum_rss_kib([*common, "--json", "search", queries[0][1], "--limit", str(args.pool_size)], env)
        result = {
            "version": profile["version"],
            "kind": "skillwick-lexical-baseline",
            "measured_at": datetime.now(timezone.utc).isoformat(),
            "profile_sha256": hashlib.sha256(args.profile.read_bytes()).hexdigest(),
            "corpus_sha256": profile["corpus"]["sha256"],
            "corpus_total": profile["corpus"]["total"],
            "conditions": {
                "cold": "fresh process; kernel file caches were not flushed",
                "warm": f"separate automatically reconciled search processes, {args.samples} samples per query",
                "optional_model_artifacts": False,
            },
            "machine": {
                "system": platform.system(), "release": platform.release(), "machine": platform.machine(),
                "processor": platform.processor(), "python": platform.python_version(),
            },
            "executable": {
                "path": Path(binary_label).name, "version": version,
                "sha256": hashlib.sha256(binary.read_bytes()).hexdigest(), "bytes": binary.stat().st_size,
            },
            "measurements": {
                "startup_ms": summary(startup), "cold_search_ms": cold_search_ms,
                "warm_search_ms": summary(warm_ms), "refresh_ms": refresh_ms,
                "index_bytes": cache.stat().st_size, "search_max_rss_kib": rss,
            },
            "case_count": len(profile["heldout"]["cases"]),
            "candidate_pool_size": args.pool_size,
            "provenance": provenance(),
            "retrieval_metrics": retrieval_metrics(profile["version"], ranking_records),
            "selection_metrics": selection_metrics(ranking_records, ranking_records),
            "quality": (task_metrics if profile["version"] == 2 else ranking_metrics)(rankings),
            "rankings": ranking_records,
        }
        args.output.write_text(json.dumps(result, indent=2) + "\n")
        print(json.dumps(result["measurements"], sort_keys=True))


def main() -> None:
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(required=True)
    freeze_parser = commands.add_parser("freeze")
    freeze_parser.add_argument("--inventory", type=Path, required=True)
    freeze_parser.add_argument("--queries", type=Path, required=True)
    freeze_parser.add_argument("--output", type=Path, required=True)
    freeze_parser.set_defaults(handler=freeze)
    validate_parser = commands.add_parser("validate")
    validate_parser.add_argument("--profile", type=Path, required=True)
    validate_parser.add_argument("--result", type=Path, action="append", default=[])
    validate_parser.set_defaults(handler=validate)
    run_parser = commands.add_parser("run")
    run_parser.add_argument("--binary", type=Path, required=True)
    run_parser.add_argument("--profile", type=Path, required=True)
    run_parser.add_argument("--output", type=Path, required=True)
    run_parser.add_argument("--pool-size", type=int, choices=range(1, 21), default=20)
    run_parser.add_argument("--samples", type=int, default=5)
    run_parser.set_defaults(handler=benchmark)
    args = parser.parse_args()
    if hasattr(args, "samples") and args.samples < 1:
        parser.error("samples must be positive")
    args.handler(args)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, RuntimeError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(1)
