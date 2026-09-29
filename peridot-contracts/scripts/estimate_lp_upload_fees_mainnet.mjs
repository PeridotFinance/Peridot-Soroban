// Unsigned upload simulations ONLY. These are fenced VALIDATION artifacts, not
// release candidates. No keys, signing, submission, or approval implied.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {createRequire} from 'node:module';
const require=createRequire(new URL('../bots/aquarius-keeper/package.json',import.meta.url));
const S=require('@stellar/stellar-sdk');
assert.equal(process.argv.length,2,'no options or execution mode');
const artifacts=[
  ['lp_lending_vault','68ce982f34c1784ccc911463b160e5133bbdba1d8da2821dae6df1dccdf52199'],
  ['lp_peridottroller','9c7c92d3c161f2a761ceef6bbf40bcefe427b0594375d362a537a17575bcfcdb'],
  ['aquarius_lp_vault','c23d93b935c4f22dac3c1c33ccebaef40ed9f1e6b6da1b5ba4cfff322ed8ab1e'],
  ['price_router','20114bc89bb2323fdfbb5e1588795bbc88dade5264e2d161e34d2b82c7c3759d'],
].map(([name,sha256])=>{
  const wasm=readFileSync(new URL(`../target/wasm32v1-none/release/${name}.wasm`,import.meta.url));
  assert.equal(createHash('sha256').update(wasm).digest('hex'),sha256,`unexpected ${name} bytes`);
  return {name,sha256,wasm};
});
const rpc=new S.rpc.Server('https://soroban-rpc.mainnet.stellar.gateway.fm',{timeout:30000});
assert.equal((await rpc.getNetwork()).passphrase,S.Networks.PUBLIC);
const deployer='GDYDTMY46RNAUIIUVG6RPD2D3I3ES4J2SSXGCKIQP2OET4Q5PV75LSPL';
const account=await rpc.getAccount(deployer),results=[];
for(const {name,sha256,wasm} of artifacts){
  const tx=new S.TransactionBuilder(account,{fee:'100',networkPassphrase:S.Networks.PUBLIC})
    .addOperation(S.Operation.uploadContractWasm({wasm})).setTimeout(60).build();
  const sim=await rpc.simulateTransaction(tx);
  assert(S.rpc.Api.isSimulationSuccess(sim),`${name} upload simulation failed`);
  assert(!sim.restorePreamble,`${name} needs separate restoration estimate`);
  const resource=BigInt(sim.minResourceFee),r=sim.transactionData.build().resources();
  results.push({name,sha256,bytes:wasm.length,ledger:sim.latestLedger,minResourceFeeStroops:String(resource),
    // One operation, nominal base fee only. Not a surge-price or signing budget.
    totalWithNominalBaseStroops:String(resource+100n),instructions:r.instructions()});
}
console.log(JSON.stringify({time:new Date().toISOString(),deployer,validationArtifactsOnly:true,
  limitation:'Preliminary four-code upload estimate only; final production bytes, creation, initialization, JRM, bindings, migration, canary, inclusion surge and safety margin are excluded. No final deployment budget or approval.',
  results,totalWithNominalBaseStroops:String(results.reduce((n,r)=>n+BigInt(r.totalWithNominalBaseStroops),0n))},null,2));
