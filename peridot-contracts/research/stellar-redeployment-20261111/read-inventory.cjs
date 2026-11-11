// Public reads only. No identities, keys, signing, restoration or submission.
const fs=require('fs');const S=require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const rpc=new S.rpc.Server('https://soroban-rpc.mainnet.stellar.gateway.fm/');
(async()=>{const text=fs.readFileSync('addresses.md','utf8').split('## Mainnet')[1];
const lines=text.split('\n').filter(l=>/COMPROMISED|retired legacy|Existing isolated|\|.*COMPROMISED/.test(l));
const ids=[...new Set(lines.flatMap(l=>l.match(/C[A-Z2-7]{55}/g)||[]))];
const out={observedAt:new Date().toISOString(),scope:'address registry discovery; not yet exhaustive historical deployment inventory',contracts:[]};
for(const id of ids){let row={id,label:lines.find(l=>l.includes(id))};try{
const key=k=>S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract:new S.Address(id).toScAddress(),key:k,durability:S.xdr.ContractDataDurability.persistent()}));
const r=await rpc.getLedgerEntries(key(S.xdr.ScVal.scvLedgerKeyContractInstance()),...['Admin','PendingAdmin','PauseGuardian'].map(n=>key(S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol(n)]))));
row.ledger=r.latestLedger;row.entries=r.entries.map(e=>({key:S.scValToNative(e.key.contractData().key()),valueXdr:e.val.toXDR('base64')}));
for(const e of r.entries){const v=e.val.contractData().val();if(v.switch().name==='scvContractInstance'){const i=v.instance();row.wasmHash=i.executable().switch().name==='contractExecutableWasm'?i.executable().wasmHash().toString('hex'):null;row.instanceStorage=(i.storage()||[]).map(x=>({key:S.scValToNative(x.key()),value:S.scValToNative(x.val())}));}else{row[S.scValToNative(e.key.contractData().key())[0]]=S.scValToNative(v);}}
}catch(e){row.error=String(e)}out.contracts.push(row);fs.writeFileSync('research/stellar-redeployment-20261111/inventory-public.json',JSON.stringify(out,(_,v)=>typeof v==='bigint'?String(v):v,2));console.log(id,row.wasmHash||row.error||'missing');}
})();
