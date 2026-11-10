// Can sign ONLY public deterministic test accounts on this hard-coded private ledger.
// No RPC, wallet configuration, environment secrets, user keys or configurable network.
const fs=require('fs'),path=require('path'),crypto=require('crypto');
const {PRIVATE,owner,keeper,xdr,S}=require('./accounts.cjs');
async function main(){
const dir=path.resolve(process.argv[2]);const input=JSON.parse(fs.readFileSync(path.join(dir,'input.json')));
if(input.network_passphrase!==PRIVATE||input.owner!==owner.publicKey()||input.keeper!==keeper.publicKey())throw Error('private test scope mismatch');
const result=JSON.parse(fs.readFileSync(path.join(dir,'recorded.json')));if(!result.success)throw Error('preflight failed');
const envelope=xdr.TransactionEnvelope.fromXDR(result.unsigned_envelope_xdr,'base64');
const tx=envelope.v1().tx();const source=S.StrKey.encodeEd25519PublicKey(tx.sourceAccount().ed25519());
const signer=[owner,keeper].find(k=>k.publicKey()===source);if(!signer)throw Error('nonfixture source refused');
if(envelope.v1().signatures().length)throw Error('already signed');
const op=tx.operations()[0].body().invokeHostFunctionOp();
let detached=false;const auth=[];
for(const a of op.auth()){
 if(a.credentials().switch().name==='sorobanCredentialsSourceAccount'){auth.push(a);continue;}
 if(a.credentials().switch().name!=='sorobanCredentialsAddress')throw Error('only explicit v1 detached authorization supported');
 if(S.Address.fromScAddress(a.credentials().address().address()).toString()!==owner.publicKey())throw Error('nonfixture address signature refused');
 auth.push(await S.authorizeEntry(a,owner,input.ledger+100,PRIVATE));detached=true;
}
op.auth(auth);
input.auth_xdr=auth.map(a=>a.toXDR('base64'));input.mode=detached?'enforcing-detached':'enforcing-source-account';input.result_path=path.join(dir,'enforced.json');
fs.writeFileSync(path.join(dir,'enforce-input.json'),JSON.stringify(input));
// Explicit 100M reservation; actual consumed instructions are reported independently.
const feeCeiling=Math.max(10000000,Math.ceil(result.fees.total*1.1)+10000);if(feeCeiling>4000000000)throw Error('private fee ceiling overflow');
const sd=tx.ext().sorobanData();sd.resources().instructions(100000000);sd.resourceFee(xdr.Int64.fromString(String(feeCeiling-100)));tx.fee(feeCeiling);tx.seqNum(xdr.Int64.fromString(String(BigInt(input.source_sequence||'0')+1n)));
const signed=new S.Transaction(envelope,PRIVATE);signed.sign(signer);
const signedEnvelope=xdr.TransactionEnvelope.fromXDR(signed.toXDR(),'base64');
if(signedEnvelope.v1().signatures().length!==1||!signer.verify(signed.hash(),signedEnvelope.v1().signatures()[0].signature()))throw Error('fixture signature verification failed');
fs.writeFileSync(path.join(dir,'signed-private-envelope.xdr'),signed.toXDR()+'\n');
const config=JSON.parse(fs.readFileSync(path.resolve(__dirname,'../../../docs/deployment-handoffs/evidence/cl-resource-decision-refresh-20261102/network-config.json')));
const settings=Object.values(config.settings).map(s=>xdr.LedgerEntryData.fromXDR(s.xdr,'base64'));
for(const d of settings){const c=d.configSetting();switch(c.switch().name){case'configSettingContractComputeV0':c.contractCompute().txMaxInstructions(xdr.Int64.fromString('100000000'));break;case'configSettingContractLedgerCostV0':c.contractLedgerCost().txMaxWriteLedgerEntries(50);c.contractLedgerCost().txMaxDiskReadEntries(100);break;case'configSettingContractLedgerCostExtV0':c.contractLedgerCostExt().txMaxFootprintEntries(100);break;}}
const lines=[PRIVATE,input.ledger,input.timestamp,signed.toXDR(),settings.length,...settings.map(s=>s.toXDR('base64')),input.entries.length];
for(const e of input.entries)lines.push(e.entry_xdr,e.ttl||0);
fs.writeFileSync(path.join(dir,'core-input.txt'),lines.join('\n')+'\n');
fs.writeFileSync(path.join(dir,'signature-evidence.json'),JSON.stringify({network:PRIVATE,source,owner:input.owner,signature_verified_locally:true,signer_is_public_test_fixture:true,signature_count:1,envelope_bytes:Buffer.from(signed.toXDR(),'base64').length,envelope_sha256:crypto.createHash('sha256').update(Buffer.from(signed.toXDR(),'base64')).digest('hex'),declared_cpu:100000000,fee_ceiling_stroops:feeCeiling,preflight_fee_components:result.fees,network_broadcast:false},null,2));
console.log('Signed private test envelope; no network operation.');

}
main().catch(e=>{console.error(e.message);process.exitCode=1;});
