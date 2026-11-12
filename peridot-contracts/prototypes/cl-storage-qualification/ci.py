#!/usr/bin/env python3
"""Collect prototype evidence; every failed/missing qualification gate stays red."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
PROFILES = ('internal-policy', 'captured-network-diagnostic')
GATES = ('invocation', 'lifecycle', 'transaction', 'checkpoint')
ROLES = 'vault,cash_receipt,share_receipt,v3,aquarius,lending_controller'
FILTER = 'compiled_atomic_qualification'


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + '\n')


def gate_result(directory, profile, summary_name='gates.json', exit_name='budget-exit-code.txt'):
    """Inspect every gate/error and its process status, never aggregate exit1."""
    summary = json.loads((directory / summary_name).read_text())
    code = int((directory / exit_name).read_text())
    assert summary['schema'] == 'cl-verification-gates-v1'
    assert summary['profile'] == profile
    assert summary['selected_gate'] == 'checkpoint'
    assert summary['release_approval'] is False
    gates = summary['gates']
    assert set(gates) == set(GATES) and all(type(v) is bool for v in gates.values())
    assert gates['checkpoint'] == all(gates[g] for g in GATES[:3])
    assert code == (0 if gates['checkpoint'] else 1), 'Gate/process status mismatch'
    for name in ('missing_required_flows', 'lifecycle_errors', 'transaction_errors'):
        assert isinstance(summary[name], list) and all(isinstance(e, str) for e in summary[name])
    if gates['checkpoint']:
        assert not any(summary[n] for n in ('missing_required_flows', 'lifecycle_errors', 'transaction_errors'))
    return {'qualified': gates['checkpoint'], 'exit_code': code, 'summary': summary}


def completed_fixture(directory):
    code = int((directory / 'test-exit-code.txt').read_text())
    text = (directory / 'tests.log').read_text()
    counts = re.findall(r'^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;', text, re.M)
    assert len(counts) == 1, 'Missing/ambiguous completed fixture result'
    passed, failed, ignored = map(int, counts[0])
    assert passed + failed == 4 and ignored == 0, 'Expected exactly four executed fixture tests'
    assert code == (0 if failed == 0 else 101), 'Cargo result/status mismatch'
    return {'qualified': code == 0, 'passed': passed, 'failed': failed, 'ignored': ignored, 'exit_code': code}


def snapshot_build(build, destination):
    """Preserve exact binaries/generated source; never upload compiler caches."""
    destination.mkdir(parents=True, exist_ok=False)
    for name in ('build-manifest.json', 'optimization-build.log'):
        if (build / name).is_file():
            shutil.copyfile(build / name, destination / name)
    for name in ('artifacts', 'harness/src'):
        if (build / name).is_dir():
            shutil.copytree(build / name, destination / name)
    for name in ('Cargo.toml', 'Cargo.lock'):
        if (build / 'harness' / name).is_file():
            (destination / 'harness').mkdir(exist_ok=True)
            shutil.copyfile(build / 'harness' / name, destination / 'harness' / name)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--profile', choices=PROFILES, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    out = args.output.resolve()
    assert out.is_relative_to(ROOT / 'target'), 'Evidence must remain under target'
    scratch = out.with_name(out.name + '-builds')
    assert not out.exists() and not scratch.exists(), 'Fresh output/build directories required'
    out.mkdir(parents=True)
    scratch.mkdir()
    commands, problems, variants, comparisons = [], [], {}, {}
    source_files = set(p for p in HERE.rglob('*') if p.is_file())
    source_files.update(p for p in (ROOT / 'prototypes/cl-atomic-qualification').rglob('*') if p.is_file())
    source_files = {p for p in source_files if p.suffix in {'.py', '.sh', '.rs', '.json', '.cjs', '.patch', '.md'}}
    source_files.add(ROOT.parent / '.github/workflows/cl-storage-prototype.yml')
    pins = {str(p.relative_to(ROOT.parent)): digest(p) for p in sorted(source_files)}
    write(out / 'source-before.json', pins)
    combined = out / 'metadata/combined-storage.patch'
    combined.parent.mkdir()
    combined.write_bytes((ROOT / 'prototypes/cl-atomic-qualification/optimization.patch').read_bytes()
                         + (HERE / 'storage.patch').read_bytes())

    def attempt(label, command, env=None):
        path = out / 'commands' / (label + '.log')
        path.parent.mkdir(exist_ok=True)
        with path.open('w') as log:
            process = subprocess.run(command, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
        record = {'label': label, 'command': list(map(str, command)), 'exit_code': process.returncode,
                  'log': str(path.relative_to(out)), 'log_sha256': digest(path)}
        commands.append(record)
        write(out / 'commands.json', commands)
        if process.returncode != 0:
            problems.append(label + ': process exit ' + str(process.returncode))
        return process.returncode

    for variant in ('baseline', 'candidate'):
        result = variants[variant] = {'qualified': False, 'views': {}, 'errors': []}
        build, prepared = scratch / variant, scratch / (variant + '-companions')
        try:
            command = ['python3', str(HERE / 'prepare_variant.py'), '--output', str(build)]
            if variant == 'candidate':
                command += ['--optimization', str(combined)]
            if attempt(variant + '-prepare', command) != 0:
                raise RuntimeError('Variant preparation failed; dependent execution unavailable')
            if attempt(variant + '-companions', ['python3', str(HERE / 'add_companions.py'),
                                               '--build', str(build), '--output', str(prepared)]) != 0:
                raise RuntimeError('Independent companion preparation failed')
            if attempt(variant + '-identify', ['python3', str(HERE / 'identify_build.py'),
                                              '--build', str(prepared)]) != 0:
                raise RuntimeError('Generated harness identity binding failed')
            result['harness_package'] = json.loads((prepared / 'build-manifest.json').read_text())['cargo_harness_identity']['name']
            snapshot_build(build, out / 'builds' / (variant + '-original'))
            snapshot_build(prepared, out / 'builds' / variant)
            raw = out / 'runs' / (variant + '-cold')
            env = dict(os.environ, CL_ATOMIC_BUILD_DIR=str(prepared),
                       CL_TRANSACTION_ARCHIVE_ROLE=ROLES, CL_STORAGE_ARCHIVE_CODE_INSTANCE='1',
                       CARGO_INCREMENTAL='0', CARGO_NET_OFFLINE='true')
            result['runner_exit'] = attempt(variant + '-run', ['bash', str(HERE / 'run_variant.sh'),
                                                             args.profile, FILTER, str(raw)], env)
            try:
                result['fixture'] = completed_fixture(raw)
                if not result['fixture']['qualified']:
                    problems.append(variant + ': fixture tests failed')
            except (OSError, ValueError, AssertionError) as error:
                result['fixture_error'] = str(error)
                problems.append(variant + ': fixture completion unavailable: ' + str(error))
            # The unmodified runner selects invocation; explicitly preserve a
            # separate complete checkpoint check for the raw cold root rows.
            check = ['bash', 'scripts/check_budget.sh', '--local-report', str(raw / 'resources.jsonl'),
                     '--profile', str(raw / 'profile.json'), '--transaction-evidence', str(raw / 'transactions.jsonl'),
                     '--proof-manifest', str(raw / 'proof-manifest.json'), '--gate', 'checkpoint',
                     '--summary-json', str(raw / 'checkpoint-gates.json')]
            status = attempt(variant + '-cold-checkpoint', check)
            (raw / 'checkpoint-exit-code.txt').write_text(str(status) + '\n')
            try:
                result['views']['cold'] = dict(gate_result(raw, args.profile, 'checkpoint-gates.json',
                                                         'checkpoint-exit-code.txt'), directory=str(raw))
            except (OSError, ValueError, KeyError, AssertionError) as error:
                result['errors'].append('cold gates unavailable/invalid: ' + str(error))
            for view in ('live', 'data'):
                dest = out / 'runs' / (variant + '-' + view)
                # Extraction returns0 only for extraction. Its fresh checker
                # exit and every gate/error below still decide qualification.
                status = attempt(variant + '-' + view + '-extract',
                                 ['python3', str(HERE / 'extract.py'), '--run', str(raw),
                                  '--output', str(dest), '--view', view])
                try:
                    assert status == 0, 'Extraction incomplete'
                    result['views'][view] = dict(gate_result(dest, args.profile), directory=str(dest))
                except (OSError, ValueError, KeyError, AssertionError) as error:
                    result['errors'].append(view + ' gates unavailable/invalid: ' + str(error))
            result['qualified'] = (result.get('fixture', {}).get('qualified') is True
                                   and result['runner_exit'] == 0 and not result['errors']
                                   and set(result['views']) == {'live', 'data', 'cold'}
                                   and all(v['qualified'] for v in result['views'].values()))
        except (OSError, ValueError, KeyError, AssertionError, RuntimeError) as error:
            result['errors'].append(str(error))
            if build.exists() and not (out / 'builds' / (variant + '-partial')).exists():
                snapshot_build(build, out / 'builds' / (variant + '-partial'))
        if not result['qualified']:
            problems.append(variant + ': required tests/gates incomplete or failed')
        write(out / 'partial-qualification.json', variants)

    (out / 'comparisons').mkdir()
    for view in ('live', 'data', 'cold'):
        baseline, candidate = out / 'runs' / ('baseline-' + view), out / 'runs' / ('candidate-' + view)
        destination = out / 'comparisons' / (view + '.json')
        status = attempt(view + '-comparison', ['node', str(HERE / 'compare.cjs'),
                                               str(baseline), str(candidate), str(destination)])
        try:
            comparison = json.loads(destination.read_text())
            assert comparison['business_equal'] is True and comparison['release_approval'] is False
            assert comparison['rows'] and status == 0
            comparisons[view] = {'business_equal': True, 'rows': len(comparison['rows']),
                                 'maintenance_review_required': comparison['maintenance_review_required'],
                                 'file': str(destination.relative_to(out))}
        except (OSError, ValueError, KeyError, AssertionError) as error:
            comparisons[view] = {'business_equal': False, 'error': str(error)}
            problems.append(view + ': logical comparison missing or failed')
    after = {str(p.relative_to(ROOT.parent)): digest(p) for p in sorted(source_files)}
    write(out / 'source-after.json', after)
    if after != pins:
        problems.append('Prototype/workflow sources changed during execution')
    qualified = (not problems and all(v['qualified'] for v in variants.values())
                 and all(c['business_equal'] for c in comparisons.values()))
    result = {'schema': 'cl-storage-prototype-ci-v1', 'profile': args.profile,
              'qualification_passed': qualified, 'stage1_complete': False, 'release_approval': False,
              'policy_change_implemented': False, 'signed_authorization_measured': False,
              'scope': 'Isolated compiled baseline/candidate; live/data/cold are independent complete unsigned host invocations from identical prestate, not new user transaction stages',
              'ci_identity': {name: os.environ.get(name) for name in
                              ('GITHUB_SHA', 'GITHUB_RUN_ID', 'GITHUB_RUN_ATTEMPT', 'GITHUB_JOB',
                               'GITHUB_REF', 'GITHUB_EVENT_NAME', 'RUNNER_OS', 'RUNNER_ARCH')},
              'variants': variants, 'comparisons': comparisons, 'errors': problems,
              'final_exit_code': 0 if qualified else 1}
    write(out / 'qualification-summary.json', result)
    write(out / 'artifact-index.json', {
        'schema': 'cl-storage-prototype-ci-artifacts-v1', 'release_approval': False,
        'files': {str(p.relative_to(out)): {'sha256': digest(p), 'bytes': p.stat().st_size}
                  for p in sorted(out.rglob('*')) if p.is_file()},
    })
    print(json.dumps({'profile': args.profile, 'qualification_passed': qualified,
                      'errors': problems, 'release_approval': False}), flush=True)
    return result['final_exit_code']


if __name__ == '__main__':
    raise SystemExit(main())
