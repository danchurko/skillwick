#!/usr/bin/env python3
"""Check frozen benchmark labels and metrics without models or downloads."""
import copy
import importlib.util
import json
import math
from pathlib import Path

root = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('benchmark', root / 'scripts/benchmark-lexical.py')
benchmark = importlib.util.module_from_spec(spec)
spec.loader.exec_module(benchmark)
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
