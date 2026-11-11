"""Reconcile actual private Core signatures/application with per-root resources."""
import json,re,hashlib,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'scripts'))
from cl_transaction_evidence import compute_fees
def messages(log,label):
    dec=json.JSONDecoder()
    return [dec.raw_decode(log[m.end():])[0] for m in re.finditer(re.escape(label+' '),log)]
rows=[]
for pair in ['pyusd-usdc','usdc-eurc']:
    base=ROOT/'target/cl-stage1-live-pairs'/f'{pair}-v1'
    run=ROOT/'target/cl-stage1-live-pairs'/f'{pair}-private-v1'
    for name in ['stage_prepare','stage_fund','stage_convert','stage_mint','stage_invest','stage_prepare_close','stage_withdraw_in_kind','stage_withdraw','stage_close_convert']:
        d=base/'private-inputs'/name;r=run/name
        log=(r/'core.log').read_text()
        assert (r/'core-exit.txt').read_text().strip()=='0' and 'All tests passed' in log
        app=messages(log,'CL_PRIVATE_APPLICATION_SUMMARY');assert len(app)==1
        metrics={}
        for event in messages(log,'CL_PRIVATE_DIAGNOSTIC'):
            v=event['event']['event']['body']['v0'];t=v['topics']
            if len(t)==2 and t[0].get('sym')=='core_metrics':metrics[t[1]['sym']]=v['data'].get('u64')
        result=json.loads((d/'enforced.json').read_text());assert result['success'] and result['limits_pass']
        sig=json.loads((d/'signature-evidence.json').read_text());assert sig['network']=='Peridot Stage1 signature application tests ONLY 2026-11-10'
        sdk=json.loads((r/'enforce/resources.jsonl').read_text().splitlines()[-1])
        gate=json.loads((r/'enforce/gates.json').read_text());assert gate['gates']['invocation'] and not gate['release_approval']
        fp=result['footprint'];keys=set(fp['read_only_xdr']+fp['read_write_xdr'])
        cpu=max(metrics['cpu_insn'],result['resources']['instructions'],sdk['instructions'])
        mem=max(metrics['mem_byte'],result['resources']['memory_bytes'],sdk['memory_bytes'])
        entries=sdk['read_entries']+sdk['disk_read_entries']+sdk['write_entries'];writes=len(fp['read_write_xdr'])
        assert cpu<=100000000 and mem<=41943040 and entries<=100 and len(keys)<=100 and writes<=50
        assert app[0]['source_debit']==app[0]['fee_charged'] and app[0]['sequence']==1 and app[0]['envelope_signatures']==1 and app[0]['replay_rejected']
        fees=compute_fees({**result['resources'],'instructions':100000000,'transaction_size_bytes':sig['envelope_bytes']},result['fee_inputs'],result['rent_changes'],result['rent_fee_inputs'],result['current_ledger'],100)
        assert fees['total']==app[0]['fee_charged'],(pair,name,fees,app[0])
        rows.append({'pair':pair,'root':name,'CPU_max':cpu,'memory_max':mem,'SDK_total':entries,'unique_keys':len(keys),'writes':writes,'signed_bytes':sig['envelope_bytes'],'CPU_headroom':100000000-cpu,'charged_fee_components':fees,'application':app[0],'core_metrics':metrics,'gate_summary':gate['gates'],'fee_refund':sig['fee_ceiling_stroops']-fees['total'],'pass':True,'input_sha256':hashlib.sha256((d/'input.json').read_bytes()).hexdigest()})
out=ROOT/'research/cl-apy-decomposition-20261111/lead'
record={'scope':'actual private Core29.0.0 source signatures and application; current venue WASM, controlled state, synthetic oracle, independently seeded roots; not live-state/capacity/final release proof','rows':rows,'core_binary_sha256':hashlib.sha256((ROOT/'target/cl-stage1-signed-local/core-v6/stellar-core-private').read_bytes()).hexdigest(),'cold_new_code':False,'detached_new_code':False,'release_approval':False}
(out/'signed-table.json').write_text(json.dumps(record,indent=2)+'\n')
lines=['|Pair / root|CPU max|Memory max|SDK total|Unique keys|Writes|Signed bytes|Fee XLM|','|---|---:|---:|---:|---:|---:|---:|---:|']
for r in rows:lines.append('|'+ '|'.join(map(str,[r['pair']+' / '+r['root'],r['CPU_max'],r['memory_max'],r['SDK_total'],r['unique_keys'],r['writes'],r['signed_bytes'],r['application']['fee_charged']/1e7]))+'|')
(out/'signed-table.md').write_text('\n'.join(lines)+'\n')
print('\n'.join(lines))
print('PASS 18 private Core applications, fee reconciliation and invocation gates. Missing release/lifecycle/transaction proofs stay failed.')
