#!/usr/bin/env python3
"""Assess measured workloads against the memo, never create/change a profile."""
import argparse
import hashlib
import json
from pathlib import Path
import re

# Analysis-only 80% workload boundaries from memo 12. No configurable override.
BOUNDARIES = dict(instructions=240_000_000, memory_bytes=26_843_545,
                  sdk_entry_total=240, unique_footprint_keys=240, write_entries=80)
REQUIRED = {'swap_open_position_v3', 'activate_open_position_v3',
            'prepare_close_position_v3', 'swap_close_position_v3',
            'finish_close_position_v3', 'liquidate_funded_v3',
            'recover_pending_open_funded', 'repay_margin_position_v3',
            'release_debt_free_position_v3', 'emergency_redeem',
            'transfer_margin_to_spot', 'withdraw', 'claim_rewards'}


def rows(path):
    with path.open() as stream:
        for line in stream:
            if line.strip():
                yield json.loads(line)


def update(table, flow, values):
    dest = table.setdefault(flow, {'samples': 0, 'maxima': {}})
    dest['samples'] += 1
    for key, value in values.items():
        dest['maxima'][key] = max(dest['maxima'].get(key, 0), value)


def distances(table):
    for value in table.values():
        value['remaining_to_workload_boundary'] = {
            k: BOUNDARIES[k] - v for k, v in value['maxima'].items() if k in BOUNDARIES}
        value['measured_boundaries_fit'] = all(v >= 0 for v in value['remaining_to_workload_boundary'].values())


def assess(directory):
    gates = json.loads((directory / 'gates.json').read_text())
    config = json.loads((directory / 'run-configuration.json').read_text())
    log = (directory / 'tests.log').read_text()
    counts = re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored', log)
    if len(counts) != 1:
        raise ValueError('Missing or ambiguous completed test counts: ' + str(directory))
    profile = json.loads((directory / 'profile.json').read_text())
    sdk = {}; transactions = {}; totals = {}; auth_cases = {}; problems = []; successful = set()
    for row in rows(directory / 'resources.jsonl'):
        assert row['config_sha256'] == profile['network_sha256']
        assert row['profile'] == profile['name'] and row['runtime_protocol'] == 29
        assert row['sdk_limits_enforced'] and row['exact_captured_cost_model']
        update(sdk, row['flow'], dict(instructions=row['instructions'], memory_bytes=row['memory_bytes'],
              sdk_entry_total=row['read_entries'] + row['disk_read_entries'] + row['write_entries'],
              write_entries=row['write_entries'], write_bytes=row['write_bytes'],
              disk_read_bytes=row['read_bytes'], disk_read_entries=row['disk_read_entries'],
              contract_events_size_bytes=row['contract_events_size_bytes']))
    for n, row in enumerate(rows(directory / 'transactions.jsonl')):
        if 'resources' not in row:
            problems.append(f'row {n}: complete transaction quantities missing: {row.get("error")}')
            continue
        assert row['config_sha256'] == profile['network_sha256']
        assert row['runtime_protocol'] == 29 and row['exact_captured_cost_model']
        assert row['signature_verification_measured'] is False and row['live_state_validation'] is False
        if not row['success'] and not row.get('expected_rejection'):
            problems.append(f'row {n}: unexpected unsigned rejection in {row["flow"]}')
        if row['success']:
            successful.add(row['flow'])
        values = dict(row['resources'])
        values['unique_footprint_keys'] = len(set(row['footprint']['read_only_xdr'] + row['footprint']['read_write_xdr']))
        values.update({f'fee_{k}_stroops': v for k, v in row['fees'].items()})
        values['authorization_template_bytes'] = row['authorization_size_bytes']
        values['restored_entries'] = len(row['restored_rw_entry_indices'])
        update(transactions, row['flow'], values)
        test = row['test_name'].split('::')[-1]
        total = totals.setdefault(test, dict(transactions=0, fees_stroops=0, rent_stroops=0, restored_transactions=0))
        total['transactions'] += 1
        total['fees_stroops'] += row['fees']['total']
        total['rent_stroops'] += row['fees']['rent']
        total['restored_transactions'] += bool(row['restored_rw_entry_indices'])
        if row['archive']['enabled'] and row['success']:
            if not row['archive']['return_matches_baseline'] or not row['archive']['contract_changes_match_baseline']:
                problems.append(f'row {n}: restored business result differs')
        if row['flow'] in REQUIRED and row['success']:
            old = auth_cases.get(row['flow'])
            if old is None or row['resources']['instructions'] > old['unsigned_instructions']:
                auth_cases[row['flow']] = {
                    'row_zero_based': n, 'test_name': row['test_name'], 'contract_address': row['contract_address'],
                    'host_function_sha256': row['host_function_sha256'], 'snapshot_sha256': row['snapshot_sha256'],
                    'unsigned_envelope_sha256': row['envelope_sha256'],
                    'unsigned_instructions': row['resources']['instructions'],
                    'remaining_cpu_before_real_authorization': BOUNDARIES['instructions'] - row['resources']['instructions'],
                    'actual_external_addresses': row['expected_external_authorizers'],
                    'remaining_test': 'Regenerate this exact fixture/root with later-authorized synthetic G actors. Pair source credentials and distinct-source address credentials; real protocol29 transaction application plus signature/nonce/threshold checks. Current C-address mocks and recording templates cannot prove either class.',
                    'signing_performed': False,
                }
    for row in rows(directory / 'calls.jsonl'):
        if not row['exclusive_reconciles']:
            problems.append('unreconciled call profile: ' + row['flow'])
    distances(sdk); distances(transactions)
    missing = sorted(REQUIRED - successful)
    completed = {'passed': int(counts[0][0]), 'failed': int(counts[0][1]), 'ignored': int(counts[0][2])}
    test_exit = int((directory / 'test-exit-code.txt').read_text())
    return {
        'run': config, 'profile': profile['name'], 'test_exit': test_exit,
        'test_counts': completed, 'existing_gates': gates, 'sdk_by_flow': sdk, 'transaction_by_flow': transactions,
        'unsigned_evidence_record_fee_aggregates': totals,
        'fee_aggregate_scope': 'All recorded roots, including probes/getters/rejections. Archived roots independently expire selected entries; these sums are not one sequential user lifecycle.',
        'missing_targeted_successful_flows': missing, 'evidence_errors': problems,
        'complete_targeted_execution': not missing and not problems and test_exit == 0 and completed['failed'] == 0 and completed['passed'] == 4,
        'all_observed_workload_boundaries_fit': all(v['measured_boundaries_fit'] for t in (sdk, transactions) for v in t.values()),
        'remaining_authorization_cases': auth_cases,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', action='append', required=True, help='label=directory')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    assert not args.output.exists(), 'Refusing existing output'
    result = {'schema': 'cl-atomic-qualification-assessment-v1', 'stage1_complete': False,
              'release_approval': False, 'policy_change_implemented': False, 'signed_authorization_qualified': False,
              'boundaries_analysis_only': BOUNDARIES,
              'scope': 'Controlled-state executions only; independent maxima are not one combined transaction.', 'runs': {}}
    for item in args.run:
        label, path = item.split('=', 1)
        assert label not in result['runs']
        result['runs'][label] = assess(Path(path))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({'assessment_written': str(args.output), 'sha256': hashlib.sha256(args.output.read_bytes()).hexdigest(), 'release_approval': False}))


if __name__ == '__main__':
    main()
