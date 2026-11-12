"""Validate actual Core cold applications and reprice exact signed native batches."""
import json,re,sys,subprocess,hashlib
from pathlib import Path
root=Path(__file__).resolve().parents[2];sys.path.insert(0,str(root/'scripts'))
from cl_transaction_evidence import compute_fees,fee_configuration
base=root/'target/cl-stage1-signed-local'
def messages(text,label):
 dec=json.JSONDecoder();return[dec.raw_decode(text[m.end():])[0]for m in re.finditer(re.escape(label+' '),text)]
config=json.loads((root/'../docs/deployment-handoffs/evidence/cl-resource-decision-refresh-20261102/network-config.json').read_text());fee,rent=fee_configuration(config)
profile=json.loads((root/'target/cl-stage1-internal-v6/profile.json').read_text());limits=profile['limits'];models=config['settings']['configSettingContractCostParamsMemoryBytes']['values']
rows=[]
for name in ['cold-invest-v2','cold-withdraw-v1']:
 d=base/name;log=(d/'core.log').read_text();assert 'All tests passed' in log
 prep=messages(log,'CL_PRIVATE_RESTORE_SUMMARY');plan=json.loads((d/'plan.json').read_text());assert len(prep)==len(plan['batches'])
 for i,r in enumerate(prep):
  decoded=json.loads(subprocess.check_output(['node',str(Path(__file__).with_name('decode-native.cjs')),str(d),str(i)],text=True));assert decoded['signature_verified'];assert int(decoded['sequence'])==i+1
  assert len(set(decoded['keys']))==r['keys']<=50
  assert decoded['signed_envelope_bytes']<=limits['transaction_size_bytes']
  assert decoded['instructions']==0 # Native reservation, NOT zero measured native CPU.
  assert decoded['disk_read_bytes']==decoded['write_bytes']==r['bytes']<=limits['write_bytes']
  changes=[]
  for e in decoded['entries']:
   assert e['entry_bytes']<=limits['max_contract_code_entry_size_bytes'if e['kind']=='contractCode'else'max_contract_data_entry_size_bytes']
   if e['kind']=='contractData':assert e['key_bytes']<=limits['max_contract_data_key_size_bytes']
   mem=0
   for c in e['code_memory_inputs']:
    m=models[c['cost_type']];mem=min(2**64-1,mem+m['constTerm']+(min(2**64-1,m['linearTerm']*c['input'])>>7))
   changes.append(dict(is_persistent=True,is_code_entry=e['kind']=='contractCode',old_size_bytes=0,new_size_bytes=min(2**32-1,e['entry_bytes']+min(2**32-1,mem)),old_live_until_ledger=0,new_live_until_ledger=r['live_until']))
  resources=dict(instructions=0,disk_read_entries=r['keys'],write_entries=r['keys'],disk_read_bytes=r['bytes'],write_bytes=r['bytes'],transaction_size_bytes=decoded['signed_envelope_bytes'],contract_events_size_bytes=0)
  fees=compute_fees(resources,fee,changes,rent,r['ledger'],100)
  assert fees['total']==r['fee_charged'],(name,i,fees,r)
  r.update(fees=fees,signed_envelope_bytes=decoded['signed_envelope_bytes'],fee_ceiling=decoded['fee_ceiling'],refund=decoded['fee_ceiling']-r['fee_charged'],resource_limits_pass=True,native_cpu=None,native_memory=None)
 actual=messages(log,'CL_PRIVATE_APPLICATION_SUMMARY');assert len(actual)==1
 metrics={v['event']['event']['body']['v0']['topics'][1]['sym']:v['event']['event']['body']['v0']['data'].get('u64')for v in messages(log,'CL_PRIVATE_DIAGNOSTIC')if len(v['event']['event']['body']['v0']['topics'])==2 and v['event']['event']['body']['v0']['topics'][0].get('sym')=='core_metrics'}
 host=json.loads((d/'enforced.json').read_text());sdk=json.loads((d/'enforce-run/resources.jsonl').read_text().splitlines()[-1]);gate=json.loads((d/'enforce-run/gates.json').read_text());assert gate['gates']['invocation'] and not gate['gates']['checkpoint']
 financial=dict(core_cpu=metrics['cpu_insn'],cpu_max=max(metrics['cpu_insn'],host['resources']['instructions'],sdk['instructions']),memory_max=max(metrics['mem_byte'],host['resources']['memory_bytes'],sdk['memory_bytes']),sdk_total=sdk['read_entries']+sdk['disk_read_entries']+sdk['write_entries'],unique_keys=len(set(host['footprint']['read_only_xdr']+host['footprint']['read_write_xdr'])),writes=len(host['footprint']['read_write_xdr']),core_actual_writes=metrics['write_entry'],core_application=actual[0],preflight_fees=host['fees'],signed_envelope_bytes=json.loads((d/'signature-evidence.json').read_text())['envelope_bytes'])
 charged=compute_fees({**host['resources'],'instructions':100000000,'transaction_size_bytes':financial['signed_envelope_bytes']},host['fee_inputs'],host['rent_changes'],host['rent_fee_inputs'],host['current_ledger'],100)
 assert charged['total']==actual[0]['fee_charged'],(name,charged,actual[0])
 financial['charged_fee_components']=charged
 assert financial['cpu_max']<=100000000 and financial['memory_max']<=41943040 and financial['sdk_total']<=100 and financial['unique_keys']<=100 and financial['writes']<=50
 rows.append(dict(journey=name,archive_scope='touched existing persistent code/instance/data; controlled hot archive genesis',restoration=prep,financial=financial,total_charged_stroops=sum(r['fee_charged']for r in prep)+actual[0]['fee_charged'],preparation_count=len(prep),signature_count=len(prep)+1,passed=True))
output=dict(schema='private-signed-cold-v1',rows=rows,release_approval=False,core_binary_sha256=hashlib.sha256((base/'core-v6/stellar-core-private').read_bytes()).hexdigest(),limitations=['independent cold probes, not additive normal lifecycle','no live archive discovery','no interruption/reexpiry/trade scenario','no complete financial poststate differential','native CPU/memory are not Soroban metered'])
(base/'cold-table.json').write_text(json.dumps(output,indent=2)+'\n')
for r in rows:print(r['journey'],r['financial'],'total',r['total_charged_stroops'])
