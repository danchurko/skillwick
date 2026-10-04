#!/usr/bin/env python3
"""Check frozen benchmark labels and metrics without models or downloads."""
import copy
import importlib.util
import json
import math
import sys
from pathlib import Path

root = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(root / 'scripts'))
spec = importlib.util.spec_from_file_location('benchmark', root / 'scripts/benchmark-lexical.py')
benchmark = importlib.util.module_from_spec(spec)
spec.loader.exec_module(benchmark)
from benchmark_metrics import confidence_metrics, retrieval_metrics, selection_metrics, provenance
from unittest.mock import patch
import subprocess

with patch("benchmark_metrics.subprocess.check_output", side_effect=subprocess.CalledProcessError(128, "git")):
    exported = provenance()
    assert exported["skillwick_commit"] is None
    assert exported["working_tree_dirty"] is None
    assert exported["implementations_sha256"]["assets/skillwick/reranker_runtime.py"]

before = [
    {'id': 'multi:1', 'query': 'task', 'relevant': ['a', 'b'], 'ranked': ['wrong', 'a', 'b']},
    {'id': 'negative:1', 'query': 'none', 'relevant': [], 'ranked': []},
]
after = copy.deepcopy(before)
after[0]['ranked'] = ['a', 'b', 'wrong']
extra = retrieval_metrics(2, after)
assert extra['positive_recall_at_1'] == 0.5
assert extra['positive_recall_at_3'] == 1
assert extra['negative_false_positive_rate'] == 0
comparison = selection_metrics(before, after)
assert comparison['improved'] == 1 and comparison['unchanged_materially'] == 1
assert comparison['relevant_ranked_first_rate'] == 1
after[0]['ranked'] = ['a', 'wrong', 'other', 'third', 'fourth', 'b']
comparison = selection_metrics(before, after)
assert comparison['relevant_displacements'] == 1
assert comparison['selected_skill_success'] is None
confidence = confidence_metrics([{
    'relevant': ['a'], 'ranked': ['a', 'wrong'], 'outcome': 'reranked',
    'judgments': [
        {'candidate': 'a', 'choice': 'relevant', 'confidence': .9, 'probability_relevant': .9},
        {'candidate': 'wrong', 'choice': 'relevant', 'confidence': .7, 'probability_relevant': .7},
    ],
}])
assert confidence['candidate_choice_confidence']['correct']['count'] == 1
assert confidence['candidate_choice_confidence']['incorrect']['median'] == .7
assert confidence['reranked_first_probability_relevant']['relevant']['median'] == .9

metrics = benchmark.task_metrics([(['a'], {'a', 'b'}), (['wrong'], set()), ([], set())])
assert metrics['positive_recall_at_5'] == 0.5
assert metrics['positive_mrr_at_5'] == 1
assert math.isclose(metrics['positive_ndcg_at_5'], 1 / (1 + 1 / math.log2(3)))
assert metrics['negative_false_positive_rate'] == 0.5
for version in [1, 2]:
    profile = json.loads((root / f'benchmarks/profile-v{version}.json').read_text())
    benchmark.validate_profile(profile)
profile = copy.deepcopy(profile)
profile['heldout']['cases'][0]['relevant'] = []
try:
    benchmark.validate_profile(profile)
except ValueError:
    pass
else:
    raise AssertionError('changed frozen labels accepted')
print('Benchmark metric and profile contracts passed')
