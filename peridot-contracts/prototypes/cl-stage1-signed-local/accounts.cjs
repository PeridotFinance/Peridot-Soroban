// PUBLIC deterministic fixture keys ONLY. Never import wallet configuration or contact a network.
const fs=require('fs'),path=require('path'),crypto=require('crypto');
const S=require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const {xdr,Address,Keypair,scValToNative}=S;
const PRIVATE='Peridot Stage1 signature application tests ONLY 2026-11-10';
const testKey=role=>Keypair.fromRawEd25519Seed(crypto.createHash('sha256').update('PUBLIC TEST FIXTURE / '+PRIVATE+' / '+role).digest());
const owner=testKey('owner'),keeper=testKey('keeper');
const COWNER='CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABDWC6';
const mapped=new Address(owner.publicKey()).toScAddress();
function map(v){
 if(v instanceof xdr.ScAddress && Address.fromScAddress(v).toString()===COWNER)return mapped;
 if(v===null||v===undefined||Buffer.isBuffer(v)||typeof v!=='object')return v;
 if(Array.isArray(v))return v.map(map);
 if(v._attributes)for(const k of Object.keys(v._attributes))v._attributes[k]=map(v._attributes[k]);
 if(Object.prototype.hasOwnProperty.call(v,'_value') && typeof v._value==='object')v._value=map(v._value);
 return v;
}
function entry(data){return new xdr.LedgerEntry({lastModifiedLedgerSeq:0,data,ext:new xdr.LedgerEntryExt(0)});}
function keyFor(e){const d=e.data();switch(d.switch().name){case'account':return xdr.LedgerKey.account(new xdr.LedgerKeyAccount({accountId:d.account().accountId()}));case'trustline':return xdr.LedgerKey.trustline(new xdr.LedgerKeyTrustLine({accountId:d.trustLine().accountId(),asset:d.trustLine().asset()}));case'contractCode':return xdr.LedgerKey.contractCode(new xdr.LedgerKeyContractCode({hash:d.contractCode().hash()}));case'contractData':return xdr.LedgerKey.contractData(new xdr.LedgerKeyContractData({contract:d.contractData().contract(),key:d.contractData().key(),durability:d.contractData().durability()}));default:throw Error('unexpected import '+d.switch().name);}}
function packet(e,ttl){return{entry_xdr:e.toXDR('base64'),key_xdr:keyFor(e).toXDR('base64'),ttl};}
function account(k,subs=0){return entry(xdr.LedgerEntryData.account(new xdr.AccountEntry({accountId:xdr.PublicKey.publicKeyTypeEd25519(k.rawPublicKey()),balance:xdr.Int64.fromString('100000000000000'),seqNum:xdr.Int64.fromString('0'),numSubEntries:subs,inflationDest:null,flags:0,homeDomain:'',thresholds:Buffer.from([1,1,1,1]),signers:[],ext:new xdr.AccountEntryExt(0)})));}
function convert(snapshot){
 const entries=snapshot.entries.map(r=>({e:xdr.LedgerEntry.fromXDR(r.entry_xdr,'base64'),ttl:r.ttl}));
 const assets=new Map(),balances=new Map();
 const nativePlaceholders=new Set(entries.filter(({e})=>e.data().switch().name==='contractCode'&&e.data().contractCode().code().toString().startsWith('test_contract_for_contract_id_')).map(({e})=>e.data().contractCode().hash().toString('hex')));
 for(const {e} of entries){const d=e.data();if(d.switch().name!=='contractData')continue;const c=d.contractData();
  if(c.val().switch().name==='scvContractInstance' && c.val().instance().executable().switch().name==='contractExecutableStellarAsset'){
   for(const kv of c.val().instance().storage()||[]){const key=scValToNative(kv.key());if(Array.isArray(key)&&key[0]==='AssetInfo'){
    const info=scValToNative(kv.val());if(info[0]!=='AlphaNum4')throw Error('only fixture alpha4');
    const a=new xdr.AlphaNum4({assetCode:Buffer.from(info[1].asset_code),issuer:xdr.PublicKey.publicKeyTypeEd25519(info[1].issuer)});
    assets.set(Address.fromScAddress(c.contract()).toString(),xdr.TrustLineAsset.assetTypeCreditAlphanum4(a));
   }}
  }
 }
 const out=[];let removedAuthorizers=0;
 for(const {e,ttl} of entries){const d=e.data();
  if(d.switch().name==='contractCode'&&nativePlaceholders.has(d.contractCode().hash().toString('hex')))continue;
  if(d.switch().name==='contractData'){
   const c=d.contractData(),addr=Address.fromScAddress(c.contract()).toString();
   if(c.val().switch().name==='scvContractInstance' && c.val().instance().executable().switch().name==='contractExecutableWasm' && nativePlaceholders.has(c.val().instance().executable().wasmHash().toString('hex'))){removedAuthorizers++;continue;}
   const k=scValToNative(c.key());
   if(assets.has(addr)&&Array.isArray(k)&&k[0]==='Balance'&&k[1]===COWNER){balances.set(addr,scValToNative(c.val()).amount);continue;}
  }
  // SDK issuer accounts are synthetic too; bound their native balance for Core totalCoins.
  if(d.switch().name==='account')d.account().balance(xdr.Int64.fromString('1000000000000'));
  out.push(packet(map(e),ttl));
 }
 for(const [addr,asset] of assets){const tl=new xdr.TrustLineEntry({accountId:xdr.PublicKey.publicKeyTypeEd25519(owner.rawPublicKey()),asset,balance:xdr.Int64.fromString(String(balances.get(addr)||0n)),limit:xdr.Int64.fromString('9223372036854775807'),flags:1,ext:new xdr.TrustLineEntryExt(0)});out.push(packet(entry(xdr.LedgerEntryData.trustline(tl)),null));}
 out.push(packet(account(owner,assets.size),null),packet(account(keeper),null));
 return {entries:out,removedAuthorizers,trustlines:assets.size};
}
if(require.main===module){
 const [capture,outDir]=process.argv.slice(2);fs.mkdirSync(outDir,{recursive:false});
 const txs=fs.readFileSync(path.join(capture,'transactions.jsonl'),'utf8').trim().split('\n').map(JSON.parse);
 const selected=txs.filter(r=>r.test_name.endsWith('compiled_stage1_full_open_close')||r.test_name.endsWith('compiled_stage1_full_open_in_kind_close')&&r.flow==='stage_withdraw');
 for(const r of selected){
  const stem=r.test_name.replaceAll('::','_')+'-'+r.current_ledger+'-'+r.flow;
  const snapshotPath=path.resolve(capture,stem+'.snapshot.json');
  const state=JSON.parse(fs.readFileSync(path.resolve(capture,stem+'.snapshot.packet.json')));
  const converted=convert(state);const env=map(xdr.TransactionEnvelope.fromXDR(r.unsigned_envelope_xdr,'base64'));
  const hf=env.v1().tx().operations()[0].body().invokeHostFunctionOp().hostFunction();
  const label=r.flow+(r.test_name.endsWith('in_kind_close')?'_in_kind':'');
  const dir=path.resolve(outDir,label);fs.mkdirSync(dir);
  const source=r.expected_external_authorizers.length?owner:keeper;
  const input={schema:'cl-private-signature-input-v1',network_passphrase:PRIVATE,mode:'record',snapshot_path:snapshotPath,source_account_xdr:xdr.PublicKey.publicKeyTypeEd25519(source.rawPublicKey()).toXDR('base64'),host_function_xdr:hf.toXDR('base64'),auth_xdr:[],entries:converted.entries,result_path:path.join(dir,'recorded.json'),ledger:state.ledger,timestamp:state.timestamp,owner:owner.publicKey(),keeper:keeper.publicKey(),flow:r.flow,baseline_envelope_sha256:r.envelope_sha256,removed_sdk_authorizer_instances:converted.removedAuthorizers,preexisting_test_trustlines:converted.trustlines};
  fs.writeFileSync(path.join(dir,'input.json'),JSON.stringify(input));
 }
 console.log('Private fixture prepared; no signatures yet.');
}
module.exports={PRIVATE,testKey,owner,keeper,xdr,Address,S,map,keyFor,packet};
