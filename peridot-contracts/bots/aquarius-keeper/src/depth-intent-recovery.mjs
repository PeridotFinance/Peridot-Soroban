// Operator-only recovery for ONE reviewed legacy Testnet intent without metadata.
// NOT imported by the reporter; never retires unknown hashes on a timeout alone.
import assert from 'node:assert/strict';
import {Account,Address,Networks,Operation,TransactionBuilder} from '@stellar/stellar-sdk';
import {validateDepthManifest,depthJournalScope,createDepthTransport} from './depth-publisher.mjs';
import {requireClosedLedger} from './depth-ledger-clock.mjs';
export const LEGACY_RECOVERY=Object.freeze({
  scope:'16e8e00b0d97b9394010ce2b655b3d5422f0dc45923cc3517bb4deb24f6cde78',
  hash:'f24bf11b911d0f03abefbc786e232124baf88adea1f353084b908e43f33eca5a',
  anchor:'dbaa682f137c787d7c14fa53f477f837de54f408815caa2bd79faedd034fb94a',
  source:'GCQFJG4JVPI4SLBOAHMQOO27JGA6II6NZWCVUY2B5L6TKLFDPON3HND7',
  sequence:'20594093306413067',bumpTo:'20594093306413069',
  // Last observed failure plus a day, not a claim to recover the missing envelope.
  notBefore:1790540777,
});
const L=LEGACY_RECOVERY;
export function validateFence(record) {
  const tx=TransactionBuilder.fromXDR(record.unsignedXdr,Networks.TESTNET);
  assert.equal(tx.hash().toString('hex'),record.hash);
  assert.equal(record.bumpTo,L.bumpTo);assert.equal(tx.source,L.source);
  assert.equal(tx.sequence,String(BigInt(L.sequence)+1n));
  assert.equal(tx.fee,'10000');assert.equal(tx.signatures.length,0);
  assert.equal(tx.operations.length,1);assert.equal(tx.operations[0].type,'bumpSequence');
  assert.equal(tx.operations[0].bumpTo,L.bumpTo);assert.equal(tx.operations[0].source,undefined);
  assert.equal(tx.memo.type,'none');
  assert(Number.isSafeInteger(Number(tx.timeBounds?.minTime))&&Number(tx.timeBounds.minTime)>=L.notBefore);
  assert.equal(Number(tx.timeBounds.maxTime)-Number(tx.timeBounds.minTime),60);
  const exact=new TransactionBuilder(new Account(L.source,L.sequence),{fee:'10000',networkPassphrase:Networks.TESTNET})
    .addOperation(Operation.bumpSequence({bumpTo:L.bumpTo}))
    .setTimebounds(Number(tx.timeBounds.minTime),Number(tx.timeBounds.maxTime)).build();
  assert.equal(tx.signatureBase().toString('hex'),exact.signatureBase().toString('hex'),'unexpected fence preconditions');
  return tx;
}
function anchor(record,m) {
  assert(record?.successful===true&&record.hash===L.anchor,'missing confirmed anchor');
  const tx=TransactionBuilder.fromXDR(record.envelope_xdr,Networks.TESTNET);
  assert.equal(tx.hash().toString('hex'),L.anchor);assert.equal(tx.source,L.source);
  assert.equal(tx.sequence,L.sequence);assert.equal(tx.operations.length,1);
  const call=tx.operations[0].func.invokeContract();
  assert.equal(Address.fromScAddress(call.contractAddress()).toString(),m.router);
  assert.equal(call.functionName().toString(),'publish_observation');
}
export async function recoverLegacyIntent({manifest,journal,server,history,loadKey,now,sleep,emit=()=>{}}) {
  const m=validateDepthManifest(manifest);
  assert.equal(depthJournalScope(m),L.scope);assert.equal(m.reporter,L.source);
  assert.equal((await server.getNetwork()).passphrase,Networks.TESTNET);
  assert.equal(await history.network(),Networks.TESTNET);
  const pending=journal.pending();
  assert(pending?.hash===L.hash&&pending.method==='publish_observation'&&!pending.details,'not the reviewed legacy intent');
  // No key read until all read-only state checks and durable fence intent succeed.
  const verify=async()=>{
    await requireClosedLedger(await server.getLatestLedger(),now);
    assert(Number.isSafeInteger(now())&&now()>=L.notBefore,'legacy review time guard');
    const transport=createDepthTransport({manifest:m,server,key:{publicKey:()=>m.reporter},now});
    await transport.verify();
    anchor(await history.lookup(L.anchor),m);
    assert.equal(await history.lookup(L.hash),null,'original intent included: stop for review');
  };
  await verify();
  let record=journal.fence();
  if(!record) {
    const a=await server.getAccount(L.source);assert.equal(a.accountId(),L.source);
    assert.equal(a.sequenceNumber(),L.sequence,'unexpected sequence: do not infer recovery');
    const tx=new TransactionBuilder(new Account(L.source,L.sequence),{fee:'10000',networkPassphrase:Networks.TESTNET})
      .addOperation(Operation.bumpSequence({bumpTo:L.bumpTo})).setTimebounds(now(),now()+60).build();
    record={hash:tx.hash().toString('hex'),unsignedXdr:tx.toXDR(),bumpTo:L.bumpTo};
    validateFence(record);
    await journal.fenceIntent(L.hash,record); // crash => reconcile ONLY, never resend
    await verify();
    assert.equal((await server.getAccount(L.source)).sequenceNumber(),L.sequence);
    assert(now()<Number(tx.timeBounds.maxTime)-10,'fence expired before signing');
    const key=await loadKey();assert.equal(key.publicKey(),L.source);
    tx.sign(key);
    // Never retain, print or return the signed envelope.
    const sent=await server.sendTransaction(tx);
    assert.equal(sent.hash,record.hash);assert(['PENDING','DUPLICATE'].includes(sent.status),'fence send unresolved');
    emit({kind:'depth_fence_submitted',hash:record.hash,network:'testnet'});
  }
  const expected=validateFence(record);
  let included=null;
  for(let n=0;n<20;n++) {
    included=await history.lookup(record.hash);
    if(included)break;
    if(n<19)await sleep(1000);
  }
  assert(included?.successful===true,'fence unresolved/failed: preserve intent, never resend');
  const tx=TransactionBuilder.fromXDR(included.envelope_xdr,Networks.TESTNET);
  assert.equal(included.hash,record.hash);assert.equal(tx.hash().toString('hex'),record.hash);
  assert.equal(tx.signatureBase().toString('hex'),expected.signatureBase().toString('hex'));
  await verify();
  const a=await server.getAccount(L.source);assert.equal(a.accountId(),L.source);
  assert.equal(a.sequenceNumber(),L.bumpTo,'fence account state mismatch');
  await journal.retire(L.hash,record.hash);
  emit({kind:'depth_intent_retired',hash:L.hash,fenceHash:record.hash,network:'testnet'});
  return {hash:L.hash,fenceHash:record.hash};
}
