"""Validate complete measured roots; never convert this bounded report into release approval."""
import argparse,json,hashlib,subprocess
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('run',type=Path);a=p.parse_args();d=a.run
load=lambda f:json.loads((d/f).read_text())
lines=lambda f:[json.loads(l) for l in (d/f).read_text().splitlines() if l.strip()]
sdk=lines('resources.jsonl');tx=lines('transactions.jsonl');gates=load('gates.json')
assert (d/'test-exit-code.txt').read_text().strip()=='0','compiled tests failed'
assert (d/'budget-exit-code.txt').read_text().strip()=='0','invocation gate failed'
assert gates['gates']['invocation'] and not gates['release_approval']
assert not gates['gates']['checkpoint'],'this incomplete prototype cannot approve release'
log=(d/'tests.log').read_text()
assert '13 passed; 0 failed' in log
assert 'ACTUAL_BACKING_MINT_PASS' in log
assert 'NAMED_INVARIANT_PASS INCUMBENT_FEE_ENTITLEMENT_ALLOCATED_BEFORE_SHARE_BURN' in log
mapping=[('stage1_prepare','stage_prepare','full_open_close'),('stage1_fund','stage_fund','full_open_close'),('stage1_convert','stage_convert','full_open_close'),('stage1_fee_checkpoint_cash_mint','stage_mint','full_open_close'),('stage1_keeper_investment','stage_invest','full_open_close'),('stage1_prepare_close','stage_prepare_close','full_open_close'),('stage1_withdraw_in_kind','stage_withdraw','full_open_in_kind_close'),('stage1_withdraw_for_conversion','stage_withdraw','full_open_close'),('stage1_optional_close_conversion','stage_close_convert','full_open_close')]
rows=[]
for label,abi,test in mapping:
 ts=[r for r in tx if r['flow']==abi and r['test_name'].endswith('compiled_stage1_'+test)]
 assert len(ts)==1,(label,len(ts));t=ts[0]
 ss=[r for r in sdk if r['flow']==label and r['profile']==gates['profile']]
 assert ss,label
 # Include both invocation and host execution, retaining their separate numbers.
 sm={k:max(r[k] for r in ss) for k in ['instructions','memory_bytes','read_entries','write_entries']}
 fp=t['footprint'];ro=set(fp['read_only_xdr']);rw=set(fp['read_write_xdr'])
 assert not ro&rw and len(ro)==len(fp['read_only_xdr']) and len(rw)==len(fp['read_write_xdr'])
 assert len(rw)==t['resources']['write_entries']
 assert t['success'] and t['limits_pass'] and t['contract_authorization_enforced']
 assert not t['signature_verification_measured'] and not t['live_state_validation']
 assert t['config_sha256']==gates['config_sha256'] and t['runtime_protocol']==29
 assert t['fees']['total']==sum(t['fees'][k] for k in ['inclusion','non_refundable','refundable','rent'])
 # Independently decode actual unsigned envelope, auth, footprint and sizes.
 decoded=subprocess.run(['node','scripts/decode_cl_transaction_evidence.cjs'],input=json.dumps(t),text=True,capture_output=True)
 assert decoded.returncode==0,(label,decoded.stderr,decoded.stdout)
 r=dict(label=label,abi=abi,test=t['test_name'],sdk=sm,host=t['resources'],unique_keys=len(ro|rw),read_only_keys=len(ro),write_keys=len(rw),fees_stroops=t['fees'],authorization_size_bytes=t['authorization_size_bytes'],return_bytes=t['return_value_size_bytes'],envelope_sha256=t['envelope_sha256'],decoded=json.loads(decoded.stdout))
 r['within_100m_100_100_50']=max(sm['instructions'],t['resources']['instructions'])<=100_000_000 and max(sm['memory_bytes'],t['resources']['memory_bytes'])<=41_943_040 and sm['read_entries']<=100 and r['unique_keys']<=100 and max(sm['write_entries'],len(rw))<=50
 assert r['within_100m_100_100_50'],label
 # Maxima are per dimension across concrete observed roots, not a synthetic run.
 candidates=[x for x in tx if x['flow']==abi]
 if label=='stage1_withdraw_in_kind':
  candidates=[x for x in candidates if any(tag in x['test_name'] for tag in ['in_kind_close','withdraw_in_kind','one_unit'])]
 elif label=='stage1_withdraw_for_conversion':
  candidates=[x for x in candidates if any(tag in x['test_name'] for tag in ['full_open_close','withdraw_for_conversion'])]
 assert candidates
 r['representative_host']=r['host'];r['representative_unique_keys']=r['unique_keys']
 r['observed_cases']=[]
 for x in candidates:
  assert x['success'] and x['limits_pass']
  keys=set(x['footprint']['read_only_xdr']+x['footprint']['read_write_xdr'])
  r['observed_cases'].append(dict(test=x['test_name'],resources=x['resources'],unique_keys=len(keys),envelope_sha256=x['envelope_sha256']))
 r['host']={k:max(x['resources'][k] for x in candidates) for k in r['host']}
 r['unique_keys']=max(x['unique_keys'] for x in r['observed_cases'])
 r['write_keys']=max(x['resources']['write_entries'] for x in candidates)
 assert max(sm['instructions'],r['host']['instructions'])<=100_000_000 and r['unique_keys']<=100 and r['write_keys']<=50
 rows.append(r)
journeys={}
for suffix,count in [('full_open_close',8),('full_open_in_kind_close',7)]:
 ts=[r for r in tx if r['test_name'].endswith('compiled_stage1_'+suffix) and r['flow'].startswith('stage_')]
 assert len(ts)==count and all(r['success'] for r in ts)
 journeys[suffix]=dict(transactions=count,fee_components_stroops={k:sum(r['fees'][k] for r in ts) for k in ['inclusion','non_refundable','refundable','rent','total']},synthetic_ledger_seconds_between_first_and_last=5*(count-1),live_latency_measured=False)
result=dict(schema='cl-stage1-staged-transaction-table-v1',profile=gates['profile'],config_sha256=gates['config_sha256'],scope='compiled controlled-state unsigned; existing runtime29 harness',rows=rows,journeys=journeys,gates=gates,release_approval=False,missing=['signature verification and signed-envelope overhead','live-state discovery/capacity','cold restoration journey for this overlay','bounded concurrent histories and economic/math qualification','claim-outage staged burn recovery','adversarial command, payout rollback and cancellation qualification','Fable high review: CLI 429 usage limit'])
(d/'stage1-table.json').write_text(json.dumps(result,indent=2)+'\n')
md=['|Transaction|CPU SDK|CPU unsigned host|Memory max bytes|SDK entries|Unique keys|Writes|Result|','|---|---:|---:|---:|---:|---:|---:|---|']
for r in rows:md.append('|'+ '|'.join(map(str,[r['label'],r['sdk']['instructions'],r['host']['instructions'],max(r['sdk']['memory_bytes'],r['host']['memory_bytes']),r['sdk']['read_entries'],r['unique_keys'],r['write_keys'],'PASS']))+'|')
(d/'stage1-table.md').write_text('\n'.join(md)+'\n')
print(json.dumps({'profile':gates['profile'],'rows':len(rows),'tests':13,'journeys':journeys,'gates':gates['gates']}))
