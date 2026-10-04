#!/usr/bin/env python3
"""Run the frozen V1 profile through Skillwick's setup-selected rerankers.

This is an explicit live verifier. It prepares pinned local and hosted backends,
then issues ordinary CLI searches without per-search reranker options. Results
contain public profile metadata and fixed failure categories only.
"""

from __future__ import annotations

import argparse
from collections import Counter
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import re
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
import tomllib

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from benchmark_metrics import provenance, quality, selection_metrics  # noqa: E402
from benchmark_profiles import profile_rows, validate_profile  # noqa: E402


def load_lexical_module():
    path = ROOT / "scripts" / "benchmark-lexical.py"
    spec = importlib.util.spec_from_file_location("skillwick_benchmark_lexical", path)
    if spec is None or spec.loader is None:
        raise ValueError("benchmark helper unavailable")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def load_runtime_module():
    path = ROOT / "assets" / "skillwick" / "reranker_runtime.py"
    spec = importlib.util.spec_from_file_location("skillwick_live_runtime", path)
    if spec is None or spec.loader is None:
        raise ValueError("runtime metadata unavailable")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


LEXICAL = load_lexical_module()
RUNTIME = load_runtime_module()


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True, help="Built Skillwick executable")
    parser.add_argument("--output", type=Path, required=True, help="New JSON evidence path")
    parser.add_argument("--workdir", type=Path, required=True, help="New private directory under the system temp root")
    parser.add_argument("--cache-seed", type=Path, help="Existing Hugging Face cache used only for the pinned TinyBERT files")
    return parser.parse_args()


def private_workdir(requested: Path) -> Path:
    temp_roots = {Path(tempfile.gettempdir()).resolve(), Path("/private/tmp").resolve()}
    requested = requested.expanduser()
    parent = requested.parent.resolve(strict=True)
    destination = parent / requested.name
    if not any(parent.is_relative_to(root) for root in temp_roots) or destination.is_symlink():
        raise ValueError("workdir must be a new, non-symlink path under the system temp root")
    if destination.exists():
        raise ValueError("workdir must not already exist")
    destination.mkdir(mode=0o700)
    os.chmod(destination, 0o700)
    return destination


def isolated_env(workdir: Path, *, with_api_key: bool) -> dict[str, str]:
    env = os.environ.copy()
    if not with_api_key:
        env.pop("TYPESAFE_API_KEY", None)
    env.pop("HF_TOKEN", None)
    env.pop("HUGGING_FACE_HUB_TOKEN", None)
    env.update(
        HOME=str(workdir / "home"),
        CODEX_HOME=str(workdir / "codex"),
        CLAUDE_CONFIG_DIR=str(workdir / "claude"),
        XDG_CONFIG_HOME=str(workdir / "config-home"),
        XDG_CACHE_HOME=str(workdir / "cache-home"),
        XDG_STATE_HOME=str(workdir / "state"),
        HF_HOME=str(workdir / "hf-home"),
        HF_HUB_DISABLE_TELEMETRY="1",
        TOKENIZERS_PARALLELISM="false",
        TYPESAFE_LOG_LEVEL="off",
        UV_CACHE_DIR=str(workdir / "uv-cache"),
        UV_PYTHON_DOWNLOADS="never",
        TMPDIR=str(workdir / "tmp"),
        RUSTUP_NO_UPDATE_CHECK="1",
        CARGO_TARGET_DIR=str(workdir / "cargo-target"),
    )
    # Rustup and Cargo binaries/toolchains remain usable while all generated
    # state and XDG data stay in this run's private directory.
    env.setdefault("CARGO_HOME", str(Path.home() / ".cargo"))
    env.setdefault("RUSTUP_HOME", str(Path.home() / ".rustup"))
    for name in (
        "home",
        "codex",
        "claude",
        "config-home",
        "cache-home",
        "state",
        "hf-home",
        "uv-cache",
        "tmp",
        "cargo-target",
    ):
        (workdir / name).mkdir(mode=0o700, exist_ok=True)
    return env


def package_pins(backend: str) -> list[str]:
    rust = (ROOT / "src" / "reranker.rs").read_text(encoding="utf-8")
    name = "JEV_PACKAGES" if backend == "jev" else "BERT_PACKAGES"
    match = re.search(rf"const {name}:\s*&\[&str\]\s*=\s*&\[(.*?)\];", rust, flags=re.DOTALL)
    if match is None:
        raise ValueError("reranker package pins unavailable")
    pins = re.findall(r'"([^"\n]+)"', match.group(1))
    if not pins:
        raise ValueError("reranker package pins invalid")
    return pins


def planned_runtime(workdir: Path, backend: str) -> Path:
    program = ROOT / "assets" / "skillwick" / "reranker_runtime.py"
    manifest = {
        "backend": backend,
        "program_sha256": sha256(program),
        "packages": package_pins(backend),
    }
    encoded = json.dumps(manifest, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    manifest_hash = hashlib.sha256(encoded).hexdigest()
    return workdir / "state" / "skillwick" / "rerankers" / f"{backend}-{manifest_hash[:16]}"


def seed_pinned_model(source: Path, target_cache: Path) -> dict:
    source = source.expanduser().resolve(strict=True)
    model = RUNTIME.TINYBERT
    repository = "models--" + model["name"].replace("/", "--")
    roots = [source]
    if (source / "hub").is_dir():
        roots.append(source / "hub")
    nested_hub = source / ".cache" / "huggingface" / "hub"
    if nested_hub.is_dir():
        roots.append(nested_hub)
    if source.name == repository:
        roots = [source.parent]

    selected: dict[str, Path] | None = None
    for cache_root in roots:
        snapshot = cache_root / repository / "snapshots" / model["revision"]
        files = {name: snapshot / name for name in model["required_files"]}
        if all(path.is_file() for path in files.values()):
            selected = files
            break
    if selected is None:
        raise ValueError("cache seed does not contain the pinned TinyBERT snapshot")

    repository_root = target_cache / repository / "snapshots" / model["revision"]
    verified = []
    for name, expected in model["required_files"].items():
        source_file = selected[name].resolve(strict=True)
        if not source_file.is_file() or source_file.stat().st_size != expected["bytes"]:
            raise ValueError("cache seed contains an invalid pinned TinyBERT artifact")
        if sha256(source_file) != expected["sha256"]:
            raise ValueError("cache seed contains an invalid pinned TinyBERT artifact")
        destination = repository_root / name
        destination.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        shutil.copyfile(source_file, destination)
        os.chmod(destination, 0o600)
        if destination.stat().st_size != expected["bytes"] or sha256(destination) != expected["sha256"]:
            raise ValueError("copied TinyBERT artifact failed its pinned checksum")
        verified.append({"file": name, "bytes": expected["bytes"], "sha256": expected["sha256"]})
    return {
        "status": "verified_and_copied",
        "model": RUNTIME.TINYBERT_MODEL_ID,
        "files": verified,
        "source_path_recorded": False,
        "runtime_rechecks_hash_and_size": True,
    }


def run_process(command: list[str], env: dict[str, str], *, cwd: Path, timeout: int):
    started = time.perf_counter_ns()
    try:
        completed = subprocess.run(
            command,
            env=env,
            cwd=cwd,
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        return completed, (time.perf_counter_ns() - started) / 1_000_000
    except subprocess.TimeoutExpired:
        return None, (time.perf_counter_ns() - started) / 1_000_000
    except OSError:
        return None, (time.perf_counter_ns() - started) / 1_000_000


def failure_category(stderr: str, *, setup: bool = False) -> str:
    lowered = stderr.lower()
    categories = (
        "missing_api_key",
        "unsafe_credential_permissions",
        "runtime_unavailable",
        "runtime_timeout",
        "dependency_unavailable",
        "unsupported_sdk_version",
        "backend_failure",
        "authentication",
        "rate_limit",
        "server_error",
        "timeout",
        "model_mismatch",
        "malformed_response",
        "artifact_mismatch",
        "transport_error",
        "model_artifact_missing",
        "model_artifact_checksum",
    )
    for category in categories:
        if category in lowered:
            return category
    if "reranker python preparation failed" in lowered or "dependency installation failed" in lowered:
        return "dependency_preparation_failed"
    if "reranker preparation failed" in lowered:
        return "runtime_preparation_failed"
    return "setup_failed" if setup else "command_failed"


def setup_backend(binary: Path, env: dict[str, str], config: Path, workspace: Path,
                  skills: Path, backend: str) -> dict:
    command = [
        str(binary),
        "--config",
        str(config),
        "--cwd",
        str(workspace),
        "init",
        "--yes",
        "--agent",
        "none",
        "--discovery",
        "explicit",
        "--root",
        str(skills),
        "--reranker",
        backend,
    ]
    completed, elapsed = run_process(command, env, cwd=ROOT, timeout=600)
    evidence = {
        "backend": backend,
        "returncode": completed.returncode if completed is not None else None,
        "elapsed_ms": round(elapsed, 3),
        "status": "passed" if completed is not None and completed.returncode == 0 else "failed",
    }
    if completed is None:
        evidence["failure_category"] = "command_timeout_or_spawn_failure"
        return evidence
    if completed.returncode:
        evidence["failure_category"] = failure_category(completed.stderr, setup=True)
        return evidence

    selected_backend, selected_runtime = config_backend(config)
    if backend == "none":
        evidence["configuration_selected"] = selected_backend == "none" and selected_runtime is None
        if not evidence["configuration_selected"]:
            evidence["status"] = "failed"
            evidence["failure_category"] = "configuration_identity_invalid"
        return evidence

    state_root = Path(env["XDG_STATE_HOME"]).resolve() / "skillwick"
    reranker_root = state_root / "rerankers"
    if selected_runtime is None:
        evidence["status"] = "failed"
        evidence["failure_category"] = "configuration_identity_invalid"
        return evidence
    full_runtime = Path(selected_runtime).resolve()
    if not full_runtime.is_relative_to(reranker_root.resolve()):
        evidence["status"] = "failed"
        evidence["failure_category"] = "runtime_outside_private_state"
        return evidence
    runtime_dir = full_runtime.relative_to(state_root)
    manifest_path = full_runtime / "ready.json"
    program_path = ROOT / "assets" / "skillwick" / "reranker_runtime.py"
    expected = {
        "backend": backend,
        "program_sha256": sha256(program_path),
        "packages": package_pins(backend),
    }
    try:
        ready = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        ready = None
    manifest_regular = manifest_path.is_file() and not manifest_path.is_symlink()
    python = full_runtime / "venv" / "bin" / "python"
    evidence.update(
        runtime_directory=runtime_dir.as_posix(),
        configuration_selected=selected_backend == backend and selected_runtime is not None
        and Path(selected_runtime).resolve() == full_runtime.resolve(),
        readiness_manifest_valid=ready == expected and manifest_regular,
        readiness_manifest_sha256=sha256(manifest_path) if manifest_regular else None,
        python_executable_present=python.is_file(),
        packages=expected["packages"],
        setup_smoke_check="passed" if ready == expected and manifest_regular else "unverified",
        model=(RUNTIME.JEV_MODEL if backend == "jev" else RUNTIME.TINYBERT_MODEL_ID),
        model_identity_source="embedded pinned runtime metadata; successful CLI response validates the same model",
    )
    if backend == "tinybert":
        evidence["model_artifacts"] = [
            {"file": name, "bytes": expected_file["bytes"], "sha256": expected_file["sha256"]}
            for name, expected_file in RUNTIME.TINYBERT["required_files"].items()
        ]
    if (ready != expected or not manifest_regular or not python.is_file()
            or not evidence["configuration_selected"]):
        evidence["status"] = "failed"
        evidence["failure_category"] = "readiness_evidence_invalid"
    return evidence


def diagnostic(stderr: str) -> str | None:
    allowed = {
        "artifact_mismatch",
        "authentication",
        "backend_failure",
        "candidate_mapping_invalid",
        "malformed_response",
        "missing_api_key",
        "model_mismatch",
        "rate_limit",
        "runtime_input_invalid",
        "runtime_input_too_large",
        "runtime_output_invalid",
        "runtime_timeout",
        "runtime_unavailable",
        "secret_in_payload",
        "server_error",
        "timeout",
        "transport_error",
        "unsafe_credential_permissions",
    }
    categories = []
    unexpected = False
    for line in stderr.splitlines():
        if line.startswith("reranking: "):
            category = line.removeprefix("reranking: ").strip()
            if category in allowed:
                categories.append(category)
            else:
                unexpected = True
        elif line.strip():
            unexpected = True
    if unexpected:
        return "unexpected_diagnostic_output"
    if not categories:
        return None
    return categories[0]


def execute_searches(binary: Path, env: dict[str, str], config: Path, workspace: Path,
                     rows: list[dict], fixture_names: dict[str, str], backend: str) -> dict:
    rankings = []
    latencies = []
    diagnostics: Counter[str] = Counter()
    failures: Counter[str] = Counter()
    reranker_invocations = 0
    successful_model_queries = 0
    for row in rows:
        command = [
            str(binary),
            "--config",
            str(config),
            "--cwd",
            str(workspace),
            "--json",
            "search",
            row["query"],
            "--limit",
            "5",
        ]
        completed, elapsed = run_process(command, env, cwd=ROOT, timeout=120)
        latencies.append(elapsed)
        ranked: list[str] = []
        candidate_count = 0
        issue: str | None = None
        diagnostic_category: str | None = None
        if completed is None:
            issue = "command_timeout_or_spawn_failure"
        else:
            diagnostic_category = diagnostic(completed.stderr)
            if diagnostic_category is not None:
                diagnostics[diagnostic_category] += 1
            if completed.returncode:
                issue = failure_category(completed.stderr)
            else:
                try:
                    response = json.loads(completed.stdout)
                    candidates = response.get("results")
                    if response.get("version") != 3 or not isinstance(candidates, list):
                        raise ValueError
                    candidate_count = len(candidates)
                    for candidate in candidates:
                        directory = Path(candidate["canonical"]).parent.name
                        identity = fixture_names[directory]
                        if identity in ranked:
                            raise ValueError
                        ranked.append(identity)
                    identities = set(fixture_names.values())
                    if len(ranked) > 5 or not set(ranked) <= identities:
                        raise ValueError
                except (KeyError, TypeError, ValueError, json.JSONDecodeError):
                    issue = "invalid_cli_result"
                    ranked = []
        if candidate_count and backend != "none":
            reranker_invocations += 1
            if issue is None and diagnostic_category is None:
                successful_model_queries += 1
        if issue is not None:
            failures[issue] += 1
        rankings.append(
            {
                "id": row["id"],
                "query": row["query"],
                "relevant": row["relevant"],
                "ranked": ranked,
                "candidate_count": candidate_count,
                "latency_ms": round(elapsed, 3),
                "diagnostic": diagnostic_category,
                "failure_category": issue,
            }
        )

    result_status = "passed"
    if failures:
        result_status = "completed_with_failures"
    elif diagnostics:
        result_status = "completed_with_fallbacks"
    result = {
        "status": result_status,
        "searches_attempted": len(rows),
        "searches_succeeded": len(rows) - sum(failures.values()),
        "ranking_count": len(rankings),
        "candidate_limit": 5,
        "search_mode": "ordinary_cli_json_search",
        "per_search_reranker_option": False,
        "api_key_environment_cleared": "TYPESAFE_API_KEY" not in env,
        "reranker_invocations": reranker_invocations,
        "successful_model_queries": successful_model_queries,
        "reranker_fallback_queries": sum(diagnostics.values()),
        "inferred_provider_query_attempts": reranker_invocations if backend == "jev" else 0,
        "inferred_provider_query_successes": successful_model_queries if backend == "jev" else 0,
        "diagnostic_counts": dict(sorted(diagnostics.items())),
        "failure_counts": dict(sorted(failures.items())),
        "latency_ms": latency_summary(latencies),
        "rankings": rankings,
        "quality": quality(1, rankings),
    }
    return result


def latency_summary(values: list[float]) -> dict:
    if not values:
        return {"samples": 0, "min": None, "median": None, "p95": None, "max": None}
    ordered = sorted(values)
    return {
        "samples": len(values),
        "min": round(ordered[0], 3),
        "median": round(statistics.median(ordered), 3),
        "p95": round(ordered[max(0, (95 * len(ordered) + 99) // 100 - 1)], 3),
        "max": round(ordered[-1], 3),
    }


def library_test(env: dict[str, str], config: Path, workdir: Path, backend: str,
                 fixture_identities: set[str]) -> dict:
    receipt_path = workdir / f"library-receipt-{backend}.json"
    command = [
        "cargo",
        "test",
        "--test",
        "reranker_library",
        "prepared_backend_library_search",
        "--",
        "--ignored",
    ]
    test_env = env.copy()
    test_env.pop("TYPESAFE_API_KEY", None)
    test_env["SKILLWICK_LIVE_CONFIG"] = str(config)
    test_env["SKILLWICK_LIVE_RECEIPT"] = str(receipt_path)
    completed, elapsed = run_process(command, test_env, cwd=ROOT, timeout=900)
    evidence = {
        "command": "cargo test --test reranker_library prepared_backend_library_search -- --ignored",
        "returncode": completed.returncode if completed is not None else None,
        "elapsed_ms": round(elapsed, 3),
        "status": "passed" if completed is not None and completed.returncode == 0 else "failed",
        "api_key_environment_cleared": True,
    }
    if completed is None:
        evidence["failure_category"] = "command_timeout_or_spawn_failure"
        return evidence
    if completed.returncode:
        evidence["failure_category"] = failure_category(completed.stderr)
        return evidence
    try:
        receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
        names = receipt.get("names")
        valid = (
            receipt.get("status") == "passed"
            and str(receipt.get("backend", "")).lower() == backend
            and isinstance(names, list)
            and bool(names)
            and all(isinstance(name, str) and name in fixture_identities for name in names)
            and receipt.get("diagnostic") is None
        )
        if valid:
            evidence["receipt"] = {
                "status": "passed",
                "backend": backend,
                "ranked_fixture_ids": names,
                "diagnostic_category": None,
            }
        else:
            evidence["status"] = "failed"
            evidence["failure_category"] = "invalid_library_receipt"
    except (OSError, json.JSONDecodeError, AttributeError, TypeError):
        evidence["status"] = "failed"
        evidence["failure_category"] = "missing_or_invalid_library_receipt"
    finally:
        try:
            receipt_path.unlink()
        except OSError:
            pass
    return evidence


def config_backend(config: Path) -> tuple[str | None, str | None]:
    try:
        value = tomllib.loads(config.read_text(encoding="utf-8"))
        reranker = value.get("reranker") or {}
        return reranker.get("backend"), reranker.get("runtime")
    except (OSError, tomllib.TOMLDecodeError, AttributeError):
        return None, None


def atomic_result(output: Path, result: dict) -> None:
    encoded = (json.dumps(result, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
    temporary = output.with_name(output.name + ".partial")
    descriptor = os.open(temporary, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    with os.fdopen(descriptor, "wb") as stream:
        stream.write(encoded)
        stream.flush()
        os.fsync(stream.fileno())
    os.replace(temporary, output)
    os.chmod(output, 0o600)


def main() -> int:
    args = parse_args()
    binary = args.binary.expanduser().resolve(strict=True)
    output = args.output.expanduser().absolute()
    partial_output = output.with_name(output.name + ".partial")
    if (output.exists() or output.is_symlink() or partial_output.exists()
            or partial_output.is_symlink() or not output.parent.is_dir()):
        raise SystemExit("output must be a new path in an existing directory")
    if not os.environ.get("TYPESAFE_API_KEY", "").strip():
        raise SystemExit("TYPESAFE_API_KEY is required for the hosted JEV setup")

    workdir = private_workdir(args.workdir)
    env = isolated_env(workdir, with_api_key=True)
    profile_path = ROOT / "benchmarks" / "profile-v1.json"
    profile = json.loads(profile_path.read_text(encoding="utf-8"))
    validate_profile(profile)
    rows = profile_rows(profile)
    if profile["version"] != 1 or len(rows) != 105:
        raise SystemExit("frozen V1 profile identity or query count changed")

    skills = workdir / "skills"
    workspace = workdir / "workspace"
    skills.mkdir(mode=0o700)
    workspace.mkdir(mode=0o700)
    LEXICAL.materialize(profile["corpus"]["records"], skills)
    fixture_names = {
        f"skill-{index:04d}": record.get("fixture_id", record["name"])
        for index, record in enumerate(profile["corpus"]["records"])
    }
    fixture_identities = set(fixture_names.values())
    if len(fixture_identities) != len(profile["corpus"]["records"]):
        raise SystemExit("frozen V1 fixture identities are not unique")

    cache_seed_evidence = None
    if args.cache_seed:
        try:
            target_cache = planned_runtime(workdir, "tinybert") / "models"
            cache_seed_evidence = seed_pinned_model(args.cache_seed, target_cache)
        except (OSError, ValueError):
            raise SystemExit("cache seed does not verify against the public pinned TinyBERT runtime") from None

    config = workdir / "config-home" / "skillwick" / "config.toml"
    base_result = {
        "kind": "skillwick-live-reranker-verification",
        "status": "running",
        "measured_at": datetime.now(timezone.utc).isoformat(),
        "profile": {
            "version": profile["version"],
            "profile_sha256": sha256(profile_path),
            "corpus_sha256": profile["corpus"]["sha256"],
            "corpus_count": profile["corpus"]["total"],
            "heldout_case_count": profile["heldout"]["case_count"],
            "query_count": len(rows),
            "candidate_limit": 5,
            "metadata_identity_check": "passed",
        },
        "executable": {
            "filename": binary.name,
            "version": None,
            "sha256": sha256(binary),
            "bytes": binary.stat().st_size,
        },
        "machine": {
            "system": platform.system(),
            "release": platform.release(),
            "machine": platform.machine(),
            "python": platform.python_version(),
        },
        "source": {
            "provenance": provenance(),
            "runtime_sha256": sha256(ROOT / "assets" / "skillwick" / "reranker_runtime.py"),
            "verifier_sha256": sha256(Path(__file__).resolve()),
        },
        "privacy": {
            "workspace_and_xdg_roots_isolated": True,
            "hosted_key_written_only_to_private_state": False,
            "key_environment_cleared_for_all_searches_and_library_tests": True,
            "raw_setup_or_search_stderr_recorded": False,
            "absolute_fixture_paths_recorded": False,
        },
        "cache_seed": cache_seed_evidence,
        "backends": {},
    }

    version_process, _version_elapsed = run_process(
        [str(binary), "--version"], env, cwd=ROOT, timeout=30
    )
    if version_process is None or version_process.returncode != 0:
        raise SystemExit("Skillwick binary version check failed")
    base_result["executable"]["version"] = version_process.stdout.strip()

    # First establish the lexical CLI baseline, then select each backend in
    # order using the same isolated configuration and fixture inventory.
    stages = (("none", False), ("tinybert", False), ("jev", True))
    for backend, needs_key in stages:
        setup_env = env.copy()
        if not needs_key:
            setup_env.pop("TYPESAFE_API_KEY", None)
        setup = setup_backend(binary, setup_env, config, workspace, skills, backend)
        stage = {"setup": setup}
        base_result["backends"][backend] = stage
        if setup["status"] != "passed":
            base_result["status"] = "setup_failed"
            base_result["failed_stage"] = backend
            atomic_result(output, base_result)
            print(json.dumps({"status": base_result["status"], "stage": backend,
                              "failure_category": setup.get("failure_category", "setup_failed")}))
            return 1

        query_env = env.copy()
        query_env.pop("TYPESAFE_API_KEY", None)
        backend_result = execute_searches(binary, query_env, config, workspace, rows,
                                          fixture_names, backend)
        stage["cli"] = backend_result
        if backend != "none":
            stage["selection_vs_lexical"] = selection_metrics(
                base_result["backends"]["none"]["cli"]["rankings"],
                backend_result["rankings"],
            )
            stage["library_test"] = library_test(
                query_env, config, workdir, backend, fixture_identities
            )
        atomic_result(output, base_result)

    final_backend, final_runtime = config_backend(config)
    private_reranker_root = (Path(env["XDG_STATE_HOME"]) / "skillwick" / "rerankers").resolve()
    runtime_path = Path(final_runtime).resolve() if final_runtime else None
    runtime_is_private = bool(runtime_path) and runtime_path.is_relative_to(private_reranker_root)
    runtime_directory_private = runtime_is_private and runtime_path.is_dir()
    if runtime_directory_private and os.name == "posix":
        runtime_directory_private = runtime_path.stat().st_mode & 0o077 == 0
    key_path = runtime_path / "api-key" if runtime_is_private else private_reranker_root / "invalid-api-key-path"
    key_private = runtime_directory_private and key_path.is_file() and not key_path.is_symlink()
    if key_private and os.name == "posix":
        key_private = key_path.stat().st_mode & 0o077 == 0
    supplied_key = os.environ.get("TYPESAFE_API_KEY", "")
    try:
        persisted_config = config.read_text(encoding="utf-8")
    except OSError:
        persisted_config = ""
    if supplied_key and supplied_key in persisted_config:
        key_private = False
    base_result["privacy"]["hosted_key_written_only_to_private_state"] = key_private
    base_result["final_isolated_config"] = {
        "backend": final_backend,
        "runtime_matches_private_jev_state": final_backend == "jev" and runtime_directory_private,
        "key_file_private": key_private,
    }
    all_completed = all(
        stage.get("cli", {}).get("ranking_count") == 105
        and stage.get("cli", {}).get("searches_succeeded") == 105
        and stage.get("cli", {}).get("status") == "passed"
        for stage in base_result["backends"].values()
    )
    library_ok = all(
        base_result["backends"][backend].get("library_test", {}).get("status") == "passed"
        for backend in ("tinybert", "jev")
    )
    base_result["status"] = (
        "passed"
        if all_completed and library_ok and final_backend == "jev"
        and base_result["final_isolated_config"]["runtime_matches_private_jev_state"]
        and key_private
        else "completed_with_failures"
    )
    if supplied_key and supplied_key in json.dumps(base_result, ensure_ascii=False):
        base_result["status"] = "completed_with_failures"
        base_result["privacy"]["secret_output_check"] = "failed"
    else:
        base_result["privacy"]["secret_output_check"] = "passed"
    atomic_result(output, base_result)
    print(json.dumps({
        "status": base_result["status"],
        "queries_per_backend": len(rows),
        "tinybert_diagnostics": base_result["backends"]["tinybert"]["cli"]["diagnostic_counts"],
        "jev_diagnostics": base_result["backends"]["jev"]["cli"]["diagnostic_counts"],
        "output": str(output),
    }, sort_keys=True))
    return 0 if base_result["status"] == "passed" else 1


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception:
        # Exceptions may contain local paths or subprocess details; keep the
        # artifact boundary restricted to fixed categories.
        raise SystemExit("live reranker verification could not be completed") from None
