#!/usr/bin/env python3
"""Standard-library contract checks for the embedded reranker runtime."""

from __future__ import annotations

import contextlib
import hashlib
import io
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "assets"))

from skillwick import reranker_runtime as runtime  # noqa: E402 - asset path is installed above


class FakeChoice:
    def __init__(self, *, instructions, criteria):
        self.instructions = instructions
        self.criteria = criteria


class FakeJudgment:
    def __init__(self, choice, confidence, probabilities):
        self.choice = choice
        self.confidence = confidence
        self.probabilities = probabilities


class FakeResponse:
    def __init__(self, choices, model=runtime.JEV_MODEL):
        self.choices = choices
        self.model = model
        self.usage = None


class FakeClient:
    def __init__(self, *, scores=None, mode="success"):
        self.scores = scores or {}
        self.mode = mode
        self.calls = []
        self.closed = False

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.closed = True

    def system_one(self, *, state, questions, model, timeout):
        self.calls.append((state, questions, model, timeout))
        self.assertions = {
            "model": model,
            "timeout": timeout,
            "state_fields": set(state),
            "candidate_fields": {tuple(sorted(candidate)) for candidate in state["candidates"]},
        }
        if self.mode == "exception":
            raise RuntimeError("provider diagnostic contains secret-provider-text")
        returned = {}
        labels = {candidate["id"]: candidate for candidate in state["candidates"]}
        for question_key in questions:
            label = question_key.removeprefix("candidate_")
            candidate = labels[label]
            probability = self.scores[candidate["name"]]
            choice = "relevant" if probability >= 0.5 else "not_relevant"
            confidence = 0.01 if candidate["name"] == "low-confidence" else 0.9
            returned[question_key] = FakeJudgment(
                choice,
                confidence,
                {"relevant": probability, "not_relevant": 1.0 - probability},
            )
        if self.mode == "missing":
            returned.pop(next(iter(returned)))
        if self.mode == "malformed":
            first = next(iter(returned))
            returned[first].probabilities = {"relevant": 0.7, "not_relevant": 0.7}
        response_model = "jev-other" if self.mode == "model_mismatch" else model
        return FakeResponse(returned, model=response_model)


def fake_factory(client, observed=None):
    def create(api_key, model, base_url, timeout):
        if observed is not None:
            observed.append((api_key, model, base_url, timeout))
        return client, FakeChoice, runtime.SDK_VERSION

    return create


def request(backend="jev", candidates=None, query="deploy an app"):
    return {
        "backend": backend,
        "query": query,
        "candidates": candidates if candidates is not None else [
            {"id": "skills/low/SKILL.md", "name": "low", "description": "Lower score."},
            {"id": "skills/a/SKILL.md", "name": "low-confidence", "description": "Strong match."},
            {"id": "skills/b/SKILL.md", "name": "high-tie", "description": "Strong match too."},
        ],
        "cache": "/private/runtime/models",
    }


class RerankerRuntimeContract(unittest.TestCase):
    def test_jev_uses_fixed_endpoint_and_model_and_stable_probability_order(self):
        client = FakeClient(scores={"low": 0.2, "low-confidence": 0.99, "high-tie": 0.99})
        observed = []
        output = runtime.run_request(
            request(),
            environ={runtime.API_KEY_ENV: "runtime-secret"},
            client_factory=fake_factory(client, observed),
        )

        self.assertEqual(
            output["ranked"],
            ["skills/a/SKILL.md", "skills/b/SKILL.md", "skills/low/SKILL.md"],
        )
        self.assertEqual(output["model"], runtime.JEV_MODEL)
        self.assertEqual(observed, [("runtime-secret", runtime.JEV_MODEL, runtime.JEV_BASE_URL, 15.0)])
        self.assertEqual(client.assertions["state_fields"], {"task", "candidates"})
        self.assertEqual(client.assertions["candidate_fields"], {("description", "id", "name")})
        self.assertTrue(client.closed)

    def test_provider_payload_uses_labels_and_excludes_identity_path_and_private_fields(self):
        identity = "skills/private/SKILL.md"
        captured = {}
        record = {
            "name": "private skill",
            "description": "A useful description.",
            "path": "/Users/example/private/SKILL.md",
            "body": "private body text",
        }

        state, questions, reverse, _size = runtime.build_request(
            "perform the task", [identity], {identity: record}, FakeChoice, captured
        )
        wire_state_and_questions = json.dumps({"state": state, "questions": questions}, default=lambda item: {
            "instructions": item.instructions,
            "criteria": item.criteria,
        })

        self.assertEqual(state["candidates"], [{"id": "c000", "name": "private skill", "description": "A useful description."}])
        self.assertEqual(reverse, {"candidate_c000": identity})
        self.assertNotIn(identity, wire_state_and_questions)
        self.assertNotIn(record["path"], wire_state_and_questions)
        self.assertNotIn(record["body"], wire_state_and_questions)
        self.assertNotIn("path", state["candidates"][0])
        self.assertNotIn("body", state["candidates"][0])
        self.assertEqual(captured["question_to_identity"], {"candidate_c000": identity})

    def test_complete_mapping_probabilities_and_exact_model_are_required(self):
        for mode, category in (
            ("missing", "missing_answer_mapping"),
            ("malformed", "malformed_response"),
            ("model_mismatch", "model_mismatch"),
        ):
            with self.subTest(mode=mode):
                client = FakeClient(scores={"low": 0.2, "low-confidence": 0.8, "high-tie": 0.7}, mode=mode)
                with self.assertRaises(runtime.RuntimeErrorCategory) as raised:
                    runtime.run_request(
                        request(),
                        environ={runtime.API_KEY_ENV: "runtime-secret"},
                        client_factory=fake_factory(client),
                    )
                self.assertEqual(raised.exception.category, category)

    def test_missing_key_and_secret_input_fail_before_provider_call(self):
        def forbidden_factory(*_args):
            self.fail("provider factory must not run")

        with self.assertRaises(runtime.RuntimeErrorCategory) as missing:
            runtime.run_request(request(), environ={}, client_factory=forbidden_factory)
        self.assertEqual(missing.exception.category, "missing_api_key")

        with self.assertRaises(runtime.RuntimeErrorCategory) as secret:
            runtime.run_request(
                request(query="runtime-secret is part of this query"),
                environ={runtime.API_KEY_ENV: "runtime-secret"},
                client_factory=forbidden_factory,
            )
        self.assertEqual(secret.exception.category, "secret_in_input")

    def test_provider_exception_text_is_replaced_by_safe_category(self):
        client = FakeClient(scores={"low": 0.2, "low-confidence": 0.8, "high-tie": 0.7}, mode="exception")
        with self.assertRaises(runtime.RuntimeErrorCategory) as raised:
            runtime.run_request(
                request(),
                environ={runtime.API_KEY_ENV: "runtime-secret"},
                client_factory=fake_factory(client),
            )
        self.assertEqual(raised.exception.category, "provider_error")
        self.assertEqual(str(raised.exception), "provider_error")
        self.assertNotIn("secret-provider-text", str(raised.exception))
        self.assertTrue(client.closed)

    def test_tinybert_uses_only_name_description_and_preserves_stable_ties(self):
        observed = []

        def score_factory(cache):
            self.assertEqual(cache, Path("/private/runtime/models"))

            def score(query, documents):
                observed.append((query, documents))
                return [0.1, 0.9, 0.9]

            return score

        output = runtime.run_request(request("tinybert"), score_factory=score_factory)
        self.assertEqual(
            output,
            {
                "ranked": ["skills/a/SKILL.md", "skills/b/SKILL.md", "skills/low/SKILL.md"],
                "model": runtime.TINYBERT_MODEL_ID,
            },
        )
        self.assertEqual(observed, [("deploy an app", ["low: Lower score.", "low-confidence: Strong match.", "high-tie: Strong match too."])])

    def test_tinybert_requires_one_finite_score_per_candidate(self):
        with self.assertRaises(runtime.RuntimeErrorCategory) as incomplete:
            runtime.run_request(
                request("tinybert"),
                score_factory=lambda _cache: lambda _query, _documents: [0.5],
            )
        self.assertEqual(incomplete.exception.category, "incomplete_scores")

        with self.assertRaises(runtime.RuntimeErrorCategory) as malformed:
            runtime.run_request(
                request("tinybert"),
                score_factory=lambda _cache: lambda _query, _documents: [0.5, float("nan"), 0.1],
            )
        self.assertEqual(malformed.exception.category, "malformed_scores")

    def test_request_rejects_paths_bodies_and_custom_endpoints(self):
        with self.assertRaises(runtime.RuntimeErrorCategory) as candidate_metadata:
            runtime.validate_request({
                **request("tinybert"),
                "candidates": [{"id": "one", "name": "skill", "description": "text", "path": "/secret"}],
            })
        self.assertEqual(candidate_metadata.exception.category, "invalid_candidates")

        with self.assertRaises(runtime.RuntimeErrorCategory) as endpoint:
            runtime.validate_request({**request("jev"), "base_url": "https://attacker.example"})
        self.assertEqual(endpoint.exception.category, "invalid_request")

    def test_prepare_jev_performs_a_fixed_model_smoke_request(self):
        client = FakeClient(scores={"runtime readiness": 0.9})
        observed = []
        with patch.dict(os.environ, {runtime.API_KEY_ENV: "setup-secret"}), patch.object(
            runtime, "_default_client_factory", fake_factory(client, observed)
        ):
            model = runtime._prepare_jev()

        self.assertEqual(model, runtime.JEV_MODEL)
        self.assertEqual(observed, [("setup-secret", runtime.JEV_MODEL, runtime.JEV_BASE_URL, 15.0)])
        self.assertEqual(client.calls[0][0]["task"], "Verify the reranker connection.")
        self.assertTrue(client.closed)

    def test_prepare_tinybert_downloads_only_the_pinned_manifest_then_smokes_it(self):
        cache = Path("/private/runtime/models")
        scorer_calls = []

        def fake_scorer(query, documents):
            scorer_calls.append((query, documents))
            return [0.25]

        with patch.object(runtime, "pinned_model") as download, patch.object(
            runtime, "load_tinybert_scorer", return_value=fake_scorer
        ) as load:
            model = runtime._prepare_tinybert(cache)

        self.assertEqual(model, runtime.TINYBERT_MODEL_ID)
        download.assert_called_once_with(cache, runtime.TINYBERT, allow_download=True)
        load.assert_called_once_with(cache)
        self.assertEqual(scorer_calls, [("runtime readiness", ["A local inference smoke check."])])

    def test_artifact_hash_and_safe_cli_error_contract(self):
        with tempfile.TemporaryDirectory() as temporary:
            artifact = Path(temporary) / "artifact"
            artifact.write_bytes(b"pinned")
            expected = {"bytes": 6, "sha256": hashlib.sha256(b"pinned").hexdigest()}
            runtime.verify_artifact(artifact, expected)
            artifact.write_bytes(b"changed")
            with self.assertRaises(runtime.RuntimeErrorCategory) as corrupt:
                runtime.verify_artifact(artifact, expected)
            self.assertEqual(corrupt.exception.category, "model_artifact_checksum")

        input_value = json.dumps({**request("unknown")}).encode("utf-8")
        fake_stdin = type("FakeStdin", (), {"buffer": io.BytesIO(input_value)})()
        output = io.StringIO()
        with patch.object(runtime.sys, "stdin", fake_stdin), contextlib.redirect_stdout(output):
            status = runtime.main([])
        self.assertEqual(status, 1)
        self.assertEqual(output.getvalue(), '{"error":"unsupported_backend"}\n')
        self.assertNotIn("Traceback", output.getvalue())


if __name__ == "__main__":
    unittest.main()
