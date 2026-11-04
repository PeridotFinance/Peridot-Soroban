#!/usr/bin/env python3
"""Unsigned native maintenance after the exact code restore; no network/signing."""
from pathlib import Path
import json,subprocess,sys,hashlib
r=Path(__file__).resolve().parent;sys.path.insert(0,str(Path.cwd()/'scripts'))
from cl_transaction_evidence import fee_configuration,compute_fees
meta=json.loads((r/'receipt-code-input.json').read_text());row=meta['row'];config=json.loads((r/'captured-network-config.json').read_text());fee,rent=fee_configuration(config)
node=r'''
const fs=require('fs'),{xdr,Operation}=require('./bots/margin-liquidator/node_modules/@stellar/stellar-sdk');const j=JSON.parse(fs.readFileSync(0,'utf8'));const e=xdr.TransactionEnvelope.fromXDR(j.xdr,'base64'),t=e.v1().tx();if(e.v1().signatures().length)throw Error('signed input');t.seqNum(xdr.SequenceNumber.fromString(String(j.seq)));
const z=t.ext().sorobanData().resources();if(j.extend){t.operations([Operation.extendFootprintTtl({extendTo:j.extend})]);z.footprint().readOnly(z.footprint().readWrite());z.footprint().readWrite([]);z.writeBytes(0);}
if(j.fee!==undefined){t.fee(j.fee);t.ext().sorobanData().resourceFee(xdr.Int64.fromString(String(j.fee-100)));}console.log(JSON.stringify({xdr:e.toXDR('base64'),bytes:e.toXDR().length}));
'''
items=[]
for kind,seq,extend in [('already_live_restore',7,0),('extend_code_ttl',8,3110399)]:
 e=json.loads(subprocess.check_output(['node','-e',node],input=json.dumps({'xdr':row['unsigned_envelope_xdr'],'seq':seq,'extend':extend}).encode()));ledger=row['current_ledger']+seq-6;res=dict(row['resources']);res['transaction_size_bytes']=e['bytes'];res['contract_events_size_bytes']=0
 changes=[];until=row['keys'][0]['new_ttl']
 if extend:
  # Core29 TransactionFrame::getNumDiskReadEntries excludes live Soroban RO keys.
  res['disk_read_entries']=0;res['write_entries']=0;res['write_bytes']=0;until=ledger+extend;size=row['rent_changes'][0]['new_size_bytes'];changes=[{'is_code_entry':True,'is_persistent':True,'old_size_bytes':size,'new_size_bytes':size,'old_live_until_ledger':row['keys'][0]['new_ttl'],'new_live_until_ledger':until}]
 fees=compute_fees(res,fee,changes,rent,ledger,100);e=json.loads(subprocess.check_output(['node','-e',node],input=json.dumps({'xdr':e['xdr'],'seq':seq,'fee':fees['total']}).encode()));items.append({'kind':kind,'ledger':ledger,'source_sequence':seq,'fees':fees,'resources':res,'rent_changes':changes,'expected_live_until':until,'unsigned_envelope_xdr':e['xdr']})
lines=[str(len(items))]
for i in items:lines += [str(i['fees']['total']),i['unsigned_envelope_xdr'],str(i['expected_live_until'])]
raw=('\n'.join(lines)+'\n').encode();(r/'maintenance-input.txt').write_bytes(raw);out={'schema':'cl-core-native-maintenance-input-v1','source_sha256':meta['source_sha256'],'initial_input_sha256':hashlib.sha256((r/'receipt-code-input.txt').read_bytes()).hexdigest(),'network_config_sha256':meta['network_config_sha256'],'input_sha256':hashlib.sha256(raw).hexdigest(),'scope':'new unsigned controlled native envelopes, exact preAuth hashes installed at genesis before first restore','fee_inputs':fee,'rent_fee_inputs':rent,'items':items,'core_application_verified':False};(r/'maintenance-input.json').write_text(json.dumps(out,indent=2)+'\n');print(json.dumps([{'kind':i['kind'],'fees':i['fees'],'ttl':i['expected_live_until']} for i in items]))
