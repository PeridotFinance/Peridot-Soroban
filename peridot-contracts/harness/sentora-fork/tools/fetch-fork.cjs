// Read-only: builds the Mainnet ledger fork for harness/sentora-fork (tests/sentora.rs).
// Never signs, never reads keys or env secrets. Only simulateTransaction / getLedgerEntries / getLatestLedger / getLedgers.
//   cd harness/sentora-fork/tools && npm ci && node fetch-fork.cjs --out ../.scratch/fork.json
// Contents: the REAL Sentora XLM and USDC vaults (CA54LV.., CAHEWH..) and their shared reward escrow (CCZSNG..): instances,
// code and the entries the RPC footprints of read-only deposit/withdraw/set_rewards_root simulations touch; the XLM and USDC
// SACs with the vaults' balances; Reflector (footprints of the controller's calls + its admin account); and the CODE of the
// live cash-group controller and receipts (their instances only to learn the wasm hashes; the test registers fresh ones).
'use strict';
const fs = require('node:fs');
const S = require('@stellar/stellar-sdk');
const RPC = 'https://soroban-rpc.mainnet.stellar.gateway.fm';
const NETWORK = S.Networks.PUBLIC;
const opt = (k, d) => { const i = process.argv.indexOf(k); return i > 0 ? process.argv[i + 1] : d; };
const rpc = async (method, params) => { const r = await (await fetch(RPC, {method: 'POST',
  headers: {'content-type': 'application/json', 'user-agent': 'rpc-client/1.0'},
  body: JSON.stringify({jsonrpc: '2.0', id: 1, method, params})})).json(); if (r.error) throw Error(JSON.stringify(r.error)); return r.result; };
const J = x => JSON.stringify(x, (_, v) => typeof v === 'bigint' ? String(v) : v);

const VIEW = 'GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP'; // simulation source only (Peridot admin, public key)
const SENTORA_ADMIN = 'GCOB2OVZWDLW3GHWA3PAON5PTMKPMPV6236Z3DQEA2OABRCECCMQZ5DI';
const ESCROW = 'CCZSNGVMEHRKMMMQGVDHAXT3KSWIMPEPRRTUE2GU3F6TW4XA2Y6EMHN3';
const XLM = 'CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA';
const USDC = 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75';
const PYUSD = 'CCCRWH6Q3FNP3I2I57BDLM5AFAT7O6OF6GKQOC6SSJNDAVRZ57SPHGU2';
const VAULTS = {XLM: 'CA54LVHMAY7HGLMVPN4W72XJB4OGKVZBZX26FWN6JD4P3HJFWQUQEHJO', USDC: 'CAHEWHOPPDBQYFMAOLDOXXGUX2BCR7EXP4CWYCRY3NEAJB35YPZMMJFF',
  PYUSD: 'CAQRAXBU6G4AAX4BZ7R4WLB62TSVAQFS5ZXJDVXRLAU2NZ2ZTGU5QOYB'};
const ASSET = {XLM, USDC, PYUSD};
// Peg-oracle inputs (group-3 fork): the Aquarius PYUSD/USDC concentrated pool the LP vaults use (router get_pools lists it),
// the time-sampled Reflector DEX feed (base USDC) and the issuers the SACs read.
const POOL = 'CAPIOQNULTKVYOJT6X2W2XKGNIVUWZDY72Y42YG6HQKJ7DU7YTHIDQYX';
const STABLE_POOL = 'CDMH535JSD224YXPET3B4SJOLXTQQ24GRSCWACGYBKSH2DKFJYWI7SUW'; // only a swap-simulation source (holds both tokens)
const REFLECTOR_DEX = 'CALI2BYU2JE6WVRUFYTS6MSBNEHGJ35P4AVCZYF3B6QOE3QKOB2PLE6M';
const PYUSD_ISSUER = 'GDQE7IXJ4HUHV6RQHIUPRJSEZE4DRS5WY577O2FY6YQ5LVWZ7JZTU2V5';
const SENTORA_VAULT_SHA = 'ca6b85a13e53353912aeee2d4742830fc58076660dde64fdad4a6afbcca9674f';
const SENTORA_ESCROW_SHA = 'f22e4a52893d04b277c79bd19b3ff0b8771e1c0ca396a35d9339dd3a027ae299';
const CONTROLLER = 'CBMRS7LK6HIZHHPXZTRJLVWZ4HOKJDMZAZ3FKDNFHZKGHXEGN72IJ7RK';
const RECEIPTS = {USDC: 'CDW3GDNKLUEPPJLEUVGLJSKGYLB3YYDQBUY7NBHJILGQWNN55AYFHY3F', XLM: 'CBHMG4CNJO2CRGADUMIO3UJ45C5MTWGCBFECMR6QE2TDDCDBB3N24WMS'};
const REFLECTOR = 'CAFJZQWSED6YAWZU3GWRTOCNPPCGBN32L7QV43XX5LZLFTK6JLN34DLN';
// Existing holders, used only as simulation sources so the RPC returns real deposit/withdraw footprints.
const HOLDER = {XLM: 'GAAVM2XEN5LICO5RPRYEE6M3X3EZYKENGL5V3X22465AR4ZX7OUJ455J'};

const sym = s => S.xdr.ScVal.scvSymbol(s), a = x => S.nativeToScVal(x, {type: 'address'});
const u32 = x => S.nativeToScVal(x, {type: 'u32'}), u64 = x => S.nativeToScVal(BigInt(x), {type: 'u64'}), u128 = x => S.nativeToScVal(BigInt(x), {type: 'u128'});
const vec = (n, ...args) => S.xdr.ScVal.scvVec([sym(n), ...args]);
const dataKey = (contract, scKey, dur = 'persistent') => S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({
  contract: new S.Address(contract).toScAddress(), key: scKey, durability: S.xdr.ContractDataDurability[dur]()}));
const inst = c => dataKey(c, S.xdr.ScVal.scvLedgerKeyContractInstance());
const accountKey = g => S.xdr.LedgerKey.account(new S.xdr.LedgerKeyAccount({accountId: S.Keypair.fromPublicKey(g).xdrAccountId()}));

const simulate = async (source, contract, fn, args) => {
  const tx = new S.TransactionBuilder(new S.Account(source, '0'), {fee: '100', networkPassphrase: NETWORK})
    .addOperation(new S.Contract(contract).call(fn, ...args)).setTimeout(0).build();
  const sim = await rpc('simulateTransaction', {transaction: tx.toXDR()});
  if (sim.error) return {error: String(sim.error).split('\n')[0].slice(0, 300), ledger: sim.latestLedger};
  const data = S.xdr.SorobanTransactionData.fromXDR(sim.transactionData, 'base64'), res = data.resources(), fp = res.footprint();
  return {keys: [...fp.readOnly(), ...fp.readWrite()], readOnly: fp.readOnly().length, readWrite: fp.readWrite().length,
    restore: !!sim.restorePreamble, cpu: Number(res.instructions()), diskReadBytes: Number(res.diskReadBytes()), writeBytes: Number(res.writeBytes()),
    minResourceFee: sim.minResourceFee, ledger: sim.latestLedger,
    result: sim.results && sim.results[0] ? S.scValToNative(S.xdr.ScVal.fromXDR(sim.results[0].xdr, 'base64')) : null};
};

(async () => {
  const keys = [], push = k => keys.push(k);
  for (const c of [ESCROW, XLM, USDC, PYUSD, REFLECTOR, REFLECTOR_DEX, POOL, STABLE_POOL, CONTROLLER, ...Object.values(VAULTS), ...Object.values(RECEIPTS)]) push(inst(c));
  for (const n of Object.keys(VAULTS)) push(dataKey(ASSET[n], vec('Balance', a(VAULTS[n]))));
  for (const t of [XLM, USDC, PYUSD]) push(dataKey(t, vec('Balance', a(ESCROW))));
  for (const t of [USDC, PYUSD]) push(dataKey(t, vec('Balance', a(POOL))));
  push(accountKey(PYUSD_ISSUER));
  push(accountKey(SENTORA_ADMIN));
  // Peridot admin (baked init admin of the deployed code): its account (XLM) and USDC trustline, so the test can seed
  // the fresh receipts from it.
  push(accountKey(VIEW));
  push(accountKey('GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN')); // USDC issuer (SAC reads its flags)
  push(S.xdr.LedgerKey.trustline(new S.xdr.LedgerKeyTrustLine({accountId: S.Keypair.fromPublicKey(VIEW).xdrAccountId(),
    asset: new S.Asset('USDC', 'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN').toTrustLineXDRObject()})));

  // Real Sentora footprints (read-only simulations; recording-mode auth, nothing signed).
  const sentoraSims = [];
  const add = async (tag, src, c, fn, args) => { const r = await simulate(src, c, fn, args); if (!r.error) r.keys.forEach(push);
    const {keys: _k, ...rest} = r; sentoraSims.push({tag, ...rest, footprint: r.keys ? r.keys.length : undefined}); };
  await add('XLM deposit 1 XLM (admin as depositor)', VIEW, VAULTS.XLM, 'deposit', [a(VIEW), u128(10_000_000), S.xdr.ScVal.scvVoid()]);
  await add('XLM withdraw 1 XLM (existing holder)', HOLDER.XLM, VAULTS.XLM, 'withdraw', [a(HOLDER.XLM), u128(10_000_000)]);
  await add('USDC deposit 0 balance (expected error)', VIEW, VAULTS.USDC, 'deposit', [a(VIEW), u128(10_000_000), S.xdr.ScVal.scvVoid()]);
  const now = Math.floor(Date.now() / 1000);
  for (const n of Object.keys(VAULTS)) await add(`set_rewards_root ${n} (admin recording mode, 1 raw)`, SENTORA_ADMIN, ESCROW, 'set_rewards_root',
    [a(ASSET[n]), a(VAULTS[n]), u32(now), S.xdr.ScVal.scvBytes(Buffer.alloc(32)), u128(1), u32(1), u64(now - 60)]);

  // Pool: footprints (tick chunks) of estimate_swap and swap at the sizes the fork test uses, and the RPC's own quotes so
  // the test can assert the fork reproduces them (a missing tick entry would read as empty, not fault).
  const poolSims = [];
  for (const [i, o] of [[0, 1], [1, 0]]) for (const k of [1000n, 50000n, 150000n, 200000n, 300000n]) {
    const amt = k * 10_000_000n;
    const q = await simulate(VIEW, POOL, 'estimate_swap', [u32(i), u32(o), u128(amt)]);
    if (!q.error) q.keys.forEach(push);
    const sw = await simulate(VIEW, POOL, 'swap', [a(STABLE_POOL), u32(i), u32(o), u128(amt), u128(0)]);
    if (!sw.error) sw.keys.forEach(push);
    poolSims.push({pool: POOL, inIdx: i, outIdx: o, amount: String(amt), estimate: q.error ? null : String(q.result), estimateFootprint: q.keys ? q.keys.length : null,
      swapFootprint: sw.keys ? sw.keys.length : null, swapError: sw.error || null, ledger: q.ledger});
  }
  for (const fn of ['get_tokens', 'get_is_killed_swap', 'get_slot0']) { const r = await simulate(VIEW, POOL, fn, []); if (!r.error) r.keys.forEach(push); }
  // Stable pool: second oracle source. Swap source for its footprint: the concentrated pool (holds both tokens).
  push(inst(STABLE_POOL));
  for (const t of [USDC, PYUSD]) push(dataKey(t, vec('Balance', a(STABLE_POOL))));
  for (const fn of ['get_tokens', 'get_is_killed_swap', 'get_reserves']) { const r = await simulate(VIEW, STABLE_POOL, fn, []); if (!r.error) r.keys.forEach(push); }
  for (const [i, o] of [[0, 1], [1, 0]]) for (const k of [1000n, 1000000n, 3000000n]) {
    const amt = k * 10_000_000n;
    const q = await simulate(VIEW, STABLE_POOL, 'estimate_swap', [u32(i), u32(o), u128(amt)]);
    if (!q.error) q.keys.forEach(push);
    const sw = await simulate(VIEW, STABLE_POOL, 'swap', [a(POOL), u32(i), u32(o), u128(10_000n * 10_000_000n), u128(0)]);
    if (!sw.error) sw.keys.forEach(push);
    poolSims.push({pool: STABLE_POOL, inIdx: i, outIdx: o, amount: String(amt), estimate: q.error ? null : String(q.result), estimateFootprint: q.keys ? q.keys.length : null,
      swapFootprint: sw.keys ? sw.keys.length : null, swapError: sw.error || null, ledger: q.ledger});
  }
  const dexProbes = [];
  for (const [fn, args] of [['decimals', []], ['resolution', []], ['base', []], ['assets', []], ['admin', []], ['lastprice', [vec('Stellar', a(PYUSD))]]]) {
    const r = await simulate(VIEW, REFLECTOR_DEX, fn, args); if (r.error) throw Error(`dex ${fn}: ${r.error}`); r.keys.forEach(push);
    dexProbes.push({fn, footprint: r.keys.length, result: r.result});
    if (fn === 'admin') push(accountKey(r.result));
  }

  // Reflector: footprints of the exact calls the controller makes, plus its admin account (test mocks set_price auth).
  const probes = [['decimals', []], ['resolution', []], ['last_timestamp', []], ['assets', []], ['admin', []]];
  for (const s of ['USDC', 'XLM']) probes.push(['lastprice', [vec('Other', sym(s))]]);
  const reflectorProbes = [];
  for (const [fn, args] of probes) {
    const r = await simulate(VIEW, REFLECTOR, fn, args); if (r.error) throw Error(`${fn}: ${r.error}`); r.keys.forEach(push);
    reflectorProbes.push({fn, cpu: r.cpu, footprint: r.keys.length, result: r.result});
    if (fn === 'admin') push(accountKey(r.result));
  }

  const uniq = [...new Map(keys.map(k => [k.toXDR('base64'), k])).values()];
  const fetchAll = async ks => { const out = []; for (let i = 0; i < ks.length; i += 200) {
    const r = await rpc('getLedgerEntries', {keys: ks.slice(i, i + 200).map(k => k.toXDR('base64'))}); out.push(...(r.entries || [])); } return out; };
  let entries = await fetchAll(uniq);
  const instanceHash = {}, codes = new Set();
  const scan = () => { for (const e of entries) { const d = S.xdr.LedgerEntryData.fromXDR(e.xdr, 'base64');
    if (d.switch().name === 'contractData' && d.contractData().key().switch().name === 'scvLedgerKeyContractInstance') {
      const ex = d.contractData().val().instance().executable(), c = S.Address.fromScAddress(d.contractData().contract()).toString();
      if (ex.switch().name === 'contractExecutableWasm') { const h = ex.wasmHash().toString('hex'); codes.add(h); instanceHash[c] = h; } else instanceHash[c] = 'stellar_asset'; } } };
  scan();
  const have = new Set(entries.map(e => e.key));
  const codeKeys = [...codes].map(h => S.xdr.LedgerKey.contractCode(new S.xdr.LedgerKeyContractCode({hash: Buffer.from(h, 'hex')}))).filter(k => !have.has(k.toXDR('base64')));
  entries = entries.concat(await fetchAll(codeKeys));
  const latest = await rpc('getLatestLedger');
  const lg = await rpc('getLedgers', {startLedger: latest.sequence - 1, pagination: {limit: 1}});
  const out = {schema: 'sentora-fork-v1', network: NETWORK, rpc: RPC, fetchedAt: new Date().toISOString(), ledger: latest.sequence - 1,
    closeTime: Number(lg.ledgers[0].ledgerCloseTime), protocolVersion: latest.protocolVersion, requestedKeys: uniq.length + codeKeys.length,
    found: entries.length, instanceHash, vaults: VAULTS, escrow: ESCROW, controller: CONTROLLER, receipts: RECEIPTS, reflector: REFLECTOR,
    sentoraSims, reflectorProbes, poolSims, dexProbes, pool: POOL, reflectorDex: REFLECTOR_DEX,
    entries: entries.map(e => ({key: e.key, xdr: e.xdr, lastModified: e.lastModifiedLedgerSeq, liveUntil: e.liveUntilLedgerSeq ?? null}))};
  fs.writeFileSync(opt('--out', 'fork.json'), J(out) + '\n');
  // --wasm-dir DIR: also write every fetched ContractCode as DIR/<sha256>.wasm (git-ignored; Sentora's bytes are
  // third-party and never committed). Native sentora-strategy tests load the pinned Sentora hashes from here.
  const wasmDir = opt('--wasm-dir', null);
  if (wasmDir) {
    fs.mkdirSync(wasmDir, {recursive: true});
    const crypto = require('node:crypto');
    for (const e of entries) { const d = S.xdr.LedgerEntryData.fromXDR(e.xdr, 'base64'); if (d.switch().name !== 'contractCode') continue;
      const code = d.contractCode().code(), h = crypto.createHash('sha256').update(code).digest('hex');
      if (h !== d.contractCode().hash().toString('hex')) throw Error('code hash mismatch ' + h);
      fs.writeFileSync(`${wasmDir}/${h}.wasm`, code); }
    for (const h of [SENTORA_VAULT_SHA, SENTORA_ESCROW_SHA]) if (!fs.existsSync(`${wasmDir}/${h}.wasm`)) throw Error('missing pinned Sentora code ' + h);
  }
  console.log(J({ledger: out.ledger, closeTime: out.closeTime, protocol: out.protocolVersion, requested: out.requestedKeys, found: out.found, instanceHash, sentoraSims}));
})().catch(e => { console.error(e); process.exit(1); });
