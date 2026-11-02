"""Validate explicitly selected resource profiles; missing evidence fails closed."""
import argparse
import json
import sys
from pathlib import Path

from cl_fee_estimate import estimate
from cl_resource_profiles import create
from cl_proof_evidence import validate_manifest
from cl_transaction_evidence import validate_transactions

REQUIRED = {
    'deposit', 'redeem', 'transfer', 'emergency_redeem', 'claim_rewards',
    'begin_open', 'swap_open_position_v3',
    'activate_open_position_v3', 'prepare_close_position_v3',
    'swap_close', 'finish_close', 'funded_liquidation', 'repay_release',
    'claim_failure_recovery', 'cancel_before_execute', 'cancel_after_execute',
    'cancel_after_swap', 'receipt_custody_ownership_change',
    'allowance_transfer_from', 'waiver_redistribution', 'quarantine_reconciliation',
    'pending_claim_boundary_matrix', 'failed_activation_recovery',
    'expired_pending_before_swap', 'expired_pending_resolution',
    'both_reward_calls_failed_funded_liquidation',
    'funded_insufficient_liquidator_rollback', 'funded_invalid_valuation_rollback',
    'funded_failed_collateral_transfer_rollback', 'funded_unauthorized_rollback',
    'funded_repeated_execution_rollback', 'historical_multiple_intervals',
    'historical_unresolved_intervals',
    'historical_shortfall',
    'history_scale_64_owners', 'history_scale_8_unresolved',
    'reserve_complete_open', 'reserve_swap_rejections',
    'reserve_complete_open_rollback', 'reserve_no_liquidation_bypass',
}
MEASUREMENTS = {
    'instructions': 'instructions', 'memory_bytes': 'mem_bytes',
    'write_entries': 'write_entries', 'disk_read_entries': 'disk_read_entries',
    'read_bytes': 'disk_read_bytes', 'write_bytes': 'write_bytes',
    'contract_events_size_bytes': 'contract_events_size_bytes',
}
NUMBERS = set(MEASUREMENTS) | {
    'read_entries', 'persistent_rent_ledger_bytes', 'persistent_entry_rent_bumps',
    'temporary_rent_ledger_bytes', 'temporary_entry_rent_bumps',
    'diagnostic_shadow_cpu_used', 'diagnostic_shadow_memory_used',
}


def load_profile(path):
    profile = json.loads(Path(path).read_text())
    expected = create(profile['network_config_path'], profile['name'], profile['mode'])
    if profile != expected:
        raise ValueError('profile differs from validated captured configuration/internal policy')
    return profile


def validate_row(row, profile):
    errors = []
    for key in NUMBERS:
        if type(row.get(key)) is not int or row[key] < 0:
            errors.append('missing/invalid ' + key)
    for key, value in {
        'profile': profile['name'], 'run_mode': 'acceptance',
        'runtime_protocol': profile['target_protocol'],
        'target_protocol': profile['target_protocol'],
        'config_sha256': profile['network_sha256'],
        'sdk_limits_enforced': True, 'diagnostic_shadow_ok': True,
        'exact_captured_cost_model': True,
    }.items():
        if row.get(key) != value or type(row.get(key)) is not type(value):
            errors.append('missing/mismatched ' + key)
    if row.get('ok') is not True:
        errors.append('invocation/proof failed')
    if not isinstance(row.get('scope'), str) or not row['scope']:
        errors.append('fixture scope unclassified')
    if errors:
        return errors
    limits = profile['limits']
    for field, limit in MEASUREMENTS.items():
        if row[field] > limits[limit]:
            errors.append(f'{field}={row[field]}>{limits[limit]}')
    # Match the pinned SDK invocation checker, including its write-entry term.
    entries = row['read_entries'] + row['write_entries'] + row['disk_read_entries']
    if entries > limits['ledger_entries']:
        errors.append(f'footprint_entries={entries}>{limits["ledger_entries"]}')
    for used, ceiling in [('diagnostic_shadow_cpu_used', 'diagnostic_shadow_cpu_limit'),
                          ('diagnostic_shadow_memory_used', 'diagnostic_shadow_memory_limit')]:
        if type(row.get(ceiling)) is not int or row[ceiling] <= 0 or row[used] > row[ceiling]:
            errors.append('diagnostic snapshot incomplete: ' + used)
    return errors


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('report', type=Path)
    parser.add_argument('--profile', required=True, type=Path,
                        help='validated profile manifest, never an implicit limit switch')
    parser.add_argument('--proof-manifest', type=Path,
                        help='explicit proof-to-test/source/log mapping; invocation rows alone are not lifecycle proofs')
    parser.add_argument('--transaction-evidence', type=Path,
                        help='unsigned target-runtime e2e simulation JSONL with serialized XDR and complete fees')
    parser.add_argument('--gate', choices=['invocation', 'lifecycle', 'transaction', 'checkpoint'],
                        default='checkpoint', help='independent evidence gate, never release approval')
    parser.add_argument('--summary-json', type=Path, help='write all gate outcomes without suppressing failures')
    args = parser.parse_args(argv)
    try:
        profile = load_profile(args.profile)
        rows = [json.loads(line) for line in args.report.read_text().splitlines() if line.strip()]
    except (OSError, KeyError, TypeError, ValueError) as error:
        print('CONFIGURATION/EVIDENCE FAIL: ' + str(error))
        return 1
    failed = profile['mode'] != 'acceptance'
    print(f"PROFILE {profile['name']} mode={profile['mode']} captured_ledger={profile['captured_ledger']}")
    if failed:
        print('FAIL: profiling-only execution is not acceptance evidence')
    seen = set()
    valid_flows = set()
    for index, row in enumerate(rows):
        if not isinstance(row, dict) or not isinstance(row.get('flow'), str):
            print(f'ROW {index}: FAIL malformed flow')
            failed = True
            continue
        seen.add(row['flow'])
        errors = validate_row(row, profile)
        entries = sum(row.get(key, 0) for key in ('read_entries', 'write_entries', 'disk_read_entries')
                      if type(row.get(key, 0)) is int)
        print(f"{row['flow']}: {'FAIL' if errors else 'PASS'} cpu={row.get('instructions')} "
              f"memory={row.get('memory_bytes')} entries={entries} writes={row.get('write_entries')} "
              f"read_bytes={row.get('read_bytes')} write_bytes={row.get('write_bytes')} "
              f"scope={row.get('scope')} runtime_protocol={row.get('runtime_protocol')}")
        for error in errors:
            print('  ' + error)
        try:
            fee = estimate(row, profile['network_config_path'])
            print('  partial_fee_estimate=' + json.dumps(fee, sort_keys=True))
        except (KeyError, TypeError, ValueError, OSError, ZeroDivisionError) as error:
            print(f'  FEE UNMEASURED: {error}')
            errors.append('fee unmeasured')
        failed |= bool(errors)
        if not errors:
            valid_flows.add(row['flow'])
    mapped_labels = seen
    if args.proof_manifest:
        try:
            mapped_labels = {p['label'] for p in json.loads(args.proof_manifest.read_text())['proofs']}
        except (OSError, KeyError, TypeError, ValueError):
            mapped_labels = set()
    missing = REQUIRED - mapped_labels
    if missing:
        print('UNMEASURED: ' + ', '.join(sorted(missing)))
    # Coverage belongs to lifecycle completeness, not numerical invocation limits.
    # Empty or profiling-only reports nevertheless cannot pass invocation checks.
    invocation_ok = not failed and bool(rows)
    proof_errors = validate_manifest(args.proof_manifest, args.report, profile, valid_flows, REQUIRED)
    if missing:
        proof_errors.append('required invocation coverage missing: ' + ', '.join(sorted(missing)))
    lifecycle_ok = not proof_errors and profile['mode'] == 'acceptance'
    transaction_errors = validate_transactions(args.transaction_evidence, profile, REQUIRED, args.proof_manifest)
    transaction_ok = not transaction_errors and profile['mode'] == 'acceptance'
    gates = {'invocation': invocation_ok, 'lifecycle': lifecycle_ok, 'transaction': transaction_ok}
    gates['checkpoint'] = all(gates.values())
    label = 'INTERNAL-POLICY' if profile['name'] == 'internal-policy' else 'CAPTURED-NETWORK-DIAGNOSTIC'
    print(f"{label}: {'PASS' if gates['checkpoint'] else 'FAIL'}")
    for gate, ok in gates.items():
        print(f"{gate.upper()} GATE: {'PASS' if ok else 'FAIL'}")
    for error in proof_errors:
        print('  LIFECYCLE: ' + error)
    for error in transaction_errors:
        print('  TRANSACTION: ' + error)
    print('RELEASE APPROVAL: NOT GRANTED — resource profiles and controlled-state evidence do not authorize release.')
    print('Scope: matching target runtime/cost model and SDK invocation limits remain required. '
          'Unsigned transaction evidence uses recording-only external authorization, not real account signatures. '
          'Controlled-state execution is not live-state validation.')
    if args.summary_json:
        args.summary_json.write_text(json.dumps({
            'schema': 'cl-verification-gates-v1', 'profile': profile['name'],
            'config_sha256': profile['network_sha256'], 'gates': gates,
            'selected_gate': args.gate, 'release_approval': False,
            'missing_required_flows': sorted(missing), 'lifecycle_errors': proof_errors,
            'transaction_errors': transaction_errors,
        }, indent=2) + '\n')
    return 0 if gates[args.gate] else 1


if __name__ == '__main__':
    sys.exit(main())
