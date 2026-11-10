"""Replay exact per-flow gates/errors. Missing lifecycle evidence stays failed."""
from pathlib import Path
import json,subprocess,sys
root=Path(__file__).resolve().parents[2];base=root/'target/cl-stage1-signed-local';out=Path(sys.argv[1]).resolve();out.mkdir(exist_ok=False)
names=['stage_prepare','stage_fund','stage_convert','stage_mint','stage_invest','stage_prepare_close','stage_withdraw_in_kind','stage_withdraw','stage_close_convert']
runs=[base/(n+'-enforced-v2')for n in names]+[base/(n+'-detached-enforced-v1')for n in ['stage_prepare','stage_fund','stage_prepare_close']]+[base/n/'enforce-run'for n in ['cold-invest-v2','cold-withdraw-v1']]
rows=[]
for i,p in enumerate(runs):
 dest=out/f'{i}.json'
 with(out/f'{i}.log').open('w')as log:r=subprocess.run(['bash','scripts/check_budget.sh','--local-report',str(p/'resources.jsonl'),'--profile',str(root/'target/cl-stage1-internal-v6/profile.json'),'--gate','invocation','--summary-json',str(dest)],cwd=root,stdout=log,stderr=subprocess.STDOUT)
 assert r.returncode==int((p/'budget-exit-code.txt').read_text()),p
 actual=json.loads(dest.read_text());expected=json.loads((p/'gates.json').read_text());assert actual==expected,p
 assert actual['gates']==dict(invocation=True,lifecycle=False,transaction=False,checkpoint=False)
 rows.append(dict(run=str(p.relative_to(root)),gate_summary_exact=True,errors_exact=True,gates=actual['gates'],missing_required_flows=len(actual['missing_required_flows'])))
(out/'summary.json').write_text(json.dumps(rows,indent=2)+'\n');print(len(rows),'exact gate/error replays PASS; lifecycle/transaction/checkpoint remain FAIL')
