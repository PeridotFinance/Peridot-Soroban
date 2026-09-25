// Isolated Testnet replay only. The production observer never imports this file.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {isDeepStrictEqual} from 'node:util';
import {Address,Asset,Contract,Networks,StrKey,TransactionBuilder,nativeToScVal,scValToNative,rpc,xdr} from '@stellar/stellar-sdk';
import {planDepthPublication,DEPTH_PRICING_POLICY} from './depth-pricing.mjs';
import {crossCheck} from './observation.mjs';
import {requireClosedLedger} from './depth-ledger-clock.mjs';
const addr=a=>new Address(a).toScVal();
const canonical=value=>JSON.stringify(value,(_,v)=>typeof v==='bigint'?v.toString():v);
const probe=1_000_000_000n;
export function validateDepthManifest(value) {
  assert(value?.kind==='isolated-depth-price-replay-v1'&&value.network===Networks.TESTNET,'Mainnet publication is release-gated');
  const m={kind:value.kind,network:value.network};
  for(const field of ['router','asset','quote','pool']) {
    assert(StrKey.isValidContract(value[field]??''),'invalid fixture contract');m[field]=value[field];
  }
  assert(new Set([m.router,m.asset,m.quote,m.pool]).size===4,'fixture contracts must differ');
  assert(m.quote===Asset.native().contractId(Networks.TESTNET),'fixture quote must be Testnet XLM');
  assert(StrKey.isValidEd25519PublicKey(value.reporter??''),'invalid reporter');m.reporter=value.reporter;
  // Never borrow the established Mainnet deployer identity for this experiment.
  assert(m.reporter!=='GDYDTMY46RNAUIIUVG6RPD2D3I3ES4J2SSXGCKIQP2OET4Q5PV75LSPL','isolated reporter required');
  for(const field of ['routerWasmHash','poolWasmHash']) {
    assert(/^[a-f0-9]{64}$/.test(value[field]??''),'pinned fixture code required');m[field]=value[field];
  }
  assert([0,1].includes(value.inIndex),'invalid asset index');m.inIndex=value.inIndex;
  return Object.freeze(m);
}
export const depthJournalScope=m=>createHash('sha256').update(canonical(validateDepthManifest(m))).digest('hex');

export function createDepthTransport({manifest,server,key,now}) {
  const m=validateDepthManifest(manifest),source=m.reporter;
  assert(key.publicKey()===source,'fixture signer mismatch');
  let floorLedger=0;
  let verifiedConfig=null;
  async function latest() {
    const l=await server.getLatestLedger();
    await requireClosedLedger(l,now);
    assert(Number.isSafeInteger(l.sequence)&&l.sequence>=floorLedger,'RPC ledger went backwards');
    floorLedger=l.sequence;return l;
  }
  function ledger(sequence) {
    assert(Number.isSafeInteger(sequence)&&sequence>=floorLedger,'stale simulation/entry');
    floorLedger=sequence;
  }
  async function build(id,method,args=[],until=now()+30) {
    assert(Number.isSafeInteger(until)&&until>now(),'expired transaction');
    const account=await server.getAccount(source);
    assert(account.accountId()===source,'RPC source account mismatch');
    return new TransactionBuilder(account,{networkPassphrase:Networks.TESTNET,fee:'100000'})
      .addOperation(new Contract(id).call(method,...args)).setTimebounds(0,until).build();
  }
  async function read(id,method,args=[]) {
    const s=await server.simulateTransaction(await build(id,method,args));
    assert(rpc.Api.isSimulationSuccess(s)&&!s.restorePreamble&&s.result,'read simulation/restoration guard');
    ledger(s.latestLedger);return scValToNative(s.result.retval);
  }
  async function code(id,expected) {
    const k=xdr.LedgerKey.contractData(new xdr.LedgerKeyContractData({contract:new Address(id).toScAddress(),
      key:xdr.ScVal.scvLedgerKeyContractInstance(),durability:xdr.ContractDataDurability.persistent()}));
    const r=await server.getLedgerEntries(k);ledger(r.latestLedger);
    assert(r.entries.length===1&&r.entries[0].key.toXDR('base64')===k.toXDR('base64'),'missing/wrong contract instance');
    const executable=r.entries[0].val.contractData().val().instance().executable();
    assert(executable.switch().name==='contractExecutableWasm'&&executable.wasmHash().toString('hex')===expected,'fixture code mismatch');
  }
  const transport={
    async verify() {
      assert((await server.getNetwork()).passphrase===Networks.TESTNET,'RPC network mismatch');
      await latest();await code(m.router,m.routerWasmHash);await code(m.pool,m.poolWasmHash);
      assert(canonical(await read(m.pool,'get_tokens'))===canonical(m.inIndex===0?[m.asset,m.quote]:[m.quote,m.asset]),'fixture pool tokens mismatch');
      assert(await read(m.asset,'decimals')===7&&await read(m.quote,'decimals')===7,'fixture decimals mismatch');
      const sourceConfig=await read(m.router,'get_source',[addr(m.asset)]);
      const c=Array.isArray(sourceConfig)&&sourceConfig[0]==='Observed'?sourceConfig[1]:null;
      assert(c&&c.reporter===source&&c.pool===m.pool&&c.quote_to===m.quote&&c.in_idx===m.inIndex&&c.out_idx===1-m.inIndex
        &&c.probe_amount===probe&&c.window_secs===1800n&&c.max_age_secs===300n&&c.min_interval_secs===120n
        &&c.max_deviation_bps===100&&c.max_step_bps===100&&c.min_ratio===DEPTH_PRICING_POLICY.minRatio
        &&c.max_ratio===DEPTH_PRICING_POLICY.maxRatio,'fixture observation policy mismatch');
      verifiedConfig=c;
      await latest();
    },
    previous:()=>read(m.router,'get_observation',[addr(m.asset)]),
    async recovery() {
      const r=await read(m.router,'get_observation_recovery',[addr(m.asset)]);
      // A changed source requires a NEW approval, never adapt the old approval.
      if(r!==null&&r!==undefined)assert(isDeepStrictEqual(r.config,verifiedConfig),'recovery source changed');
      return r??null;
    },
    async quotes() {
      const arg=(v,type)=>nativeToScVal(v,{type});
      const quote=(i,j)=>read(m.pool,'estimate_swap',[arg(i,'u32'),arg(j,'u32'),arg(probe,'u128')]);
      return {probe,sell:await quote(m.inIndex,1-m.inIndex),buy:await quote(1-m.inIndex,m.inIndex)};
    },
    async prepare(method,candidate) {
      assert(['publish_observation','publish_recovery_observation','invalidate_observation'].includes(method),'unexpected publication method');
      const args=[addr(source),addr(m.asset)];
      const publishing=method!=='invalidate_observation';
      if(publishing)args.push(nativeToScVal(BigInt(candidate.ratio),{type:'u128'}),
        nativeToScVal(BigInt(candidate.start),{type:'u64'}),nativeToScVal(BigInt(candidate.end),{type:'u64'}));
      // Inclusion is bounded by both report freshness and a thirty-second TTL.
      const until=publishing?Math.min(now()+30,candidate.end+60):now()+30;
      const tx=await build(m.router,method,args,until),sim=await server.simulateTransaction(tx);
      assert(rpc.Api.isSimulationSuccess(sim)&&!sim.restorePreamble&&sim.result,'publication simulation/restoration guard');
      ledger(sim.latestLedger);
      const auth=sim.result.auth??[];
      assert(auth.length===1&&auth[0].credentials().switch().name==='sorobanCredentialsSourceAccount','unexpected publication signer');
      // Reporter must authorize only this exact invocation, never nested actions.
      const root=auth[0].rootInvocation(),fn=root.function();
      assert(root.subInvocations().length===0&&fn.switch().name==='sorobanAuthorizedFunctionTypeContractFn','unexpected authorization tree');
      const authorized=fn.contractFn();
      assert(authorized.contractAddress().toXDR('base64')===new Address(m.router).toScAddress().toXDR('base64')
        &&authorized.functionName().toString()===method
        &&canonical(authorized.args().map(v=>v.toXDR('base64')))===canonical(args.map(v=>v.toXDR('base64'))),'authorization invocation mismatch');
      const prepared=rpc.assembleTransaction(tx,sim).build();
      assert(BigInt(prepared.fee)<=1_000_000n,'publication exceeds 0.1 XLM fee limit');
      return {hash:prepared.hash().toString('hex'),until,
        async send() {
          assert(now()<until,'transaction expired before signing');
          prepared.sign(key);
          const sent=await server.sendTransaction(prepared);
          assert(sent.hash===prepared.hash().toString('hex')&&['PENDING','DUPLICATE'].includes(sent.status),'submission unresolved');
        }};
    },
    async status(hash) {
      const r=await server.getTransaction(hash);
      assert(r.txHash===hash&&['SUCCESS','FAILED','NOT_FOUND'].includes(r.status),'transaction status mismatch');
      return {status:r.status,ledger:r.ledger};
    }
  };
  return transport;
}

// All calls serialized by caller + exclusive journal lock. Reconciliation never
// resends an envelope, treats NOT_FOUND as terminal, or authorizes another hash.
export function createDepthPublisher({transport,journal,now,sleep,emit=()=>{},monotonic=()=>performance.now()}) {
  let busy=false,poisoned=false;
  const exclusive=fn=>async(...args)=>{
    assert(!busy&&!poisoned,'publisher stopped or concurrent cycle');busy=true;
    try {return await fn(...args);}catch(error){poisoned=true;throw error;}finally{busy=false;}
  };
  async function confirmation(hash) {
    // A restarted process may beat ledger inclusion/RPC ingestion. Poll only
    // the durable hash, with bounded attempts/time; never rebuild or rebroadcast.
    const deadline=monotonic()+30_000;
    for(let attempt=0;attempt<20;attempt++) {
      const result=await transport.status(hash);
      if(result.status!=='NOT_FOUND')return result;
      if(attempt===0)emit({kind:'depth_confirmation_pending',hash,network:'testnet'});
      if(attempt===19||monotonic()>=deadline)break;
      await sleep(1000);
      if(monotonic()>=deadline)break;
    }
    emit({kind:'depth_confirmation_unresolved',hash,network:'testnet'});
    throw Error('unresolved transaction: manual hash reconciliation required');
  }
  async function reconcile() {
    assert(!journal.failed(),'failed transaction requires operator review');
    const pending=journal.pending();if(!pending)return;
    const result=await confirmation(pending.hash);
    await journal.resolve(pending.hash,result.status);
    assert(result.status==='SUCCESS','failed transaction requires operator review');
    emit({kind:'depth_reconciled',hash:pending.hash,ledger:result.ledger,network:'testnet'});
  }
  async function decision(input) {
    const previous=await transport.previous();
    const recovery=await transport.recovery();
    const plan=planDepthPublication({...input,now:now(),previous,recovery});
    assert(plan.action!=='halt','invalid on-chain observation state');
    if(plan.candidate.state==='candidate') {
      try {
        const q=await transport.quotes();crossCheck({ratio:BigInt(plan.candidate.ratio)},q.sell,q.buy,q.probe);
      } catch {
        return {...plan,action:previous.valid?'invalidate':'hold',reason:'live_crosscheck_failed',previous,recovery};
      }
    }
    return {...plan,previous,recovery};
  }
  return {
    reconcile:exclusive(async()=>{await transport.verify();await reconcile();}),
    cycle:exclusive(async input=>{
      await transport.verify();await reconcile();
      const plan=await decision(input);
      if(plan.action==='hold')return {action:'hold',reason:plan.reason};
      const method=plan.action==='publish_candidate'?'publish_observation':
        plan.action==='publish_recovery_candidate'?'publish_recovery_observation':'invalidate_observation';
      const prepared=await transport.prepare(method,plan.candidate);
      // No cached policy, observation or pool quote is sufficient for signing.
      await transport.verify();
      const fresh=await decision(input);
      if(fresh.action!==plan.action||canonical(fresh.previous)!==canonical(plan.previous)
        ||canonical(fresh.recovery)!==canonical(plan.recovery)
        ||canonical(fresh.candidate)!==canonical(plan.candidate)||now()>=prepared.until)
        return {action:'hold',reason:'state_or_freshness_changed_before_signing'};
      await journal.intent(prepared.hash,method); // durable BEFORE any signature/send
      await prepared.send();
      emit({kind:'depth_submitted',method,hash:prepared.hash,network:'testnet'});
      const result=await confirmation(prepared.hash);
      await journal.resolve(prepared.hash,result.status);
      assert(result.status==='SUCCESS','publication failed');
      emit({kind:'depth_confirmed',method,hash:prepared.hash,ledger:result.ledger,network:'testnet'});
      return {action:'confirmed',method,hash:prepared.hash};
    })
  };
}
