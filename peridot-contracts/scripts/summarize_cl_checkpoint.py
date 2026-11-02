#!/usr/bin/env python3
"""Summarize a completed controlled-state runner without replacing its gates.

Requires the runner's terminal status, both profiles, all three compiled-suite
logs, native/integrity logs, gate summaries, proof manifests and eight WASMs.
Failed execution remains failed; missing measurements are never replaced by zero.
No network, identity, signing or submission access.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

PROFILES = ('internal-policy', 'captured-network-diagnostic')
SUITES = ('integrity', 'harness', 'archive', 'history-scale-archive', 'native-funded')
ARTIFACTS = ('cl_integration_fixtures', 'cl_share_vault', 'cl_share_pool_facade',
             'margin_controller', 'swap_adapter', 'receipt_vault',
             'simple_peridottroller', 'jump_rate_model')
GATES = ('invocation', 'lifecycle', 'transaction', 'checkpoint')
FEES = ('inclusion', 'non_refundable', 'refundable', 'rent', 'total')
RESULT = re.compile(r'test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; '
                    r'(\d+) ignored; (\d+) measured; (\d+) filtered out;')


def require(condition, message):
    if not condition:
        raise ValueError(message)


def read(path):
    require(path.is_file(), f'missing required file: {path}')
    data = path.read_bytes()
    require(bool(data), f'empty required file: {path}')
    return data


def digest(path):
    return hashlib.sha256(read(path)).hexdigest()


def document(path):
    return json.loads(read(path))


def rows(path):
    result = [json.loads(line) for line in read(path).splitlines() if line.strip()]
    require(bool(result), f'empty measurement report: {path}')
    require(all(isinstance(row, dict) for row in result), f'malformed rows: {path}')
    return result


def counts(path):
    matches = RESULT.findall(read(path).decode())
    require(bool(matches), f'missing terminal test result: {path}')
    sections = [dict(zip(('passed', 'failed', 'ignored', 'measured', 'filtered_out'),
                         map(int, match[1:])), result=match[0]) for match in matches]
    require(any(section['passed'] + section['failed'] for section in sections),
            f'no tests executed: {path}')
    require(all((s['failed'] == 0) == (s['result'] == 'ok') for s in sections),
            f'inconsistent test result: {path}')
    return {'passed': sum(s['passed'] for s in sections),
            'failed': sum(s['failed'] for s in sections),
            'ignored': sum(s['ignored'] for s in sections),
            'sections': sections, 'sha256': digest(path)}


def number(value, name):
    require(type(value) is int and value >= 0, f'missing/invalid numeric field: {name}')
    return value


def sdk_summary(records):
    fields = ('instructions', 'memory_bytes', 'read_entries', 'write_entries',
              'disk_read_entries', 'read_bytes', 'write_bytes',
              'contract_events_size_bytes', 'persistent_rent_ledger_bytes',
              'persistent_entry_rent_bumps', 'temporary_rent_ledger_bytes',
              'temporary_entry_rent_bumps')
    for row in records:
        for key in fields:
            number(row.get(key), key)
        require(type(row.get('ok')) is bool, 'SDK row missing success/failure')
        number(row.get('runtime_protocol'), 'runtime_protocol')
    return {'rows': len(records), 'failed_rows': sum(not r['ok'] for r in records),
            'max': {key: max((r[key] for r in records), default=None) for key in fields},
            'sdk_entries_max': max((r['read_entries'] + r['write_entries'] +
                                    r['disk_read_entries'] for r in records), default=None),
            'runtime_protocols': sorted({r.get('runtime_protocol') for r in records})}


def key_names(encoded_keys):
    """Decode only public LedgerKey XDR with the same SDK used by the verifier."""
    if not encoded_keys:
        return {'available': True, 'names': {}}
    module = Path(__file__).resolve().parent.parent / 'bots/margin-liquidator/node_modules/@stellar/stellar-sdk'
    code = r'''
const fs = require('node:fs');
const { xdr } = require(process.argv[1]);
const out = {};
for (const encoded of JSON.parse(fs.readFileSync(0, 'utf8'))) {
  const key = xdr.LedgerKey.fromXDR(encoded, 'base64');
  if (key.toXDR('base64') !== encoded) throw Error('non-canonical ledger key');
  let name = null;
  if (key.switch().name === 'contractData') {
    const val = key.contractData().key();
    const head = val.switch().name === 'scvVec' ? val.vec()?.[0] : val;
    if (head?.switch().name === 'scvSymbol') name = head.sym().toString();
  }
  out[encoded] = name;
}
process.stdout.write(JSON.stringify(out));
'''
    try:
        result = subprocess.run(['node', '-e', code, str(module)],
                                input=json.dumps(sorted(encoded_keys)), text=True,
                                capture_output=True, timeout=30, check=False)
        require(result.returncode == 0, result.stderr.strip())
        return {'available': True, 'names': json.loads(result.stdout)}
    except (OSError, subprocess.SubprocessError, ValueError) as error:
        # Exact serialized keys remain in the summary. Never infer a symbolic
        # historical key name from a substring or an unverified index.
        return {'available': False, 'names': {}, 'error': str(error)}


def transaction_summary(records):
    measured, incomplete, restored = [], [], []
    for index, row in enumerate(records):
        require(type(row.get('success')) is bool, 'transaction missing outcome')
        require(type(row.get('expected_rejection')) is bool, 'transaction missing rejection classification')
        require(not (row['expected_rejection'] and row['success']), 'expected rejection unexpectedly succeeded')
        if 'resources' not in row or 'fees' not in row:
            require(row['success'] is False and bool(row.get('error')),
                    'successful transaction is missing resources/fees')
            incomplete.append({'row': index, 'flow': row.get('flow'), 'error': row['error']})
            continue
        for key in ('instructions', 'memory_bytes', 'transaction_size_bytes',
                    'write_entries', 'disk_read_entries', 'disk_read_bytes',
                    'write_bytes', 'contract_events_size_bytes'):
            number(row['resources'].get(key), key)
        for key in FEES:
            number(row['fees'].get(key), 'fees.' + key)
        require(isinstance(row.get('fee_inputs'), dict) and isinstance(row.get('rent_fee_inputs'), dict),
                'measured transaction missing fee configuration')
        require(row['fees']['total'] == sum(row['fees'][key] for key in FEES if key != 'total'),
                'transaction fee components do not sum to total')
        footprint = row['footprint']
        require(isinstance(footprint['read_only_xdr'], list) and isinstance(footprint['read_write_xdr'], list),
                'transaction missing serialized footprint')
        measured.append(row)
        indices = row['restored_rw_entry_indices']
        if indices:
            require(row['archive']['restoration_observed'] is True, 'restoration flag/index mismatch')
            selected = {key['key_xdr'] for key in row['archive']['modified_keys']}
            exact = []
            for entry_index in indices:
                require(type(entry_index) is int and 0 <= entry_index < len(footprint['read_write_xdr']),
                        'invalid restored footprint index')
                key = footprint['read_write_xdr'][entry_index]
                require(key in selected, 'restored key not in deliberately archived set')
                exact.append(key)
            restored.append({'row': index, 'flow': row['flow'], 'test_name': row['test_name'],
                             'key_xdr': sorted(set(exact)), 'resources': row['resources'],
                             'fees': row['fees'], 'baseline_fees': row['archive'].get('baseline_fees'),
                             'return_matches_baseline': row['archive'].get('return_matches_baseline'),
                             'contract_changes_match_baseline': row['archive'].get('contract_changes_match_baseline')})
    named = key_names({key for row in restored for key in row['key_xdr']})
    for row in restored:
        row['restored_key_names'] = sorted({named['names'][key] for key in row['key_xdr'] if named['names'].get(key)})
        row['historical_key_xdr'] = [key for key in row['key_xdr'] if (named['names'].get(key) or '').startswith('History')]
        if row['baseline_fees'] is not None:
            row['fee_delta_from_unarchived_baseline'] = {key: row['fees'][key] - row['baseline_fees'][key] for key in FEES}
    resource_fields = ('instructions', 'memory_bytes', 'transaction_size_bytes', 'write_entries',
                       'disk_read_entries', 'disk_read_bytes', 'write_bytes', 'contract_events_size_bytes')
    return {
        'rows': len(records), 'successful': sum(r['success'] for r in records),
        'expected_rejections': sum(r['expected_rejection'] for r in records),
        'unexpected_failures': sum(not r['success'] and not r['expected_rejection'] for r in records),
        'measured_rows': len(measured), 'unmeasured_failures': incomplete,
        'max_resources': {key: max((r['resources'][key] for r in measured), default=None) for key in resource_fields},
        'max_unique_footprint_entries': max((len(set(r['footprint']['read_only_xdr'] + r['footprint']['read_write_xdr'])) for r in measured), default=None),
        'max_authorization_bytes': max((number(r.get('authorization_size_bytes'), 'authorization_size_bytes') for r in measured), default=None),
        'max_return_bytes': max((number(r.get('return_value_size_bytes'), 'return_value_size_bytes') for r in measured), default=None),
        'max_fee_components_stroops': {key: max((r['fees'][key] for r in measured), default=None) for key in FEES},
        'fee_rate_configurations': [json.loads(s) for s in sorted({json.dumps(
            {'fee_inputs': r['fee_inputs'], 'rent_fee_inputs': r['rent_fee_inputs']}, sort_keys=True) for r in measured})],
        'fee_component_maxima_scope': 'Independent maxima; not the fee components of one transaction.',
        'restored_transactions': len(restored), 'restored_variants': restored,
        'restored_key_decoding': named,
    }


def summarize(run, runner_log):
    completion = read(runner_log).decode()
    match = re.search(r'Checkpoint: (.+)\nStatus: ([01]) \(missing proofs remain failures\)\s*\Z', completion)
    require(match is not None, 'runner log lacks terminal checkpoint/status: runner is incomplete')
    require(Path(match[1]).resolve() == run, 'runner completion belongs to a different directory')
    artifacts = {}
    for name in ARTIFACTS:
        path = run / 'artifacts' / (name + '.wasm')
        require(read(path).startswith(b'\0asm\1\0\0\0'), f'invalid WASM header: {path}')
        build = run / 'logs' / ('build-' + name.replace('_', '-') + '.log')
        require('Build Complete' in read(build).decode(), f'incomplete build: {build}')
        artifacts[path.name] = {'sha256': digest(path), 'bytes': path.stat().st_size}
    # Default and historical receipt fixtures are separate executable identities.
    additional = {}
    for role, relative, log in [
        ('default_margin', 'default-artifacts/margin_controller.wasm', 'logs/build-margin-controller-default.log'),
        ('legacy_receipt', 'legacy-receipt/artifacts/receipt_vault_legacy_abi.wasm', 'legacy-receipt/build.log'),
    ]:
        path = run / relative
        require(read(path).startswith(b'\0asm\1\0\0\0'), f'invalid WASM header: {path}')
        require('Build Complete' in read(run / log).decode(), f'incomplete build: {log}')
        additional[role] = {'path': relative, 'sha256': digest(path), 'bytes': path.stat().st_size}
    legacy = document(run / 'legacy-receipt/manifest.json')
    require(legacy['source_commit'] == '47858c1a13c2588970395faa2212e7b68bd33060', 'historical receipt source mismatch')
    require(legacy['wasm_sha256'] == additional['legacy_receipt']['sha256'] and
            legacy['wasm_bytes'] == additional['legacy_receipt']['bytes'], 'historical receipt artifact mismatch')
    result = {'schema': 'cl-completed-execution-summary-v1', 'run_directory': str(run),
              'runner_log_sha256': digest(runner_log), 'runner_exit_status': int(match[2]),
              'scope': 'Compiled controlled state; synthetic external authorization; unsigned target-runtime simulation. Not live state, real signatures or release approval.',
              'release_approval': False, 'artifacts': artifacts,
              'additional_fixture_artifacts': additional, 'legacy_receipt_build': legacy,
              'native_default_margin_tests': counts(run / 'logs/default-margin-tests.log'),
              'network_config_sha256': digest(run / 'network-config.json'),
              'native_reward_tests': counts(run / 'logs/reward-tests.log'), 'profiles': {}}
    for profile in PROFILES:
        suites = {suite: counts(run / 'logs' / f'{profile}-{suite}.log') for suite in SUITES}
        gate_path = run / f'{profile}-checkpoint-summary.json'
        gate = document(gate_path)
        require(gate['profile'] == profile and gate['config_sha256'] == result['network_config_sha256'], 'gate profile/config mismatch')
        require(set(gate['gates']) == set(GATES) and all(type(v) is bool for v in gate['gates'].values()), 'missing/malformed gates')
        require(gate['release_approval'] is False, 'summary must not imply release approval')
        for key in ('missing_required_flows', 'lifecycle_errors', 'transaction_errors'):
            require(isinstance(gate.get(key), list), 'missing gate errors: ' + key)
        checkpoint_log = read(run / 'logs' / f'{profile}-checkpoint.log').decode()
        for name, passed in gate['gates'].items():
            require(f'{name.upper()} GATE: {"PASS" if passed else "FAIL"}' in checkpoint_log, 'gate summary/log disagreement')
        require('CHECKPOINT GATE:' in read(run / 'logs' / f'{profile}-native-funded-budget.log').decode(), 'incomplete native budget log')
        setting = document(run / f'{profile}.json')
        require(setting['name'] == profile and setting['network_sha256'] == result['network_config_sha256'], 'profile/config mismatch')
        manifest = document(run / f'{profile}-proof-manifest.json')
        require(manifest['profile'] == profile and manifest['resource_report_sha256'] == digest(run / f'{profile}-resources.jsonl'), 'proof/resource mismatch')
        bound_logs = {Path(item['path']).name: item for item in manifest['test_runs']}
        for suite in ('harness', 'archive', 'history-scale-archive'):
            name = f'{profile}-{suite}.log'
            require(name in bound_logs and bound_logs[name]['sha256'] == suites[suite]['sha256'], 'proof/test-log mismatch: ' + name)
            require((bound_logs[name]['exit_code'] == 0) == (suites[suite]['failed'] == 0), 'test exit/result mismatch: ' + name)
        sdk = rows(run / f'{profile}-resources.jsonl')
        calls = rows(run / f'{profile}-calls.jsonl')
        tx = rows(run / f'{profile}-transactions.jsonl')
        require(all(r['profile'] == profile for r in sdk + tx), 'mixed profile evidence')
        history_sdk = [r for r in sdk if r['flow'].startswith('history_scale_')]
        history_tx = [r for r in tx if '::history_scale_tests::' in r['test_name']]
        result['profiles'][profile] = {'limits': setting['limits'], 'gate_summary': gate,
            'gate_summary_sha256': digest(gate_path), 'test_suites': suites,
            'sdk': sdk_summary(sdk), 'unsigned_transactions': transaction_summary(tx),
            'call_profile_rows': len(calls),
            'native_funded_sdk_separate': sdk_summary(rows(run / f'{profile}-native-funded.jsonl')),
            'history_scale': {'sdk': sdk_summary(history_sdk),
                'sdk_rows': history_sdk, 'unsigned_transactions': transaction_summary(history_tx)}}
    require(result['runner_exit_status'] != 0 or all(all(p['gate_summary']['gates'].values())
            for p in result['profiles'].values()), 'runner status zero contradicts failed gates')
    return result


def markdown(summary):
    out = ['# Completed CL execution evidence', '', summary['scope'], '',
           f"Runner exit: **{summary['runner_exit_status']}**. Stage 1 remains incomplete; release approval: **false**.", '',
           '| Profile | Invocation | Lifecycle | Transaction | Checkpoint |',
           '|---|---|---|---|---|']
    for name, data in summary['profiles'].items():
        out.append('| ' + name + ' | ' + ' | '.join('PASS' if data['gate_summary']['gates'][g] else 'FAIL' for g in GATES) + ' |')
    out += ['', '| Profile | Suite | Passed | Failed | Ignored |', '|---|---|---:|---:|---:|']
    for name, data in summary['profiles'].items():
        for suite, count in data['test_suites'].items():
            out.append(f"| {name} | {suite} | {count['passed']} | {count['failed']} | {count['ignored']} |")
    out += ['', '| Profile | SDK CPU max | SDK memory max | SDK entries max | Writes max |', '|---|---:|---:|---:|---:|']
    for name, data in summary['profiles'].items():
        sdk = data['sdk']
        out.append(f"| {name} | {sdk['max']['instructions']} | {sdk['max']['memory_bytes']} | {sdk['sdk_entries_max']} | {sdk['max']['write_entries']} |")
    for name, data in summary['profiles'].items():
        tx = data['unsigned_transactions']
        out += ['', f'## {name}', '',
                f"Unsigned transactions: {tx['successful']} succeeded, {tx['expected_rejections']} expected rejections, {tx['unexpected_failures']} unexpected failures; {tx['restored_transactions']} restoration variants. {len(tx['unmeasured_failures'])} failures lack execution resource measurements.", '',
                'Fee component maxima (stroops; independent maxima, not one transaction):', '',
                '| Inclusion | Non-refundable | Refundable | Rent | Total |', '|---:|---:|---:|---:|---:|',
                '| ' + ' | '.join(str(tx['max_fee_components_stroops'][k]) for k in FEES) + ' |', '',
                f"History-scale SDK rows: {data['history_scale']['sdk']['rows']}; unsigned restoration variants: {data['history_scale']['unsigned_transactions']['restored_transactions']}. Exact restored key XDR, decoded historical names, resource numbers and fee deltas are preserved in the JSON summary.", '', 'Recorded gate errors:', '']
        for key in ('missing_required_flows', 'lifecycle_errors', 'transaction_errors'):
            errors = data['gate_summary'][key]
            out.append(f'- {key}: {len(errors)}')
            for error in errors:
                out.append('  - ' + str(error).replace('\n', ' '))
    out += ['', '## Separate WASM hashes', '', '| Artifact | Bytes | SHA256 |', '|---|---:|---|']
    for name, data in summary['artifacts'].items():
        out.append(f"| {name} | {data['bytes']} | `{data['sha256']}` |")
    for name, data in summary['additional_fixture_artifacts'].items():
        out.append(f"| {name} (separate fixture) | {data['bytes']} | `{data['sha256']}` |")
    return '\n'.join(out) + '\n'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('runner_directory', type=Path)
    parser.add_argument('--runner-log', required=True, type=Path)
    parser.add_argument('--output-prefix', type=Path,
                        default=Path('target/cl-next-publication/execution-summary'))
    args = parser.parse_args()
    try:
        summary = summarize(args.runner_directory.resolve(), args.runner_log)
        destinations = [Path(str(args.output_prefix) + suffix) for suffix in ('.json', '.md')]
        require(not any(p.exists() for p in destinations), 'refusing to overwrite an existing execution summary')
        args.output_prefix.parent.mkdir(parents=True, exist_ok=True)
        destinations[0].write_text(json.dumps(summary, indent=2) + '\n')
        destinations[1].write_text(markdown(summary))
        print('Summary written; recorded gate failures remain failures; release approval remains false.')
    except (OSError, KeyError, TypeError, ValueError) as error:
        print('INCOMPLETE/INVALID EVIDENCE: ' + str(error), file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
