#!/usr/bin/env python3
"""Build an unsigned, controlled receipt code+observed-state restore fixture.
This is a new local envelope, not an official simulation/helper output.
"""
from pathlib import Path
import hashlib,json,subprocess,sys
ROOT=Path(__file__).resolve().parent
sys.path.insert(0,str(Path.cwd()/'scripts'))
from cl_transaction_evidence import fee_configuration,compute_fees
meta=json.loads((ROOT/'receipt-code-input.json').read_text());code=meta['row']
source=Path(meta['original_source']); assert hashlib.sha256(source.read_bytes()).hexdigest()==meta['source_sha256']
rows=[json.loads(x) for x in source.read_text().splitlines() if x.strip()]
cursor=code
for batch in reversed(range(code['batch'])):
    candidates=[r for r in rows if r['profile']==code['profile'] and r['flow']==code['flow'] and r['current_ledger']==code['current_ledger'] and r['batch']==batch and r['source_sha256']==code['source_sha256'] and r['snapshot_after_sha256']==cursor['snapshot_before_sha256']]
    assert len(candidates)==1, 'ambiguous or broken original preparation chain'
    cursor=candidates[0]
data=cursor
assert data['batch']==0
selector=r'''
const fs=require('fs'),{xdr}=require('./bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const inp=JSON.parse(fs.readFileSync(0,'utf8')),code=inp.code,data=inp.data;
const ce=xdr.LedgerEntry.fromXDR(code.keys[0].entry_xdr,'base64'),hash=ce.data().contractCode().hash();
const entry=k=>xdr.LedgerEntry.fromXDR(k.entry_xdr,'base64');
const matches=data.keys.filter(k=>{const e=entry(k);return e.data().switch().name==='contractData'&&e.data().contractData().key().switch().name==='scvLedgerKeyContractInstance'&&e.data().contractData().val().instance().executable().switch().name==='contractExecutableWasm'&&e.data().contractData().val().instance().executable().wasmHash().equals(hash);});
if(matches.length!==1)throw Error('exactly one receipt instance required');
const id=entry(matches[0]).data().contractData().contract().toXDR('base64');
const indices=data.keys.map((k,i)=>[k,i]).filter(([k,i])=>{const e=entry(k);return e.data().switch().name==='contractData'&&e.data().contractData().contract().toXDR('base64')===id;}).map(([k,i])=>i);
const keys=indices.map(i=>data.keys[i]).concat(code.keys),rent_changes=indices.map(i=>data.rent_changes[i]).concat(code.rent_changes);
const e=xdr.TransactionEnvelope.fromXDR(code.unsigned_envelope_xdr,'base64');if(e.v1().signatures().length)throw Error('signed input');
const r=e.v1().tx().ext().sorobanData().resources();r.footprint().readWrite(keys.map(k=>xdr.LedgerKey.fromXDR(k.key_xdr,'base64')));
const bytes=keys.reduce((n,k)=>n+Buffer.from(k.entry_xdr,'base64').length,0);r.diskReadBytes(bytes);r.writeBytes(bytes);
console.log(JSON.stringify({keys,rent_changes,entry_bytes:bytes,unsigned_envelope_xdr:e.toXDR('base64'),transaction_size_bytes:e.toXDR().length,receipt_address_xdr:id,code_sha256:hash.toString('hex')}));
'''
selected=json.loads(subprocess.check_output(['node','-e',selector],input=json.dumps({'code':code,'data':data}).encode()))
config=Path(meta['network_config']);assert hashlib.sha256(config.read_bytes()).hexdigest()==meta['network_config_sha256'];settings=json.loads(config.read_text())
fee,rent=fee_configuration(settings)
resources=dict(instructions=0,disk_read_entries=len(selected['keys']),write_entries=len(selected['keys']),disk_read_bytes=selected['entry_bytes'],write_bytes=selected['entry_bytes'],transaction_size_bytes=selected['transaction_size_bytes'],contract_events_size_bytes=0)
fees=compute_fees(resources,fee,selected['rent_changes'],rent,code['current_ledger'],100)
setter=r'''
const fs=require('fs'),{xdr}=require('./bots/margin-liquidator/node_modules/@stellar/stellar-sdk');const j=JSON.parse(fs.readFileSync(0,'utf8'));const e=xdr.TransactionEnvelope.fromXDR(j.xdr,'base64');e.v1().tx().fee(j.fee);e.v1().tx().ext().sorobanData().resourceFee(xdr.Int64.fromString(String(j.fee-100)));if(e.v1().signatures().length)throw Error('signed envelope');console.log(e.toXDR('base64'));
'''
unsigned=subprocess.check_output(['node','-e',setter],input=json.dumps({'xdr':selected['unsigned_envelope_xdr'],'fee':fees['total']}).encode()).decode().strip()
assert len(__import__('base64').b64decode(unsigned))==resources['transaction_size_bytes']
lines=[str(code['current_ledger']),str(fees['total']),unsigned,str(len(settings['settings']))]+[s['xdr'] for s in settings['settings'].values()]+[str(len(selected['keys']))]
for k in selected['keys']:lines.extend([k['entry_xdr'],str(k['old_ttl'])])
raw=('\n'.join(lines)+'\n').encode();(ROOT/'receipt-state-input.txt').write_bytes(raw)
out={'scope':'new unsigned controlled Core restore envelope containing real receipt code and its exact observed instance/persistent footprint entries; not an official simulation output, not financial execution or full live-state proof','source_path':str(source),'original_data_snapshot_before_sha256':data['snapshot_before_sha256'],'original_code_snapshot_after_sha256':code['snapshot_after_sha256'],'receipt_address_xdr':selected['receipt_address_xdr'],'code_sha256':selected['code_sha256'],'source_sha256':meta['source_sha256'],'network_config_sha256':meta['network_config_sha256'],'current_ledger':code['current_ledger'],'input_sha256':hashlib.sha256(raw).hexdigest(),'keys':selected['keys'],'rent_changes':selected['rent_changes'],'resources':resources,'fee_inputs':fee,'rent_fee_inputs':rent,'predicted_fees':fees,'unsigned_envelope_xdr':unsigned,'core_application_verified':False,'signatures':0}
(ROOT/'receipt-state-input.json').write_text(json.dumps(out,indent=2)+'\n');print(json.dumps({'keys':len(out['keys']),'bytes':selected['entry_bytes'],'fees':fees,'input_sha256':out['input_sha256']}))
