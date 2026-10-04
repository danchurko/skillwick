#!/usr/bin/env python3
"""Check frozen benchmark labels and metrics without models or downloads."""
import copy
import argparse
import hashlib
import importlib.util
import json
import math
import os
import sys
from pathlib import Path
from tempfile import TemporaryDirectory

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


def real_inventory():
    rows = []
    for identifier, name, description in (
        ('alpha@native', 'alpha', 'Use alpha for this task.'),
        ('beta@native', 'beta', 'Use beta for a different task.'),
    ):
        rows.append({
            'id': identifier,
            'name': name,
            'description': description,
            'scope': 'global',
            'path': f'/private/native/{name}/SKILL.md',
            'canonical': f'/private/native/{name}/SKILL.md',
            'base': f'/private/native/{name}',
            'source': '/private/native',
            'source_kind': 'filesystem',
            'enabled': True,
            'plugin_id': None,
            'degraded': False,
            'hash': ('a' if name == 'alpha' else 'b') * 64,
            'origins': [{
                'id': identifier,
                'path': f'/private/native/{name}/SKILL.md',
                'canonical': f'/private/native/{name}/SKILL.md',
                'base': f'/private/native/{name}',
                'source': '/private/native',
                'scope': 'global',
                'plugin_id': None,
            }],
            'grouping_diagnostic': None,
        })
    return {'version': 3, 'total': len(rows), 'results': rows}


real_cases = {
    'version': 1,
    'label_policy': 'Fixed before replay from task intent and package metadata.',
    'cases': [
        {'id': 'alpha-positive', 'kind': 'positive', 'queries': ['Use alpha for the task.'], 'relevant': ['alpha']},
        {'id': 'ordinary-negative', 'kind': 'negative', 'queries': ['What is unrelated?'], 'relevant': []},
    ],
    'diagnostics': [
        {'id': 'known-name-misuse', 'query': 'alpha beta', 'reason': 'Known names without task intent.'},
    ],
}
real_rows = real_inventory()
real_profile = benchmark.build_real_profile(
    real_rows,
    real_cases,
    inventory_raw_sha256='c' * 64,
    config_sha256='d' * 64,
    config_policy={
        'discovery': 'auto', 'configured_root_count': 1, 'project_count': 0,
        'agent_count': 0, 'reranker_backend': 'none',
    },
    executable={'path': 'skillwick', 'version': 'skillwick 0.4.0', 'sha256': 'e' * 64, 'bytes': 42},
    case_input_sha256='f' * 64,
)
benchmark.validate_real_profile(real_profile)
assert real_profile['corpus']['records'][0]['fixture_id'] == 'alpha@native'
assert real_profile['heldout']['cases'][0]['relevant'] == ['alpha@native']
assert len(real_profile['heldout']['cases']) == 2
assert real_profile['diagnostics']['case_count'] == 1
assert 'private/native' not in json.dumps(real_profile['corpus']['records'])
changed_origin = copy.deepcopy(real_rows)
changed_origin['results'][0]['origins'][0]['canonical'] = '/private/other/alpha/SKILL.md'
assert benchmark.inventory_sha256(changed_origin) != benchmark.inventory_sha256(real_rows)

missing_labels = copy.deepcopy(real_cases)
missing_labels['cases'][0]['relevant'] = ['absent']
try:
    benchmark.build_real_profile(
        real_rows, missing_labels, inventory_raw_sha256='c' * 64, config_sha256='d' * 64,
        config_policy=real_profile['provenance']['config_policy'],
        executable=real_profile['provenance']['executable'], case_input_sha256='f' * 64,
    )
except ValueError as error:
    assert 'missing labelled targets: alpha-positive:absent' in str(error)
else:
    raise AssertionError('real profile silently dropped a missing positive target')

ambiguous_inventory = copy.deepcopy(real_rows)
duplicate = copy.deepcopy(ambiguous_inventory['results'][0])
duplicate['id'] = 'alpha@second-root'
duplicate['path'] = '/private/second/alpha/SKILL.md'
duplicate['canonical'] = duplicate['path']
duplicate['base'] = '/private/second/alpha'
duplicate['origins'][0]['id'] = duplicate['id']
duplicate['origins'][0]['path'] = duplicate['path']
duplicate['origins'][0]['canonical'] = duplicate['canonical']
duplicate['origins'][0]['base'] = duplicate['base']
ambiguous_inventory['results'].append(duplicate)
ambiguous_inventory['total'] += 1
try:
    benchmark.build_real_profile(
        ambiguous_inventory, real_cases, inventory_raw_sha256='c' * 64, config_sha256='d' * 64,
        config_policy=real_profile['provenance']['config_policy'],
        executable=real_profile['provenance']['executable'], case_input_sha256='f' * 64,
    )
except ValueError as error:
    assert 'ambiguous labelled targets: alpha-positive:alpha (2 eligible rows)' in str(error)
else:
    raise AssertionError('real profile accepted an ambiguous positive target')

with TemporaryDirectory() as temporary_name:
    temporary = Path(temporary_name)
    binary = temporary / 'skillwick'
    binary.write_bytes(b'test binary fingerprint')
    config = temporary / 'config.toml'
    config.write_text('version = 1\ndiscovery = "auto"\nroots = ["/private/native"]\n')
    cwd = temporary / 'workspace'
    cwd.mkdir()
    cases_path = temporary / 'cases.json'
    cases_path.write_text(json.dumps(real_cases))
    profile_path = temporary / 'profile.json'
    inventory_path = temporary / 'inventory.json'
    output_path = temporary / 'replay.json'
    cache_dir = temporary / 'cache'
    inventory_text = json.dumps(real_rows)
    invocations = []
    protected_env = ('HOME', 'CODEX_HOME', 'CLAUDE_CONFIG_DIR', 'XDG_CONFIG_HOME', 'XDG_STATE_HOME')

    def fake_real_command(command, env, actual_cwd, check=True):
        invocations.append((list(command), dict(env), actual_cwd))
        assert actual_cwd == cwd.resolve()
        assert all(env.get(key) == os.environ.get(key) for key in protected_env)
        assert str(cache_dir.resolve()) in env['XDG_CACHE_HOME']
        assert 'init' not in command and '--root' not in command
        if '--version' in command:
            stdout = 'skillwick 0.4.0\n'
        elif command[-1] == 'list':
            stdout = inventory_text
        else:
            index = command.index('search')
            query = command[index + 1]
            if query == 'Use alpha for the task.':
                stdout = json.dumps({'version': 3, 'results': real_rows['results']})
            elif query == 'alpha beta':
                stdout = json.dumps({'version': 3, 'results': [real_rows['results'][0]]})
            else:
                stdout = json.dumps({'version': 3, 'results': []})
        return subprocess.CompletedProcess(command, 0, stdout, ''), 1.0

    with patch.object(benchmark, 'run_real_command', side_effect=fake_real_command):
        benchmark.freeze_real(argparse.Namespace(
            binary=binary, config=config, cwd=cwd, cache_dir=cache_dir, cases=cases_path,
            inventory_output=inventory_path, output=profile_path,
        ))
        assert json.loads(profile_path.read_text())['diagnostics']['case_count'] == 1
        benchmark.replay_real(argparse.Namespace(
            binary=binary, config=config, cwd=cwd, cache_dir=cache_dir,
            profile=profile_path, output=output_path, pool_size=20,
        ))
    replay_result = json.loads(output_path.read_text())
    benchmark.validate_real_result(
        json.loads(profile_path.read_text()), hashlib.sha256(profile_path.read_bytes()).hexdigest(), replay_result
    )
    assert len(replay_result['rankings']) == 2
    assert len(replay_result['diagnostics']) == 1
    assert replay_result['rankings'][0]['ranked'] == ['alpha@native', 'beta@native']
    assert replay_result['diagnostics'][0]['ranked'] == ['alpha@native']
    benchmark.validate(argparse.Namespace(profile=profile_path, result=[output_path]))

    semantic_result = {
        'version': 2,
        'kind': 'skillwick-reranker-experiment',
        'profile_sha256': hashlib.sha256(profile_path.read_bytes()).hexdigest(),
        'corpus_sha256': real_profile['corpus']['sha256'],
        'corpus_total': real_profile['corpus']['total'],
        'candidate_pool_size': 20,
        'source_candidate_pool_size': 20,
        'candidate_source_kind': replay_result['kind'],
        'candidate_source_sha256': hashlib.sha256(output_path.read_bytes()).hexdigest(),
        'rankings': copy.deepcopy(replay_result['rankings']),
    }
    semantic_result['rankings'][0]['ranked'].reverse()
    semantic_result['quality'] = benchmark.task_metrics([
        (item['ranked'], set(item['relevant'])) for item in semantic_result['rankings']
    ])
    semantic_result['retrieval_metrics'] = retrieval_metrics(2, semantic_result['rankings'])
    semantic_result['selection_metrics'] = selection_metrics(
        replay_result['rankings'], semantic_result['rankings']
    )
    semantic_path = temporary / 'tinybert.json'
    semantic_path.write_text(json.dumps(semantic_result))
    benchmark.validate(argparse.Namespace(profile=profile_path, result=[output_path, semantic_path]))

    for label, mutate in (
        ('candidate source digest', lambda invalid: invalid.update(candidate_source_sha256='0' * 64)),
        ('source pool', lambda invalid: invalid.update(source_candidate_pool_size=10)),
        ('rerank pool', lambda invalid: invalid.update(candidate_pool_size=10)),
        ('dropped candidate', lambda invalid: invalid['rankings'][0]['ranked'].pop()),
        ('added candidate', lambda invalid: invalid['rankings'][0]['ranked'].append('unknown@native')),
    ):
        invalid_semantic = copy.deepcopy(semantic_result)
        mutate(invalid_semantic)
        try:
            benchmark.validate_real_semantic_result(
                json.loads(profile_path.read_text()),
                hashlib.sha256(profile_path.read_bytes()).hexdigest(),
                invalid_semantic,
                output_path,
                replay_result,
            )
        except ValueError:
            pass
        else:
            raise AssertionError(f'invalid real-source semantic {label} accepted')
    try:
        benchmark.validate(argparse.Namespace(profile=profile_path, result=[semantic_path]))
    except ValueError as error:
        assert 'missing its matching actual-source lexical candidate receipt' in str(error)
    else:
        raise AssertionError('real-source semantic result without its lexical candidate receipt accepted')

    invalid_diagnostics = copy.deepcopy(replay_result)
    invalid_diagnostics['diagnostics'][0]['query'] = 'changed query'
    try:
        benchmark.validate_real_result(
            json.loads(profile_path.read_text()),
            hashlib.sha256(profile_path.read_bytes()).hexdigest(),
            invalid_diagnostics,
        )
    except ValueError:
        pass
    else:
        raise AssertionError('changed real-source diagnostic was accepted')
    assert all('search' in command for command, _, _ in invocations if 'search' in command)
    cli_commands = [command for command, _, _ in invocations if 'list' in command or 'search' in command]
    assert all('--config' in command and '--cwd' in command for command in cli_commands)
    assert all(env['XDG_CACHE_HOME'].startswith(str(cache_dir.resolve())) for _, env, _ in invocations)

# Observe the live verifier at its recorded CLI diagnostic boundary.
live_spec = importlib.util.spec_from_file_location(
    "reranker_live", root / "scripts/verify-reranker-live.py"
)
live = importlib.util.module_from_spec(live_spec)
live_spec.loader.exec_module(live)
assert live.diagnostic("reranking: missing_api_key; using lexical order\n") == "missing_api_key"
assert live.diagnostic("reranking: runtime_timeout; using lexical order\n") == "runtime_timeout"
assert live.diagnostic("") is None
assert live.diagnostic("reranking: unknown; using lexical order\n") == "unexpected_diagnostic_output"
assert live.diagnostic("reranking: missing_api_key; raw-secret\n") == "unexpected_diagnostic_output"

# A one-query smoke receipt cannot claim complete library profile coverage.
expected_rows = [
    {"id": "one:1", "query": "SQLite", "relevant": ["sqlite"]},
    {"id": "two:1", "query": "no matching skill", "relevant": []},
]
library_receipt = {
    "backend": "jev", "profile_version": 1, "status": "passed", "metadata_preserved": True,
    "rankings": [
        {**expected_rows[0], "ranked": ["sqlite"], "latency_ms": 10.0,
         "diagnostic": None, "metadata_preserved": True},
        {**expected_rows[1], "ranked": [], "latency_ms": 0.1,
         "diagnostic": None, "metadata_preserved": True},
    ],
}
assert len(live.library_rankings(library_receipt, "jev", {"sqlite"}, expected_rows)) == 2
for field, value in (("rankings", library_receipt["rankings"][:1]),
                     ("metadata_preserved", False), ("profile_version", True)):
    invalid = copy.deepcopy(library_receipt)
    invalid[field] = value
    try:
        live.library_rankings(invalid, "jev", {"sqlite"}, expected_rows)
    except ValueError:
        pass
    else:
        raise AssertionError(f"invalid library {field} accepted")
for field, value in (("ranked", ["unknown"]), ("ranked", ["sqlite", "sqlite"]),
                     ("latency_ms", math.nan), ("diagnostic", "authentication"),
                     ("metadata_preserved", False), ("query", "changed task")):
    invalid = copy.deepcopy(library_receipt)
    invalid["rankings"][0][field] = value
    try:
        live.library_rankings(invalid, "jev", {"sqlite"}, expected_rows)
    except ValueError:
        pass
    else:
        raise AssertionError(f"invalid library row {field} accepted")
print('Benchmark metric, profile, and live receipt contracts passed')
