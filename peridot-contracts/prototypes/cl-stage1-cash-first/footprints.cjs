// Offline public fixture XDR decoding. No network, accounts, signatures or keys.
const fs=require('fs'),path=require('path');
const {xdr,Address,scValToNative}=require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const dir=process.argv[2];
const txs=fs.readFileSync(path.join(dir,'transactions.jsonl'),'utf8').trim().split('\n').map(JSON.parse);
const calls=fs.readFileSync(path.join(dir,'calls.jsonl'),'utf8').trim().split('\n').map(JSON.parse);
const roles={};for(const r of calls)for(const c of r.calls)if(c.address)roles[c.address]=c.role;
const stringify=x=>JSON.stringify(x,(_,v)=>typeof v==='bigint'?v.toString():v);
const out=[];
for(const r of txs){
 if(!r.flow.startsWith('stage_'))continue;
 const keys=[];
 for(const [mode,xs] of Object.entries(r.footprint))for(const raw of xs){
  const k=xdr.LedgerKey.fromXDR(raw,'base64');let owner=k.switch().name,purpose='';
  if(owner==='contractData'){const d=k.contractData(),a=Address.fromScAddress(d.contract()).toString();owner=roles[a]||'external-authorizer-or-token';purpose=stringify(scValToNative(d.key()));}
  else if(owner==='contractCode'){purpose='compiled WASM '+k.contractCode().hash().toString('hex');}
  else purpose=k.switch().name;
  keys.push({contract:owner,purpose,access:mode==='read_write_xdr'?'read-write':'read-only',key_xdr:raw});
 }
 if(new Set(keys.map(k=>k.key_xdr)).size!==keys.length)throw Error('duplicate/overlapping keys');
 out.push({test:r.test_name,flow:r.flow,keys});
}
console.log(JSON.stringify(out,null,2));
