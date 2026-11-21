// Mainnet signer for the CL liquidator (HG4), modelled on bots/cash-liquidator (key file on a Droplet).
//
// Nothing here runs unless the config names the exact Mainnet passphrase. Live sending additionally needs `--live`,
// DRY_RUN=false in the environment, the start checks (network, code pins, roles, single instance) and a passed
// dry-run gate. The Testnet CLI-alias executor in chain.mjs is unchanged and stays Testnet-only.
//
// Every send: build (source = the configured account) -> simulate -> refuse unless the call is on the per-stage
// allowlist with the exact arguments, every authorization entry is source-account credentials for exactly that root
// call with no nested invocation, no restore is needed, declared CPU <= 97M and the fee <= its cap and the day's
// budget -> code pins re-read -> sign with the key file -> journal the hash (onSigned) -> send once -> poll.
// An unknown outcome (no final status within the poll window) is returned as `unknown`, so the caller leaves the
// journal intent unresolved. Nothing is ever re-sent automatically. Errors and logs never contain key material.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {MAINNET, loadSdk} from './chain.mjs';
import {loadKeyFile} from './keyfile.mjs';

export const CPU_STOP = 97_000_000;
// Accounts the CL liquidator and heartbeat keys must never be (admin Ledger, cash liquidator, cash pause key, keeper).
export const FORBIDDEN_SIGNERS = Object.freeze({
  GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP: 'admin Ledger',
  GDQARPHRQFU7K52XMESPNBSKZE6GPCORZV54EBGDVSXJQBOIDUMO5B5G: 'cash liquidator',
  GBSK3FYCGNYZLHHPSYRJNE53NZXDFL6ED24KLT3GUUETLFKDAUD7PQX3: 'cash pause key',
  GBHWPV6QTQIAPJI7WHLHWZJB4GELG3B5NAR2NJ27FQNKSIGYGBYFZON4: 'investment keeper',
});
// Pilot and retired contracts (addresses.md / clg-mainnet.json `pilot` + `retired`): never a target.
export const NEVER_TOUCH = Object.freeze(['CCGKTKPAWQHKI3TMX4RHLLZ3WYFELHERVNWOWMAZUHTKIAU2ENMHXBCI', 'CD6N2EQJN47SYLSIYUK3YVBKD4YISATZ2CZZJQZCXKNNVB4HVR6AWL63',
  'CA2FIPJ7U6BG3N7EOZFI74XPJZOEOD4TYWXFVCIO5VDCHTVAGS6F4UKK', 'CAB4JOLSCNELJVDQKZLVGHKWJCLXFDBZZMITJAFL4GBGTHIKWO47PYFH', 'CANCOWOI6R2FZBDLZKUL6BUZJN3VONPZSUUSWFL3KF3MPG5INAF25EKY',
  'CAQZ7XPUSOSBI66A4RPSNPEBI2EADBMBVUBSW6R2DYWC64QHDM3HKGIN', 'CB3WLG4QITFRELACDR74N63VEPICMQ35QW3DSAMF4KCFOITKOJSHH6RW', 'CBIOHQFWKSYTRET3LJV4LTO3ZQWRQMQ7I2SJAZ62IZCQHDST4YO3AZP7',
  'CBNVNCPEW2XXGBEGVMZQXBSODO5V2HMGPT5FFLVLT355SXJMYY53MLMA', 'CBP2R5KYAWJCOCVDTSNTEVL3O6JBTWOOH7SZOX7DX5DLGVZCAMLBDZM3', 'CBRJTPI3327YPP57KGIZIU4Z6APBUN5F6LJ2Q3MPKCISUQJLAQFFZECZ',
  'CBU4Y7CJFOUZZE3QBOXTKM54UTUYW3SDJWTNMDGJBNCR5HS5UCEKV3BE', 'CBVUJJIJTRJNOORPPCVH72DP7YDCOMDHI6WYKP3WOFVEPSCVP3TBXHIN', 'CCB2AR5X3KP4WQKE7HNSUSDS7SHFMC2WPVSZ2ZXJ6DHXOKHFFKOZE6GK',
  'CCI5LBBNYOASPQ62GIRY54PDEYWWURJB75HNRAFOU4LTOU3XBC73IB5I', 'CCKTLDG6I2MMJCKFWXXBXMA42LJ3XN2IOW6M7TK6EWNPJTS736ETFF2N', 'CCPJFBH5WSNZVMCUQCBM4X5334L6ZL3W4Q33XJAK45RCDHJ2JGJ5AP6A',
  'CCVUFGXKFVPAHWMMDDL6HXKUN2B2G73Z27VRM3WXZBBSQEUTNLI6YPEX', 'CCZKDMAP23ZFL55RVITKSW4LAONQABGPSK2Y77RS64GPHOVDDFF5ENGC', 'CD3WN3PLW63HFZXE56OTRLMBV46WG54TFPGRL4RDQ43HQTTWVB4RPO3G',
  'CDMH535JSD224YXPET3B4SJOLXTQQ24GRSCWACGYBKSH2DKFJYWI7SUW', 'CDNJSOJKURHQUDBO7OHK7Z64R2CNMIAWXENHM24ALK7Y3H56EU6PUOKR']);
const G = /^G[A-Z2-7]{55}$/, C = /^C[A-Z2-7]{55}$/, H64 = /^[0-9a-f]{64}$/;
const PIN_ROLES = ['receipt', 'vault', 'controller', 'usdcMarket'];
// Per-transaction RESOURCE fee ceilings (stroops; the padded simulated resource fee, i.e. what the envelope authorises,
// rent included). Testnet: the first checkpoint_nav on fresh code silently paid 70.97 XLM of rent. Above its ceiling a
// call is refused (alert), never signed. Light calls 0.5 XLM, liquidation / seizure / claim / exit calls 2 XLM.
export const RESOURCE_CEILING_DEFAULTS = Object.freeze({checkpoint_nav: 5_000_000, checkpoint_fallback: 5_000_000, cache_price: 5_000_000, approve: 5_000_000,
  checkpoint_rewards: 20_000_000, liquidate: 20_000_000, sync_seized: 20_000_000,
  stage_prepare_close: 20_000_000, stage_withdraw: 20_000_000, stage_emergency_exit: 20_000_000, stage_emergency_pay: 20_000_000});
const FEE_DEFAULTS = {inclusionFeeStroops: 100_000, maxFeeStroops: 21_000_000, dailyBudgetStroops: 50_000_000, resourceCeilingStroops: RESOURCE_CEILING_DEFAULTS,
  heartbeat: {maxFeeStroops: 5_100_000, resourceCeilingStroops: 5_000_000, dailyBudgetStroops: 50_000_000}};
export const isMainnet = cfg => cfg?.networkPassphrase === MAINNET;

/** Offline validation of the Mainnet-only config fields (config.mjs calls it when the passphrase is Mainnet). */
export function validateMainnetConfig(out) {
  if (!isMainnet(out)) throw Error('validateMainnetConfig: not a Mainnet config');
  for (const k of ['executorIdentity', 'heartbeatIdentity']) if (out[k] != null) throw Error(`refused: ${k} (CLI alias) is Testnet-only; use ${k === 'executorIdentity' ? 'executorKeyFile' : 'heartbeatKeyFile'} on Mainnet`);
  for (const [k, v] of Object.entries(out.contracts)) {
    if (!C.test(String(v))) throw Error(`config.contracts.${k} must be a contract id (placeholder left?)`);
    if (NEVER_TOUCH.includes(v)) throw Error(`refused: config.contracts.${k} is a pilot/retired contract`);
  }
  for (const r of PIN_ROLES) if (!H64.test(out.pins?.[r] ?? '')) throw Error(`config.pins.${r} (64-hex WASM hash) required on Mainnet`);
  if (!path.isAbsolute(out.executorKeyFile ?? '')) throw Error('config.executorKeyFile (absolute path of the 0600 key file) required on Mainnet');
  // The heartbeat is required on Mainnet: with it the plan loop never sends D3 L1a from the liquidator, and the
  // debt gate (makeDebtGatedHeartbeat) decides when D3 observations are sent at all.
  if (out.heartbeatAccount == null) throw Error('heartbeatAccount (and heartbeatKeyFile, heartbeatRpcUrls/heartbeatRpcUrlFiles) required on Mainnet');
  if (out.heartbeatAccount != null || out.heartbeatKeyFile != null) {
    if (!G.test(out.heartbeatAccount ?? '') || out.heartbeatAccount === out.liquidator) throw Error('heartbeatAccount must be a public G... address distinct from the liquidator');
    if (!path.isAbsolute(out.heartbeatKeyFile ?? '') || out.heartbeatKeyFile === out.executorKeyFile) throw Error('heartbeatKeyFile must be an absolute path distinct from executorKeyFile');
    if (out.heartbeatPeriodMs != null && !(out.heartbeatPeriodMs >= 3000 && out.heartbeatPeriodMs <= 15000)) throw Error('heartbeatPeriodMs must be 3000..15000 (on-chain gap target <= 20 s)');
    // Heartbeat RPC failover (5f48b0d) is REQUIRED on Mainnet: >= 2 RPCs in order, heartbeatRpcUrls (plain, e.g.
    // gateway.fm, primary) then heartbeatRpcUrlFiles (0600 files holding a URL that carries an API key, e.g. Alchemy).
    // The file contents are read and checked at start (readUrlFile), never put into the config, a log or the journal.
    const urls = out.heartbeatRpcUrls ?? [], files = out.heartbeatRpcUrlFiles ?? [];
    if (!Array.isArray(urls) || !urls.every(u => /^https:\/\/[^\s@]+$/.test(u)) || new Set(urls).size !== urls.length) throw Error('heartbeatRpcUrls must be distinct https URLs without credentials');
    if (!Array.isArray(files) || !files.every(f => typeof f === 'string' && path.isAbsolute(f)) || new Set(files).size !== files.length) throw Error('heartbeatRpcUrlFiles must be distinct absolute paths');
    if ([out.executorKeyFile, out.heartbeatKeyFile].some(k => files.includes(k))) throw Error('heartbeatRpcUrlFiles must not name a key file');
    if (urls.length + files.length < 2) throw Error('heartbeatRpcUrls + heartbeatRpcUrlFiles: at least 2 RPCs are required on Mainnet');
    if (out.heartbeatFeeStroops != null && !(Number.isInteger(out.heartbeatFeeStroops) && out.heartbeatFeeStroops >= 100 && out.heartbeatFeeStroops <= 1_000_000)) throw Error('heartbeatFeeStroops must be an integer 100..1000000');
  }
  for (const a of [out.liquidator, out.heartbeatAccount].filter(Boolean)) {
    if (FORBIDDEN_SIGNERS[a]) throw Error(`refused: signer is the ${FORBIDDEN_SIGNERS[a]}`);
    if (out.team3 && a === out.team3) throw Error('refused: signer is T3');
    if (out.admin && a === out.admin) throw Error('refused: signer is the admin');
  }
  if (out.team3 != null && !G.test(out.team3)) throw Error('config.team3 must be null or a public G... address');
  for (const b of out.borrowers) if (!G.test(b) || b === out.liquidator || b === out.heartbeatAccount) throw Error('config.borrowers must be public G... addresses other than the bot accounts');
  const fees = {...FEE_DEFAULTS, ...out.fees, resourceCeilingStroops: {...RESOURCE_CEILING_DEFAULTS, ...out.fees?.resourceCeilingStroops},
    heartbeat: {...FEE_DEFAULTS.heartbeat, ...out.fees?.heartbeat}};
  for (const [fn, v] of Object.entries(fees.resourceCeilingStroops)) {
    if (!(fn in RESOURCE_CEILING_DEFAULTS)) throw Error(`config.fees.resourceCeilingStroops.${fn}: unknown call`);
    if (!Number.isSafeInteger(v) || v <= 0 || v > 50_000_000) throw Error(`config.fees.resourceCeilingStroops.${fn} must be 1..50000000 stroops`);
    if (v + fees.inclusionFeeStroops > fees.maxFeeStroops) throw Error(`config.fees.resourceCeilingStroops.${fn} + inclusion fee exceeds maxFeeStroops`);
  }
  const hc = fees.heartbeat.resourceCeilingStroops;
  if (!Number.isSafeInteger(hc) || hc <= 0 || hc > 5_000_000) throw Error('config.fees.heartbeat.resourceCeilingStroops must be 1..5000000 (0.5 XLM)');
  for (const [k, v] of [['inclusionFeeStroops', fees.inclusionFeeStroops], ['maxFeeStroops', fees.maxFeeStroops], ['dailyBudgetStroops', fees.dailyBudgetStroops],
    ['heartbeat.maxFeeStroops', fees.heartbeat.maxFeeStroops], ['heartbeat.dailyBudgetStroops', fees.heartbeat.dailyBudgetStroops]])
    if (!Number.isSafeInteger(v) || v <= 0) throw Error(`config.fees.${k} must be a positive integer (stroops)`);
  if (fees.inclusionFeeStroops > fees.maxFeeStroops || fees.maxFeeStroops > 50_000_000 || fees.maxFeeStroops > fees.dailyBudgetStroops) throw Error('fees: inclusion <= per-tx cap <= min(5 XLM, daily budget)');
  if (fees.heartbeat.maxFeeStroops > 6_000_000 || fees.heartbeat.maxFeeStroops > fees.heartbeat.dailyBudgetStroops) throw Error('fees.heartbeat: per-tx cap <= min(0.6 XLM, daily budget)');
  if (BigInt(out.heartbeatFeeStroops ?? 10_000) >= BigInt(fees.heartbeat.maxFeeStroops)) throw Error('fees.heartbeat.maxFeeStroops must exceed the heartbeat inclusion fee (heartbeatFeeStroops, default 10000)');
  out.fees = fees;
  out.dryRunGateHours = out.dryRunGateHours ?? 12;
  if (!(out.dryRunGateHours >= 2 && out.dryRunGateHours <= 72)) throw Error('dryRunGateHours must be 2..72 (default 12)');
  out.dryRunMaxGapSec = out.dryRunMaxGapSec ?? 120;
  if (out.repayCapRaw == null) throw Error('repayCapRaw required on Mainnet (also the approval ceiling)');
  return out;
}

// ------------------------------------------------------------------------------------------------- RPC facade
/** SDK server with getTransaction / getLatestLedger as raw JSON (SDK 16 cannot decode newer-protocol metadata). */
export function makeRpc(cfg, {S = loadSdk(), fetchImpl = fetch} = {}) {
  const rpc = new S.rpc.Server(cfg.rpcUrl, {timeout: 30, allowHttp: cfg.rpcUrl.startsWith('http://')});
  const call = async (method, params) => {
    const r = await (await fetchImpl(cfg.rpcUrl, {method: 'POST', headers: {'content-type': 'application/json'}, body: JSON.stringify({jsonrpc: '2.0', id: 1, method, params})})).json();
    if (r.error) throw Error(`rpc ${method}: ${JSON.stringify(r.error).slice(0, 200)}`);
    return r.result;
  };
  rpc.getTransaction = async hash => {
    const r = await call('getTransaction', {hash});
    let feeCharged = null, returnValue;
    try { if (r.resultXdr) feeCharged = BigInt(S.xdr.TransactionResult.fromXDR(r.resultXdr, 'base64').feeCharged().toString()); } catch { /* undecodable */ }
    try { if (r.resultMetaXdr) { const m = S.xdr.TransactionMeta.fromXDR(r.resultMetaXdr, 'base64'); const arm = m.switch() === 4 ? m.v4() : m.v3(); returnValue = arm.sorobanMeta()?.returnValue?.() ?? undefined; } } catch { /* newer meta */ }
    return {status: r.status, ledger: r.ledger, createdAt: r.createdAt, feeCharged, returnValue};
  };
  rpc.rawCall = call;
  return rpc;
}

const instanceKey = (S, id) => S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract: new S.Address(id).toScAddress(),
  key: S.xdr.ScVal.scvLedgerKeyContractInstance(), durability: S.xdr.ContractDataDurability.persistent()}));
const dataKey = (S, id, parts) => S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract: new S.Address(id).toScAddress(),
  key: S.xdr.ScVal.scvVec(parts.map(p => (typeof p === 'string' && /^[A-Z][A-Za-z]*$/.test(p) ? S.xdr.ScVal.scvSymbol(p) : new S.Address(p).toScVal()))),
  durability: S.xdr.ContractDataDurability.persistent()}));

/** One getLedgerEntries: code hash of each pinned role, and which pinned contracts have a pending upgrade proposal. */
export async function readPins(cfg, rpc, S = loadSdk()) {
  const ids = PIN_ROLES.map(r => cfg.contracts[r]);
  const keys = [...ids.map(id => instanceKey(S, id)), ...ids.map(id => dataKey(S, id, ['PendingUpgradeHash']))];
  const r = await rpc.getLedgerEntries(...keys);
  const code = {}, pending = [];
  for (const e of r.entries) {
    const d = e.val.contractData(), id = S.Address.fromScAddress(d.contract()).toString(), role = PIN_ROLES[ids.indexOf(id)];
    if (d.key().switch().name === 'scvLedgerKeyContractInstance') code[role] = d.val().instance().executable().wasmHash().toString('hex');
    else pending.push(role);
  }
  return {code, pending, ledger: r.latestLedger};
}
export function assertPins(cfg, {code}) {
  for (const r of PIN_ROLES) {
    if (!code[r]) throw Error(`refused: ${r} instance missing (not deployed or archived)`);
    if (code[r] !== cfg.pins[r]) throw Error(`refused: ${r} code ${code[r].slice(0, 8)} differs from pin ${cfg.pins[r].slice(0, 8)}`);
  }
}

/** Start checks: RPC network, code pins, role separation against on-chain guardians/keepers. Returns a summary. */
export async function startChecks(cfg, {rpc, view, S = loadSdk(), out = () => {}}) {
  const n = await rpc.getNetwork();
  if (n.passphrase !== MAINNET || cfg.networkPassphrase !== MAINNET) throw Error('refused: RPC or config network is not Mainnet');
  const p = await readPins(cfg, rpc, S); assertPins(cfg, p);
  if (p.pending.length) out({level: 'alert', msg: 'upgrade proposal pending on a pinned contract: the bot keeps liquidating on the pinned code and refuses once the code changes', pending: p.pending});
  const raw = await rpc.getLedgerEntries(dataKey(S, cfg.contracts.controller, ['PauseGuardian']));
  const pauseGuardian = raw.entries.length ? S.scValToNative(raw.entries[0].val.contractData().val()) : null;
  const roles = {pauseGuardian, receiptGuardian: (await view(cfg.contracts.receipt, 'stage_config')).guardian,
    vaultGuardian: await view(cfg.contracts.vault, 'stage_guardian'), investKeeper: await view(cfg.contracts.vault, 'stage_investment_keeper'),
    receiptAdmin: await view(cfg.contracts.receipt, 'get_admin'), vaultAdmin: await view(cfg.contracts.vault, 'get_admin')};
  for (const a of [cfg.liquidator, cfg.heartbeatAccount].filter(Boolean)) for (const [role, holder] of Object.entries(roles))
    if (holder && String(holder) === a) throw Error(`refused: signer ${a.slice(0, 6)}... holds the on-chain role ${role}`);
  return {network: n.passphrase, protocolVersion: n.protocolVersion, code: p.code, pendingUpgrades: p.pending, roles};
}

// ------------------------------------------------------------------------------------------------- trustlines
// The liquidator needs BOTH: USDC for its inventory, and PYUSD + USDC to be paid an in-kind exit of seized units
// (Mainnet sims: stage_emergency_pay reverts with SAC #13 without the PYUSD trustline; the command stays HELD).
export const CLASSIC = Object.freeze({
  USDC: {code: 'USDC', issuer: 'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN', contract: 'usdc'},
  PYUSD: {code: 'PYUSD', issuer: 'GDQE7IXJ4HUHV6RQHIUPRJSEZE4DRS5WY577O2FY6YQ5LVWZ7JZTU2V5', contract: 'pyusd'},
});
/** {USDC: bool, PYUSD: bool} for `account`, from one getLedgerEntries of the two trustline keys. */
export async function trustlineStatus(cfg, rpc, account, S = loadSdk()) {
  const names = Object.keys(CLASSIC);
  const keys = names.map(n => S.xdr.LedgerKey.trustline(new S.xdr.LedgerKeyTrustLine({accountId: S.Keypair.fromPublicKey(account).xdrAccountId(),
    asset: new S.Asset(CLASSIC[n].code, CLASSIC[n].issuer).toTrustLineXDRObject()})));
  const r = await rpc.getLedgerEntries(...keys);
  const have = new Set(r.entries.map(e => {
    const a = e.val.trustLine().asset(), arm = a.switch().name === 'assetTypeCreditAlphanum12' ? a.alphaNum12() : a.alphaNum4();
    const code = arm.assetCode().toString('latin1').replace(/\0+$/, ''), issuer = S.StrKey.encodeEd25519PublicKey(arm.issuer().ed25519());
    return names.find(n => CLASSIC[n].code === code && CLASSIC[n].issuer === issuer);
  }).filter(Boolean));
  return Object.fromEntries(names.map(n => [n, have.has(n)]));
}
export async function requireTrustlines(cfg, rpc, account, S = loadSdk()) {
  const t = await trustlineStatus(cfg, rpc, account, S);
  const missing = Object.entries(t).filter(([, v]) => !v).map(([n]) => n);
  if (missing.length) throw Error(`refused: liquidator trustline(s) missing: ${missing.join(', ')} (run src/trustlines.mjs; PYUSD is needed to receive in-kind exits)`);
  return t;
}

// ------------------------------------------------------------------------------------------------- fee budget
const day = ms => new Date(ms).toISOString().slice(0, 10);
/** Persisted per-UTC-day spend (stroops) per account kind ('liquidator' | 'heartbeat'). Charged before sending at the max fee,
 *  then trued up to the actual fee charged when the outcome is final (unknown outcomes stay charged at the max). */
export function openBudget(dir, cfg, clock = () => Date.now()) {
  const file = path.join(dir, 'fee-budget.json');
  const load = () => { try { return JSON.parse(fs.readFileSync(file, 'utf8')); } catch { return {}; } };
  const save = s => { fs.mkdirSync(dir, {recursive: true}); fs.writeFileSync(file + '.tmp', JSON.stringify(s)); fs.renameSync(file + '.tmp', file); };
  const limit = kind => BigInt(kind === 'heartbeat' ? cfg.fees.heartbeat.dailyBudgetStroops : cfg.fees.dailyBudgetStroops);
  const cur = kind => { const s = load(), d = day(clock()); return s[kind]?.day === d ? BigInt(s[kind].spent) : 0n; };
  return {
    spent: cur, limit,
    reserve(kind, fee) {
      const s = load(), d = day(clock()), spent = s[kind]?.day === d ? BigInt(s[kind].spent) : 0n;
      if (spent + BigInt(fee) > limit(kind)) throw Error(`refused: ${kind} daily fee budget (${spent} + ${fee} > ${limit(kind)} stroops)`);
      s[kind] = {day: d, spent: String(spent + BigInt(fee))}; save(s);
    },
    refund(kind, stroops) {
      if (BigInt(stroops) <= 0n) return;
      const s = load(), d = day(clock()); if (s[kind]?.day !== d) return;
      const v = BigInt(s[kind].spent) - BigInt(stroops); s[kind] = {day: d, spent: String(v < 0n ? 0n : v)}; save(s);
    },
  };
}

// ------------------------------------------------------------------------------------------------- allowlists
const TYPES = {id: 'u64', expiration_ledger: 'u32', amount: 'i128', repay_amount: 'u128', shares: 'u128', min_swap: 'u128', deadline: 'u64', accept_reward_waiver: 'bool'};
export function scArgs(S, call) {
  return Object.entries(call.args).map(([k, v]) => TYPES[k] === 'bool' ? S.xdr.ScVal.scvBool(v === true || v === 'true')
    : S.nativeToScVal(TYPES[k] === 'u32' ? Number(v) : TYPES[k] ? BigInt(v) : v, {type: TYPES[k] ?? 'address'}));
}
const eq = (a, b) => String(a) === String(b);
/** The liquidation bot's allowlist (stages of main.mjs stageCall). Throws a `refused:` error for anything else. */
export function botAllow(cfg, contract, fn, args) {
  const c = cfg.contracts, keys = Object.keys(args).join(',');
  const rule = {
    [`${c.receipt}:checkpoint_nav`]: () => keys === '',
    [`${c.receipt}:checkpoint_fallback`]: () => keys === '',
    [`${c.receipt}:sync_seized`]: () => keys === 'id' && /^\d+$/.test(String(args.id)),
    [`${c.vault}:checkpoint_rewards`]: () => keys === '',
    [`${c.controller}:cache_price`]: () => keys === 'token' && eq(args.token, c.usdc),
    [`${c.usdc}:approve`]: () => keys === 'from,spender,amount,expiration_ledger' && eq(args.from, cfg.liquidator) && eq(args.spender, c.usdcMarket)
      && BigInt(args.amount) > 0n && BigInt(args.amount) <= BigInt(cfg.repayCapRaw),
    [`${c.controller}:liquidate`]: () => keys === 'borrower,repay_market,collateral_market,repay_amount,liquidator' && cfg.borrowers.includes(String(args.borrower))
      && eq(args.repay_market, c.usdcMarket) && eq(args.collateral_market, c.receipt) && eq(args.liquidator, cfg.liquidator)
      && BigInt(args.repay_amount) > 0n && BigInt(args.repay_amount) <= BigInt(cfg.repayCapRaw),
  }[`${contract}:${fn}`];
  if (NEVER_TOUCH.includes(contract)) throw Error('refused: pilot/retired contract');
  if (!rule) throw Error(`refused: ${fn} on ${String(contract).slice(0, 6)}... is not on the liquidator allowlist`);
  if (!rule()) throw Error(`refused: ${fn} arguments outside the allowlist`);
}

// ------------------------------------------------------------------------------------------------- sender
/**
 * makeSender(cfg, {rpc, S, account, keyFile, kind, allow, budget, pinsBeforeSign, clock, sleep, pollMs, pollTimeoutMs})
 * -> invoke(contract, fn, args, {onSigned}) -> {ok, unknown, hash, stdout, stderr}. Same result shape as the Testnet
 * CLI executor (main.mjs journals it unchanged).
 */
export function makeSender(cfg, {rpc, S = loadSdk(), account, keyFile, kind = 'liquidator', allow, budget, pinsBeforeSign = true,
  loadKey = loadKeyFile, clock = () => Date.now(), sleep = ms => new Promise(r => setTimeout(r, ms)), pollMs = 2000, pollTimeoutMs = 90_000}) {
  if (!isMainnet(cfg)) throw Error('refused: Mainnet sender on a non-Mainnet config');
  let kp = null;
  // A refusal or failure BEFORE signing is a plain failure (nothing can land); after signing it is `unknown`.
  return async function invoke(contract, fn, args = {}, opts = {}) {
    const st = {signedHash: null};
    try { return await attempt(contract, fn, args, opts, st); }
    catch (e) {
      const msg = String(e?.message ?? e).split('\n')[0].slice(0, 300);
      return st.signedHash ? {ok: false, unknown: true, hash: st.signedHash, stdout: '', stderr: msg} : {ok: false, unknown: false, refused: /^refused/.test(msg), hash: null, stdout: '', stderr: msg};
    }
  };
  async function attempt(contract, fn, args, {onSigned} = {}, st) {
    if (!isMainnet(cfg)) throw Error('refused: config network changed');     // re-checked per call: configs are mutable
    allow(cfg, contract, fn, args);
    const acc = await rpc.getAccount(account);
    const call = {contract, fn, args};
    const tx = new S.TransactionBuilder(acc, {fee: String(cfg.fees.inclusionFeeStroops), networkPassphrase: MAINNET})
      .addOperation(new S.Contract(contract).call(fn, ...scArgs(S, call))).setTimeout(60).build();
    const sim = await rpc.simulateTransaction(tx);
    if (!S.rpc.Api.isSimulationSuccess(sim)) {
      const e = String(sim.error ?? 'simulation failed').split('\n')[0].slice(0, 300);
      return {ok: false, unknown: false, hash: null, stdout: '', stderr: `simulation failed: ${e}`};
    }
    if (sim.restorePreamble) return {ok: false, unknown: false, hash: null, stdout: '', stderr: 'refused: restore required'};
    for (const a of sim.result?.auth ?? []) {
      if (a.credentials().switch().name !== 'sorobanCredentialsSourceAccount') throw Error('refused: non-source-account authorization');
      const root = a.rootInvocation(), f = root.function();
      if (f.switch().name !== 'sorobanAuthorizedFunctionTypeContractFn') throw Error('refused: authorization is not a contract call');
      const inv = f.contractFn();
      if (S.Address.fromScAddress(inv.contractAddress()).toString() !== contract || inv.functionName().toString() !== fn) throw Error('refused: authorization root differs from the call');
      if (root.subInvocations().length) throw Error('refused: nested authorization');
    }
    const pad = BigInt(sim.minResourceFee) * 120n / 100n + 10_000n;
    sim.transactionData.setResourceFee(pad); sim.minResourceFee = String(pad);
    const prepared = S.rpc.assembleTransaction(tx, sim).build();
    const res = prepared.toEnvelope().v1().tx().ext().sorobanData().resources();
    const cpu = Number(res.instructions()), fee = BigInt(prepared.fee);
    const cap = BigInt(kind === 'heartbeat' ? cfg.fees.heartbeat.maxFeeStroops : cfg.fees.maxFeeStroops);
    const ceiling = BigInt(kind === 'heartbeat' ? cfg.fees.heartbeat.resourceCeilingStroops : cfg.fees.resourceCeilingStroops[fn] ?? 0);
    if (pad > ceiling) throw Error(`refused: resource fee ${pad} above the ${fn} ceiling ${ceiling} stroops (rent/new entries?): not signed`);
    if (cpu > (cfg.cpuStop ?? CPU_STOP)) throw Error(`refused: declared cpu ${cpu} above the 97M stop`);
    if (fee > cap) throw Error(`refused: fee ${fee} above cap ${cap}`);
    if (prepared.source !== account) throw Error('refused: transaction source is not the configured signer');
    if (pinsBeforeSign) assertPins(cfg, await readPins(cfg, rpc, S));
    kp ??= loadKey(keyFile, {expectedPublicKey: account}, S);
    budget.reserve(kind, fee);
    const hash = prepared.hash().toString('hex');
    st.signedHash = hash;
    prepared.sign(kp);
    if (onSigned) onSigned(hash);                                           // journal the hash BEFORE sending
    let sent;
    try { sent = await rpc.sendTransaction(prepared); }
    catch (e) { return {ok: false, unknown: true, hash, stdout: '', stderr: `send threw: ${String(e.message).slice(0, 200)}`}; }
    if (sent.status === 'ERROR' || sent.status === 'TRY_AGAIN_LATER') {
      budget.refund(kind, fee);                                             // rejected before inclusion: no fee
      return {ok: false, unknown: false, hash, stdout: '', stderr: `send ${sent.status}`};
    }
    const end = clock() + pollTimeoutMs;
    while (clock() < end) {
      await sleep(pollMs);
      let g; try { g = await rpc.getTransaction(hash); } catch { continue; }
      if (g.status === 'NOT_FOUND') continue;
      if (g.feeCharged != null) budget.refund(kind, fee - g.feeCharged);
      const ok = g.status === 'SUCCESS';
      let stdout = '';
      try { if (ok && g.returnValue) { const v = S.scValToNative(g.returnValue); stdout = typeof v === 'boolean' ? String(v) : JSON.stringify(v, (_, x) => (typeof x === 'bigint' ? String(x) : x)) ?? ''; } } catch { /* leave empty */ }
      if (ok && !stdout && sim.result?.retval) { const v = S.scValToNative(sim.result.retval); if (typeof v === 'boolean') stdout = String(v); }
      return {ok, unknown: false, hash, ledger: g.ledger, stdout, stderr: ok ? '' : `transaction ${g.status}`, cpu, fee: String(fee)};
    }
    return {ok: false, unknown: true, hash, stdout: '', stderr: 'no final status within the poll window'};
  }
}

/**
 * One RPC URL from a 0600 file owned by this service user (same rules as key files). The URL may carry an API key:
 * it is returned only to build the RPC client; callers log `label` = host + '#' + first 8 hex of sha256(url).
 * Errors never include the file contents.
 */
export function readUrlFile(file) {
  if (!file || !path.isAbsolute(file)) throw Error('RPC URL file path must be absolute');
  let st; try { st = fs.lstatSync(file); } catch { throw Error(`RPC URL file ${path.basename(file)} missing or unreadable`); }
  if (st.isSymbolicLink() || !st.isFile()) throw Error(`RPC URL file ${path.basename(file)} must be a regular file, not a symlink`);
  if ((st.mode & 0o077) !== 0) throw Error(`RPC URL file ${path.basename(file)} mode ${(st.mode & 0o777).toString(8)} too open (chmod 600)`);
  if (typeof process.getuid === 'function' && st.uid !== process.getuid()) throw Error(`RPC URL file ${path.basename(file)} is not owned by this service user`);
  const url = fs.readFileSync(file, 'utf8').trim();
  if (!url) throw Error(`RPC URL file ${path.basename(file)} is empty (operator: fill it with read -s, see SIGNER-KEEPER-V6.md §3)`);
  let u; try { u = new URL(url); } catch { throw Error(`RPC URL file ${path.basename(file)} does not hold one URL`); }
  if (u.protocol !== 'https:' || /\s/.test(url) || u.username || u.password) throw Error(`RPC URL file ${path.basename(file)} must hold one https URL without user:password`);
  return {url, label: urlLabel(url)};
}
export const urlLabel = url => `${new URL(url).host}#${crypto.createHash('sha256').update(url).digest('hex').slice(0, 8)}`;
/** Replace every occurrence of a secret URL (and its path) in a message with its label. */
export function redact(text, sources) {
  let t = String(text ?? '');
  for (const {url, label} of sources) { t = t.split(url).join(label); const p = new URL(url); if (p.pathname.length > 8) t = t.split(p.pathname).join('/<redacted>'); if (p.search) t = t.split(p.search).join('?<redacted>'); }
  return t;
}

/**
 * Heartbeat RPC on Mainnet: every server (cfg.heartbeatRpcUrls, then cfg.heartbeatRpcUrlFiles) must report the Mainnet passphrase at start (else the
 * start is refused), and each server re-checks its passphrase before its first call and then at most every
 * `recheckMs`; a server that answers another network throws, so the failover (heartbeat.mjs failoverRpc) moves past it.
 * Every server is a raw-getTransaction facade (makeRpc).
 */
export async function makeHeartbeatRpc(cfg, {S = loadSdk(), failover, make = url => makeRpc({...cfg, rpcUrl: url}, {S}), clock = () => Date.now(), recheckMs = 600_000} = {}) {
  if (!isMainnet(cfg)) throw Error('refused: Mainnet heartbeat RPC on a non-Mainnet config');
  const guarded = [], sources = [...(cfg.heartbeatRpcUrls ?? []).map(url => ({url, label: urlLabel(url)})), ...(cfg.heartbeatRpcUrlFiles ?? []).map(f => readUrlFile(f))];
  if (sources.length < 2) throw Error('refused: at least 2 heartbeat RPCs required');
  if (new Set(sources.map(x => x.url)).size !== sources.length) throw Error('refused: duplicate heartbeat RPC (a URL file repeats another RPC)');
  const scrub = e => Object.assign(Error(redact(e?.message ?? e, sources)), {code: e?.code});   // never the API key, also not in stack/cause
  for (const {url, label} of sources) {
    const raw = make(url);
    let checkedAt = null;
    const check = async () => {
      if (checkedAt != null && clock() - checkedAt < recheckMs) return;
      const n = await raw.getNetwork();
      if (n?.passphrase !== MAINNET) { checkedAt = null; throw Error(`refused: heartbeat RPC ${label} reports another network`); }
      checkedAt = clock();
    };
    try { await check(); } catch (e) { throw Error(`refused: heartbeat RPC ${label} is not a reachable Mainnet RPC (${redact(e.message, sources).slice(0, 120)})`); }
    guarded.push(Object.fromEntries(HB_RPC_METHODS.map(m => [m, async (...args) => { try { await check(); return await raw[m](...args); } catch (e) { throw scrub(e); } }])));
  }
  // Plain objects with a fixed method list: a Proxy would answer `then` and be treated as a thenable.
  const f = failover(guarded);
  const api = Object.fromEntries(HB_RPC_METHODS.map(m => [m, async (...args) => { try { return await f[m](...args); } catch (e) { throw scrub(e); } }]));
  Object.defineProperty(api, 'labels', {value: sources.map(x => x.label)});   // host#hash only, for the start line
  return api;
}

/**
 * Debt-gated heartbeat (user decision 21 Nov): heartbeats run only while a listed borrower has CL debt. `update(debt)`
 * is called on every plan cycle with the debt read in that cycle's observation; it starts the timer when debt appears
 * and stops it when all debt is zero (a stopped heartbeat sends and self-observes nothing). `observe` passes through.
 */
/** Plan-cycle hook: update the gate from this cycle's borrower debts; debt-free, drop every D3 L1a (start/extend). */
export function applyDebtGate(stages, borrowers, gate) {
  const debt = (borrowers ?? []).some(b => BigInt(b.debt) > 0n);
  gate.update(debt);
  return debt ? stages : stages.filter(x => !(x.stage === 'L1a' && /^D3/.test(x.reason ?? '')));
}
export function makeDebtGatedHeartbeat(hb, out = () => {}) {
  let running = false;
  return {
    observe: blocked => hb.observe(blocked),
    update(debt) {
      if (debt && !running) { hb.start(); running = true; out({level: 'info', msg: 'heartbeat started: CL debt exists'}); }
      else if (!debt && running) { hb.stop(); running = false; out({level: 'info', msg: 'heartbeat stopped: no listed borrower has CL debt'}); }
      return running;
    },
    get running() { return running; },
    stop() { if (running) hb.stop(); running = false; },
    stats: () => hb.stats(),
  };
}
const HB_RPC_METHODS = ['getNetwork', 'getAccount', 'simulateTransaction', 'sendTransaction', 'getTransaction', 'getLedgerEntries', 'getLatestLedger', 'getFeeStats'];

/** Heartbeat `sign(xdr)` for src/heartbeat.mjs on Mainnet: accepts ONLY receipt.checkpoint_nav() from the heartbeat
 *  account, no auth entries, fee <= its cap and the heartbeat daily budget (charged at the envelope fee at signing). */
export function makeHeartbeatSigner(cfg, {S = loadSdk(), budget, rpc = null, loadKey = loadKeyFile}) {
  if (!isMainnet(cfg)) throw Error('refused: Mainnet heartbeat signer on a non-Mainnet config');
  let kp = null;
  return async xdr => {
    if (!isMainnet(cfg)) throw Error('refused: config network changed');
    const tx = S.TransactionBuilder.fromXDR(xdr, MAINNET);
    if (tx.source !== cfg.heartbeatAccount) throw Error('refused: heartbeat source is not heartbeatAccount');
    if (tx.operations.length !== 1 || tx.operations[0].type !== 'invokeHostFunction') throw Error('refused: heartbeat must be one invokeHostFunction');
    const op = tx.operations[0];
    if (op.func.switch().name !== 'hostFunctionTypeInvokeContract') throw Error('refused: heartbeat must invoke a contract');
    const inv = op.func.invokeContract();
    if (S.Address.fromScAddress(inv.contractAddress()).toString() !== cfg.contracts.receipt || inv.functionName().toString() !== 'checkpoint_nav' || inv.args().length)
      throw Error('refused: heartbeat may only call receipt.checkpoint_nav()');
    if ((op.auth ?? []).length) throw Error('refused: heartbeat carries authorization entries');
    const fee = BigInt(tx.fee), resource = BigInt(tx.toEnvelope().v1().tx().ext().value()?.resourceFee?.()?.toString() ?? fee);
    if (resource > BigInt(cfg.fees.heartbeat.resourceCeilingStroops)) throw Error(`refused: heartbeat resource fee ${resource} above the ceiling ${cfg.fees.heartbeat.resourceCeilingStroops} stroops (rent/new entries?): not signed`);
    if (fee > BigInt(cfg.fees.heartbeat.maxFeeStroops)) throw Error(`refused: heartbeat fee ${fee} above cap`);
    // Code pins re-checked through the heartbeat's own RPC (whichever failover server is in use) before every signature.
    if (rpc) assertPins(cfg, await readPins(cfg, rpc, S)); else throw Error('refused: heartbeat signer needs its RPC for the code-pin check');
    kp ??= loadKey(cfg.heartbeatKeyFile, {expectedPublicKey: cfg.heartbeatAccount}, S);
    budget.reserve('heartbeat', fee);
    tx.sign(kp);
    return tx.toXDR();
  };
}

// ------------------------------------------------------------------------------------------------- dry-run gate
const here = path.dirname(fileURLToPath(import.meta.url));
/** Binding of a soak: the config (canonical JSON, no secrets in it) plus every src/*.mjs file. */
export function gateBinding(cfg, srcDir = here) {
  const sort = v => (Array.isArray(v) ? v.map(sort) : v && typeof v === 'object' ? Object.fromEntries(Object.keys(v).sort().map(k => [k, sort(v[k])])) : v);
  // `borrowers` / `team3` are excluded: adding T3 after G46 must not restart the soak (it only widens whom L3 may target).
  const {borrowers, team3, ...bound} = cfg;
  const h = crypto.createHash('sha256').update(JSON.stringify(sort(bound), (_, v) => (typeof v === 'bigint' ? String(v) : v)));
  for (const f of fs.readdirSync(srcDir).filter(f => f.endsWith('.mjs')).sort()) h.update(f).update(fs.readFileSync(path.join(srcDir, f)));
  return h.digest('hex');
}
/** Dry-run soak record in <dataDir>/dryrun-gate.json. A failed cycle, a gap > maxGapSec or a changed binding restarts it. */
export function openGate(dir, cfg, {binding = gateBinding(cfg), clock = () => Date.now()} = {}) {
  const file = path.join(dir, 'dryrun-gate.json');
  const load = () => { try { return JSON.parse(fs.readFileSync(file, 'utf8')); } catch { return null; } };
  const save = s => { fs.mkdirSync(dir, {recursive: true}); fs.writeFileSync(file + '.tmp', JSON.stringify(s, null, 1)); fs.renameSync(file + '.tmp', file); };
  const fresh = now => ({binding, liquidator: cfg.liquidator, since: now, last: now, cycles: 0, resets: 0, armedAt: null});
  return {
    file, load,
    record(ok) {
      const now = Math.floor(clock() / 1000); let s = load();
      if (!s || s.binding !== binding || s.liquidator !== cfg.liquidator) s = fresh(now);
      if (s.armedAt) return s;                                             // armed soaks are never rewritten by a dry run
      if (!ok || now - s.last > cfg.dryRunMaxGapSec) { s = {...fresh(now), resets: s.resets + 1}; if (!ok) { save(s); return s; } }
      s.last = now; s.cycles += 1; save(s); return s;
    },
    /** Live start: same binding, soak >= dryRunGateHours, last dry cycle <= 10 min ago; then armed once. Re-arming an
     *  armed record (restart of the live unit) needs only the same binding. */
    requireArmed() {
      const now = Math.floor(clock() / 1000), s = load(), need = cfg.dryRunGateHours * 3600;
      if (!s) throw Error('refused: no dry-run soak recorded (run --watch in dry run first)');
      if (s.binding !== binding || s.liquidator !== cfg.liquidator) throw Error('refused: config or source changed since the soak (soak again)');
      if (s.armedAt) return s;
      if (s.last - s.since < need) throw Error(`refused: dry-run soak ${Math.floor((s.last - s.since) / 60)} min < ${cfg.dryRunGateHours} h`);
      if (now - s.last > 600) throw Error('refused: dry-run soak ended more than 10 min ago (stop the dry run only when switching)');
      s.armedAt = now; save(s); return s;
    },
  };
}

// ------------------------------------------------------------------------------------------------- single instance
/** Exclusive <dataDir>/run.lock with this pid; a lock whose pid is gone is taken over (crash/systemd restart). */
export function acquireRunLock(dir, {pid = process.pid, alive = p => { try { process.kill(p, 0); return true; } catch (e) { return e.code === 'EPERM'; } }} = {}) {
  fs.mkdirSync(dir, {recursive: true});
  const file = path.join(dir, 'run.lock');
  for (let i = 0; i < 2; i++) {
    try { const fd = fs.openSync(file, 'wx', 0o600); fs.writeSync(fd, String(pid)); fs.closeSync(fd); return () => { try { if (fs.readFileSync(file, 'utf8') === String(pid)) fs.unlinkSync(file); } catch { /* gone */ } }; }
    catch (e) {
      if (e.code !== 'EEXIST') throw e;
      const other = Number(fs.readFileSync(file, 'utf8'));
      if (Number.isSafeInteger(other) && other > 0 && alive(other)) throw Error(`refused: another cl-liquidator (pid ${other}) holds ${file}`);
      fs.unlinkSync(file);
    }
  }
  throw Error('refused: could not take the run lock');
}
