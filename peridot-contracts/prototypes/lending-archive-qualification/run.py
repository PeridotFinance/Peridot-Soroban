#!/usr/bin/env python3
"""Offline bounded qualification; a passing test never hides failed global gates."""
import argparse, hashlib, json, os, subprocess, sys
from pathlib import Path
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
from cl_resource_profiles import create

def main():
    p = argparse.ArgumentParser()
    p.add_argument('--artifacts', type=Path, required=True)
    p.add_argument('--network-config', type=Path, required=True)
    p.add_argument('--cap-caller', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    a = p.parse_args()
    out = a.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    network = out / 'network-config.json'
    network.write_bytes(a.network_config.read_bytes())
    artifacts = a.artifacts.resolve()
    identities = {n: hashlib.sha256((artifacts / n).read_bytes()).hexdigest()
                  for n in ['receipt_vault.wasm', 'simple_peridottroller.wasm', 'jump_rate_model.wasm']}
    identities['synthetic_cap_caller.wasm'] = hashlib.sha256(a.cap_caller.read_bytes()).hexdigest()
    (out / 'artifact-hashes.json').write_text(json.dumps(identities, indent=2) + '\n')
    statuses = {}
    for name in ['internal-policy', 'captured-network-diagnostic']:
        profile = out / f'{name}-profile.json'
        profile.write_text(json.dumps(create(network, name, 'acceptance'), indent=2) + '\n')
        report = out / f'{name}.json'
        env = dict(os.environ, CL_RESOURCE_PROFILE=str(profile),
                   LENDING_ARCHIVE_WASM_DIR=str(artifacts), LENDING_ARCHIVE_REPORT=str(report),
                   LENDING_CAP_CALLER_WASM=str(a.cap_caller.resolve()))
        command = ['cargo', 'test', '--locked', '--offline', '--manifest-path',
                   str(Path(__file__).parent / 'Cargo.toml'), '--', '--nocapture', '--test-threads=1']
        with (out / f'{name}.log').open('w') as f:
            code = subprocess.run(command, cwd=ROOT, env=env, stdout=f, stderr=subprocess.STDOUT).returncode
        statuses[name] = {'test_exit': code}
        command = ['bash', 'scripts/check_budget.sh', '--local-report', str(report) + '.sdk.jsonl',
                   '--profile', str(profile), '--summary-json', str(out / f'{name}-gates.json')]
        with (out / f'{name}-checker.log').open('w') as f:
            gate = subprocess.run(command, cwd=ROOT, stdout=f, stderr=subprocess.STDOUT).returncode
        statuses[name]['checker_exit'] = gate
    (out / 'statuses.json').write_text(json.dumps(statuses, indent=2) + '\n')
    print(json.dumps(statuses, indent=2))
    # Missing global lifecycle/transaction labels are failures, not approvals.
    return int(any(v['test_exit'] or v['checker_exit'] for v in statuses.values()))

if __name__ == '__main__':
    sys.exit(main())
