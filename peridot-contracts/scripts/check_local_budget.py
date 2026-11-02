"""Validate explicitly selected resource profiles; missing evidence fails closed."""
import argparse
import json
import sys
from pathlib import Path

from cl_fee_estimate import estimate
from cl_resource_profiles import create

REQUIRED = {
    'deposit', 'redeem', 'transfer', 'emergency_redeem', 'claim_rewards',
    'begin_open', 'swap_open_position_v3',
    'activate_open_position_v3', 'prepare_close_position_v3',
    'swap_close', 'finish_close', 'funded_liquidation', 'repay_release',
    'claim_failure_recovery', 'cancel_before_execute', 'cancel_after_execute',
    'cancel_after_swap', 'receipt_custody_ownership_change',
    'allowance_transfer_from', 'waiver_redistribution', 'quarantine_reconciliation',
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
    missing = REQUIRED - seen
    if missing:
        print('UNMEASURED: ' + ', '.join(sorted(missing)))
        failed = True
    label = 'INTERNAL-POLICY' if profile['name'] == 'internal-policy' else 'CAPTURED-NETWORK-DIAGNOSTIC'
    print(f"{label}: {'FAIL' if failed else 'PASS'}")
    print('RELEASE GATE: FAIL — full transaction envelope/return/history/base fee evidence is unmeasured; '
          'release_approval=false. Diagnostic profile selection cannot grant release approval.')
    print('Scope: per-row fixture classification. Matching target runtime and captured cost model required. '
          'SDK key/entry/code size checks remain enforced; no live-state RPC simulation claim.')
    # This local checker cannot supply the absent full-transaction execution proof.
    return 1


if __name__ == '__main__':
    sys.exit(main())
