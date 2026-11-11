"""Exact gate/error replay; preserves missing-proof failures."""
from pathlib import Path
import json,subprocess
ROOT=Path(__file__).resolve().parents[2]
out=ROOT/'target/cl-stage1-live-pairs/gate-replay-v1';out.mkdir(exist_ok=False)
runs=[]
for pair in ['pyusd-usdc','usdc-eurc']:
    p=ROOT/'target/cl-stage1-live-pairs'/f'{pair}-capture-v6'
    runs.append((p,p/'profile.json',p/'transactions.jsonl'))
    for p in sorted((ROOT/'target/cl-stage1-live-pairs'/f'{pair}-private-v1').glob('*/enforce')):
        runs.append((p,ROOT/'target/cl-stage1-internal-v6/profile.json',None))
assert len(runs)==20
results=[]
for i,(p,profile,tx) in enumerate(runs):
    cmd=['bash','scripts/check_budget.sh','--local-report',str(p/'resources.jsonl'),'--profile',str(profile),'--gate','invocation','--summary-json',str(out/f'{i}.json')]
    if tx:cmd+=['--transaction-evidence',str(tx)]
    with(out/f'{i}.log').open('w')as f:r=subprocess.run(cmd,cwd=ROOT,stdout=f,stderr=subprocess.STDOUT)
    actual=json.loads((out/f'{i}.json').read_text());expected=json.loads((p/'gates.json').read_text())
    assert r.returncode==int((p/'budget-exit-code.txt').read_text())==0
    assert actual==expected,(p,actual,expected)
    assert actual['gates']=={'invocation':True,'lifecycle':False,'transaction':False,'checkpoint':False}
    results.append({'run':str(p.relative_to(ROOT)),'gates':actual['gates'],'exact_errors_and_gates':True,'missing_flows':len(actual['missing_required_flows'])})
(out/'summary.json').write_text(json.dumps(results,indent=2)+'\n')
print('PASS20 exact gate/error replays; invocation PASS, lifecycle/transaction/checkpoint FAIL')
