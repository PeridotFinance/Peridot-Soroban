"""Actual private Core application table, distinct from host preflight and SDK metering."""
import json,re,hashlib,sys
from pathlib import Path
root=Path(__file__).resolve().parents[2];base=root/'target/cl-stage1-signed-local'
sys.path.insert(0,str(root/'scripts'))
from cl_transaction_evidence import compute_fees
def messages(text,label):
 dec=json.JSONDecoder();out=[];prefix=label+' '
 for m in re.finditer(re.escape(prefix),text):out.append(dec.raw_decode(text[m.end():])[0])
 return out
names=['stage_prepare','stage_fund','stage_convert','stage_mint','stage_invest','stage_prepare_close','stage_withdraw_in_kind','stage_withdraw','stage_close_convert']
rows=[]
for name in names+[n+'-detached' for n in ['stage_prepare','stage_fund','stage_prepare_close']]:
 d=base/'inputs-v2'/name;log=(base/(name+'-core-v2.log')).read_text();assert 'All tests passed' in log,name
 app=messages(log,'CL_PRIVATE_APPLICATION_SUMMARY');assert len(app)==1
 metrics={}
 for event in messages(log,'CL_PRIVATE_DIAGNOSTIC'):
  v=event['event']['event']['body']['v0'];t=v['topics']
  if len(t)==2 and t[0].get('sym')=='core_metrics':metrics[t[1]['sym']]=v['data'].get('u64')
 assert metrics['cpu_insn']<=100_000_000 and metrics['mem_byte']<=41_943_040
 result=json.loads((d/'enforced.json').read_text());assert result['success'] and result['limits_pass']
 signature=json.loads((d/'signature-evidence.json').read_text());assert signature['network']=='Peridot Stage1 signature application tests ONLY 2026-11-10'
 sr=base/(name+('-enforced-v1'if name.endswith('-detached')else'-enforced-v2'))
 sdk=json.loads((sr/'resources.jsonl').read_text().splitlines()[-1]);gates=json.loads((sr/'gates.json').read_text());assert gates['gates']['invocation']
 total=sdk['read_entries']+sdk['disk_read_entries']+sdk['write_entries'];assert total<=100
 fp=result['footprint'];keys=set(fp['read_only_xdr']+fp['read_write_xdr']);assert len(keys)<=100 and len(fp['read_write_xdr'])<=50
 row={'transaction':name,'core_cpu':metrics['cpu_insn'],'core_memory':metrics['mem_byte'],'host_cpu':result['resources']['instructions'],'host_memory':result['resources']['memory_bytes'],'sdk_cpu':sdk['instructions'],'sdk_memory':sdk['memory_bytes'],'sdk_memory_reads':sdk['read_entries'],'sdk_disk_reads':sdk['disk_read_entries'],'sdk_total_entries':total,'unique_soroban_keys':len(keys),'writes':len(fp['read_write_xdr']),'signed_envelope_bytes':signature['envelope_bytes'],'core_application':app[0],'core_metrics':metrics,'signature_form':'detached G address v1 + keeper envelope'if name.endswith('-detached')else'source envelope; SourceAccount auth where required','within_limits':True}
 assert app[0]['source_debit']==app[0]['fee_charged'] and app[0]['envelope_signatures']==1 and app[0]['replay_rejected']
 fees=compute_fees({**result['resources'],'instructions':100000000,'transaction_size_bytes':signature['envelope_bytes']},result['fee_inputs'],result['rent_changes'],result['rent_fee_inputs'],result['current_ledger'],100)
 assert fees['total']==app[0]['fee_charged'],(name,fees,app[0])
 row['charged_fee_components']=fees;row['fee_ceiling_stroops']=10000000;row['refunded_stroops']=10000000-fees['total']
 rows.append(row)
output={'schema':'cl-private-core-signed-table-v1','base':'3188ae05a7143c26ce67a442e98c49a9aa1d3524','network':'private synthetic ledger only','rows':rows,'release_approval':False,'cold_restoration_complete':False,'core_version':'29.0.0','host_preflight_version':'29.0.1','core_binary_sha256':hashlib.sha256((base/'core-v4/stellar-core-private').read_bytes()).hexdigest(),'source_signature_cost_scope':'native Core validation; outside Soroban CPU, included in Core application and charged envelope bytes','sdk_column_correction':'SDK total = memory reads + disk reads + writes; accepted unsigned table showed memory reads under SDK entries, but enforced SDK/checker totals were correct'}
(base/'signed-table.json').write_text(json.dumps(output,indent=2)+'\n')
lines=['|Transaction|Core CPU|Conservative CPU max|Memory max|SDK total|Unique keys|Writes|Signed bytes|Charged stroops|','|---|---:|---:|---:|---:|---:|---:|---:|---:|']
for r in rows:lines.append('|'+ '|'.join(map(str,[r['transaction'],r['core_cpu'],max(r['core_cpu'],r['host_cpu'],r['sdk_cpu']),max(r['core_memory'],r['host_memory'],r['sdk_memory']),r['sdk_total_entries'],r['unique_soroban_keys'],r['writes'],r['signed_envelope_bytes'],r['core_application']['fee_charged']]))+'|')
(base/'signed-table.md').write_text('\n'.join(lines)+'\n');print('\n'.join(lines))
