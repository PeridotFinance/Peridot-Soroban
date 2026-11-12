#!/usr/bin/env python3
"""Replay exact gates/errors; aggregate nonzero exit alone is never evidence."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def replay(item, out):
    label, directory = item.split('=', 1)
    assert label and all(c.isalnum() or c in '-_' for c in label)
    source = Path(directory).resolve()
    dest = out / label
    dest.mkdir(exist_ok=False)
    inputs = ['resources.jsonl', 'profile.json', 'transactions.jsonl',
              'proof-manifest.json', 'tests.log', 'calls.jsonl', 'gates.json']
    pins = {name: digest(source / name) for name in inputs}
    old = json.loads((source / 'gates.json').read_text())
    expected = json.loads(json.dumps(old))
    expected['selected_gate'] = 'checkpoint'
    # Remove only the two old-decoder errors for explicitly repaired rows whose
    # exporter already proved baseline equality. The new decoder independently
    # validates every marker against actual entry/footprint XDR before accepting.
    removed = []
    for i, line in enumerate((source / 'transactions.jsonl').open()):
        row = json.loads(line)
        if (row.get('recording_projection_repair', {}).get('schema') ==
                'host29-contract-footprint-placeholder-projection-v1'
                and row.get('archive', {}).get('contract_changes_match_baseline') is True):
            for error in [f'transaction row {i}: serialized transaction mismatch: post_state_data_sha256',
                          f'transaction row {i}: archive simulation changed contract state or historical entitlements']:
                if error in expected['transaction_errors']:
                    expected['transaction_errors'].remove(error)
                    removed.append(error)
    command = ['bash', 'scripts/check_budget.sh', '--local-report', str(source / 'resources.jsonl'),
               '--profile', str(source / 'profile.json'), '--transaction-evidence', str(source / 'transactions.jsonl'),
               '--proof-manifest', str(source / 'proof-manifest.json'), '--gate', 'checkpoint',
               '--summary-json', str(dest / 'gates.json')]
    with (dest / 'budget.log').open('w') as log:
        process = subprocess.run(command, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT)
    actual = json.loads((dest / 'gates.json').read_text())
    expected_exit = 0 if expected['gates']['checkpoint'] else 1
    comparison = {
        'command': command, 'original_directory': str(source), 'input_sha256': pins,
        'expected_removed_old_projection_errors': removed,
        'all_gate_fields_and_errors_match': actual == expected,
        'exit_code': process.returncode, 'expected_exit_code': expected_exit,
        'expected_summary': expected, 'actual_summary': actual,
        'release_approval': False,
    }
    (dest / 'comparison.json').write_text(json.dumps(comparison, indent=2) + '\n')
    assert all(digest(source / name) == value for name, value in pins.items()), 'Input changed'
    assert actual == expected, f'{label}: gate/error drift (see comparison.json)'
    assert process.returncode == expected_exit and actual['release_approval'] is False
    print(json.dumps({'run': label, 'exact_gates_and_errors_verified': True,
                      'exit_code': process.returncode, 'gates': actual['gates'],
                      'old_projection_errors_resolved': len(removed)}), flush=True)
    return label, comparison


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', action='append', required=True, help='label=existing-run-directory')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    files = [ROOT / 'scripts/check_budget.sh', ROOT / 'scripts/check_local_budget.py',
             ROOT / 'scripts/decode_cl_transaction_evidence.cjs', ROOT / 'bots/margin-liquidator/package-lock.json',
             Path(__file__).resolve()] + list((ROOT / 'scripts').glob('cl_*.py'))
    pins = {str(p): digest(p) for p in files}
    with ThreadPoolExecutor(max_workers=3) as pool:
        results = dict(pool.map(lambda item: replay(item, out), args.run))
    assert all(digest(Path(name)) == value for name, value in pins.items()), 'Checker changed during replay'
    (out / 'index.json').write_text(json.dumps({
        'schema': 'cl-atomic-exact-gate-replay-v1', 'release_approval': False,
        'checker_sha256': pins, 'runs': results,
    }, indent=2) + '\n')


if __name__ == '__main__':
    main()
