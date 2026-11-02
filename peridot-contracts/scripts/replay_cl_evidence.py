"""Relocate hash-bound saved evidence; no contract execution or approval.

Run from the exact peridot-contracts source checkout used for the checkpoint.
Original evidence is read-only; output must be new. Failed gates remain failures.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys

from cl_replay_summary import compare, compare_invocations


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def inside(root, relative):
    path = (root / relative).resolve()
    require(path.is_relative_to(root), 'path escapes evidence/source root')
    return path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('evidence', type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--source-root', type=Path, default=Path.cwd())
    args = parser.parse_args()
    root, evidence, output = args.source_root.resolve(), args.evidence.resolve(), args.output.resolve()
    require(not output.exists(), 'refusing to overwrite replay output')
    checksums = json.loads((evidence / 'package-checksums.json').read_text())
    require(bool(checksums), 'empty package checksums')
    actual_files = {str(p.relative_to(evidence)) for p in evidence.rglob('*') if p.is_file() and p.name != 'package-checksums.json'}
    require(set(checksums) == actual_files, 'package checksum coverage differs')
    for name, expected in checksums.items():
        require(digest(inside(evidence, name)) == expected, 'package hash mismatch: ' + name)
    spec = importlib.util.spec_from_file_location('cl_replay_pinned_profiles', root / 'scripts/cl_resource_profiles.py')
    require(spec is not None and spec.loader is not None, 'pinned profile validator unavailable')
    pinned_profiles = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(pinned_profiles)
    create = pinned_profiles.create
    mapping_path = root / 'config/cl-proof-map.json'
    mapping = json.loads(mapping_path.read_text())
    require(digest(mapping_path) == digest(evidence / 'proof-map.json'), 'checkout proof map differs')
    prepared = []
    for name in ['internal-policy', 'captured-network-diagnostic']:
        old_profile = json.loads((evidence / (name + '.json')).read_text())
        profile = create(evidence / 'network-config.json', name, old_profile['mode'])
        comparable = dict(profile, network_config_path=old_profile['network_config_path'])
        require(comparable == old_profile, 'profile settings changed: ' + name)
        manifest = json.loads((evidence / (name + '-proof-manifest.json')).read_text())
        paths = []
        expected_summary = json.loads((evidence / (name + '-summary.json')).read_text())
        require(manifest['reviewed_map']['sha256'] == digest(mapping_path), 'reviewed map hash differs')
        require(manifest['proofs'] == mapping['proofs'], 'proof mapping changed')
        require(set(manifest['sources']) == set(mapping['sources']), 'source coverage changed')
        for test, item in manifest['sources'].items():
            path = inside(root, mapping['sources'][test])
            require(path.suffix == '.rs', 'unexpected proof source type')
            require(digest(path) == item['sha256'], 'checkout source differs: ' + test)
            item['path'] = str(path)
        for item in manifest['test_runs']:
            path = inside(evidence, 'logs/' + Path(item['path']).name)
            require(digest(path) == item['sha256'], 'test log hash differs')
            paths.extend([(item['path'], '<log:' + path.name + '>'), (str(path), '<log:' + path.name + '>')])
            item['path'] = str(path)  # Preserve original exit_code, including failures.
        calls = evidence / (name + '-calls.jsonl')
        require(digest(calls) == manifest['call_profile']['sha256'], 'call profile differs')
        manifest['call_profile']['path'] = str(calls)
        report = evidence / (name + '-resources.jsonl')
        require(digest(report) == manifest['resource_report_sha256'], 'resource report differs')
        manifest['reviewed_map']['path'] = str(mapping_path)
        manifest['source_root'] = str(root)
        prepared.append((name, profile, manifest, report, expected_summary, paths))
    output.mkdir(parents=True)
    status = 0
    verification = {}
    for name, profile, manifest, report, expected_summary, paths in prepared:
        profile_path, manifest_path = output / (name + '.json'), output / (name + '-proof-manifest.json')
        profile_path.write_text(json.dumps(profile, indent=2) + '\n')
        manifest_path.write_text(json.dumps(manifest, indent=2) + '\n')
        transactions = [evidence / (name + '-transactions.jsonl' + suffix) for suffix in ('', '.xz', '.gz')]
        transactions = [p for p in transactions if p.is_file()]
        require(len(transactions) == 1, 'expected exactly one transaction evidence file')
        with (output / (name + '-budget.log')).open('w') as log:
            result = subprocess.run([
                'bash', 'scripts/check_budget.sh', '--local-report', str(report),
                '--profile', str(profile_path), '--proof-manifest', str(manifest_path),
                '--transaction-evidence', str(transactions[0]),
                '--gate', 'checkpoint', '--summary-json', str(output / (name + '-summary.json')),
            ], stdout=log, stderr=subprocess.STDOUT, check=False, cwd=root)
        observed = json.loads((output / (name + '-summary.json')).read_text())
        verification[name] = compare(expected_summary, observed, result.returncode, paths)
        expected_logs = [evidence / 'logs' / (name + suffix) for suffix in ('-budget.log', '-checkpoint.log')]
        expected_logs = [p for p in expected_logs if p.is_file()]
        require(len(expected_logs) == 1, 'expected exactly one saved invocation checker log')
        verification[name]['invocation_resources_and_errors_verified'] = compare_invocations(expected_logs[0].read_text(), (output / (name + '-budget.log')).read_text())
        (output / 'replay-verification.json').write_text(json.dumps(verification, indent=2) + '\n')
        print(name + ': exact gates/errors verified; checker exit ' + str(result.returncode), flush=True)
        status |= result.returncode != 0
    return int(status)


if __name__ == '__main__':
    try:
        sys.exit(main())
    except (OSError, KeyError, TypeError, ValueError) as error:
        print('REPLAY FAIL: ' + str(error), file=sys.stderr)
        sys.exit(2)
