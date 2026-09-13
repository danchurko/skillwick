#!/usr/bin/env python3
"""Focused offline checks for the evaluation artifact helpers."""

from __future__ import annotations

import importlib.util
import json
import os
from pathlib import Path
import stat
import subprocess
import tempfile
import textwrap
import unittest


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("evaluate_skills", ROOT / "scripts/evaluate_skills.py")
assert SPEC and SPEC.loader
evaluate_skills = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(evaluate_skills)


class EvaluationArtifactTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory(prefix="skillwick-eval-")
        self.root = Path(self.temp.name)
        self.records = [
            {
                "id": "skill-a@source",
                "name": "skill-a",
                "description": "Build a widget",
                "hash": "hash-a",
                "source_kind": "fixture",
                "scope": "global",
                "path": "/source/skill-a/SKILL.md",
                "canonical": "/source/skill-a/SKILL.md",
                "base": "/source/skill-a",
                "plugin_id": None,
                "enabled": True,
            },
            {
                "id": "skill-b@source",
                "name": "skill-b",
                "description": "Deploy a service",
                "hash": "hash-b",
                "source_kind": "fixture",
                "scope": "global",
                "path": "/source/skill-b/SKILL.md",
                "canonical": "/source/skill-b/SKILL.md",
                "base": "/source/skill-b",
                "plugin_id": None,
                "enabled": True,
            },
            {
                "id": "plugin:skill-c@source",
                "name": "skill-c",
                "description": "Review a change",
                "hash": "hash-c",
                "source_kind": "plugin",
                "scope": "global",
                "path": "/source/plugin/skill-c/SKILL.md",
                "canonical": "/source/plugin/skill-c/SKILL.md",
                "base": "/source/plugin/skill-c",
                "plugin_id": "fixture@plugin",
                "enabled": True,
            },
            {
                "id": "skill-d@source",
                "name": "skill-d",
                "description": "Write documentation",
                "hash": "hash-d",
                "source_kind": "fixture",
                "scope": "global",
                "path": "/source/skill-d/SKILL.md",
                "canonical": "/source/skill-d/SKILL.md",
                "base": "/source/skill-d",
                "plugin_id": None,
                "enabled": True,
            },
        ]
        self.skillwick = self._script(
            "skillwick.py",
            """
            import json, os, sys
            records = json.loads(os.environ["FAKE_CORPUS"])
            if sys.argv[1:] != ["--json", "list", "--all"]:
                raise SystemExit("unexpected Skillwick command")
            print(json.dumps({"version": 1, "total": len(records), "results": records}))
            """,
        )
        self.env = {**os.environ, "FAKE_CORPUS": json.dumps(self.records)}

    def tearDown(self) -> None:
        self.temp.cleanup()

    def _script(self, name: str, source: str) -> Path:
        path = self.root / name
        path.write_text("#!/usr/bin/env python3\n" + textwrap.dedent(source).lstrip(), encoding="utf-8")
        path.chmod(path.stat().st_mode | stat.S_IXUSR)
        return path

    def _manifest(self) -> Path:
        manifest = evaluate_skills.manifest_from_records(
            self.records,
            command=[str(self.skillwick), "--json", "list", "--all"],
            stderr="",
            coverage=0.75,
            seed=7,
        )
        path = self.root / "corpus.json"
        evaluate_skills.write_json(path, manifest)
        return path

    def _dataset(self, manifest_path: Path) -> Path:
        manifest = evaluate_skills.load_manifest(manifest_path)
        sample_ids = manifest["sample"]["ids"]
        queries = [
            {"perspective": perspective, "query": f"{perspective} widget"}
            for perspective in evaluate_skills.PERSPECTIVES
        ]
        dataset = {
            "version": 2,
            "name": "fixture",
            "corpus_sha256": manifest["corpus"]["sha256"],
            "sample_sha256": manifest["sample"]["sha256"],
            "perspectives": list(evaluate_skills.PERSPECTIVES),
            "labeling": {
                "method": "independent_review",
                "search_results_used": False,
                "reviewed": True,
            },
            "population": {
                "coverage": 0.75,
                "corpus_total": 4,
                "total_cases": 3,
                "positive_cases": 2,
                "no_skill_cases": 1,
            },
            "skill_coverage": {"sampled_skill_count": 3, "corpus_total": 4},
            "cases": [
                {
                    "id": "case-positive",
                    "split": "dev",
                    "kind": "positive",
                    "task": "Build the widget",
                    "queries": queries,
                    "relevant": [sample_ids[0], sample_ids[2]],
                    "reviewed": True,
                },
                {
                    "id": "case-heldout",
                    "split": "heldout",
                    "kind": "positive",
                    "task": "Build another widget",
                    "queries": queries,
                    "relevant": [sample_ids[1]],
                    "reviewed": True,
                },
            ],
            "negative_cases": [
                {
                    "id": "case-negative",
                    "split": "heldout",
                    "kind": "negative",
                    "task": "Plan a picnic",
                    "queries": queries,
                    "relevant": [],
                    "reviewed": True,
                }
            ],
        }
        path = self.root / "dataset.json"
        evaluate_skills.write_json(path, dataset)
        return path

    def _evidence(self, manifest_path: Path, dataset_path: Path) -> Path:
        manifest = evaluate_skills.load_manifest(manifest_path)
        _, dataset_sha256, cases = evaluate_skills.validate_dataset(dataset_path, manifest)
        query_results = {
            workflow: {
                case["id"]: [
                    {
                        "perspective": query["perspective"],
                        "query": f"{workflow}: {query['query']}",
                        "results": [{"id": identifier} for identifier in case["relevant"]],
                        "wall_ms": 1.25,
                    }
                    for query in case["queries"]
                ]
                for case in cases
            }
            for workflow in evaluate_skills.WORKFLOWS
        }
        workflows = {
            "direct": {
                case["id"]: {
                    "selected_ids": case["relevant"],
                    "usage": {
                        "root": {"input_tokens": 2, "output_tokens": 3, "cached_input_tokens": 1},
                        "total": {"input_tokens": 2, "output_tokens": 3, "cached_input_tokens": 1},
                    },
                }
                for case in cases
            },
            "delegated": {
                case["id"]: {
                    "selected_ids": case["relevant"],
                    "usage": {
                        "root": {"input_tokens": 5, "output_tokens": 6, "cached_input_tokens": 2},
                        "total": {"input_tokens": 8, "output_tokens": 9, "cached_input_tokens": 3},
                    },
                }
                for case in cases
            },
            "native": {
                case["id"]: {
                    "selected_ids": case["relevant"],
                    "usage": {
                        "root": {"input_tokens": 2, "output_tokens": 3, "cached_input_tokens": 1},
                        "total": {"input_tokens": 2, "output_tokens": 3, "cached_input_tokens": 1},
                    },
                }
                for case in cases
            },
        }
        adjudication = {
            "direct": {
                case["id"]: {
                    "reviewed": True,
                    "relevant": ([manifest["sample"]["ids"][1]] if case["kind"] == "positive" else []),
                }
                for case in cases
            },
            "delegated": {
                case["id"]: {"reviewed": True, "relevant": case["relevant"]}
                for case in cases
            },
            "native": {
                case["id"]: {"reviewed": True, "relevant": case["relevant"]}
                for case in cases
            },
        }
        evidence = {
            "version": 1,
            "kind": "skillwick-evaluation-evidence",
            "corpus_sha256": manifest["corpus"]["sha256"],
            "sample_sha256": manifest["sample"]["sha256"],
            "dataset_sha256": dataset_sha256,
            "search_budget": {"max_queries_per_case": 3},
            "query_results": query_results,
            "workflows": workflows,
            "adjudication": adjudication,
        }
        path = self.root / "evidence.json"
        evaluate_skills.write_json(path, evidence)
        return path

    def _run(self, *args: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [os.environ.get("PYTHON", "python3"), str(ROOT / "scripts/evaluate_skills.py"), *args],
            cwd=ROOT,
            env=self.env,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )

    def test_portable_identity_and_mapping_ignore_relocation(self) -> None:
        moved = [
            {
                **record,
                "id": record["id"].replace("source", "staged"),
                "path": record["path"].replace("/source", "/staged"),
                "canonical": record["canonical"].replace("/source", "/staged"),
                "base": record["base"].replace("/source", "/staged"),
            }
            for record in self.records
        ]
        self.assertEqual(evaluate_skills.corpus_identity(self.records), evaluate_skills.corpus_identity(moved))
        mapping = evaluate_skills.identity_mapping(self.records)
        moved_mapping = evaluate_skills.identity_mapping(moved)
        self.assertEqual(
            set(mapping["source_to_portable"].values()), set(moved_mapping["source_to_portable"].values())
        )
        self.assertNotEqual(
            evaluate_skills.corpus_identity(self.records),
            evaluate_skills.corpus_identity([dict(self.records[0], description="changed"), *self.records[1:]]),
        )

    def test_stratified_sample_is_ceil_and_repeatable(self) -> None:
        first = evaluate_skills.stratified_sample(self.records, 0.3, 13)
        second = evaluate_skills.stratified_sample(self.records, 0.3, 13)
        self.assertEqual(first, second)
        self.assertEqual(len(first), 2)

    def test_export_and_dataset_validation(self) -> None:
        manifest = self.root / "exported.json"
        result = self._run(
            "export",
            "--skillwick",
            str(self.skillwick),
            "--output",
            str(manifest),
            "--coverage",
            "0.75",
            "--seed",
            "7",
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        loaded = evaluate_skills.load_manifest(manifest)
        self.assertEqual(loaded["corpus"]["total"], 4)
        self.assertEqual(loaded["sample"]["count"], 3)
        self.assertEqual(
            set(loaded["identity"]["source_to_portable"]), {record["id"] for record in self.records}
        )
        dataset = self._dataset(manifest)
        result = self._run("validate", "--corpus", str(manifest), "--dataset", str(dataset))
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_recorded_evidence_scores_frozen_and_adjudicated_separately(self) -> None:
        manifest = self._manifest()
        dataset = self._dataset(manifest)
        evidence = self._evidence(manifest, dataset)
        loaded_manifest = evaluate_skills.load_manifest(manifest)
        _, dataset_sha256, cases = evaluate_skills.validate_dataset(dataset, loaded_manifest)
        normalized = evaluate_skills.validate_evidence(evidence, loaded_manifest, dataset_sha256, cases)
        report = evaluate_skills.score_evidence(normalized, loaded_manifest, cases)
        self.assertEqual(report["status"], "complete")
        self.assertEqual(report["frozen"]["direct"]["selection"]["recall_at_5"], 1.0)
        self.assertEqual(report["adjudicated"]["direct"]["selection"]["recall_at_5"], 0.5)
        self.assertEqual(report["frozen"]["delegated"]["selection"]["recall_at_5"], 1.0)
        self.assertEqual(report["frozen"]["native"]["selection"]["recall_at_5"], 1.0)
        self.assertEqual(report["usage"]["direct"]["root"]["input_tokens"], 6)
        self.assertEqual(report["usage"]["delegated"]["total"]["input_tokens"], 24)
        self.assertEqual(report["usage"]["combined"]["root"]["input_tokens"], 27)
        self.assertEqual(report["usage"]["combined"]["total"]["input_tokens"], 36)
        self.assertEqual(report["usage"]["combined"]["total"]["cached_input_tokens"], 15)

    def test_adaptive_queries_may_use_a_shorter_equal_budget(self) -> None:
        manifest = self._manifest()
        dataset = self._dataset(manifest)
        evidence = self._evidence(manifest, dataset)
        loaded = json.loads(evidence.read_text(encoding="utf-8"))
        for case_id in loaded["query_results"]["direct"]:
            loaded["query_results"]["direct"][case_id] = loaded["query_results"]["direct"][case_id][:1]
            loaded["query_results"]["delegated"][case_id] = loaded["query_results"]["delegated"][case_id][:2]
        evaluate_skills.write_json(evidence, loaded)
        loaded_manifest = evaluate_skills.load_manifest(manifest)
        _, dataset_sha256, cases = evaluate_skills.validate_dataset(dataset, loaded_manifest)
        evaluate_skills.validate_evidence(evidence, loaded_manifest, dataset_sha256, cases)

    def test_usage_aggregation_is_pure_and_keeps_cache_separate(self) -> None:
        calls = [
            {"usage": {"input_tokens": 10, "output_tokens": 2, "cached_input_tokens": 4}},
            {"usage": {"input_tokens": 7, "output_tokens": 1, "cached_input_tokens": 3}},
        ]
        self.assertEqual(
            evaluate_skills.aggregate_usage(calls),
            {"input_tokens": 17, "output_tokens": 3, "cached_input_tokens": 7},
        )
        self.assertEqual(
            evaluate_skills.aggregate_usage(
                [
                    calls[0],
                    {"usage": {"input_tokens": None, "output_tokens": 1, "cached_input_tokens": 1}},
                ]
            )["input_tokens"],
            None,
        )

    def test_missing_or_outside_evidence_is_rejected(self) -> None:
        manifest = self._manifest()
        dataset = self._dataset(manifest)
        evidence = self._evidence(manifest, dataset)
        loaded = json.loads(evidence.read_text(encoding="utf-8"))
        del loaded["query_results"]["direct"]["case-heldout"]
        evaluate_skills.write_json(evidence, loaded)
        loaded_manifest = evaluate_skills.load_manifest(manifest)
        _, dataset_sha256, cases = evaluate_skills.validate_dataset(dataset, loaded_manifest)
        with self.assertRaisesRegex(evaluate_skills.EvaluationError, "query evidence is missing"):
            evaluate_skills.validate_evidence(evidence, loaded_manifest, dataset_sha256, cases)

    def test_score_command_accepts_only_recorded_artifacts(self) -> None:
        manifest = self._manifest()
        dataset = self._dataset(manifest)
        evidence = self._evidence(manifest, dataset)
        output = self.root / "report.json"
        result = self._run(
            "score",
            "--corpus",
            str(manifest),
            "--dataset",
            str(dataset),
            "--evidence",
            str(evidence),
            "--output",
            str(output),
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads(output.read_text(encoding="utf-8"))
        self.assertEqual(report["evidence_sha256"], evaluate_skills.sha256_bytes(evidence.read_bytes()))
        self.assertEqual(report["case_count"], 3)

    def test_deleted_live_orchestration_is_not_present(self) -> None:
        source = (ROOT / "scripts/evaluate_skills.py").read_text(encoding="utf-8")
        for obsolete in ("run_model", "prompt_for_", "credential_store", "--resume", "--codex"):
            self.assertNotIn(obsolete, source)


if __name__ == "__main__":
    unittest.main()
