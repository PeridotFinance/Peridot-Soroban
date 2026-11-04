// Decode exact transaction keys; SDK invocation-entry totals are not footprints.
const fs = require('node:fs');
const crypto = require('node:crypto');
const {xdr, StrKey, scValToNative} = require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const [run, output, flowFilter] = process.argv.slice(2);
if (!run || !output || fs.existsSync(output)) throw Error('Need run and fresh output');
const read = name => fs.readFileSync(`${run}/${name}`,'utf8').trim().split('\n').filter(Boolean).map(JSON.parse);
const transactions = read('transactions.jsonl'), calls = read('calls.jsonl');
const roles = new Map();
for (const r of calls) for (const c of r.calls) if (c.address && c.role !== 'unlabelled') roles.set(c.address,c.role);
const codeOwners=new Map();
for(const r of transactions)for(const item of r.entry_size_checks||[]){
 const e=xdr.LedgerEntry.fromXDR(item.entry_xdr,'base64').data();
 if(e.switch().name!=='contractData')continue;
 const d=e.contractData();
 if(d.val().switch().name!=='scvContractInstance'||d.contract().switch().name!=='scAddressTypeContract')continue;
 const executable=d.val().instance().executable();
 if(executable.switch().name!=='contractExecutableWasm')continue;
 const h=executable.wasmHash().toString('hex'), address=StrKey.encodeContract(d.contract().contractId());
 if(!codeOwners.has(h))codeOwners.set(h,new Set());
 codeOwners.get(h).add(address);
}
const json = value => JSON.stringify(value, (_,v) => typeof v === 'bigint' ? v.toString() : v);
const hash = data => crypto.createHash('sha256').update(data).digest('hex');
function decode(encoded) {
  const k=xdr.LedgerKey.fromXDR(encoded,'base64');
  if(k.switch().name === 'contractCode') {
    const h=k.contractCode().hash().toString('hex');
    return {contract:'code:'+h,contracts_using_code:[...(codeOwners.get(h)||[])].map(address=>({address,role:roles.get(address)||'unlabelled'})),role:'code',key:'WASM',kind:'code'};
  }
  if(k.switch().name !== 'contractData') return {contract:null,role:'host',key:k.switch().name,kind:k.switch().name};
  const d=k.contractData(), a=d.contract();
  const contract=a.switch().name==='scAddressTypeContract'?StrKey.encodeContract(a.contractId()):StrKey.encodeEd25519PublicKey(a.accountId().ed25519());
  const key=d.key().switch().name === 'scvLedgerKeyContractInstance' ? 'INSTANCE' : d.key().switch().name === 'scvLedgerKeyNonce' ? 'AUTH_NONCE' : scValToNative(d.key());
  return {contract,role:roles.get(contract)||'unlabelled',key,kind:d.durability().name};
}
// Assign missing roles from exact stored key identities, not guessed addresses.
for(const r of transactions) if(r.footprint) for(const k of [...r.footprint.read_only_xdr,...r.footprint.read_write_xdr]) {
  const d=decode(k);
  if(Array.isArray(d.key)&&d.key[0]==='BaseRatePerYear') roles.set(d.contract,'interest_model');
}
function purpose(d) {
  const n=Array.isArray(d.key)?d.key[0]:d.key;
  if(n==='WASM'||n==='INSTANCE') return ['runtime','Load executable/instance state and authenticate nested contract calls.'];
  if(n==='AUTH_NONCE') return ['authorization','External authorization replay protection; unsigned template only.'];
  if(d.role==='vault') {
    if(/History|Boundary/.test(n)) return ['required-reward-accounting','Preserve historical accrual boundary, interval and beneficiary weight; unresolved records remain retained.'];
    if(/Allocation|Beneficial|Registered|Total|Transfer/.test(n)) return ['required-custody-auth','Verify registered custodian and reassign beneficial ownership at the actual claim-first boundary.'];
    return ['required-reward-accounting','Share balance / holder reward debt and pending entitlement settlement.'];
  }
  if(d.role==='aquarius') return ['required-venue','Actual pinned Aquarius position, tick/range, liquidity or reward accounting invoked by claim/deposit/withdraw/swap.'];
  if(d.role==='v3') return ['required-state-machine','Position ownership/status/debt/collateral identities, pending state and applicable execution/fee terms; settlement or recovery transition.'];
  if(/receipt/.test(d.role)) {
    if(['Admin','PendingAdmin','FlashLoanFeeScaled','CollateralFactorScaled'].includes(n)) return ['broad-maintenance-candidate','Core keepalive sweep; determine whole-root removal from measured candidate. CollateralFactor is still required on ordinary health branches.'];
    if(n==='SupplyCap') return ['operation-dependent','Supply admission cap for deposit; otherwise broad keepalive candidate.'];
    if(n==='BorrowCap'||n==='TotalBorrowPrincipal') return ['required-accounting-or-branch','Borrow cap/principal bookkeeping on borrow or repayment; optional missing-key probe also occupies footprint.'];
    if(n==='Initialized'||n==='UnderlyingToken') return ['required-identity','Operation initialization and exact underlying-token identity.'];
    if(n==='MarginController'||n==='Peridottroller'||n==='FlashLoanActive'||n==='MarginWithdrawBypassV2') return ['required-authorization','Controller identity, reentry/flash-loan exclusion or one-use guarded withdrawal authorization.'];
    if(/Balance|HasBorrowed|BorrowSnapshots|BorrowPrincipal/.test(n)) return ['required-accounting','Receipt balances or user/position debt principal/index snapshot; transfer/repayment bookkeeping.'];
    if(/Boosted|IdleCash|ReserveBps|BorrowingEnabled/.test(n)) return ['required-branch','Optional strategy/valuation/cash-reserve policy branch; missing state must remain safe.'];
    return ['interest-or-valuation','Interest accrual, cash, fee/reserve totals or share exchange-rate valuation; may be sweep-only for a specific zero-debt branch.'];
  }
  if(d.role==='lending_controller') {
    if(['Admin','CloseFactorScaled','LiquidationFeeScaled','LiquidationIncentiveScaled','ReserveRecipient'].includes(n)) return ['broad-maintenance-candidate','Global controller sweep; these V3 roots use stored position liquidation terms, not ordinary-liquidation defaults.'];
    if(n==='SupportedMarkets') return ['broad-maintenance-candidate','Global market-list sweep; exact support is checked by SupportedToken on these roots.'];
    if(/^Pause/.test(n)) return ['required-risk-check','Applicable pause gate; recovery preserves its existing allowed/blocked behavior.'];
    if(/Oracle|PriceCache|SupportedToken/.test(n)) return ['required-valuation','Fresh oracle identity/policy, exact market support and price-cache validation.'];
    return ['required-branch','Account-market health or optional reward-controller branch; no global bypass.'];
  }
  if(d.role==='interest_model') return [n==='Admin'?'broad-maintenance-unchanged':'required-interest-model','Rate-model parameter/read maintenance; model is unchanged in this experiment.'];
  if(d.role==='adapter') return ['binding-authorization','Allowed direct-pool binding; adapter maintenance is unchanged.'];
  if(['underlying','paired','reward'].includes(d.role)) return ['required-token-accounting','Token balance, movement/refund or claim receipt measurement.'];
  if(d.role==='facade') return ['required-facade','Allowed controller, exact nested swap authorization and operation balance deltas.'];
  return ['required-runtime-or-dependency','Unmodified dependency key; exact address/XDR retained for audit, not proposed for removal.'];
}
const roots=[];
for(const flow of (flowFilter ? flowFilter.split(',') : ['liquidate_funded_v3','activate_open_position_v3','swap_close_position_v3','swap_open_position_v3'])) {
  const options=transactions.map((r,i)=>({r,i})).filter(({r})=>r.flow===flow&&r.success&&r.resources);
  options.sort((a,b)=>b.r.resources.write_entries-a.r.resources.write_entries||b.r.resources.instructions-a.r.resources.instructions);
  if(!options.length) throw Error('Missing complete root: '+flow);
  const {r,i}=options[0], restored=new Set(r.restored_rw_entry_indices.map(n=>r.footprint.read_write_xdr[n]));
  const keys=[];
  for(const status of ['read_only','read_write']) for(const encoded of r.footprint[status+'_xdr']) {
    const d=decode(encoded), [classification,reason]=purpose(d);
    keys.push({...d,key_xdr:encoded,key_sha256:hash(Buffer.from(encoded,'base64')),status,restored:restored.has(encoded),classification,reason});
  }
  roots.push({flow,row_zero_based:i,test:r.test_name,selection:'maximum writes, then CPU; not necessarily maximum memory/CPU row',resources:r.resources,fees:r.fees,keys});
}
const result={schema:'cl-storage-per-key-v1',source:run,source_sha256:hash(fs.readFileSync(`${run}/transactions.jsonl`)),classification_scope:'Source-reviewed purpose categories; per-key removal must be demonstrated by candidate comparison. SDK entry totals are separate.',roots};
fs.writeFileSync(output,json(result)+'\n');
console.log(json(roots.map(r=>({flow:r.flow,row:r.row_zero_based,keys:r.keys.length,rw:r.keys.filter(k=>k.status==='read_write').length,restored:r.keys.filter(k=>k.restored).length}))));
