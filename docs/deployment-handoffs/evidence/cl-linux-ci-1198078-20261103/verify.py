import hashlib, json, re, subprocess, sys, tarfile, zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT.parents[1] / 'scripts'))
from cl_proof_evidence import cargo_outcomes
from cl_replay_summary import compare
from cl_ci_captures import CAPTURES

capture = CAPTURES['1198078']
sha = lambda data: hashlib.sha256(data).hexdigest()
artifact = ROOT / 'artifact-13176432381.zip'
assert artifact.stat().st_size == 200952624
assert sha(artifact.read_bytes()) == capture['original_zip_sha256']
out = {'scope': 'Baseline CI only; neither split nor atomic qualification nor subsequent storage candidates executed',
       **capture, 'checker_tests': {'job_id': 125508485816, 'passed': 42},
       'profiles': {}, 'wasm': {}, 'replay_performed': False,
       'verification': 'GitHub digest, exact member identities, gates and error strings bound to logs, proof map/source/log/report hashes, completed libtest outcomes; no fresh contract execution or full checker replay',
       'release_approval': False}
with zipfile.ZipFile(artifact) as z:
    names = z.namelist()
    assert len(names) == len(set(names)) == 75
    read = lambda name: z.read('cl-ci-checkpoint/' + name)
    out['config_sha256'] = sha(read('network-config.json'))
    for name in names:
        if name.endswith('.wasm'):
            out['wasm'][name] = {'sha256': sha(z.read(name)), 'bytes': len(z.read(name))}
    for profile in ['internal-policy', 'captured-network-diagnostic']:
        summary = json.loads(read(profile + '-checkpoint-summary.json'))
        compare(summary, summary, 0 if summary['gates']['checkpoint'] else 1)
        log = read('logs/' + profile + '-checkpoint.log').decode()
        for gate, ok in summary['gates'].items():
            assert gate.upper() + ' GATE: ' + ('PASS' if ok else 'FAIL') in log
        for key in ['missing_required_flows', 'lifecycle_errors', 'transaction_errors']:
            for error in summary[key]:
                assert error in log, (key, error)
        manifest = json.loads(read(profile + '-proof-manifest.json'))
        assert manifest['resource_report_sha256'] == sha(read(profile + '-resources.jsonl'))
        assert manifest['call_profile']['sha256'] == sha(read(profile + '-calls.jsonl'))
        assert summary['config_sha256'] == manifest['config_sha256'] == out['config_sha256']
        mapping = json.loads(subprocess.check_output(['git', 'show', capture['commit'] + ':peridot-contracts/config/cl-proof-map.json']))
        assert mapping['proofs'] == manifest['proofs']
        for entry in [manifest['reviewed_map'], *manifest['sources'].values()]:
            relative = entry['path'].split('/peridot-contracts/', 1)[1]
            assert sha(subprocess.check_output(['git', 'show', capture['commit'] + ':peridot-contracts/' + relative])) == entry['sha256']
        tests = {}
        for run in manifest['test_runs']:
            relative = run['path'].split('/target/cl-ci-checkpoint/', 1)[1]
            data = read(relative)
            assert sha(data) == run['sha256']
            tests[relative] = {'recorded_exit_code': run['exit_code'],
                              'outcomes': cargo_outcomes(data.decode()),
                              'terminal_summaries': re.findall(r'^test result:.*$', data.decode(), re.M)}
        sdk = [json.loads(line) for line in read(profile + '-resources.jsonl').splitlines()]
        tx = [json.loads(line) for line in read(profile + '-transactions.jsonl').splitlines()]
        measured = [row for row in tx if 'resources' in row]
        out['profiles'][profile] = {
            'gate_summary': summary, 'tests': tests, 'proof_count': len(manifest['proofs']),
            'sdk_rows': len(sdk), 'transaction_rows': len(tx), 'transaction_unmeasured': len(tx) - len(measured),
            'max_sdk': {k: max(row[k] for row in sdk) for k in ['instructions', 'memory_bytes', 'write_entries']},
            'max_sdk_entries': max(row['read_entries'] + row['disk_read_entries'] + row['write_entries'] for row in sdk),
            'max_unsigned': {k: max(row['resources'][k] for row in measured) for k in ['instructions', 'memory_bytes', 'transaction_size_bytes', 'write_entries']},
            'max_unique_footprint': max(len(set(row['footprint']['read_only_xdr'] + row['footprint']['read_write_xdr'])) for row in measured),
            'max_unsigned_fee': max(row['fees']['total'] for row in measured),
            'restoration_rows': sum(bool(row.get('restored_rw_entry_indices')) for row in tx)}
    out['other_test_summaries'] = {
        name: re.findall(r'^test result:.*$', read('logs/' + name).decode(), re.M)
        for name in ['reward-tests.log', 'default-margin-tests.log', 'captured-network-diagnostic-native-funded.log',
                     'internal-policy-native-funded.log', 'captured-network-diagnostic-integrity.log', 'internal-policy-integrity.log']}
    archive_root = ROOT / 'linux-1198078'
    index = json.loads((archive_root / 'index.json').read_text())
    archive = archive_root / 'linux-ci-payload.tar.xz'
    assert archive.stat().st_size == index['archive_bytes'] and sha(archive.read_bytes()) == index['archive_sha256']
    with tarfile.open(archive, 'r:xz') as tar:
        assert len(tar.getnames()) == len(set(tar.getnames()))
        assert set(tar.getnames()) == set(index['payload_members'])
        for member in tar:
            data = tar.extractfile(member).read()
            expected = index['payload_members'][member.name]
            assert len(data) == expected['bytes'] and sha(data) == expected['sha256']
            if member.name.startswith('artifact/'):
                assert data == z.read(member.name[len('artifact/'):])
(ROOT / 'verified-summary.json').write_text(json.dumps(out, indent=2) + '\n')
(ROOT / 'capture-identity.json').write_text(json.dumps(capture, indent=2) + '\n')
print(json.dumps({p: {k: v for k, v in row.items() if k not in ['gate_summary', 'tests']} for p, row in out['profiles'].items()}, indent=2))
print(json.dumps({p: {'gates': row['gate_summary']['gates'], 'errors': {k: len(row['gate_summary'][k]) for k in ['missing_required_flows', 'lifecycle_errors', 'transaction_errors']}, 'tests': {n: t['terminal_summaries'] for n, t in row['tests'].items()}} for p, row in out['profiles'].items()}, indent=2))
print(json.dumps(out['other_test_summaries'], indent=2))
