import test from 'node:test';
import assert from 'node:assert/strict';
import {Account,Address,Contract,Keypair,Networks,StrKey,TransactionBuilder,nativeToScVal} from '@stellar/stellar-sdk';
import {verifyReplayResume} from '../src/depth-replay-resume.mjs';
const contract=n=>StrKey.encodeContract(Buffer.alloc(32,n));
function fixture() {
  const key=Keypair.random(),manifest={network:Networks.TESTNET,reporter:key.publicKey(),router:contract(1),asset:contract(2)};
  const observed={valid:true,ratio:999_000_000_000n,start:1000n,end:2800n};
  const tx=new TransactionBuilder(new Account(manifest.reporter,'1'),{fee:'100',networkPassphrase:Networks.TESTNET})
    .addOperation(new Contract(manifest.router).call('publish_observation',
      new Address(manifest.reporter).toScVal(),new Address(manifest.asset).toScVal(),
      nativeToScVal(observed.ratio,{type:'u128'}),nativeToScVal(observed.start,{type:'u64'}),nativeToScVal(observed.end,{type:'u64'})))
    .setTimebounds(0,2830).build();
  tx.sign(key);
  const hash=tx.hash().toString('hex');
  return {manifest,hash,transaction:{status:'SUCCESS',txHash:hash,envelopeXdr:tx.toEnvelope()},
    post:{observed,recovery:null,dependent:null},now:4000,allowExpired:true};
}
test('explicit expired recovery verifies the exact included observation without claiming timely restart',()=>{
  const f=fixture();
  assert.deepEqual(verifyReplayResume(f),{expired:true,timelyRestartVerified:false,referenceRatio:f.post.observed.ratio});
  assert.throws(()=>verifyReplayResume({...f,allowExpired:false}),/expired/);
  f.now=2850;f.allowExpired=false;f.post.dependent=[{price:1n},14,60];
  assert.equal(verifyReplayResume(f).timelyRestartVerified,true);
  assert.throws(()=>verifyReplayResume({...f,allowExpired:true}),/expired/);
});
test('expired replay rejects mismatched hash, network, report, recovery, price and source',()=>{
  for(const mutate of [f=>f.transaction.status='NOT_FOUND',f=>f.transaction.txHash='0'.repeat(64),
    f=>f.hash='0'.repeat(64),f=>f.manifest.network=Networks.PUBLIC,
    f=>f.manifest.router=contract(3),f=>f.manifest.asset=contract(3),
    f=>f.manifest.reporter=Keypair.random().publicKey(),f=>f.post.observed.ratio++,
    f=>f.post.observed.end++,f=>f.post.observed.valid=false,
    f=>f.post.recovery={},f=>f.post.dependent={},f=>f.now=2000,f=>f.now=NaN]) {
    const f=fixture();mutate(f);assert.throws(()=>verifyReplayResume(f));
  }
});
