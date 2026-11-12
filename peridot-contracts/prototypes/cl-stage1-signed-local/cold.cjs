// Local private fixture only. Prepare signed native restore batches; never RPC.
const fs=require('fs'),path=require('path');
const {S,xdr,PRIVATE,keeper}=require('./accounts.cjs');
const root=path.resolve(__dirname,'../..');
const source=path.resolve(process.argv[2]),out=path.resolve(process.argv[3]);
if(fs.existsSync(out))throw Error('fresh output required');fs.mkdirSync(out,{recursive:true});
const input=JSON.parse(fs.readFileSync(path.join(source,'input.json'))),measured=JSON.parse(fs.readFileSync(path.join(source,'enforced.json')));
if(input.network_passphrase!==PRIVATE)throw Error('private fixture only');
const footprint=new Set([...measured.footprint.read_only_xdr,...measured.footprint.read_write_xdr]);
const rows=input.entries.filter(r=>{const k=xdr.LedgerKey.fromXDR(r.key_xdr,'base64');return footprint.has(r.key_xdr)&&(k.switch().name==='contractCode'||(k.switch().name==='contractData'&&k.contractData().durability().name==='persistent'));});
const batches=[];let batch=[],bytes=0;
for(const r of rows){const n=Buffer.from(r.entry_xdr,'base64').length;if(n>120000)throw Error('indivisible restore too large');if(bytes+n>120000||batch.length===50){batches.push(batch);batch=[];bytes=0;}batch.push(r);bytes+=n;}if(batch.length)batches.push(batch);
const envelopes=batches.map((rows,i)=>{
 const size=rows.reduce((n,r)=>n+Buffer.from(r.entry_xdr,'base64').length,0);
 const data=new S.SorobanDataBuilder().setReadWrite(rows.map(r=>xdr.LedgerKey.fromXDR(r.key_xdr,'base64'))).setResources(0,size,size).setResourceFee('3999999900').build();
 const t=new S.TransactionBuilder(new S.Account(keeper.publicKey(),String(i)),{fee:'100',networkPassphrase:PRIVATE}).addOperation(S.Operation.restoreFootprint({})).setSorobanData(data).setTimeout(0).build();t.sign(keeper);
 fs.writeFileSync(path.join(out,`restore-${i}.xdr`),t.toXDR()+'\n');return t.toXDR();
});
fs.copyFileSync(path.join(source,'core-input.txt'),path.join(out,'core-input.txt'));
input.result_path=path.join(out,'recorded.json');fs.writeFileSync(path.join(out,'input.json'),JSON.stringify(input));
fs.writeFileSync(path.join(out,'chain.txt'),[envelopes.length,...envelopes].join('\n')+'\n');
fs.writeFileSync(path.join(out,'plan.json'),JSON.stringify({network:PRIVATE,source,archived_keys:rows.length,batches:batches.map((r,i)=>({index:i,keys:r.length,bytes:r.reduce((n,e)=>n+Buffer.from(e.entry_xdr,'base64').length,0)})),native_cpu_and_memory:'not Soroban-metered',maximum_fee_stroops:4000000000},null,2));
console.log(out,rows.length,'archived keys',batches.length,'signed restore batches');
