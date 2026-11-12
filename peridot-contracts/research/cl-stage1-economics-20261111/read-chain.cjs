// READ ONLY: public ledger data and unsigned getter/estimate simulations only.
// No sign/submit method, wallet, environment variable or identity-file access.
const S = require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const fs = require('node:fs');
const crypto = require('node:crypto');
const path = require('node:path');
const dir=__dirname;
const endpoint='https://rpc.lightsail.network/';
const server=new S.rpc.Server(endpoint,{timeout:30});
const pool='CAPIOQNULTKVYOJT6X2W2XKGNIVUWZDY72Y42YG6HQKJ7DU7YTHIDQYX';
const source='GBNZILSTVQZ4R7IKQDGHYGY2QXL5QOFJYQMXPKWRRM5PAV7Y4M67AQUA';
const assets={PYUSD:new S.Asset('PYUSD','GDQE7IXJ4HUHV6RQHIUPRJSEZE4DRS5WY577O2FY6YQ5LVWZ7JZTU2V5'),USDC:new S.Asset('USDC','GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN')};
const json=x=>JSON.stringify(x,(_,v)=>typeof v==='bigint'?v.toString():v,2);
const result={retrievedAt:new Date().toISOString(),endpoint,scope:'external venue data / unsigned read-only getter simulations; rolling ledger, not atomic snapshot',assetIDs:Object.fromEntries(Object.entries(assets).map(([k,v])=>[k,v.contractId(S.Networks.PUBLIC)])),views:[],entries:[]};
function cdkey(id,key){return S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract:new S.Address(id).toScAddress(),key,durability:S.xdr.ContractDataDurability.persistent()}));}
async function entry(id,key){const r=await server.getLedgerEntries(cdkey(id,key));result.entries.push({contract:id,key:key.toXDR('base64'),ledger:r.latestLedger,entries:r.entries.map(e=>({key:e.key.toXDR('base64'),xdr:e.val.toXDR('base64'),liveUntilLedgerSeq:e.liveUntilLedgerSeq,value:e.val.contractData().val().switch().name==='scvContractInstance'?{wasmHash:e.val.contractData().val().instance().executable().wasmHash().toString('hex'),storage:(e.val.contractData().val().instance().storage()??[]).map(v=>[S.scValToNative(v.key()),S.scValToNative(v.val())])}:S.scValToNative(e.val.contractData().val())}))});}
(async()=>{
result.network=await server.getNetwork();if(result.network.passphrase!==S.Networks.PUBLIC)throw Error('network mismatch');{const latest=await server.getLatestLedger(); result.latest={id:latest.id,sequence:latest.sequence,protocolVersion:latest.protocolVersion,closeTime:latest.closeTime};}const acct=await server.getAccount(source);
async function view(id,method,args=[],label=method){let row={contract:id,method,label};try{const tx=new S.TransactionBuilder(acct,{fee:'100',networkPassphrase:S.Networks.PUBLIC}).addOperation(new S.Contract(id).call(method,...args)).setTimeout(60).build();const sim=await server.simulateTransaction(tx);row.ledger=sim.latestLedger;if(S.rpc.Api.isSimulationSuccess(sim)){row.value=S.scValToNative(sim.result.retval);row.returnXdr=sim.result.retval.toXDR('base64');row.resourceFee=sim.minResourceFee;row.requiresRestore=!!sim.restorePreamble;}else row.error=sim.error;}catch(e){row.error=e.message;}result.views.push(row);fs.writeFileSync(path.join(dir,'chain-read.json'),json(result)+'\n');}
await entry(pool,S.xdr.ScVal.scvLedgerKeyContractInstance());
for(const method of ['get_tokens','get_reserves','get_fee_fraction','get_protocol_fee_fraction','get_active_liquidity','get_total_shares','get_slot0'])await view(pool,method);
await view(pool,'get_rewards_info',[new S.Address(source).toScVal()]);
for(const capital of [1000,10000])for(const [name,lo,hi]of [['full',-887260,887260],['wide',-2000,2000]])await view(pool,'estimate_deposit_position',[S.nativeToScVal(lo,{type:'i32'}),S.nativeToScVal(hi,{type:'i32'}),S.xdr.ScVal.scvVec([S.nativeToScVal(BigInt(capital)*5000000n,{type:'u128'}),S.nativeToScVal(BigInt(capital)*5000000n,{type:'u128'})])],name+'-'+capital);
for(const id of ['CBAKAZUEJBAFA2DCGBRS2WKJT3FNIMJBHTFL76AY7XMAVEINQNNIQWAO','CAJJZSGMMM3PD7N33TAPHGBUGTB43OC73HVIK2L2G6BNGGGYOSSYBXBD']){await entry(id,S.xdr.ScVal.scvLedgerKeyContractInstance());for(const sym of ['ResConfig','ResData'])await entry(id,S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol(sym),new S.Address(assets.USDC.contractId(S.Networks.PUBLIC)).toScVal()]));}
for(const capital of [1000,10000]){await view(pool,'estimate_swap',[S.nativeToScVal(0,{type:'u32'}),S.nativeToScVal(1,{type:'u32'}),S.nativeToScVal(BigInt(capital)*5000000n,{type:'u128'})],'convert-PYUSD-half-'+capital);await view(pool,'estimate_swap',[S.nativeToScVal(1,{type:'u32'}),S.nativeToScVal(0,{type:'u32'}),S.nativeToScVal(BigInt(capital)*5000000n,{type:'u128'})],'convert-USDC-half-'+capital);}
fs.writeFileSync(path.join(dir,'chain-read.json'),json(result)+'\n');console.log(json(result.views));console.log(result.entries.map(e=>({contract:e.contract,ledger:e.ledger,value:e.entries.map(v=>v.value)})));
})().catch(e=>{fs.writeFileSync(path.join(dir,'chain-read-failure.json'),json({partial:result,error:e.message})+'\n');console.error(e);process.exitCode=1});
