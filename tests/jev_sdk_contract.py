#!/usr/bin/env python3
"""Opt-in real SDK wire-contract check; transport is mocked and never goes online."""

import json
import os
import sys
import tempfile
from pathlib import Path

os.environ['TYPESAFE_LOG_LEVEL'] = 'off'
import httpx2
from typesafe_sdk import Choice, RetryPolicy, TypeSafeClient

root = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(root / 'scripts'))
import benchmark_jev as jev
from benchmark_metrics import provenance, quality
from benchmark_profiles import canonical_hash

secret = 'sdk-test-secret-do-not-persist'
records = [
    {'name': 'package:first', 'description': 'First candidate'},
    {'name': 'package:second', 'description': 'Second candidate'},
]
profile = {
    'version': 1, 'kind': 'skillwick-lexical-profile',
    'corpus': {'total': 2, 'sha256': canonical_hash(records), 'records': records},
    'heldout': {'case_count': 1, 'query_count': 1, 'cases': [
        {'id': 'case', 'kind': 'positive', 'queries': ['do task'], 'relevant': ['package:second']},
    ]},
}
ranking = {'id': 'case:1', 'query': 'do task', 'relevant': ['package:second'],
           'ranked': ['package:first', 'package:second']}

with tempfile.TemporaryDirectory(prefix='skillwick-jev-sdk-') as directory:
    temporary = Path(directory)
    profile_path, candidates_path, output = (temporary / name for name in ('profile.json', 'candidates.json', 'result.json'))
    profile_path.write_text(json.dumps(profile))
    candidates_path.write_text(json.dumps({
        'version': 1, 'kind': 'skillwick-lexical-baseline',
        'candidate_pool_size': 20, 'profile_sha256': jev.digest(profile_path),
        'corpus_sha256': profile['corpus']['sha256'], 'corpus_total': 2,
        'provenance': provenance(), 'quality': quality(1, [ranking]), 'rankings': [ranking],
    }))

    for mode in ('success', 'authentication', 'rate_limit', 'server_error', 'timeout', 'malformed_response'):
        requests = []

        def handle(request):
            requests.append(request)
            assert request.method == 'POST'
            assert str(request.url) == 'https://api.typesafe.ai/v1/systemone'
            assert request.headers['authorization'] == f'Bearer {secret}'
            assert secret.encode() not in request.content
            body = json.loads(request.content)
            assert body['model'] == 'jev-latest'
            # The SDK is responsible for serializing typed questions.
            assert all(question['type'] == 'choice' for question in body['questions'].values())
            if mode == 'timeout':
                raise httpx2.ReadTimeout(secret, request=request)
            status = {'authentication': 401, 'rate_limit': 429, 'server_error': 503}.get(mode)
            if status:
                return httpx2.Response(status, json={'error': secret})
            if mode == 'malformed_response':
                return httpx2.Response(200, json={'bad': secret})
            return httpx2.Response(200, json={
                'model': 'jev-test-1', 'usage': {'input_tokens': 15, 'output_tokens': 0},
                'answers': {
                    key: {'type': 'choice', 'choice': 'not_relevant' if index == 0 else 'relevant',
                          'confidence': .8, 'probabilities': {'relevant': .1 if index == 0 else .9,
                                                           'not_relevant': .9 if index == 0 else .1}}
                    for index, key in enumerate(body['questions'])
                },
            })

        def factory(api_key, model, base_url, timeout):
            return (TypeSafeClient(api_key=api_key, model=model, base_url=base_url,
                                   timeout=timeout, retry=RetryPolicy(max_retries=0),
                                   transport=httpx2.MockTransport(handle)), Choice, jev.SDK_VERSION)

        status = jev.run_experiment(profile_path, candidates_path, output, live=True,
                                    environ={'TYPESAFE_API_KEY': secret}, client_factory=factory)
        result_text = output.read_text()
        assert secret not in result_text
        result = json.loads(result_text)
        assert len(requests) == 1, 'SDK retries must be disabled'
        wire = json.loads(requests[0].content)
        logged = result['rankings'][0]['request_trace']['payload']
        assert wire['state'] == logged['state']
        assert set(wire['questions']) == set(logged['questions'])
        for key, question in logged['questions'].items():
            assert wire['questions'][key]['instructions'] == question['instructions']
            assert wire['questions'][key]['criteria'] == question['criteria']
        if mode == 'success':
            assert status == 0
            assert result['rankings'][0]['ranked'] == ['package:second', 'package:first']
            assert result['rankings'][0]['resolved_model'] == 'jev-test-1'
        else:
            assert status != 0
            assert result['rankings'][0]['ranked'] == ranking['ranked']
            category = result['rankings'][0]['fallback_category']
            assert category == mode, (mode, category)

print('Pinned TypeSafe SDK wire and failure contracts passed (mock transport, no network)')
