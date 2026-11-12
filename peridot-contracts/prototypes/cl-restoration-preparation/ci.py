#!/usr/bin/env python3
"""Focused compiled restoration evidence; missing/failed gates keep CI red."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--output', type=Path, required=True)
    a = p.parse_args()
    out = a.output.resolve()
    scratch = out.with_name(out.name + '-builds')
    assert out.is_relative_to(ROOT / 'target')
    assert not out.exists() and not scratch.exists()
    out.mkdir(parents=True)
    scratch.mkdir()
    records = []
    def run(label, command, env=None):
        log = out / (label + '.log')
        with log.open('w') as f:
            result = subprocess.run(list(map(str, command)), cwd=ROOT, env=env,
                                    stdout=f, stderr=subprocess.STDOUT)
        records.append({'label': label, 'command': list(map(str, command)),
                        'exit_code': result.returncode,
                        'log_sha256': hashlib.sha256(log.read_bytes()).hexdigest()})
        (out / 'commands.json').write_text(json.dumps(records, indent=2) + '\n')
        return result.returncode
    patch = scratch / 'combined.patch'
    patch.write_bytes((ROOT / 'prototypes/cl-atomic-qualification/optimization.patch').read_bytes()
        + (ROOT / 'prototypes/cl-storage-qualification/storage.patch').read_bytes()
        + (ROOT / 'prototypes/cl-storage-qualification/repayment.patch').read_bytes())
    clean, final = scratch / 'clean', scratch / 'final'
    build_steps = [
        ('clean-build', ['python3', HERE / 'build.py', '--output', clean, '--optimization', patch]),
        ('prepare', ['python3', HERE / 'prepare.py', '--build', clean, '--output', final]),
        ('identify', ['python3', ROOT / 'prototypes/cl-storage-qualification/identify_build.py', '--build', final]),
        ('fetch-runtime', ['cargo', 'fetch', '--locked', '--manifest-path', final / 'harness/Cargo.toml']),
    ]
    for label, command in build_steps:
        if run(label, command) != 0:
            return 1
    spec = importlib.util.spec_from_file_location('storage_ci', ROOT / 'prototypes/cl-storage-qualification/ci.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    module.snapshot_build(clean, out / 'builds/clean')
    module.snapshot_build(final, out / 'builds/final')
    env = dict(os.environ, CL_ATOMIC_BUILD_DIR=str(final))
    filters = {
        'combined': 'combined_repayment_cases',
        'open-close': 'test::atomic_qualification::compiled_atomic_qualification_swap_close_64_owners_128_changes_8_unresolved_5pct_trade',
        'liquidation-history': 'test::atomic_qualification::compiled_atomic_qualification_8_independent_liquidations_repeated_both_call_outages',
        'pending-history': 'test::atomic_qualification::compiled_atomic_qualification_8_independent_pending_recoveries_repeated_both_call_outages',
    }
    statuses = {}
    for label, selected in filters.items():
        statuses[label] = run(label, ['bash', HERE / 'run.sh', selected, out / label], env)
    (out / 'ci-result.json').write_text(json.dumps({'runs': statuses,
        'profiles': ['internal-policy', 'captured-network-diagnostic'],
        'qualified': all(code == 0 for code in statuses.values()),
        'release_approval': False, 'stage1_complete': False}, indent=2) + '\n')
    return 0 if all(code == 0 for code in statuses.values()) else 1

if __name__ == '__main__':
    raise SystemExit(main())
