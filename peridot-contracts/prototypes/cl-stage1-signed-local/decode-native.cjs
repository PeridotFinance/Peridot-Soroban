// Verify the actual private signature, then reuse the unchanged unsigned structural decoder.
const fs=require('fs'),path=require('path'),cp=require('child_process');
const {S,xdr,PRIVATE,keeper}=require('./accounts.cjs');
const d=process.argv[2],i=Number(process.argv[3]);const input=JSON.parse(fs.readFileSync(path.join(d,'input.json')));
if(input.network_passphrase!==PRIVATE)throw Error('private only');
const encoded=fs.readFileSync(path.join(d,`restore-${i}.xdr`),'utf8').trim(),t=new S.Transaction(encoded,PRIVATE);
if(t.source!==keeper.publicKey()||t.signatures.length!==1||!keeper.verify(t.hash(),t.signatures[0].signature()))throw Error('private restore signature invalid');
const e=xdr.TransactionEnvelope.fromXDR(encoded,'base64'),bytes=e.toXDR().length;e.v1().signatures([]);
const data=e.v1().tx().ext().sorobanData();const keys=data.resources().footprint().readWrite().map(k=>{const r=input.entries.find(v=>v.key_xdr===k.toXDR('base64'));if(!r)throw Error('missing restored entry');return{...r,predicted_entry_xdr:r.entry_xdr};});
const result=cp.spawnSync(process.execPath,[path.resolve(__dirname,'../cl-restoration-preparation/decode-restore.cjs')],{input:JSON.stringify({unsigned_envelope_xdr:e.toXDR('base64'),helper_transaction_data_xdr:data.toXDR('base64'),keys}),encoding:'utf8'});if(result.status!==0)throw Error(result.stderr);
console.log(JSON.stringify({...JSON.parse(result.stdout),signed_envelope_bytes:bytes,signature_verified:true,source:t.source,sequence:t.sequence,fee_ceiling:Number(t.fee)}));
