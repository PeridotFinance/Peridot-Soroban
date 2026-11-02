"""Bind a reviewed proof-to-test map to exact source/log/resource artifacts.

This records evidence; it does not turn failed tests into successful outcomes.
Use check_local_budget.py afterwards to evaluate the resulting manifest.
"""
import argparse
import json
from pathlib import Path

from cl_proof_evidence import sha256
from cl_resource_profiles import create


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--map', required=True, type=Path)
    parser.add_argument('--profile', required=True, type=Path)
    parser.add_argument('--report', required=True, type=Path)
    parser.add_argument('--test-log', required=True, type=Path, action='append')
    parser.add_argument('--test-exit-code', required=True, type=int, action='append')
    parser.add_argument('--call-profile', type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    if len(args.test_log) != len(args.test_exit_code):
        raise SystemExit('one explicit exit code is required for each test log')
    mapping = json.loads(args.map.read_text())
    if mapping['schema'] != 'cl-proof-map-v1':
        raise SystemExit('unsupported proof map')
    profile = json.loads(args.profile.read_text())
    if profile != create(profile['network_config_path'], profile['name'], profile['mode']):
        raise SystemExit('profile configuration mismatch')
    # Source paths in the reviewed map are relative to the current workspace.
    def bound(path):
        path = Path(path).resolve()
        return {'path': str(path), 'sha256': sha256(path)}
    manifest = {
        'schema': 'cl-lifecycle-proof-v1', 'profile': profile['name'],
        'config_sha256': profile['network_sha256'],
        'resource_report_sha256': sha256(args.report),
        'reviewed_map': bound(args.map),
        'source_root': str(Path.cwd().resolve()),
        'test_runs': [dict(bound(log), exit_code=code) for log, code in zip(args.test_log, args.test_exit_code)],
        'sources': {test: bound(path) for test, path in mapping['sources'].items()},
        'proofs': mapping['proofs'], 'contracts': {},
    }
    if args.call_profile:
        manifest['call_profile'] = bound(args.call_profile)
        contracts = {}
        for line in args.call_profile.read_text().splitlines():
            row = json.loads(line)
            for call in row['calls']:
                role, address = call.get('role'), call.get('address')
                if role and role != 'unlabelled' and address:
                    contracts.setdefault(role, set()).add(address)
        manifest['contracts'] = {role: sorted(values) for role, values in contracts.items()}
    if args.output.exists():
        raise SystemExit('refusing to overwrite proof manifest')
    args.output.write_text(json.dumps(manifest, indent=2) + '\n')


if __name__ == '__main__':
    main()
