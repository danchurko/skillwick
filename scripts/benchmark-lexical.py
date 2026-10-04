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
import tomllib
import uuid
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


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def new_cache_home(parent: Path) -> Path:
    parent.mkdir(parents=True, exist_ok=True)
    path = parent.resolve() / f"xdg-cache-{uuid.uuid4().hex}"
    path.mkdir(mode=0o700)
    return path


def real_environment(cache_home: Path, base: dict[str, str] | None = None) -> dict[str, str]:
    """Preserve agent/config/state roots; isolate only the derived CLI cache."""
    env = (os.environ if base is None else base).copy()
    env["XDG_CACHE_HOME"] = str(cache_home.resolve())
    return env


def real_config_policy(path: Path) -> dict:
    try:
        config = tomllib.loads(path.read_text())
    except (OSError, tomllib.TOMLDecodeError) as error:
        raise ValueError(f"cannot read real-source config: {error}") from None
    if config.get("discovery") != "auto":
        raise ValueError("real-source qualification requires discovery = auto")
    reranker = config.get("reranker", {})
    if not isinstance(reranker, dict) or reranker.get("backend", "none") != "none":
        raise ValueError("real-source qualification requires reranker backend none")
    roots = config.get("roots", [])
    projects = config.get("projects", [])
    agents = config.get("agents", [])
    if not all(isinstance(value, list) for value in (roots, projects, agents)):
        raise ValueError("real-source config roots, projects, and agents must be arrays")
    return {
        "discovery": "auto",
        "configured_root_count": len(roots),
        "project_count": len(projects),
        "agent_count": len(agents),
        "reranker_backend": "none",
    }


def validate_inventory(inventory: dict) -> list[dict]:
    if not isinstance(inventory, dict) or inventory.get("version") != 3:
        raise ValueError("real-source inventory must use Skillwick JSON format version 3")
    rows = result_rows(inventory)
    total = inventory.get("total")
    if isinstance(total, bool) or not isinstance(total, int) or total != len(rows):
        raise ValueError("real-source inventory is incomplete")
    ids = set()
    for row in rows:
        if not isinstance(row, dict):
            raise ValueError("real-source inventory contains an invalid row")
        identifier, name, description = row.get("id"), row.get("name"), row.get("description")
        if not all(isinstance(value, str) and value.strip() for value in (identifier, name, description)):
            raise ValueError("real-source inventory row is missing id, name, or description")
        if identifier in ids:
            raise ValueError(f"real-source inventory contains duplicate id: {identifier}")
        ids.add(identifier)
        if row.get("source_kind") != "filesystem" or row.get("enabled") is not True:
            raise ValueError(f"installed eligible inventory contains an ineligible row: {identifier}")
        if not isinstance(row.get("scope"), str) or not isinstance(row.get("degraded"), bool):
            raise ValueError(f"real-source inventory eligibility metadata is incomplete: {identifier}")
        if not isinstance(row.get("hash"), str) or not re.fullmatch(r"[0-9a-f]{64}", row["hash"]):
            raise ValueError(f"real-source inventory content fingerprint is invalid: {identifier}")
        if not isinstance(row.get("origins"), list) or not row["origins"]:
            raise ValueError(f"real-source inventory grouping data is incomplete: {identifier}")
    return rows


def inventory_sha256(inventory: dict) -> str:
    """Fingerprint the full private inventory, including paths and origins."""
    rows = validate_inventory(inventory)
    canonical = {
        "version": inventory["version"],
        "total": inventory["total"],
        "results": sorted(rows, key=lambda row: row["id"]),
    }
    return canonical_hash(canonical)


def validate_case_input(document: dict) -> None:
    if not isinstance(document, dict) or document.get("version") != 1:
        raise ValueError("unsupported real-source case input")
    if not isinstance(document.get("label_policy"), str) or not document["label_policy"].strip():
        raise ValueError("real-source case input requires a label policy")
    cases, diagnostics = document.get("cases"), document.get("diagnostics")
    if not isinstance(cases, list) or not cases or not isinstance(diagnostics, list):
        raise ValueError("real-source case input requires cases and diagnostics arrays")
    identities = set()
    for case in cases:
        if not isinstance(case, dict):
            raise ValueError("invalid real-source scored case")
        case_id, kind, queries, relevant = (
            case.get("id"), case.get("kind"), case.get("queries"), case.get("relevant")
        )
        if not isinstance(case_id, str) or not case_id.strip() or case_id in identities:
            raise ValueError("real-source case identities must be nonempty and unique")
        if not isinstance(kind, str) or kind not in {"positive", "negative"}:
            raise ValueError(f"invalid scored case kind: {case_id}")
        if not isinstance(queries, list) or not queries or any(
            not isinstance(query, str) or not query.strip() for query in queries
        ):
            raise ValueError(f"real-source queries must be nonempty strings: {case_id}")
        if not isinstance(relevant, list) or any(not isinstance(name, str) or not name.strip() for name in relevant):
            raise ValueError(f"invalid relevance labels: {case_id}")
        if len(set(relevant)) != len(relevant) or (kind == "positive") != bool(relevant):
            raise ValueError(f"case kind and relevance labels disagree: {case_id}")
        identities.add(case_id)
    for diagnostic in diagnostics:
        if not isinstance(diagnostic, dict):
            raise ValueError("invalid real-source diagnostic")
        diagnostic_id, query, reason = (
            diagnostic.get("id"), diagnostic.get("query"), diagnostic.get("reason")
        )
        if not isinstance(diagnostic_id, str) or not diagnostic_id.strip() or diagnostic_id in identities:
            raise ValueError("real-source case and diagnostic identities must be unique")
        if not isinstance(query, str) or not query.strip() or not isinstance(reason, str) or not reason.strip():
            raise ValueError(f"real-source diagnostic requires query and reason: {diagnostic_id}")
        identities.add(diagnostic_id)


def build_real_profile(
    inventory: dict,
    case_input: dict,
    *,
    inventory_raw_sha256: str,
    config_sha256: str,
    config_policy: dict,
    executable: dict,
    case_input_sha256: str,
) -> dict:
    """Freeze labels against the complete currently eligible installed corpus."""
    rows = validate_inventory(inventory)
    validate_case_input(case_input)
    if not re.fullmatch(r"[0-9a-f]{64}", inventory_raw_sha256):
        raise ValueError("invalid raw inventory fingerprint")
    if not re.fullmatch(r"[0-9a-f]{64}", config_sha256):
        raise ValueError("invalid config fingerprint")
    if not re.fullmatch(r"[0-9a-f]{64}", case_input_sha256):
        raise ValueError("invalid case input fingerprint")

    records = []
    by_name: dict[str, list[dict]] = {}
    for row in rows:
        origins = row["origins"]
        if any(not isinstance(origin, dict) for origin in origins):
            raise ValueError(f"invalid origin record for {row['id']}")
        record = {
            "fixture_id": row["id"],
            "name": row["name"],
            "description": row["description"],
            "source_family": "plugin" if row.get("plugin_id") else "native",
            "content_sha256": row["hash"],
            "source_kind": row["source_kind"],
            "scope": row["scope"],
            "enabled": row["enabled"],
            "degraded": row["degraded"],
            "source_origin_count": len(origins),
            "grouping_diagnostic_present": row.get("grouping_diagnostic") is not None,
            "plugin_id_present": row.get("plugin_id") is not None,
            # `list` is sourced from the installed CLI's model-discoverable inventory.
            "invocation_policy": "model-discoverable",
        }
        records.append(record)
        by_name.setdefault(row["name"], []).append(record)
    records.sort(key=lambda record: record["fixture_id"])

    missing, ambiguous = [], []
    cases = []
    for case in case_input["cases"]:
        relevant_ids = []
        for name in case["relevant"]:
            matches = by_name.get(name, [])
            if not matches:
                missing.append(f"{case['id']}:{name}")
            elif len(matches) > 1:
                ambiguous.append(f"{case['id']}:{name} ({len(matches)} eligible rows)")
            else:
                relevant_ids.append(matches[0]["fixture_id"])
        cases.append({
            "id": case["id"],
            "kind": case["kind"],
            "queries": list(case["queries"]),
            "relevant": sorted(relevant_ids),
        })
    if missing or ambiguous:
        details = []
        if missing:
            details.append("missing labelled targets: " + ", ".join(missing))
        if ambiguous:
            details.append("ambiguous labelled targets: " + ", ".join(ambiguous))
        raise ValueError("real-source corpus is not ready: " + "; ".join(details))

    diagnostics = [
        {"id": item["id"], "query": item["query"], "reason": item["reason"]}
        for item in case_input["diagnostics"]
    ]
    profile = {
        "version": 2,
        "kind": "skillwick-lexical-profile",
        "frozen_at": datetime.now(timezone.utc).isoformat(),
        "provenance": {
            "source_mode": "actual-installed-source",
            "inventory_command": "skillwick --json list",
            "inventory_total": inventory["total"],
            "inventory_format_version": inventory["version"],
            "inventory_canonical_sha256": inventory_sha256(inventory),
            "inventory_raw_sha256": inventory_raw_sha256,
            "source_policy": "discovery=auto; installed CLI list eligibility is filesystem, enabled, and model-discoverable",
            "eligibility_fields": "name, description, content fingerprint, source kind, scope, enabled/degraded state, grouping count/diagnostic, plugin presence, and invocation policy",
            "config_sha256": config_sha256,
            "config_policy": config_policy,
            "case_input_sha256": case_input_sha256,
            "labels": case_input["label_policy"],
            "privacy": "full source paths and origin records are bound by the private inventory digest; sanitized metadata only",
            "executable": executable,
            "evaluator": provenance(),
        },
        "corpus": {
            "total": len(records),
            "sha256": canonical_hash(records),
            "records": records,
        },
        "heldout": {
            "case_count": len(cases),
            "query_count": sum(len(case["queries"]) for case in cases),
            "sha256": canonical_hash(cases),
            "cases": cases,
        },
        "diagnostics": {
            "case_count": len(diagnostics),
            "query_count": len(diagnostics),
            "sha256": canonical_hash(diagnostics),
            "cases": diagnostics,
        },
    }
    validate_real_profile(profile)
    return profile


def validate_real_profile(profile: dict) -> None:
    validate_profile(profile)
    provenance_value = profile.get("provenance", {})
    if provenance_value.get("source_mode") != "actual-installed-source":
        raise ValueError("profile is not a real installed-source profile")
    for field in ("inventory_canonical_sha256", "inventory_raw_sha256", "config_sha256", "case_input_sha256"):
        if not isinstance(provenance_value.get(field), str) or not re.fullmatch(r"[0-9a-f]{64}", provenance_value[field]):
            raise ValueError(f"real-source profile has invalid {field}")
    if provenance_value.get("inventory_total") != profile["corpus"]["total"]:
        raise ValueError("real-source profile inventory and corpus totals differ")
    if provenance_value.get("inventory_format_version") != 3:
        raise ValueError("real-source profile has an unsupported inventory format")
    executable = provenance_value.get("executable")
    if not isinstance(executable, dict) or not isinstance(executable.get("version"), str):
        raise ValueError("real-source profile is missing executable identity")
    if not isinstance(executable.get("sha256"), str) or not re.fullmatch(r"[0-9a-f]{64}", executable["sha256"]):
        raise ValueError("real-source profile has an invalid executable fingerprint")
    if isinstance(executable.get("bytes"), bool) or not isinstance(executable.get("bytes"), int) or executable["bytes"] <= 0:
        raise ValueError("real-source profile has an invalid executable size")
    for record in profile["corpus"]["records"]:
        if not isinstance(record.get("content_sha256"), str) or not re.fullmatch(r"[0-9a-f]{64}", record["content_sha256"]):
            raise ValueError("real-source profile has an invalid content fingerprint")
    diagnostics = profile.get("diagnostics")
    if not isinstance(diagnostics, dict) or not isinstance(diagnostics.get("cases"), list):
        raise ValueError("real-source profile has no diagnostic cases")
    diagnostic_cases = diagnostics["cases"]
    if diagnostics.get("case_count") != len(diagnostic_cases) or diagnostics.get("query_count") != len(diagnostic_cases):
        raise ValueError("real-source profile diagnostic counts are incorrect")
    if diagnostics.get("sha256") != canonical_hash(diagnostic_cases):
        raise ValueError("real-source diagnostic labels differ from frozen identity")
    identities = {case["id"] for case in profile["heldout"]["cases"]}
    for case in diagnostic_cases:
        if (not isinstance(case, dict) or not isinstance(case.get("id"), str)
                or not case["id"].strip() or case["id"] in identities):
            raise ValueError("real-source diagnostic identities must be unique")
        if (not isinstance(case.get("query"), str) or not case["query"].strip()
                or not isinstance(case.get("reason"), str) or not case["reason"].strip()):
            raise ValueError("invalid real-source diagnostic")
        identities.add(case["id"])


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
    real_source = profile.get("provenance", {}).get("source_mode") == "actual-installed-source"
    if real_source:
        validate_real_profile(profile)
    else:
        validate_profile(profile)
    profile_hash = hashlib.sha256(args.profile.read_bytes()).hexdigest()
    query_count = profile["heldout"]["query_count"]
    results = [(path, read_json(path)) for path in args.result]
    for path, result in results:
        if result.get("profile_sha256") != profile_hash or result.get("version") != profile["version"]:
            raise ValueError(f"result profile identity mismatch: {path}")
        if real_source and (
            result.get("corpus_sha256") != profile["corpus"]["sha256"]
            or result.get("corpus_total") != profile["corpus"]["total"]
        ):
            raise ValueError(f"result corpus identity mismatch: {path}")
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
        if real_source:
            ranking_records = [
                {"id": item["id"], "query": item["query"],
                 "relevant": item["relevant"], "ranked": item["ranked"]}
                for item in rankings
            ]
            if result.get("retrieval_metrics") != retrieval_metrics(profile["version"], ranking_records):
                raise ValueError(f"result retrieval metrics are inconsistent: {path}")
            if result.get("kind") == "skillwick-lexical-baseline":
                validate_real_result(profile, profile_hash, result)
            elif result.get("kind") != "skillwick-reranker-experiment":
                raise ValueError(f"real-source result has an unsupported candidate kind: {path}")
        executable = result.get("executable")
        if executable and Path(executable["path"]).is_absolute():
            raise ValueError(f"result exposes an absolute executable path: {path}")
    if real_source:
        lexical_sources = [
            (path, result)
            for path, result in results
            if result.get("kind") == "skillwick-lexical-baseline"
        ]
        for path, result in results:
            if result.get("kind") != "skillwick-reranker-experiment":
                continue
            source_hash = result.get("candidate_source_sha256")
            if not isinstance(source_hash, str) or not re.fullmatch(r"[0-9a-f]{64}", source_hash):
                raise ValueError(f"real-source semantic result has no valid candidate source hash: {path}")
            matching_sources = [
                (source_path, source)
                for source_path, source in lexical_sources
                if file_sha256(source_path) == source_hash
            ]
            if not matching_sources:
                raise ValueError(
                    f"real-source semantic result is missing its matching actual-source lexical candidate receipt: {path}"
                )
            if len(matching_sources) != 1:
                raise ValueError(f"real-source semantic candidate receipt is ambiguous: {path}")
            source_path, source = matching_sources[0]
            validate_real_semantic_result(profile, profile_hash, result, source_path, source)
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


def run_real_command(
    command: list[str], env: dict[str, str], cwd: Path, check: bool = True
) -> tuple[subprocess.CompletedProcess[str], float]:
    started = time.perf_counter_ns()
    completed = subprocess.run(command, env=env, cwd=cwd, text=True, capture_output=True, timeout=120)
    elapsed_ms = (time.perf_counter_ns() - started) / 1_000_000
    if check and completed.returncode:
        raise RuntimeError(f"command failed ({completed.returncode}): {' '.join(command)}\n{completed.stderr}")
    return completed, elapsed_ms


def executable_identity(binary: Path, env: dict[str, str], cwd: Path) -> dict:
    if not binary.is_file():
        raise ValueError(f"binary not found: {binary}")
    completed, _ = run_real_command([str(binary), "--version"], env, cwd)
    version = completed.stdout.strip()
    if not version:
        raise ValueError("installed CLI returned an empty version")
    return {
        "path": binary.name,
        "version": version,
        "sha256": file_sha256(binary),
        "bytes": binary.stat().st_size,
    }


def same_executable(left: dict, right: dict) -> bool:
    return all(left.get(field) == right.get(field) for field in ("version", "sha256", "bytes"))


def cli_prefix(binary: Path, config: Path, cwd: Path) -> list[str]:
    return [str(binary), "--config", str(config), "--cwd", str(cwd)]


def capture_real_inventory(
    binary: Path, config: Path, cwd: Path, env: dict[str, str]
) -> tuple[dict, str, float]:
    command = [*cli_prefix(binary, config, cwd), "--json", "list"]
    completed, elapsed_ms = run_real_command(command, env, cwd)
    try:
        inventory = json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        raise ValueError(f"installed CLI returned invalid inventory JSON: {error}") from None
    validate_inventory(inventory)
    return inventory, completed.stdout, elapsed_ms


def write_once(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.{uuid.uuid4().hex}.tmp")
    try:
        descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descriptor, "w", encoding="utf-8") as output:
            output.write(content)
            output.flush()
            os.fsync(output.fileno())
        if path.exists():
            raise ValueError(f"refusing to overwrite existing artifact: {path}")
        os.link(temporary, path)
    except FileExistsError:
        raise ValueError(f"refusing to overwrite existing artifact: {path}") from None
    finally:
        temporary.unlink(missing_ok=True)


def freeze_real(args: argparse.Namespace) -> None:
    binary, config, cwd = args.binary.resolve(strict=True), args.config.resolve(strict=True), args.cwd.resolve(strict=True)
    if not cwd.is_dir():
        raise ValueError(f"working directory is not a directory: {cwd}")
    if args.output.resolve() == args.inventory_output.resolve():
        raise ValueError("profile and raw inventory outputs must be different paths")
    if args.output.exists() or args.inventory_output.exists():
        raise ValueError("real-source outputs must be new paths; existing artifacts are preserved")
    config_policy = real_config_policy(config)
    case_input_bytes = args.cases.read_bytes()
    try:
        case_input = json.loads(case_input_bytes)
    except json.JSONDecodeError as error:
        raise ValueError(f"invalid real-source case input: {error}") from None
    validate_case_input(case_input)
    cache_home = new_cache_home(args.cache_dir)
    env = real_environment(cache_home)
    config_sha_before = file_sha256(config)
    executable_before = executable_identity(binary, env, cwd)
    inventory, inventory_text, inventory_ms = capture_real_inventory(binary, config, cwd, env)
    inventory_raw_sha = hashlib.sha256(inventory_text.encode()).hexdigest()
    write_once(args.inventory_output, inventory_text)
    executable_after = executable_identity(binary, env, cwd)
    config_sha_after = file_sha256(config)
    if not same_executable(executable_before, executable_after) or config_sha_before != config_sha_after:
        raise ValueError("installed CLI or config changed while freezing the real-source inventory")
    profile = build_real_profile(
        inventory,
        case_input,
        inventory_raw_sha256=inventory_raw_sha,
        config_sha256=config_sha_before,
        config_policy=config_policy,
        executable=executable_before,
        case_input_sha256=hashlib.sha256(case_input_bytes).hexdigest(),
    )
    profile["provenance"]["inventory_capture_ms"] = inventory_ms
    write_once(args.output, json.dumps(profile, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({
        "status": "frozen",
        "eligible_records": profile["corpus"]["total"],
        "scored_cases": profile["heldout"]["case_count"],
        "scored_queries": profile["heldout"]["query_count"],
        "diagnostics": profile["diagnostics"]["case_count"],
        "inventory_sha256": profile["provenance"]["inventory_canonical_sha256"],
    }, sort_keys=True))


def query_rows(profile: dict) -> list[tuple[str, str, set[str]]]:
    return [
        (f"{case['id']}:{index}", query, set(case["relevant"]))
        for case in profile["heldout"]["cases"]
        for index, query in enumerate(case["queries"], 1)
    ]


def validate_real_result(profile: dict, profile_hash: str, result: dict) -> None:
    if result.get("kind") != "skillwick-lexical-baseline":
        raise ValueError("real-source result has an invalid candidate kind")
    replay = result.get("actual_source_replay")
    if not isinstance(replay, dict) or replay.get("source_mode") != "actual-installed-source":
        raise ValueError("result does not prove an actual installed-source replay")
    provenance_value = profile["provenance"]
    if result.get("profile_sha256") != profile_hash:
        raise ValueError("real-source result profile identity mismatch")
    if replay.get("inventory_before_sha256") != provenance_value["inventory_canonical_sha256"]:
        raise ValueError("real-source result pre-replay inventory differs from the frozen profile")
    if replay.get("inventory_after_sha256") != provenance_value["inventory_canonical_sha256"]:
        raise ValueError("real-source result post-replay inventory differs from the frozen profile")
    if replay.get("config_sha256") != provenance_value["config_sha256"]:
        raise ValueError("real-source result config differs from the frozen profile")
    if replay.get("case_input_sha256") != provenance_value["case_input_sha256"]:
        raise ValueError("real-source result case input differs from the frozen profile")
    if replay.get("backend") != "none":
        raise ValueError("real-source replay must use lexical search without a configured backend")
    if replay.get("isolated_cache_home") is not True or replay.get("preserved_home_state") is not True:
        raise ValueError("real-source replay does not preserve user state with an isolated cache")
    if not same_executable(replay.get("executable", {}), provenance_value["executable"]):
        raise ValueError("real-source result executable differs from the frozen profile")
    if result.get("selection_metrics") != selection_metrics(result.get("rankings", []), result.get("rankings", [])):
        raise ValueError("real-source lexical selection metrics are inconsistent")
    diagnostics = result.get("diagnostics")
    expected = profile["diagnostics"]["cases"]
    if not isinstance(diagnostics, list) or len(diagnostics) != len(expected):
        raise ValueError("real-source result diagnostic coverage mismatch")
    identities = {record["fixture_id"]: record for record in profile["corpus"]["records"]}
    for expected_row, actual in zip(expected, diagnostics):
        if not isinstance(actual, dict) or any(actual.get(key) != expected_row[key] for key in ("id", "query", "reason")):
            raise ValueError("real-source result diagnostics differ from the frozen profile")
        ranked = actual.get("ranked")
        if not isinstance(ranked, list) or len(ranked) > result.get("candidate_pool_size", 0):
            raise ValueError("real-source result has invalid diagnostic candidates")
        if any(not isinstance(identity, str) for identity in ranked):
            raise ValueError("real-source diagnostic contains a non-string candidate identity")
        if len(set(ranked)) != len(ranked) or not set(ranked) <= set(identities):
            raise ValueError("real-source diagnostic contains unknown or duplicate candidate identities")


def validate_real_semantic_result(
    profile: dict, profile_hash: str, result: dict, source_path: Path, source: dict
) -> None:
    """Require an offline reranker to preserve the exact actual-source lexical candidate set."""
    validate_real_result(profile, profile_hash, source)
    if result.get("kind") != "skillwick-reranker-experiment":
        raise ValueError("real-source semantic result has an invalid candidate kind")
    if result.get("candidate_source_kind") != source.get("kind"):
        raise ValueError("real-source semantic candidate kind differs from its lexical receipt")
    source_hash = result.get("candidate_source_sha256")
    if not isinstance(source_hash, str) or file_sha256(source_path) != source_hash:
        raise ValueError("real-source semantic candidate source hash does not match lexical receipt bytes")
    lexical_pool = source.get("candidate_pool_size")
    if (isinstance(lexical_pool, bool) or not isinstance(lexical_pool, int)
            or not 1 <= lexical_pool <= 20 or result.get("source_candidate_pool_size") != lexical_pool):
        raise ValueError("real-source semantic result must use the same bounded lexical candidate pool")
    if result.get("candidate_pool_size") != lexical_pool:
        raise ValueError("real-source semantic rerank pool differs from its lexical source pool")
    lexical_rankings, semantic_rankings = source.get("rankings"), result.get("rankings")
    if not isinstance(lexical_rankings, list) or not isinstance(semantic_rankings, list):
        raise ValueError("real-source semantic result is missing candidate rankings")
    if len(lexical_rankings) != len(semantic_rankings):
        raise ValueError("real-source semantic query coverage differs from lexical candidates")
    for lexical, semantic in zip(lexical_rankings, semantic_rankings):
        if (semantic.get("id") != lexical.get("id")
                or semantic.get("query") != lexical.get("query")
                or semantic.get("relevant") != lexical.get("relevant")):
            raise ValueError("real-source semantic query or labels differ from lexical candidates")
        lexical_candidates, semantic_candidates = lexical.get("ranked"), semantic.get("ranked")
        if (not isinstance(lexical_candidates, list) or not isinstance(semantic_candidates, list)
                or set(semantic_candidates) != set(lexical_candidates)):
            raise ValueError("real-source semantic result added or dropped lexical candidate identities")
    if result.get("selection_metrics") != selection_metrics(lexical_rankings, semantic_rankings):
        raise ValueError("real-source semantic selection metrics are inconsistent")


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


def candidates_to_fixture_ids(rows: list[dict], records_by_id: dict[str, dict], limit: int) -> list[str]:
    if len(rows) > limit:
        raise ValueError("installed CLI returned more candidates than the requested limit")
    ranked = []
    for row in rows:
        if not isinstance(row, dict) or not isinstance(row.get("id"), str):
            raise ValueError("installed CLI returned a candidate without a stable id")
        record = records_by_id.get(row["id"])
        if record is None:
            raise ValueError(f"installed CLI returned a candidate outside the frozen corpus: {row['id']}")
        if any(row.get(field) != expected for field, expected in (
            ("name", record["name"]),
            ("hash", record["content_sha256"]),
            ("scope", record["scope"]),
            ("source_kind", record["source_kind"]),
            ("enabled", record["enabled"]),
            ("degraded", record["degraded"]),
        )):
            raise ValueError(f"installed CLI candidate changed since freeze: {row['id']}")
        ranked.append(row["id"])
    if len(set(ranked)) != len(ranked):
        raise ValueError("installed CLI returned duplicate candidate identities")
    return ranked


def replay_real(args: argparse.Namespace) -> None:
    profile_bytes = args.profile.read_bytes()
    try:
        profile = json.loads(profile_bytes)
    except json.JSONDecodeError as error:
        raise ValueError(f"invalid real-source profile: {error}") from None
    validate_real_profile(profile)
    profile_hash = hashlib.sha256(profile_bytes).hexdigest()
    binary, config, cwd = args.binary.resolve(strict=True), args.config.resolve(strict=True), args.cwd.resolve(strict=True)
    if not cwd.is_dir():
        raise ValueError(f"working directory is not a directory: {cwd}")
    if args.output.exists():
        raise ValueError(f"refusing to overwrite existing artifact: {args.output}")
    config_policy = real_config_policy(config)
    if config_policy != profile["provenance"].get("config_policy"):
        raise ValueError("real-source config policy differs from the frozen profile")
    cache_home = new_cache_home(args.cache_dir)
    env = real_environment(cache_home)
    config_before = file_sha256(config)
    executable_before = executable_identity(binary, env, cwd)
    inventory_before, _, inventory_before_ms = capture_real_inventory(binary, config, cwd, env)
    inventory_before_sha = inventory_sha256(inventory_before)
    provenance_value = profile["provenance"]
    if config_before != provenance_value["config_sha256"]:
        raise ValueError("real-source config differs from the frozen profile")
    if not same_executable(executable_before, provenance_value["executable"]):
        raise ValueError("installed CLI differs from the frozen profile")
    if inventory_before_sha != provenance_value["inventory_canonical_sha256"]:
        raise ValueError("current eligible inventory differs from the frozen profile")

    records_by_id = {record["fixture_id"]: record for record in profile["corpus"]["records"]}
    rankings, search_ms = [], []
    for query_id, query, relevant in query_rows(profile):
        command = [*cli_prefix(binary, config, cwd), "--json", "search", query, "--limit", str(args.pool_size)]
        completed, elapsed = run_real_command(command, env, cwd)
        search_ms.append(elapsed)
        try:
            rows = result_rows(json.loads(completed.stdout))
        except json.JSONDecodeError as error:
            raise ValueError(f"installed CLI returned invalid search JSON for {query_id}: {error}") from None
        ranked = candidates_to_fixture_ids(rows, records_by_id, args.pool_size)
        rankings.append({
            "id": query_id,
            "query": query,
            "relevant": sorted(relevant),
            "ranked": ranked,
        })

    diagnostics = []
    for diagnostic in profile["diagnostics"]["cases"]:
        command = [
            *cli_prefix(binary, config, cwd), "--json", "search", diagnostic["query"],
            "--limit", str(args.pool_size),
        ]
        completed, elapsed = run_real_command(command, env, cwd)
        search_ms.append(elapsed)
        try:
            rows = result_rows(json.loads(completed.stdout))
        except json.JSONDecodeError as error:
            raise ValueError(f"installed CLI returned invalid diagnostic JSON for {diagnostic['id']}: {error}") from None
        diagnostics.append({
            **diagnostic,
            "ranked": candidates_to_fixture_ids(rows, records_by_id, args.pool_size),
            "elapsed_ms": elapsed,
        })

    inventory_after, _, inventory_after_ms = capture_real_inventory(binary, config, cwd, env)
    inventory_after_sha = inventory_sha256(inventory_after)
    executable_after = executable_identity(binary, env, cwd)
    config_after = file_sha256(config)
    if inventory_after_sha != provenance_value["inventory_canonical_sha256"]:
        raise ValueError("eligible inventory changed during real-source replay")
    if config_after != config_before:
        raise ValueError("real-source config changed during replay")
    if not same_executable(executable_after, executable_before):
        raise ValueError("installed CLI changed during real-source replay")

    result = {
        "version": profile["version"],
        "kind": "skillwick-lexical-baseline",
        "measured_at": datetime.now(timezone.utc).isoformat(),
        "profile_sha256": profile_hash,
        "corpus_sha256": profile["corpus"]["sha256"],
        "corpus_total": profile["corpus"]["total"],
        "conditions": {
            "source": "actual installed eligible sources; no synthetic materialization or init",
            "backend": "none; lexical CLI search only",
            "cache": "fresh derived XDG_CACHE_HOME; actual HOME and agent/config/state roots preserved",
            "measurements": "single replay sample; timing is an observation, not a performance claim",
        },
        "executable": executable_before,
        "measurements": {
            "inventory_before_ms": inventory_before_ms,
            "inventory_after_ms": inventory_after_ms,
            "search_ms": summary(search_ms),
        },
        "case_count": profile["heldout"]["case_count"],
        "candidate_pool_size": args.pool_size,
        "provenance": provenance(),
        "retrieval_metrics": retrieval_metrics(profile["version"], rankings),
        "selection_metrics": selection_metrics(rankings, rankings),
        "quality": (task_metrics if profile["version"] == 2 else ranking_metrics)(
            [(item["ranked"], set(item["relevant"])) for item in rankings]
        ),
        "rankings": rankings,
        "diagnostics": diagnostics,
        "actual_source_replay": {
            "source_mode": "actual-installed-source",
            "inventory_before_sha256": inventory_before_sha,
            "inventory_after_sha256": inventory_after_sha,
            "config_sha256": config_before,
            "case_input_sha256": provenance_value["case_input_sha256"],
            "executable": executable_before,
            "backend": "none",
            "isolated_cache_home": True,
            "preserved_home_state": True,
        },
    }
    validate_real_result(profile, profile_hash, result)
    write_once(args.output, json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"quality": result["quality"], "diagnostics": len(diagnostics)}, sort_keys=True))


def main() -> None:
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(required=True)
    freeze_parser = commands.add_parser("freeze")
    freeze_parser.add_argument("--inventory", type=Path, required=True)
    freeze_parser.add_argument("--queries", type=Path, required=True)
    freeze_parser.add_argument("--output", type=Path, required=True)
    freeze_parser.set_defaults(handler=freeze)
    real_freeze_parser = commands.add_parser("freeze-real")
    real_freeze_parser.add_argument("--binary", type=Path, required=True)
    real_freeze_parser.add_argument("--config", type=Path, required=True)
    real_freeze_parser.add_argument("--cwd", type=Path, required=True)
    real_freeze_parser.add_argument("--cache-dir", type=Path, required=True)
    real_freeze_parser.add_argument("--cases", type=Path, required=True)
    real_freeze_parser.add_argument("--inventory-output", type=Path, required=True)
    real_freeze_parser.add_argument("--output", type=Path, required=True)
    real_freeze_parser.set_defaults(handler=freeze_real)
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
    replay_parser = commands.add_parser("replay")
    replay_parser.add_argument("--binary", type=Path, required=True)
    replay_parser.add_argument("--config", type=Path, required=True)
    replay_parser.add_argument("--cwd", type=Path, required=True)
    replay_parser.add_argument("--cache-dir", type=Path, required=True)
    replay_parser.add_argument("--profile", type=Path, required=True)
    replay_parser.add_argument("--output", type=Path, required=True)
    replay_parser.add_argument("--pool-size", type=int, choices=range(1, 21), default=20)
    replay_parser.set_defaults(handler=replay_real)
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
