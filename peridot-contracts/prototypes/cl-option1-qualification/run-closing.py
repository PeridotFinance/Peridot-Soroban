#!/usr/bin/env python3
"""Run exactly seven joined controlled-state cases with unchanged budget gates."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import subprocess

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
SKIP = [
    'compiled_atomic_qualification_8_independent_liquidations_repeated_both_call_outages',
    'compiled_atomic_qualification_8_independent_pending_recoveries_repeated_both_call_outages',
    'compiled_atomic_qualification_8_independent_repay_release_and_exited_entitlements',
]

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--profile', required=True, choices=['captured-network-diagnostic', 'internal-policy'])
    parser.add_argument('--build', required=True, type=Path)
    parser.add_argument('--run', required=True, type=Path)
    args = parser.parse_args()
    build, run = args.build.resolve(), args.run.resolve()
    assert not run.exists(), 'Refusing existing evidence directory'
    driver_record = run.with_name(run.name + '.driver.json')
    assert not driver_record.exists(), 'Refusing existing driver record'
    base = ROOT/'prototypes/cl-storage-qualification/run_variant.sh'
    script = base.read_text()
    cd = 'cd "$(dirname "$0")/../.."'
    assert script.count(cd) == 1
    script = script.replace(cd, 'cd '+shlex.quote(str(ROOT)))
    needle = '"$filter" -- --nocapture --test-threads=1'
    assert script.count(needle) == 1, 'Unrecognized pinned runner command'
    script = script.replace(needle, '"$filter" -- '+''.join('--skip '+shlex.quote(n)+' ' for n in SKIP)+'--nocapture --test-threads=1')
    files = [Path(__file__).resolve(),base,HERE/'build-closing.py',HERE/'joined-proof-map.json',build/'build-manifest.json']
    files += [HERE/n for n in ['closing-recovery.patch','closing-recovery-tests.rs','pending-recovery.patch','pending-recovery-tests.rs','pending-unhealthy.patch','pending-unhealthy-tests.rs','combined-pending-regression-tests.rs']]
    pins = {str(p):digest(p) for p in files}
    record = {'scope':'seven controlled compiled cases; no signed/live-state approval',
              'expected_case_count':7,'filter':'closing_qualification',
              'excluded_inherited_cases':SKIP,'profile':args.profile,
              'build':str(build),'run':str(run),'source_sha256':pins,
              'effective_runner_sha256':hashlib.sha256(script.encode()).hexdigest(),
              'release_approval':False}
    driver_record.write_text(json.dumps(record,indent=2)+'\n')
    env = dict(os.environ, CL_ATOMIC_BUILD_DIR=str(build), CARGO_BUILD_JOBS='1')
    result = subprocess.run(['bash','-c',script,'cl-closing-driver',args.profile,'closing_qualification',str(run)],cwd=ROOT,env=env)
    assert all(digest(Path(p))==h for p,h in pins.items()), 'Source changed during run'
    log = (run/'tests.log').read_text() if (run/'tests.log').exists() else ''
    counts = re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed;',log)
    record['runner_exit_code'] = result.returncode
    record['test_counts'] = [{'passed':int(p),'failed':int(f)} for p,f in counts]
    record['seven_cases_completed'] = len(counts)==1 and sum(map(int,counts[0]))==7
    # The base runner's initial gates are retained. Bind the seven actual test
    # names to the exact generated Rust files, then evaluate the full checkpoint.
    mapping = json.loads((HERE/'joined-proof-map.json').read_text())
    manifest = json.loads((build/'build-manifest.json').read_text())
    for name, source in mapping['sources'].items():
        leaf = 'closing_fixture.rs' if source.endswith('/tests/combined.rs') else Path(source).name
        generated = build/'harness/src'/leaf
        assert digest(generated)==manifest['harness_generated_sources'][str(generated.relative_to(build))]
        mapping['sources'][name] = str(generated)
    reviewed = run/'joined-proof-map.json'
    reviewed.write_text(json.dumps(mapping,indent=2)+'\n')
    proof = run/'joined-proof-manifest.json'
    test_exit = (run/'test-exit-code.txt').read_text().strip()
    subprocess.run(['python3','scripts/make_cl_proof_manifest.py','--map',str(reviewed),
        '--profile',str(run/'profile.json'),'--report',str(run/'resources.jsonl'),
        '--test-log',str(run/'tests.log'),'--test-exit-code',test_exit,
        '--call-profile',str(run/'calls.jsonl'),'--output',str(proof)],cwd=ROOT,check=True)
    with (run/'joined-budget.log').open('w') as log_file:
        gate = subprocess.run(['bash','scripts/check_budget.sh','--local-report',str(run/'resources.jsonl'),
            '--profile',str(run/'profile.json'),'--transaction-evidence',str(run/'transactions.jsonl'),
            '--proof-manifest',str(proof),'--gate','checkpoint','--summary-json',str(run/'joined-gates.json')],
            cwd=ROOT,stdout=log_file,stderr=subprocess.STDOUT)
    record['joined_checkpoint_exit_code'] = gate.returncode
    record['joined_gates'] = json.loads((run/'joined-gates.json').read_text())
    record['generated_map_sha256'] = digest(reviewed)
    driver_record.write_text(json.dumps(record,indent=2)+'\n')
    raise SystemExit(result.returncode or gate.returncode or (0 if record['seven_cases_completed'] else 2))

if __name__ == '__main__':
    main()
