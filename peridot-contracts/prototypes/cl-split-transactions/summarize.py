#!/usr/bin/env python3
"""Summarize preserved split evidence without converting failed gates to approval."""
import argparse, collections, hashlib, json, re, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
from cl_proof_evidence import cargo_outcomes

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def sdk(row):
    keys = ['instructions', 'memory_bytes', 'write_entries', 'read_bytes',
            'write_bytes', 'contract_events_size_bytes']
    return {**{k: row[k] for k in keys},
            'sdk_entries': sum(row[k] for k in ['read_entries', 'disk_read_entries', 'write_entries']),
            'ok': row['ok']}

def transaction(row):
    return {'resources': row.get('resources'), 'fees_stroops': row.get('fees'),
            'unique_footprint_keys': sum(len(v) for v in row.get('footprint', {}).values()),
            'authorization_size_bytes': row.get('authorization_size_bytes'),
            'return_value_size_bytes': row.get('return_value_size_bytes'),
            'contract_events_size_bytes': row.get('contract_events_size_bytes'),
            'outcome': row.get('outcome'), 'limits_pass': row.get('limits_pass'),
            'proof_case': row.get('proof_case')}

def summarize(run):
    log = (run / 'tests.log').read_text()
    outcomes = cargo_outcomes(log)
    assert outcomes and all(v in ('ok', 'FAILED') for v in outcomes.values()), 'unfinished tests'
    rows = [json.loads(s) for s in (run / 'resources.jsonl').read_text().splitlines() if s.strip()]
    txs = [json.loads(s) for s in (run / 'transactions.jsonl').read_text().splitlines() if s.strip()]
    per_test, test = {}, None
    logged_rows = []
    for line in log.splitlines():
        m = re.match(r'^test ([A-Za-z0-9_:]+) \.\.\. ', line)
        if m: test = m[1]
        if line.startswith('{'):
            row = json.loads(line)
            if 'flow' in row and 'instructions' in row:
                assert test is not None
                logged_rows.append(row)
                per_test.setdefault(test, []).append(row)
    assert logged_rows == rows, 'raw report differs from test log'
    stages = {}
    for test, measured in per_test.items():
        if test.endswith('::compiled_split_open_complete'):
            names = ['transfer_spot_to_margin', 'begin_open_position_v3', 'split_fund',
                     'split_convert', 'split_prepare', 'split_mint', 'split_deliver',
                     'activate_open_position_v3', 'prepare_close_position_v3',
                     'swap_close_position_v3', 'finish_close_position_v3']
            for name in names:
                rs = [r for r in measured if r['flow'] == name]
                ts = [t for t in txs if t.get('test_name') == test and t['flow'] == name
                      and t.get('expected_rejection') is not True]
                if rs:
                    stages[name] = {'sdk': sdk(rs[0]), 'unsigned': transaction(ts[0]) if ts else None}
    full_lifecycle = outcomes.get('test::split_tests::compiled_split_open_complete') == 'ok' and len(stages) == 11
    lifecycle = {'completed': full_lifecycle, 'transaction_count': len(stages),
                 'signature_verification_measured': False, 'live_state_validation': False,
                 'network_latency_measured': False}
    call_profiles = [json.loads(s) for s in (run / 'calls.jsonl').read_text().splitlines() if s.strip()]
    for name, stage in stages.items():
        matches = [c for c in call_profiles if c['flow'] == name
                   and c['root_instructions'] == stage['sdk']['instructions']]
        if matches:
            c = matches[0]; roles = {}
            for call in c['calls']:
                role = call.get('role', 'unlabelled')
                totals = roles.setdefault(role, {'exclusive_instructions': 0, 'exclusive_memory_counter_bytes': 0})
                totals['exclusive_instructions'] += call['exclusive']['instructions']
                totals['exclusive_memory_counter_bytes'] += call['exclusive']['memory_counter_contribution_bytes']
            stage['call_breakdown'] = {'roles': roles, 'memory_attribution': c['memory_attribution'],
                'aquarius_calls': [{k: call[k] for k in ['depth', 'function', 'inclusive', 'exclusive']}
                                  for call in c['calls'] if call.get('role') == 'aquarius']}
    if full_lifecycle:
        assert all(s['unsigned']['outcome'] == 'succeeded' for s in stages.values())
        for label, selected in [('open_including_margin_transfer', list(stages)[:8]),
                                ('open_and_close_including_margin_transfer', list(stages))]:
            lifecycle[label] = {'transactions': len(selected),
                'fees_stroops': {k: sum(stages[n]['unsigned']['fees_stroops'][k] for n in selected)
                                for k in ['inclusion', 'non_refundable', 'refundable', 'rent', 'total']},
                'sdk_cpu_sum': sum(stages[n]['sdk']['instructions'] for n in selected)}
        quote = [t for t in txs if t.get('test_name') == 'test::split_tests::compiled_split_open_complete'
                 and t['flow'] == 'estimate_swap']
        lifecycle['separate_unsigned_quote_views'] = [transaction(t) for t in quote]
        lifecycle['synthetic_test_latency_seconds'] = int(re.search(r'SPLIT_LIFECYCLE .*test_ledger_latency_seconds=(\d+)', log)[1])
    selected_tests = {}
    for test, measured in per_test.items():
        selected_tests[test] = {'outcome': outcomes[test], 'flows': {}}
        for row in measured:
            name = row['flow']
            entry = selected_tests[test]['flows'].setdefault(name, {'measurements': []})
            item = sdk(row)
            if item not in entry['measurements']: entry['measurements'].append(item)
    maxima = {key: max(sdk(r)[key] for r in rows) for key in
              ['instructions', 'memory_bytes', 'sdk_entries', 'write_entries', 'read_bytes',
               'write_bytes', 'contract_events_size_bytes']}
    return {'profile': json.loads((run / 'profile.json').read_text()),
            'test_exit': int((run / 'test-exit-code.txt').read_text()),
            'invocation_checker_exit': int((run / 'budget-exit-code.txt').read_text()),
            'outcomes': outcomes, 'test_counts': dict(collections.Counter(outcomes.values())),
            'gates': json.loads((run / 'gates.json').read_text()),
            'sdk_rows': len(rows), 'unsigned_rows': len(txs), 'maxima': maxima,
            'stage1_complete': False, 'release_approval': False,
            'core_stages': stages, 'core_lifecycle': lifecycle, 'test_measurements': selected_tests,
            'raw_hashes': {p.name: digest(p) for p in run.iterdir() if p.is_file()}}

def main():
    p = argparse.ArgumentParser();p.add_argument('run', type=Path);p.add_argument('output', type=Path)
    a = p.parse_args();assert not a.output.exists(), 'refusing existing summary'
    a.output.write_text(json.dumps(summarize(a.run), indent=2) + '\n')

if __name__ == '__main__': main()
