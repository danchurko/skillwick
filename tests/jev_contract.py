#!/usr/bin/env python3
"""Offline contracts for the opt-in JEV adapter; no SDK import or network access."""

import importlib.util
import json
import math
import sys
import tempfile
from dataclasses import dataclass, field
from pathlib import Path

root = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(root / "scripts"))
import benchmark_jev as jev
from benchmark_metrics import quality
from benchmark_profiles import canonical_hash

semantic_spec = importlib.util.spec_from_file_location("benchmark_semantic", root / "scripts/benchmark-semantic.py")
semantic = importlib.util.module_from_spec(semantic_spec)
semantic_spec.loader.exec_module(semantic)
assert hasattr(semantic, "jev_command")
assert "typesafe_sdk" not in sys.modules, "normal contract import must not load the optional SDK"


class FakeChoice:
    def __init__(self, *, instructions, criteria):
        self.instructions = instructions
        self.criteria = criteria


@dataclass
class FakeJudgment:
    choice: str
    confidence: float
    probabilities: dict


@dataclass
class FakeUsage:
    input_tokens: int | None = 11
    output_tokens: int | None = 0


@dataclass
class FakeResponse:
    choices: dict
    model: str = "jev-1.13.0"
    usage: FakeUsage | None = field(default_factory=FakeUsage)


class FakeClient:
    def __init__(self, *, mode="rank", model="jev-1.13.0"):
        self.mode = mode
        self.model = model
        self.calls = []
        self.closed = False

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.closed = True

    def system_one(self, *, state, questions, model, timeout):
        self.calls.append((state, questions, model, timeout))
        assert set(state) == {"task", "candidates"}
        for candidate in state["candidates"]:
            assert set(candidate) == {"id", "name", "description"}
            assert candidate["id"].startswith("c")
        labels = {row["id"]: row for row in state["candidates"]}
        answer_map = {}
        for key, question in questions.items():
            label = key.removeprefix("candidate_")
            candidate = labels[label]
            assert label in question.instructions
            assert question.criteria == jev.CHOICE_CRITERIA
            assert "source_family" not in candidate
            if self.mode == "low" and state["task"] == "second task":
                probability = 0.54
            elif candidate["name"] in {"pkg:second", "deploy"}:
                probability = 0.90
            elif candidate["name"] in {"pkg:first", "sqlite-maintenance"}:
                probability = 0.90 if state["task"] == "second task" else 0.70
            else:
                probability = 0.10
            choice = "relevant" if probability >= 0.5 else "not_relevant"
            confidence = probability if choice == "relevant" else 1 - probability
            if self.mode == "uncertain_distractor" and candidate["name"] == "pkg:first":
                confidence = .4
            answer_map[key] = FakeJudgment(
                choice, confidence,
                {"relevant": probability, "not_relevant": 1 - probability},
            )
        return FakeResponse(answer_map, model=self.model)


class TimeoutClient(FakeClient):
    def system_one(self, **_):
        self.calls.append(None)
        raise TimeoutError("timeout text contains timeout-secret")


def profile(version=1, secret_query=None):
    if version == 1:
        records = [
            {"name": "pkg:first", "description": "First description", "source_family": "private"},
            {"name": "pkg:second", "description": "Second description", "source_family": "private"},
            {"name": "pkg:other", "description": "Other description", "source_family": "private"},
        ]
        cases = [
            {"id": "one", "kind": "positive", "queries": ["first task"], "relevant": ["pkg:second"]},
            {"id": "two", "kind": "positive", "queries": [secret_query or "second task"], "relevant": ["pkg:first"]},
            {"id": "three", "kind": "negative", "queries": ["no matching task"], "relevant": []},
        ]
    else:
        records = [
            {"fixture_id": "aws", "name": "deploy", "description": "Deploy AWS services.", "source_family": "private"},
            {"fixture_id": "edge", "name": "deploy", "description": "Deploy edge sites.", "source_family": "private"},
            {"fixture_id": "sqlite", "name": "sqlite-maintenance", "description": "Maintain SQLite.", "source_family": "private"},
        ]
        cases = [
            {"id": "multi", "kind": "positive", "queries": ["deploy app"], "relevant": ["aws", "edge"]},
            {"id": "negative", "kind": "negative", "queries": ["unrelated request"], "relevant": []},
        ]
    doc = {
        "version": version,
        "kind": "skillwick-lexical-profile",
        "corpus": {"total": len(records), "sha256": canonical_hash(records), "records": records},
        "heldout": {
            "case_count": len(cases), "query_count": sum(len(case["queries"]) for case in cases),
            "cases": cases,
        },
    }
    if version == 2:
        doc["heldout"]["sha256"] = canonical_hash(cases)
    return doc


def write_inputs(directory, version=1, secret_query=None):
    profile_doc = profile(version, secret_query)
    profile_path = directory / f"profile-v{version}.json"
    candidates_path = directory / f"candidates-v{version}.json"
    profile_path.write_text(json.dumps(profile_doc, ensure_ascii=False))
    rows = jev.profile_queries(profile_doc)
    if version == 1:
        rankings = [
            {"id": rows[0]["id"], "query": rows[0]["query"], "relevant": rows[0]["relevant"],
             "ranked": ["pkg:first", "pkg:second", "pkg:other"]},
            {"id": rows[1]["id"], "query": rows[1]["query"], "relevant": rows[1]["relevant"],
             "ranked": ["pkg:second", "pkg:first", "pkg:other"]},
            {"id": rows[2]["id"], "query": rows[2]["query"], "relevant": rows[2]["relevant"],
             "ranked": ["pkg:first", "pkg:second", "pkg:other"]},
        ]
    else:
        rankings = [
            {"id": rows[0]["id"], "query": rows[0]["query"], "relevant": rows[0]["relevant"],
             "ranked": ["edge", "sqlite", "aws"]},
            {"id": rows[1]["id"], "query": rows[1]["query"], "relevant": rows[1]["relevant"],
             "ranked": ["aws", "sqlite", "edge"]},
        ]
    candidates = {
        "version": version, "kind": "skillwick-lexical-baseline",
        "profile_sha256": jev.digest(profile_path), "corpus_sha256": profile_doc["corpus"]["sha256"],
        "corpus_total": profile_doc["corpus"]["total"], "candidate_pool_size": 3,
        "provenance": {
            "skillwick_commit": "a" * 40, "working_tree_dirty": False,
            "implementations_sha256": {
                "scripts/benchmark-lexical.py": "b" * 64,
                "scripts/benchmark_metrics.py": "c" * 64,
                "scripts/benchmark_profiles.py": "d" * 64,
            },
        },
        "quality": quality(version, rankings), "rankings": rankings,
    }
    candidates_path.write_text(json.dumps(candidates, ensure_ascii=False))
    return profile_doc, profile_path, candidates_path, rows, rankings


def factory_for(client):
    return lambda api_key, model, base_url, timeout: (client, FakeChoice, jev.SDK_VERSION)


def run(directory, version=1, *, client=None, key="test-secret-key", **kwargs):
    _, profile_path, candidates_path, rows, lexical = write_inputs(directory, version)
    output = directory / f"result-v{version}.json"
    status = jev.run_experiment(
        profile_path, candidates_path, output, live=True,
        environ={"TYPESAFE_API_KEY": key} if key is not None else {},
        client_factory=factory_for(client) if client is not None else None,
        **kwargs,
    )
    return status, json.loads(output.read_text()), rows, lexical, output


# Payloads carry only task and a bounded candidate view; local fixture identities remain local.
v2 = profile(2)
records_by_id = {row["fixture_id"]: row for row in v2["corpus"]["records"]}
captured = {}

def capture_choice(*, instructions, criteria):
    captured[instructions] = criteria
    return FakeChoice(instructions=instructions, criteria=criteria)

state, questions, mapping, byte_count = jev.build_request("deploy app", ["edge", "aws"], records_by_id, capture_choice)
assert [row["id"] for row in state["candidates"]] == ["c000", "c001"]
assert [row["name"] for row in state["candidates"]] == ["deploy", "deploy"]
assert mapping == {"candidate_c000": "edge", "candidate_c001": "aws"}
assert all("c000" in text or "c001" in text for text in captured)
assert byte_count > 0
assert not ({"fixture_id", "source_family", "path", "body"} & set(state["candidates"][0]))
for invalid, category in ((["edge", "edge"], "duplicate_candidate_mapping"), (["missing"], "missing_candidate_mapping")):
    try:
        jev.build_request("task", invalid, records_by_id, FakeChoice)
    except jev.JEVInputError as error:
        assert error.category == category
    else:
        raise AssertionError(f"{category} accepted")

# Complete mapping, normalized probabilities, bounded confidence and max-choice agreement are required.
valid_judgment = FakeJudgment("relevant", 0.8, {"relevant": 0.8, "not_relevant": 0.2})
valid_response = FakeResponse({"candidate_c000": valid_judgment})
parsed, resolved, usage = jev.parse_response(valid_response, ["candidate_c000"])
assert parsed["candidate_c000"]["probability_relevant"] == 0.8
assert resolved == "jev-1.13.0" and usage == {"input_tokens": 11, "output_tokens": 0}
independent_confidence = FakeJudgment("not_relevant", 0.34, {"relevant": 0.33, "not_relevant": 0.67})
parsed, _, _ = jev.parse_response(FakeResponse({"candidate_c000": independent_confidence}), ["candidate_c000"])
assert parsed['candidate_c000']['confidence'] == .34
assert parsed['candidate_c000']['probability_relevant'] == .33
for returned, category in (({}, "missing_answer_mapping"),
                           ({"candidate_c000": valid_judgment, "unexpected": valid_judgment}, "extra_answer_mapping")):
    try:
        jev.parse_response(FakeResponse(returned), ["candidate_c000"])
    except jev.JEVInputError as error:
        assert error.category == category
    else:
        raise AssertionError(f"{category} accepted")
malformed_judgments = [
    FakeJudgment("relevant", 0.8, {"relevant": math.nan, "not_relevant": 0.0}),
    FakeJudgment("relevant", 0.8, {"relevant": 1.2, "not_relevant": -0.2}),
    FakeJudgment("relevant", 0.7, {"relevant": 0.8, "not_relevant": 0.3}),
    FakeJudgment("not_relevant", 0.2, {"relevant": 0.8, "not_relevant": 0.2}),
    FakeJudgment("relevant", 1.2, {"relevant": 0.8, "not_relevant": 0.2}),
]
for invalid in malformed_judgments:
    try:
        jev.parse_response(FakeResponse({"candidate_c000": invalid}), ["candidate_c000"])
    except jev.JEVInputError as error:
        assert error.category == "malformed_response"
    else:
        raise AssertionError("malformed probability or confidence accepted")

# V1 reranking, stable equal-score lexical order, cost from verified current resolved model, and provenance.
with tempfile.TemporaryDirectory(prefix="skillwick-jev-v1-") as temporary:
    status, result, rows, lexical, output = run(Path(temporary), 1, client=FakeClient())
    assert status == 0 and result["status"] == "completed"
    assert result["request_counts"] == {"attempted": 3, "valid_responses": 3, "failed_requests": 0,
                                         "single_request_per_query": True}
    assert result["rankings"][0]["ranked"] == ["pkg:second", "pkg:first", "pkg:other"]
    assert result["rankings"][1]["ranked"] == lexical[1]["ranked"]
    assert result["rankings"][1]["judgments"][0]["candidate"] == "pkg:second"
    trace = result["rankings"][0]["request_trace"]
    assert trace["payload"]["state"]["task"] == rows[0]["query"]
    assert list(trace["question_to_identity"].values()) == lexical[0]["ranked"]
    assert result["rankings"][0]["raw_provider_ranked"] == result["rankings"][0]["ranked"]
    assert "payload_sha256" in trace
    assert result["quality"]["queries"] == 3
    assert result["selection_metrics"]["queries"] == 3
    assert result["confidence_metrics"]["candidate_choice_confidence"]["correct"]["count"] == 5
    assert result["cost"]["estimated_cost_usd"] == 3 * 11 * 0.042 / 1_000_000
    assert result["cost"]["actual_cost_usd"] is None
    assert result["provenance"]["implementations_sha256"]["scripts/benchmark_jev.py"]

# An uncertain distractor vetoes the integration while the raw winner remains useful.
with tempfile.TemporaryDirectory(prefix="skillwick-jev-veto-") as temporary:
    status, result, _, lexical, _ = run(Path(temporary), 1, client=FakeClient(mode="uncertain_distractor"))
    first = result["rankings"][0]
    assert status == 0
    assert first["outcome"] == "lexical_fallback_low_confidence"
    assert first["ranked"] == lexical[0]["ranked"]
    assert first["raw_provider_ranked"][0] == "pkg:second"
    assert first["low_confidence_candidates"] == ["pkg:first"]

# V2 keeps fixture IDs through duplicate visible names, multi-relevant metrics and negative cases.
with tempfile.TemporaryDirectory(prefix="skillwick-jev-v2-") as temporary:
    status, result, rows, lexical, _ = run(Path(temporary), 2, client=FakeClient())
    assert status == 0
    assert result["rankings"][0]["ranked"] == ["edge", "aws", "sqlite"]
    assert result["quality"]["positive_queries"] == 1
    assert result["quality"]["negative_queries"] == 1
    assert result["selection_metrics"]["positive_queries"] == 1
    assert result["retrieval_metrics"]["positive_recall_at_1"] == 0.5

# Low confidence keeps the whole lexical row, marks a distinct fallback and remains a valid provider response.
with tempfile.TemporaryDirectory(prefix="skillwick-jev-low-") as temporary:
    status, result, _, lexical, _ = run(Path(temporary), 1, client=FakeClient(mode="low"))
    assert status == 0
    assert result["rankings"][1]["outcome"] == "lexical_fallback_low_confidence"
    assert result["rankings"][1]["ranked"] == lexical[1]["ranked"]
    assert result["rankings"][1]["provider_status"] == "valid_response"

# Smoke runs preserve a complete row set while suppressing incomplete aggregate metrics.
with tempfile.TemporaryDirectory(prefix="skillwick-jev-smoke-") as temporary:
    status, result, _, _, _ = run(Path(temporary), 1, client=FakeClient(), max_queries=1)
    assert status == 0 and result["smoke"]["enabled"]
    assert result["quality"] is None and result["retrieval_metrics"] is None
    assert result["aggregate_metrics_status"] == "suppressed_partial_smoke"
    assert sum(result["outcomes"].values()) == 3

# Missing key and provider exceptions preserve rankings, write categorized output, and never expose exception text.
with tempfile.TemporaryDirectory(prefix="skillwick-jev-no-key-") as temporary:
    status, result, _, lexical, output = run(Path(temporary), 1, key=None)
    assert status == 1 and not result["request_counts"]["attempted"]
    assert result["rankings"][0]["fallback_category"] == "missing_api_key"
    assert lexical[0]["ranked"] == result["rankings"][0]["ranked"]

with tempfile.TemporaryDirectory(prefix="skillwick-jev-timeout-") as temporary:
    status, result, _, lexical, output = run(Path(temporary), 1, client=TimeoutClient(), max_queries=1)
    assert status == 1 and result["rankings"][0]["fallback_category"] == "timeout"
    assert result["rankings"][0]["ranked"] == lexical[0]["ranked"]
    assert "timeout-secret" not in output.read_text()

with tempfile.TemporaryDirectory(prefix="skillwick-jev-secret-model-") as temporary:
    secret = "secret-model-token-923456"
    status, result, _, _, output = run(Path(temporary), 1, client=FakeClient(model=secret), key=secret,
                                       max_queries=1)
    assert status == 1 and result["rankings"][0]["fallback_category"] == "secret_in_response"
    assert secret not in output.read_text()

with tempfile.TemporaryDirectory(prefix="skillwick-jev-secret-input-") as temporary:
    directory = Path(temporary)
    secret = "secret-input-token-abcdef"
    _, profile_path, candidates_path, _, _ = write_inputs(directory, 1, secret_query=secret)
    called = []
    try:
        jev.run_experiment(profile_path, candidates_path, directory / "result.json", live=True,
                           environ={"TYPESAFE_API_KEY": secret},
                           client_factory=lambda *args: called.append(args))
    except jev.JEVInputError as error:
        assert error.category == "secret_in_payload"
    else:
        raise AssertionError("credential-bearing task sent to provider")
    assert not called

print("JEV offline contracts passed; no TypeSafe SDK import or network request")
