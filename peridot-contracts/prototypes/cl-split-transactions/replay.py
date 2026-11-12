#!/usr/bin/env python3
"""Verify durable prototype evidence, then replay exact per-profile failures.

Exit 1: the verified checkpoint remains incomplete. Exit 2: evidence mismatch.
No signing, networking, account keys, or execution of archived shell scripts.
"""
import argparse, hashlib, json, os, subprocess, sys, tarfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
from cl_proof_evidence import cargo_outcomes
from cl_replay_summary import compare_invocations

def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def require(ok, message):
    if not ok: raise ValueError(message)

def main():
    p = argparse.ArgumentParser();p.add_argument('evidence', type=Path);p.add_argument('--output', type=Path, required=True)
    a = p.parse_args();index = json.loads((a.evidence / 'index.json').read_text())
    require(not a.output.exists(), 'output must be fresh')
    archive = a.evidence / 'checkpoint-payload.tar.xz'
    require(sha(archive) == index['archive_sha256'], 'archive hash mismatch')
    out = a.output.resolve();out.mkdir(parents=True)
    with tarfile.open(archive, 'r:xz') as tar:
        members = tar.getmembers()
        require(len(members) == len(index['files']), 'archive member count differs')
        require({m.name for m in members} == set(index['files']), 'archive file set differs')
        for member in members:
            dest = (out / member.name).resolve()
            require(member.isfile() and dest.is_relative_to(out), 'unsafe archive member')
            data = tar.extractfile(member).read();pin = index['files'][member.name]
            require(len(data) == pin['bytes'] and hashlib.sha256(data).hexdigest() == pin['sha256'], 'member hash mismatch')
            dest.parent.mkdir(parents=True, exist_ok=True);dest.write_bytes(data)
    for original, archived in index['source_bindings'].items():
        require(sha(out / archived['path']) == archived['sha256'], 'run source pin mismatch: ' + original)
    for name, pin in index['wasm'].items():
        require(sha(out / 'build' / name) == pin['sha256'], 'WASM mismatch: ' + name)
    # The decoder uses only XDR types. It rejects signatures and never opens RPC.
    modules = ROOT / 'bots/margin-liquidator/node_modules'
    require(modules.is_dir(), 'install pinned margin-liquidator package-lock dependencies first')
    dest = out / 'source/bots/margin-liquidator/node_modules';dest.parent.mkdir(parents=True, exist_ok=True)
    dest.symlink_to(modules, target_is_directory=True)
    verified = {}
    for name, expected in index['profiles'].items():
        run = out / 'runs' / name
        require(cargo_outcomes((run / 'tests.log').read_text()) == expected['outcomes'], 'completed test results differ')
        require(json.loads((run / 'gates.json').read_text()) == expected['gates'], 'saved per-profile gates differ')
        profile = json.loads((run / 'profile.json').read_text())
        profile['network_config_path'] = str(out / 'fixture/network-config.json')
        replay = out / 'replay' / name;replay.mkdir(parents=True)
        pp = replay / 'profile.json';pp.write_text(json.dumps(profile))
        cmd = [sys.executable, str(out / 'source/scripts/check_local_budget.py'), str(run / 'resources.jsonl'),
               '--profile', str(pp), '--transaction-evidence', str(run / 'transactions.jsonl'),
               '--gate', 'invocation', '--summary-json', str(replay / 'gates.json')]
        env = dict(os.environ, PYTHONPYCACHEPREFIX=str(out / 'pycache'))
        with (replay / 'budget.log').open('w') as log:
            result = subprocess.run(cmd, stdout=log, stderr=subprocess.STDOUT, env=env, check=False)
        observed = json.loads((replay / 'gates.json').read_text())
        require(observed == expected['gates'], 'per-profile gates/errors differ: ' + name)
        require(result.returncode == expected['invocation_checker_exit'], 'checker status differs: ' + name)
        compare_invocations((run / 'budget.log').read_text(), (replay / 'budget.log').read_text())
        verified[name] = {'gates_and_errors_match': True, 'invocation_rows_match': True,
                          'completed_tests_match': True, 'gates': observed['gates'], 'release_approval': False}
        print(name + ': exact gates, errors, invocation rows and completed tests verified', flush=True)
    require(set(verified) == {'internal-policy', 'captured-network-diagnostic'}, 'both profiles required')
    (out / 'replay-verification.json').write_text(json.dumps(verified, indent=2) + '\n')
    return 1 if not all(v['gates']['checkpoint'] for v in verified.values()) else 0

if __name__ == '__main__':
    try: sys.exit(main())
    except (ValueError, KeyError, OSError, tarfile.TarError) as error:
        print('EVIDENCE REPLAY MISMATCH: ' + str(error), file=sys.stderr);sys.exit(2)
