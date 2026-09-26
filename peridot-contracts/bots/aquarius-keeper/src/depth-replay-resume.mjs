import assert from 'node:assert/strict';
import {Address,Networks,TransactionBuilder,scValToNative} from '@stellar/stellar-sdk';

// Test harness only: an expired report is NOT evidence of a timely restart.
// Verify that this exact confirmed transaction produced the retained observation
// before permitting the separately selected expired-report recovery experiment.
export function verifyReplayResume({transaction,hash,manifest,post,now,allowExpired=false}) {
  assert.equal(manifest.network,Networks.TESTNET,'Testnet resume only');
  assert.equal(transaction.status,'SUCCESS','publication not confirmed');
  assert.equal(transaction.txHash,hash,'publication hash mismatch');
  const tx=TransactionBuilder.fromXDR(transaction.envelopeXdr,Networks.TESTNET);
  assert.equal(tx.hash().toString('hex'),hash,'publication envelope mismatch');
  assert.equal(tx.source,manifest.reporter,'publication reporter mismatch');
  assert.equal(tx.operations.length,1,'unexpected publication operations');
  assert.equal(tx.operations[0].type,'invokeHostFunction');
  assert.equal(tx.operations[0].func.switch().name,'hostFunctionTypeInvokeContract');
  const call=tx.operations[0].func.invokeContract();
  assert.equal(Address.fromScAddress(call.contractAddress()).toString(),manifest.router);
  assert.equal(call.functionName().toString(),'publish_observation');
  assert(post.observed?.valid===true&&post.recovery===null,'unexpected replay state');
  const o=post.observed;
  assert.deepEqual(call.args().map(scValToNative),[manifest.reporter,manifest.asset,o.ratio,o.start,o.end],
    'confirmed publication differs from retained observation');
  assert(Number.isSafeInteger(now)&&o.start>0n&&o.end>o.start&&o.end<=BigInt(now),'invalid report times');
  const expired=BigInt(now)-o.end>300n;
  if(allowExpired) {
    assert(expired&&post.dependent===null,'explicit expired-report recovery requires expired unavailable price');
  } else {
    assert(!expired&&post.dependent!==null,'first publication expired before restart verification');
  }
  return {expired,timelyRestartVerified:!expired,referenceRatio:o.ratio};
}
