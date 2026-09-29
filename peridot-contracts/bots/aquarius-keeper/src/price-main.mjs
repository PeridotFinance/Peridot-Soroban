// Separate opt-in process. Existing main.mjs / live keeper settings are untouched.
// Mainnet observation is read-only; signing is fenced to isolated Testnet until
// the new oracle and complete lending release pass deployment review.
import {Asset,Address,Contract,Keypair,Networks,TransactionBuilder,nativeToScVal,scValToNative,rpc,StrKey} from '@stellar/stellar-sdk';
import {collectWindow} from './price-collector.mjs';
import {POLICY,runObservationCycle} from './observation.mjs';
// Do not serialize SDK exceptions, which may contain RPC headers or envelopes.
process.on('uncaughtException',error=>{
  const reason=String(error?.message??'pricing stopped').replace(/https?:\/\/\S+/g,'[endpoint]').replace(/\bS[A-Z2-7]{55}\b/g,'[secret]').slice(0,300);
  console.error(JSON.stringify({state:'stopped',reason}));process.exitCode=1;
});
const env=process.env, network=env.PRICE_NETWORK==='testnet'?Networks.TESTNET:Networks.PUBLIC;
if(env.PRICE_NETWORK && !['mainnet','testnet'].includes(env.PRICE_NETWORK))throw Error('invalid PRICE_NETWORK');
const live=env.PRICE_MODE==='publish';
if(env.PRICE_MODE && !['observe','publish'].includes(env.PRICE_MODE))throw Error('invalid PRICE_MODE');
if(live && (network!==Networks.TESTNET || env.CONFIRM_PRICE_TESTNET!=='ISOLATED_ORACLE'))throw Error('Mainnet pricing publication is release-gated');
const mainnet=network===Networks.PUBLIC;
const target={asset:env.PRICE_ASSET??(mainnet?'CBZVSNVB55ANF24QVJL2K5QCLOAB6XITGTGXYEAF6NPTXYKEJUYQOHFC':undefined),
  quote:env.PRICE_QUOTE??(mainnet?'CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA':undefined),
  pool:env.PRICE_POOL??(mainnet?'CADMDTCQHSC2GCYPDCYQ7FBYVOXED3E3WGCYJHHT524ZBALM7VYHFS7F':undefined),router:env.PRICE_ROUTER};
for(const [k,v] of Object.entries(target))if((k!=='router'||live) && !StrKey.isValidContract(v??''))throw Error(`PRICE_${k.toUpperCase()} is required/invalid`);
let key=null;
if(live){try{key=Keypair.fromSecret(env.PRICE_REPORTER_SECRET??'');}catch{throw Error('invalid pricing reporter secret');}}
const source=key?.publicKey()??env.PRICE_REPORTER_PUBLIC_KEY??(mainnet?'GDYDTMY46RNAUIIUVG6RPD2D3I3ES4J2SSXGCKIQP2OET4Q5PV75LSPL':undefined);
if(!StrKey.isValidEd25519PublicKey(source??''))throw Error('PRICE_REPORTER_PUBLIC_KEY required');
if(key && env.PRICE_REPORTER_PUBLIC_KEY && source!==env.PRICE_REPORTER_PUBLIC_KEY)throw Error('reporter key mismatch');
const server=new rpc.Server(env.PRICE_RPC_URL??(mainnet?'https://soroban-rpc.mainnet.stellar.gateway.fm':'https://soroban-testnet.stellar.org'),{timeout:15_000});
if((await server.getNetwork()).passphrase!==network)throw Error('RPC network mismatch');
const addr=a=>new Address(a).toScVal();
const log=x=>console.log(JSON.stringify(x,(_,v)=>typeof v==='bigint'?v.toString():v));
async function build(id,method,args=[]) {
  return new TransactionBuilder(await server.getAccount(source),{networkPassphrase:network,fee:'100000'})
    .addOperation(new Contract(id).call(method,...args)).setTimeout(60).build();
}
async function read(id,method,args=[]) {
  const sim=await server.simulateTransaction(await build(id,method,args));
  if(!rpc.Api.isSimulationSuccess(sim)||sim.restorePreamble)throw Error(`${method} unavailable/restoration needed`);
  return scValToNative(sim.result.retval);
}
async function write(method,args) {
  if(!live)throw Error('read-only pricing mode');
  const tx=await build(target.router,method,args),sim=await server.simulateTransaction(tx);
  if(!rpc.Api.isSimulationSuccess(sim)||sim.restorePreamble)throw Error(`${method} simulation failed/restoration needed`);
  if(!(sim.result.auth??[]).every(a=>a.credentials().switch().name==='sorobanCredentialsSourceAccount'))throw Error('unexpected pricing signer');
  const prepared=rpc.assembleTransaction(tx,sim).build();
  if(BigInt(prepared.fee)>1_000_000n)throw Error('pricing fee exceeds 0.1 XLM cap');
  prepared.sign(key);const hash=prepared.hash().toString('hex');log({state:'submitting',hash,method});
  // No write retries. A failure or timeout exits the process; reconcile hash first.
  const sent=await server.sendTransaction(prepared);
  if(!['PENDING','DUPLICATE'].includes(sent.status))throw Error(`pricing submission stopped: ${hash}`);
  for(let i=0;i<30;i++){
    const result=await server.getTransaction(hash);
    if(result.status==='SUCCESS'){log({state:'confirmed',hash,ledger:result.ledger});return;}
    if(result.status==='FAILED')throw Error(`pricing transaction failed: ${hash}`);
    await new Promise(resolve=>setTimeout(resolve,1000));
  }
  throw Error(`pricing transaction unresolved, do not restart before reconciliation: ${hash}`);
}
const name=await read(target.asset,'name');
if(typeof name!=='string')throw Error('asset metadata unavailable');
const [code,issuer,...extra]=name.split(':');
if(extra.length || !code || !StrKey.isValidEd25519PublicKey(issuer??'') || new Asset(code,issuer).contractId(network)!==target.asset)throw Error('SAC asset identity mismatch');
if(Asset.native().contractId(network)!==target.quote)throw Error('pricing quote must be native XLM');
if(await read(target.asset,'decimals')!==7 || await read(target.quote,'decimals')!==7)throw Error('pricing requires seven-decimal pair');
const tokens=await read(target.pool,'get_tokens'),inIdx=tokens.indexOf(target.asset),outIdx=tokens.indexOf(target.quote);
if(tokens.length!==2||inIdx<0||outIdx<0||inIdx===outIdx)throw Error('pricing pool identity mismatch');
const probe=1_000_000_000n; // 100 units each direction; candidate policy, no exposure approval.
if(live){
  const s=await read(target.router,'get_source',[addr(target.asset)]),c=Array.isArray(s)&&s[0]==='Observed'?s[1]:null;
  if(!c || c.reporter!==source || c.pool!==target.pool || c.quote_to!==target.quote || c.in_idx!==inIdx || c.out_idx!==outIdx
    || c.probe_amount!==probe || c.window_secs!==BigInt(POLICY.windowSeconds) || c.max_age_secs!==BigInt(POLICY.maxAgeSeconds)
    || c.max_deviation_bps!==Number(POLICY.maxDeviationBps) || c.min_interval_secs!==300n || c.max_step_bps!==100
    || c.min_ratio!==POLICY.minRatio || c.max_ratio!==POLICY.maxRatio)throw Error('on-chain observation policy mismatch');
}
const now=()=>Math.floor(Date.now()/1000);
async function cycle(){
  return runObservationCycle({now,
    readPrevious:()=>live?read(target.router,'get_observation',[addr(target.asset)]):Promise.resolve(null),
    collect:()=>collectWindow({horizonUrl:env.PRICE_HORIZON_URL??(mainnet?'https://horizon.stellar.org':'https://horizon-testnet.stellar.org'),networkPassphrase:network,code,issuer,now}),
    quotes:async()=>({probe,sell:await read(target.pool,'estimate_swap',[nativeToScVal(inIdx,{type:'u32'}),nativeToScVal(outIdx,{type:'u32'}),nativeToScVal(probe,{type:'u128'})]),
      buy:await read(target.pool,'estimate_swap',[nativeToScVal(outIdx,{type:'u32'}),nativeToScVal(inIdx,{type:'u32'}),nativeToScVal(probe,{type:'u128'})])}),
    publish:async r=>live?write('publish_observation',[addr(source),addr(target.asset),nativeToScVal(r.ratio,{type:'u128'}),nativeToScVal(r.start,{type:'u64'}),nativeToScVal(r.end,{type:'u64'})]):log({state:'observation-only',...r}),
    invalidate:()=>write('invalidate_observation',[addr(source),addr(target.asset)])});
}
if(process.argv.slice(2).some(a=>a!=='--loop'))throw Error('unknown pricing argument');
do {
  const result=await cycle();log({...result,state:!live&&result.state==='published'?'observed':result.state,live});
  if(!process.argv.includes('--loop'))break;
  await new Promise(resolve=>setTimeout(resolve,60_000));
}while(true);
