// Public unsigned fee planning only. No keys, signing, sending or old contracts.
const fs=require('node:fs');
const crypto=require('node:crypto');
const S=require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const assert=require('node:assert/strict');
const root='research/stellar-redeployment-20261111/';
const source='GDBS26DYO2FMMLXGFPZ6JX3ZTHJBM2FVAAKD34J7K6ZWVKGLLANZTOYE';
const endpoint='https://soroban-rpc.mainnet.stellar.gateway.fm';
const server=new S.rpc.Server(endpoint,{timeout:45});
const pins=JSON.parse(fs.readFileSync(root+'medium-final-build-hashes.json'));
const out={observedAt:new Date().toISOString(),endpoint,source,scope:'unsigned host fee planning; source account not yet created; sequence0 placeholder is not executable',uploads:[],deployments:[]};
const save=()=>fs.writeFileSync(root+'fresh-deployment-fee-quote.json',JSON.stringify(out,null,2)+'\n');
async function sim(label,op){
 const tx=new S.TransactionBuilder(new S.Account(source,'0'),{fee:'100',networkPassphrase:S.Networks.PUBLIC}).addOperation(op).setTimeout(300).build();
 const r=await server.simulateTransaction(tx);const row={label,latestLedger:r.latestLedger};
 if(!S.rpc.Api.isSimulationSuccess(r)){row.error=r.error;return row;}
 const d=r.transactionData.build(),v=d.resources(),f=v.footprint();
 Object.assign(row,{minResourceFeeStroops:r.minResourceFee,feeWithNominalInclusionStroops:(BigInt(r.minResourceFee)+100n).toString(),instructions:v.instructions(),diskReadBytes:v.diskReadBytes(),writeBytes:v.writeBytes(),uniqueKeys:f.readOnly().length+f.readWrite().length,writes:f.readWrite().length,restoreRequired:!!r.restorePreamble,transactionDataXdr:d.toXDR('base64'),authEntries:r.result.auth.length});
 return row;
}
(async()=>{
 out.network=await server.getNetwork();assert.equal(out.network.passphrase,S.Networks.PUBLIC);
 const latest=await server.getLatestLedger(); out.latestLedger={id:latest.id,sequence:latest.sequence,protocolVersion:latest.protocolVersion,closeTime:latest.closeTime};
 for(const name of ['simple_peridottroller.wasm','receipt_vault.wasm']){
  const bytes=fs.readFileSync('target/lending-medium-final-20261111/artifacts/'+name);assert.equal(crypto.createHash('sha256').update(bytes).digest('hex'),pins[name].sha256);
  const key=S.xdr.LedgerKey.contractCode(new S.xdr.LedgerKeyContractCode({hash:Buffer.from(pins[name].sha256,'hex')}));
  const code=await server.getLedgerEntries(key);
  const row=await sim('upload-'+name,S.Operation.uploadContractWasm({wasm:bytes}));Object.assign(row,{sha256:pins[name].sha256,bytes:bytes.length,codeCurrentlyPresent:code.entries.length===1});out.uploads.push(row);save();
 }
 for(const label of ['controller','USDC','XLM','PYUSD','EURC']){
  const name=label==='controller'?'simple_peridottroller.wasm':'receipt_vault.wasm';
  const salt=crypto.createHash('sha256').update('peridot-fresh-fee-quote-only-20261111-'+label).digest();
  const row=await sim('deploy-'+label,S.Operation.createCustomContract({address:new S.Address(source),wasmHash:Buffer.from(pins[name].sha256,'hex'),salt,constructorArgs:[]}));out.deployments.push(row);save();
 }
 console.log(JSON.stringify(out,null,2));
})().catch(e=>{out.error=String(e);save();console.error(String(e));process.exitCode=1;});
