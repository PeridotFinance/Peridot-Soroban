#!/usr/bin/env python3
"""Offline reuse of accepted call traces; no new execution or build."""
import json,hashlib,tarfile
from pathlib import Path
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[2]
evidence=ROOT/'../docs/deployment-handoffs/evidence'
p=evidence/'cl-stage1-staged-20261110';index=json.loads((p/'index.json').read_text());arc=p/index['archive']['file']
assert hashlib.sha256(arc.read_bytes()).hexdigest()==index['archive']['sha256']
name='runs/cl-stage1-internal-v6/calls.jsonl'
with tarfile.open(arc)as t:b=t.extractfile(name).read()
assert hashlib.sha256(b).hexdigest()==index['members'][name]['sha256']
rows=[]
for x in map(json.loads,b.decode().splitlines()):
 calls=[c for c in x['calls']if c['function']in ['lastprice','resolution','get_slot0']]
 if not calls:continue
 row={'flow':x['flow'],'root_cpu':x['root_instructions'],'calls':[{'function':c['function'],'role':c['role'],'path':c['path'],'cpu':c['inclusive']['instructions'],'new_memory_read_entries':c['inclusive']['memory_read_entries_delta'],'new_write_entries':c['inclusive']['write_entries_delta']}for c in calls]}
 row['oracle_calls_cpu']=sum(c['cpu']for c in row['calls']if c['role']=='oracle')
 row['slot_cpu']=sum(c['cpu']for c in row['calls']if c['function']=='get_slot0')
 row['covered_calls_cpu']=row['oracle_calls_cpu']+row['slot_cpu'];rows.append(row)
signed=json.loads((evidence/'cl-stage1-private-signed-20261110/signed-table.json').read_text())
headroom=[]
for r in signed['rows']:
 cpu=max(r['core_cpu'],r['host_cpu'],r['sdk_cpu']);mem=max(r['core_memory'],r['host_memory'],r['sdk_memory'])
 headroom.append({'flow':r['transaction'],'cpu':cpu,'cpu_remaining':100000000-cpu,'memory':mem,'memory_remaining':41943040-mem,'sdk_total':r['sdk_total_entries'],'sdk_remaining':100-r['sdk_total_entries'],'unique_keys':r['unique_soroban_keys'],'keys_remaining':100-r['unique_soroban_keys'],'writes':r['writes'],'writes_remaining':50-r['writes']})
result={'scope':'accepted historical compiled synthetic-oracle fixture only; not live Reflector or new guard qualification','archive_sha256':index['archive']['sha256'],'calls_member':name,'calls_sha256':index['members'][name]['sha256'],'oracle_wasm_sha256':'bff4b6856c0199893515ca688c674847c23eedd097fe143c1b7734691bcd7f7f','traces':rows,'signed_baseline_headroom':headroom,'new_guard_complete_transaction_cost':None,'new_guard_resource_gate':'UNMEASURED; no build or new financial execution authorized in this research'}
(HERE/'resource-attribution.json').write_text(json.dumps(result,indent=2)+'\n')
for flow in sorted(set(x['flow']for x in rows)):
 xs=[x for x in rows if x['flow']==flow];print(flow,max(x['oracle_calls_cpu']for x in xs),max(x['slot_cpu']for x in xs))
print('PASS accepted trace hashes; new guard resource gate remains UNMEASURED')
