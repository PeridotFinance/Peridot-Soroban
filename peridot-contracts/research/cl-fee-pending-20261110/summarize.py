"""Strict part-(a) coverage/resource summary; never turns lifecycle gates green."""
import argparse
import hashlib
import json
import re
from pathlib import Path

p = argparse.ArgumentParser()
p.add_argument('run', type=Path)
a = p.parse_args()
run = a.run
def rows(name):
    return [json.loads(x) for x in (run/name).read_text().splitlines()]
sdk, tx, comparisons = rows('resources.jsonl'), rows('transactions.jsonl'), rows('comparisons.jsonl')
assert (run/'test-exit-code.txt').read_text().strip() == '0'
assert re.search(r'test result: ok\. 6 passed; 0 failed;', (run/'tests.log').read_text())
assert (run/'budget-exit-code.txt').read_text().strip() == '0'
gates = json.loads((run/'gates.json').read_text())
assert gates['gates'] == dict(invocation=True, lifecycle=False, transaction=False, checkpoint=False)
assert gates['missing_required_flows'] and gates['lifecycle_errors'] and gates['transaction_errors']
workloads = [('narrow','zero'),('narrow','fees'),('narrow','outside'),('narrow','return'),('wide','fees'),('full','fees')]
operations = ['deposit_dust','deposit_normal','withdraw_dust','withdraw_partial','withdraw_full']
expected = {f'fee_pending_{r}_{s}_{op}_{branch}' for r,s in workloads for op in operations for branch in ['pending','swept']}
assert {x['case'] for x in comparisons} == expected and len(comparisons) == 60
by_case = {x['case']:x for x in comparisons}
for c in comparisons:
    assert c['fee_conservation_exact']
    other=by_case[c['case'].removesuffix('_swept').removesuffix('_pending')+'_pending']
    assert c['prestate_sha256']==other['prestate_sha256']
    assert c['principal']==other['principal'] and c['liquidity']==other['liquidity']
    assert c['pre_fees']==other['pre_fees']
    if c['operation'].startswith('withdraw'): assert c['wallet_delta']==other['wallet_delta']

result=[]
index=-1
current_sdk=None
for s in sdk:
    flow=s['flow']
    # auth() records its ABI name and then the case alias. Read-only calls only
    # get a case alias. Match strictly in execution order, never by maximum.
    fresh=not flow.startswith('fee_pending_') or flow.endswith(('_quote','_fee_view'))
    if fresh:
        index+=1
        current_sdk=s
        t=tx[index]
        if flow.endswith('_fee_view'): expected_function='get_all_position_fees'
        elif flow.endswith('_quote'): expected_function='estimate_deposit_position' if '_deposit_' in flow else 'estimate_withdraw_position'
        else: expected_function=flow
        assert t['flow']==expected_function,(index,flow,t['flow'])
    else:
        assert all(s[k]==current_sdk[k] for k in ['instructions','memory_bytes','read_entries','write_entries','ok'])
        t=tx[index]
    assert s['ok'] and s['profile']=='internal-policy' and s['run_mode']=='acceptance'
    assert t['success'] and t['limits_pass'] and t['contract_authorization_enforced']
    assert t['profile']=='internal-policy' and not t['signature_verification_measured']
    ro=t['footprint']['read_only_xdr'];rw=t['footprint']['read_write_xdr']
    assert len(set(ro+rw))==len(ro)+len(rw)
    keys=len(ro)+len(rw)
    assert max(s['instructions'],t['resources']['instructions'])<=100_000_000
    assert max(s['memory_bytes'],t['resources']['memory_bytes'])<=41_943_040
    assert s['read_entries']<=100 and keys<=100
    assert max(s['write_entries'],t['resources']['write_entries'])<=50
    if not flow.startswith('fee_pending_'): continue
    result.append(dict(case=flow, function=t['flow'], sdk_cpu=s['instructions'], host_cpu=t['resources']['instructions'],
        sdk_memory=s['memory_bytes'], host_memory=t['resources']['memory_bytes'],sdk_entries=s['read_entries'],
        unique_keys=keys, sdk_writes=s['write_entries'], host_writes=t['resources']['write_entries'],
        write_bytes=t['resources']['write_bytes'], envelope_bytes=t['resources']['transaction_size_bytes'],
        authorization_bytes=t['authorization_size_bytes'], event_bytes=t['contract_events_size_bytes'],
        return_bytes=t['return_value_size_bytes'], fees_stroops=t['fees'],
        signature_verification_measured=False, account_authorization=t['account_authorization_mode']))
assert index+1==len(tx)==210
assert len(result)==210
assert {r['case'] for r in result if r['case'] in expected}==expected
groups={}
for fn in sorted({r['function']for r in result}):
    rr=[r for r in result if r['function']==fn]
    nums=['sdk_cpu','host_cpu','sdk_memory','host_memory','sdk_entries','unique_keys','sdk_writes','host_writes','write_bytes','envelope_bytes','authorization_bytes','event_bytes','return_bytes']
    groups[fn]=dict(count=len(rr), maxima={k:max(x[k]for x in rr)for k in nums},
        max_sdk_case=max(rr,key=lambda r:r['sdk_cpu'])['case'],max_host_case=max(rr,key=lambda r:r['host_cpu'])['case'],
        fee_min=min(x['fees_stroops']['total']for x in rr),fee_max=max(x['fees_stroops']['total']for x in rr),
        max_fee_components=max(rr,key=lambda r:r['fees_stroops']['total'])['fees_stroops'])
deltas={}
for op in operations:
    values=[]
    for r,state in workloads:
        prefix=f'fee_pending_{r}_{state}_{op}_'
        pending=next(x for x in result if x['case']==prefix+'pending')
        swept=next(x for x in result if x['case']==prefix+'swept')
        values.append(dict(case=prefix,sdk_cpu_delta=pending['sdk_cpu']-swept['sdk_cpu'],host_cpu_delta=pending['host_cpu']-swept['host_cpu']))
    deltas[op]=values
summary=dict(schema='cl-fee-pending-part-a-v1',tests_passed=6,matched_branches=30,comparison_rows=60,
    measured_top_level_calls=len(tx),groups=groups,pending_minus_swept=deltas,rows=result,gates=gates['gates'],
    missing_required_flows=gates['missing_required_flows'],lifecycle_error_count=len(gates['lifecycle_errors']),
    transaction_error_count=len(gates['transaction_errors']),release_approval=False,
    scope='bounded controlled-state venue calls; not complete staged vault transactions or universal venue bounds',
    source_hashes={name:hashlib.sha256((run/name).read_bytes()).hexdigest()for name in ['resources.jsonl','transactions.jsonl','comparisons.jsonl','gates.json','tests.log']})
(run/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
print(json.dumps({k:v for k,v in summary.items()if k not in ['rows','groups','missing_required_flows','source_hashes']},indent=2))
print(json.dumps(groups,indent=2))
