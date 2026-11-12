"""Lead-invoked nine-root private fixture application. No network capability.

Uses only the accepted deterministic private-account signing guard. Refuses any
other passphrase and preserves each command, exit status and log independently.
"""
from pathlib import Path
import json, os, subprocess, sys
ROOT=Path(__file__).resolve().parents[2]
pair=sys.argv[1]
assert pair in ['pyusd-usdc','usdc-eurc']
build=ROOT/'target/cl-stage1-live-pairs'/f'{pair}-v1'
out=ROOT/'target/cl-stage1-live-pairs'/f'{pair}-private-v1'
roots=['stage_prepare','stage_fund','stage_convert','stage_mint','stage_invest','stage_prepare_close','stage_withdraw','stage_withdraw_in_kind','stage_close_convert']
private='Peridot Stage1 signature application tests ONLY 2026-11-10'
records=[]
for name in roots:
    inp=build/'private-inputs'/name
    data=json.loads((inp/'input.json').read_text())
    assert data['network_passphrase']==private
    r=out/name;r.mkdir(parents=True,exist_ok=True)
    record=r/'record'
    if record.exists():
        # Only resume the one previously completed recording; never suppress a
        # failed invocation or skip enforcing/application results.
        assert (record/'test-exit-code.txt').read_text().strip()=='0'
        assert json.loads((inp/'recorded.json').read_text())['success'] is True
    else:
        subprocess.run(['bash',str(ROOT/'prototypes/cl-stage1-live-pairs/run-host.sh'),str(build),str(inp/'input.json'),str(record)],cwd=ROOT,check=True)
    assert not (inp/'signed-private-envelope.xdr').exists(),'refuse signing twice'
    subprocess.run(['node',str(build/'sign-local.cjs'),str(inp)],cwd=ROOT,check=True)
    subprocess.run(['bash',str(ROOT/'prototypes/cl-stage1-live-pairs/run-host.sh'),str(build),str(inp/'enforce-input.json'),str(r/'enforce')],cwd=ROOT,check=True)
    env=dict(os.environ,CL_CORE_FORBID_SIGNING='1',CL_STAGE1_PRIVATE_CORE_INPUT=str(inp/'core-input.txt'))
    cmd=[str(ROOT/'target/cl-stage1-signed-local/core-v6/stellar-core-private'),'test','[cl-stage1-private-signed]','--version','29','--rng-seed','20261110','--ll','warning']
    assert not (r/'core.log').exists()
    (r/'core-command.json').write_text(json.dumps({'cmd':cmd,'private_input':str(inp/'core-input.txt'),'Core_signing_guard':1})+'\n')
    with (r/'core.log').open('w')as f:p=subprocess.run(cmd,cwd=ROOT,env=env,stdout=f,stderr=subprocess.STDOUT)
    (r/'core-exit.txt').write_text(str(p.returncode)+'\n')
    log=(r/'core.log').read_text()
    assert p.returncode==0 and 'All tests passed' in log,(pair,name,p.returncode)
    assert 'CL_PRIVATE_APPLICATION_SUMMARY' in log
    records.append({'root':name,'Core_exit':p.returncode,'application_summary_present':True,'signed_fixture_only':True})
    (out/'completed.json').write_text(json.dumps(records,indent=2)+'\n')
    print(pair,name,'private signed application PASS',flush=True)
assert len(records)==9
