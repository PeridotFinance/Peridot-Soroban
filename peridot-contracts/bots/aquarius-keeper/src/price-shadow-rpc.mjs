// Unsigned quote simulations only. No key access or transaction submission.
import assert from 'node:assert/strict';
import {Asset,Contract,Networks,TransactionBuilder,nativeToScVal,scValToNative,rpc} from '@stellar/stellar-sdk';
import {YXLM_ISSUER} from './price-diagnostics.mjs';
const POOL='CADMDTCQHSC2GCYPDCYQ7FBYVOXED3E3WGCYJHHT524ZBALM7VYHFS7F';
const SOURCE='GDYDTMY46RNAUIIUVG6RPD2D3I3ES4J2SSXGCKIQP2OET4Q5PV75LSPL';
export async function aquariusSnapshot() {
  const server=new rpc.Server('https://soroban-rpc.mainnet.stellar.gateway.fm',{timeout:15_000});
  assert.equal((await server.getNetwork()).passphrase,Networks.PUBLIC);
  const account=await server.getAccount(SOURCE),ledgers=[];
  const read=async(method,args=[])=>{
    const tx=new TransactionBuilder(account,{networkPassphrase:Networks.PUBLIC,fee:'100'})
      .addOperation(new Contract(POOL).call(method,...args)).setTimeout(60).build();
    const sim=await server.simulateTransaction(tx);
    assert(rpc.Api.isSimulationSuccess(sim) && !sim.restorePreamble, `${method} unavailable/restoration required`);
    ledgers.push(sim.latestLedger);return scValToNative(sim.result.retval);
  };
  const tokens=await read('get_tokens');
  const xlm=Asset.native().contractId(Networks.PUBLIC),yxlm=new Asset('yXLM',YXLM_ISSUER).contractId(Networks.PUBLIC);
  assert(Array.isArray(tokens)&&tokens.length===2);
  const input=tokens.indexOf(yxlm),output=tokens.indexOf(xlm);
  assert(input>=0&&output>=0&&input!==output,'pool asset mismatch');
  const quote=(i,j)=>read('estimate_swap',[nativeToScVal(i,{type:'u32'}),nativeToScVal(j,{type:'u32'}),nativeToScVal(1000000000n,{type:'u128'})]);
  const sell=await quote(input,output),buy=await quote(output,input);
  assert(typeof sell==='bigint'&&typeof buy==='bigint'&&sell>0n&&buy>0n,'empty Aquarius quote');
  assert(ledgers.every(Number.isSafeInteger)&&Math.max(...ledgers)-Math.min(...ledgers)<=6,'incoherent RPC quote sample');
  return {pool:POOL,ledgerMin:Math.min(...ledgers),ledgerMax:Math.max(...ledgers),
    probeRaw:'1000000000',soldForRaw:sell.toString(),boughtRaw:buy.toString()};
}
