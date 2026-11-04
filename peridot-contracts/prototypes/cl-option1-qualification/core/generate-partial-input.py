#!/usr/bin/env python3
"""Controlled mixed-live receipt footprint; no signing or network operations."""
from pathlib import Path
import json,hashlib,subprocess,sys
r=Path(__file__).resolve().parent;sys.path.insert(0,str(Path.cwd()/'scripts'))
from cl_transaction_evidence import compute_fees
j=json.loads((r/'receipt-state-input.json').read_text());code=[i for i,k in enumerate(j['keys']) if j['rent_changes'][i]['is_code_entry']];assert len(code)==1
fees=compute_fees(j['resources'],j['fee_inputs'],[j['rent_changes'][code[0]]],j['rent_fee_inputs'],j['current_ledger'],100)
node=r'''
const fs=require('fs'),{xdr}=require('./bots/margin-liquidator/node_modules/@stellar/stellar-sdk');const j=JSON.parse(fs.readFileSync(0,'utf8'));const e=xdr.TransactionEnvelope.fromXDR(j.xdr,'base64');if(e.v1().signatures().length)throw Error('signed');e.v1().tx().fee(j.fee);e.v1().tx().ext().sorobanData().resourceFee(xdr.Int64.fromString(String(j.fee-100)));console.log(e.toXDR('base64'));
'''
e=subprocess.check_output(['node','-e',node],input=json.dumps({'xdr':j['unsigned_envelope_xdr'],'fee':fees['total']}).encode()).decode().strip();settings=json.loads((r/'captured-network-config.json').read_text())['settings'];lines=[str(j['current_ledger']),str(fees['total']),e,str(len(settings))]+[x['xdr'] for x in settings.values()]+[str(len(j['keys']))]
for i,k in enumerate(j['keys']):lines.extend([k['entry_xdr'],str(k['old_ttl'] if i in code else k['new_ttl'])])
raw=('\n'.join(lines)+'\n').encode();(r/'partial-input.txt').write_bytes(raw);out={'schema':'cl-core-partial-restore-input-v1','source_state_input_sha256':j['input_sha256'],'network_config_sha256':j['network_config_sha256'],'scope':'controlled receipt instance and21data entries already live at expected restore TTL; only actual code expired; exact entry data unchanged','input_sha256':hashlib.sha256(raw).hexdigest(),'expired_keys':1,'already_live_keys':len(j['keys'])-1,'fees':fees,'unsigned_envelope_xdr':e,'resources':j['resources'],'rent_changes':[j['rent_changes'][code[0]]],'core_application_verified':False};(r/'partial-input.json').write_text(json.dumps(out,indent=2)+'\n');print(json.dumps({'fees':fees,'live':22,'expired':1}))
