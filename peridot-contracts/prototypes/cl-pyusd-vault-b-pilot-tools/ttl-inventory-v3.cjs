// Read-only TTL (storage-rent) inventory for the PYUSD Vault-B pilot contracts, plus an UNSIGNED ExtendFootprintTTL
// preparer. No signer, no key access, no sendTransaction. Separate from the cash TTL keeper: its signing exception does
// NOT cover these contracts, and nothing here reads or writes research/stellar-cash-ttl-20261112/.
//
// Key discovery (RPC cannot list a contract's keys):
//  (1) instance + code entries of the vault, the staged receipt and the fresh controller;
//  (2) every footprint key of the 19 confirmed deployment transactions (journal, read only);
//  (3) the footprint of the step-20 activation simulation (activate/step20-simulation-*.json);
//  (4) footprints of fresh read-only simulations of vault/receipt views that touch reward history, staged state,
//      the venue (pool, plane, router, config storage, oracle, reward feed/boost) and the token contracts.
// Only contract-data and contract-code keys have TTLs; account/trustline keys are dropped.
// Entries created LATER (staged jobs, owner indexes, reward boundaries/segments/weights, holder records) are not yet
// on chain; rerun `inventory` after activation, after the seed and after the pauses to pick them up.
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const assert = require('node:assert/strict');
const S = require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const ROOT = path.resolve(__dirname, '../..');
const REL = 'research/cl-pyusd-tight-pilot-20261111/vault-b-pilot-release-20261114/';
const JOURNAL = REL + 'deploy/run-seed800-20261115T132829Z/journal/journal.json';
const PLAN = REL + 'deploy/mainnet-deployment-plan-v3-seed800.json';
const A = 'GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP';
const KEEPER = 'GBHWPV6QTQIAPJI7WHLHWZJB4GELG3B5NAR2NJ27FQNKSIGYGBYFZON4';
const V = 'CD6N2EQJN47SYLSIYUK3YVBKD4YISATZ2CZZJQZCXKNNVB4HVR6AWL63', R = 'CCGKTKPAWQHKI3TMX4RHLLZ3WYFELHERVNWOWMAZUHTKIAU2ENMHXBCI';
const CONTROLLER = 'CBMRS7LK6HIZHHPXZTRJLVWZ4HOKJDMZAZ3FKDNFHZKGHXEGN72IJ7RK';
const CASH = new Set(['CDW3GDNKLUEPPJLEUVGLJSKGYLB3YYDQBUY7NBHJILGQWNN55AYFHY3F', 'CBHMG4CNJO2CRGADUMIO3UJ45C5MTWGCBFECMR6QE2TDDCDBB3N24WMS',
  'CD6OTGA6J6UIEKXTXA5CLPLSJA4VQ5HEOYP3CA375CZEHI3KMIL2OYZC', 'CAXWP5M2SF74DW22EQAYOXR4ZHN2FO2HGL737KQ2WFPEZTA4Z3W5E2M4']);
const CODES = {vault: 'e66380303e50c6177492d1c9e880edcc84b4258da2cd46be46a1b3f8fe12e3e9', receipt: '140dcaab08680be910f9ea65f6682b72c1c4029bd991bca9397b516af9b66823',
  controller: '847894a65fb59675c74707ea8899633a7638aa2716a183b0b2efe35a8bc2b902'};
const SECONDS_PER_LEDGER = 5.8; // approximate Mainnet average; days are indicative only
const sha = b => crypto.createHash('sha256').update(b).digest('hex');
const json = v => JSON.stringify(v, (_, x) => typeof x === 'bigint' ? String(x) : x, 2) + '\n';
const instanceKey = id => S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract: new S.Address(id).toScAddress(), key: S.xdr.ScVal.scvLedgerKeyContractInstance(), durability: S.xdr.ContractDataDurability.persistent()}));
const codeKey = hash => S.xdr.LedgerKey.contractCode(new S.xdr.LedgerKeyContractCode({hash: Buffer.from(hash, 'hex')}));
const ttlRelevant = k => ['contractData', 'contractCode'].includes(k.switch().name);
// Readable description of a key, and which contract owns it.
function describe(k, roles) {
  if (k.switch().name === 'contractCode') {
    const h = k.contractCode().hash().toString('hex'), role = Object.entries(CODES).find(([, v]) => v === h)?.[0];
    const dep = Object.entries(roles).find(([k]) => k === 'code:' + h)?.[1];
    return {type: 'code', owner: role ? `${role} code` : 'dependency code', ownerRole: role || dep || 'dependency', hash: h, label: `code ${h.slice(0, 12)}…`};
  }
  const d = k.contractData(), id = S.Address.fromScAddress(d.contract()).toString(), role = roles[id] || 'unknown';
  let label;
  if (d.key().switch().name === 'scvLedgerKeyContractInstance') label = 'instance';
  else { try { label = JSON.stringify(S.scValToNative(d.key()), (_, x) => typeof x === 'bigint' ? String(x) : x); } catch (_) { label = d.key().toXDR('base64'); } }
  return {type: 'data', durability: d.durability().name, contract: id, ownerRole: role, label: label.length > 160 ? label.slice(0, 157) + '…' : label};
}
function rolesFrom(deps) {
  const roles = {[V]: 'vault', [R]: 'receipt', [CONTROLLER]: 'controller'};
  for (const [role, pin] of Object.entries(deps || {})) {roles[pin.contract] = `venue:${role}`; if (pin.wasmSha256) roles['code:' + pin.wasmSha256] ??= `venue:${role} code`;}
  return roles;
}
// (2) + (3): keys from confirmed journal rows and saved simulation footprints. Pure (file reads only).
function staticKeys() {
  const plan = JSON.parse(fs.readFileSync(path.resolve(ROOT, PLAN))), journal = JSON.parse(fs.readFileSync(path.resolve(ROOT, JOURNAL)));
  const keys = new Map(), add = (k, source) => { if (!ttlRelevant(k)) return; const x = k.toXDR('base64'); if (!keys.has(x)) keys.set(x, {key: k, sources: new Set()}); keys.get(x).sources.add(source); };
  for (const id of [V, R, CONTROLLER]) add(instanceKey(id), 'instance');
  for (const h of Object.values(CODES)) add(codeKey(h), 'code');
  for (const row of journal.entries) {
    const fp = S.TransactionBuilder.fromXDR(row.unsignedXdr, plan.network).toEnvelope().v1().tx().ext().sorobanData().resources().footprint();
    for (const k of [...fp.readOnly(), ...fp.readWrite()]) add(k, `journal:${row.step}`);
  }
  const act = path.resolve(ROOT, REL, 'activate');
  for (const f of fs.existsSync(act) ? fs.readdirSync(act).filter(x => /^step20-simulation-.*\.json$/.test(x)) : []) {
    const sim = JSON.parse(fs.readFileSync(path.join(act, f))).step20;
    for (const x of [...(sim?.footprintKeysXdr?.readOnly || []), ...(sim?.footprintKeysXdr?.readWrite || [])]) add(S.xdr.LedgerKey.fromXDR(x, 'base64'), 'step20-simulation');
  }
  return keys;
}
const u128 = v => S.nativeToScVal(BigInt(v), {type: 'u128'}), u32 = v => S.nativeToScVal(v, {type: 'u32'}), ad = v => new S.Address(v).toScVal();
// (4) views whose simulation footprints reach reward history, staged state, venue and tokens. Some trap before
// activation; a failed simulation contributes nothing and is listed.
const VIEWS = [[V, 'stage_quote_open', [u128('8000000000'), u32(20), u32(5000)]], [V, 'stage_quote_close', [u128('1000000'), u32(20)]],
  [V, 'reward_totals', []], [V, 'reward_history_totals', []], [V, 'reward_account', [ad(A)]], [V, 'reward_history_count', [ad(A)]],
  [V, 'get_config', []], [V, 'get_params', []], [V, 'stage_receipt', []], [V, 'stage_investment_keeper', []], [V, 'stage_guardian', []],
  [V, 'stage_investment_guard', []], [V, 'stage_reserved', []], [V, 'total_supply', []], [V, 'balance', [ad(R)]], [V, 'get_position_liquidity', []],
  [V, 'stage_composition_dirty', []], [V, 'get_primary_reward_token', []], [V, 'checkpoint_rewards', []],
  [R, 'stage_is_active', []], [R, 'stage_state', []], [R, 'stage_config', []], [R, 'stage_nav', []], [R, 'stage_commands', [ad(A)]], [R, 'stage_units', [ad(A)]],
  [R, 'get_minimum_supply', []], [R, 'get_total_ptokens', []], [R, 'get_ptoken_balance', [ad(R)]], [R, 'get_admin', []],
  [CONTROLLER, 'is_market_supported', [ad(R)]], [CONTROLLER, 'is_deposit_paused', [ad(R)]], [CONTROLLER, 'get_market_cf', [ad(R)]]];
async function viewKeys(rpc, account, keys) {
  const failed = [];
  for (const [c, m, args] of VIEWS) {
    const tx = new S.TransactionBuilder(new S.Account(A, account), {fee: '1000', networkPassphrase: S.Networks.PUBLIC}).addOperation(new S.Contract(c).call(m, ...args)).setTimeout(0).build();
    const sim = await rpc.simulateTransaction(tx);
    if (!S.rpc.Api.isSimulationSuccess(sim)) { failed.push(`${m}: ${String(sim.error || '').split('\n')[0].slice(0, 120)}`); continue; }
    const fp = sim.transactionData.build().resources().footprint();
    for (const k of [...fp.readOnly(), ...fp.readWrite()]) if (ttlRelevant(k)) { const x = k.toXDR('base64'); if (!keys.has(x)) keys.set(x, {key: k, sources: new Set()}); keys.get(x).sources.add(`view:${m}`); }
  }
  return failed;
}
// Classify one live row. `latest` = current ledger.
function row(entry, info, latest, roles) {
  const d = describe(info.key, roles), live = entry?.liveUntilLedgerSeq ?? null;
  return {...d, keyXdr: info.key.toXDR('base64'), present: !!entry, liveUntilLedger: live, ledgersLeft: live === null ? null : live - latest,
    approxDaysLeft: live === null ? null : Math.round((live - latest) * SECONDS_PER_LEDGER / 864) / 100, sources: [...info.sources].sort(),
    ours: ['vault', 'receipt', 'controller'].includes(d.ownerRole) || d.ownerRole === 'vault code' || ['vault', 'receipt', 'controller'].includes(d.ownerRole)};
}
async function inventory(rpc, {deps = null} = {}) {
  assert.equal((await rpc.getNetwork()).passphrase, S.Networks.PUBLIC);
  const keys = staticKeys(), account = (await rpc.getAccount(A)).sequenceNumber();
  // Every pinned venue dependency (pool, oracle, tokens, reward feed/boost, pool vault, config storage, plane, router): instance + code.
  for (const [role, pin] of Object.entries(deps || {})) for (const k of [instanceKey(pin.contract), ...(pin.wasmSha256 ? [codeKey(pin.wasmSha256)] : [])]) {
    const x = k.toXDR('base64'); if (!keys.has(x)) keys.set(x, {key: k, sources: new Set()}); keys.get(x).sources.add(`dependency:${role}`); }
  const failedViews = await viewKeys(rpc, account, keys);
  for (const info of keys.values()) if (info.key.switch().name === 'contractData') assert(!CASH.has(S.Address.fromScAddress(info.key.contractData().contract()).toString()), 'cash market key reached the inventory');
  const list = [...keys.values()], rows = []; let latest = null;
  for (let i = 0; i < list.length; i += 100) {
    const got = await rpc.getLedgerEntries(...list.slice(i, i + 100).map(x => x.key)); latest ??= got.latestLedger;
    const by = new Map(got.entries.map(e => [e.key.toXDR('base64'), e]));
    for (const info of list.slice(i, i + 100)) rows.push(row(by.get(info.key.toXDR('base64')), info, latest, rolesFrom(deps)));
  }
  rows.sort((a, b) => (a.liveUntilLedger ?? Infinity) - (b.liveUntilLedger ?? Infinity));
  const present = rows.filter(r => r.present), ours = present.filter(r => ['vault', 'receipt', 'controller'].includes(r.ownerRole));
  const earliest = list => list.length ? {liveUntilLedger: list[0].liveUntilLedger, ledgersLeft: list[0].ledgersLeft, approxDaysLeft: list[0].approxDaysLeft, label: list[0].label, ownerRole: list[0].ownerRole, contract: list[0].contract || null} : null;
  return {status: 'READ_ONLY_TTL_INVENTORY', signing: false, submission: false, latestLedger: latest, secondsPerLedgerAssumed: SECONDS_PER_LEDGER,
    counts: {keys: rows.length, present: present.length, absent: rows.length - present.length, ours: ours.length, dependencies: present.length - ours.length},
    earliest: {all: earliest(present), ours: earliest(ours), dependencies: earliest(present.filter(r => !['vault', 'receipt', 'controller'].includes(r.ownerRole)))},
    failedViews, rows,
    notes: ['Entries the pilot creates later (jobs, owner indexes, reward boundaries/segments/weights, holder records, SAC balances of new holders) are absent until created; rerun after each ceremony step.',
      'Dependency entries (pool, plane, router, oracle, tokens) belong to other parties; extending them is optional and costs rent; their archival would make the vault fail closed (restore needed).']};
}
// Unsigned ExtendFootprintTTL for the given keys. Read-only simulation; the result is NEVER signed here.
async function prepareExtension(rpc, {source, keysXdr, extendTo, inclusionFee = 1000, maxFeeStroops = 50000000}) {
  assert(S.StrKey.isValidEd25519PublicKey(source)); assert(Number.isSafeInteger(extendTo) && extendTo > 0 && extendTo <= 3110400);
  assert(Array.isArray(keysXdr) && keysXdr.length > 0 && keysXdr.length <= 40, 'at most 40 keys per extension transaction');
  const keys = keysXdr.map(x => S.xdr.LedgerKey.fromXDR(x, 'base64'));
  for (const k of keys) { assert(ttlRelevant(k)); if (k.switch().name === 'contractData') assert(!CASH.has(S.Address.fromScAddress(k.contractData().contract()).toString()), 'REFUSED: cash market key'); }
  const latest = await rpc.getLatestLedger(), acc = await rpc.getAccount(source);
  const data = new S.SorobanDataBuilder().setReadOnly(keys).build();
  const tx = new S.TransactionBuilder(acc, {fee: String(inclusionFee), networkPassphrase: S.Networks.PUBLIC}).setSorobanData(data)
    .addOperation(S.Operation.extendFootprintTtl({extendTo})).setLedgerbounds(latest.sequence + 1, latest.sequence + 20).setTimebounds(Number(latest.closeTime), Number(latest.closeTime) + 180).build();
  const sim = await rpc.simulateTransaction(tx);
  assert(S.rpc.Api.isSimulationSuccess(sim), `extension simulation failed: ${String(sim.error || '').split('\n')[0]}`);
  assert(!sim.restorePreamble, 'an entry is archived: a RestoreFootprint transaction is needed first (not prepared here)');
  const assembled = S.rpc.assembleTransaction(tx, sim).build();
  assert(BigInt(assembled.fee) <= BigInt(maxFeeStroops), `extension fee ${assembled.fee} above cap ${maxFeeStroops}`);
  return {status: 'UNSIGNED_EXTENSION_OPERATOR_REVIEW_REQUIRED', signing: false, submission: false, source, extendTo, keys: keysXdr.length,
    latestLedger: latest.sequence, validLedgers: {min: latest.sequence + 1, max: latest.sequence + 20}, sequence: assembled.sequence,
    resourceFeeStroops: String(sim.minResourceFee), maxFeeStroops: assembled.fee, hash: assembled.hash().toString('hex'), unsignedXdr: assembled.toXDR(),
    warning: source === A ? 'Source is the Ledger admin: this MOVES its sequence. Never send during activate -> seed -> pause; regenerate any pilot/FULL config afterwards.' : 'Source is a fee-only account: it needs no authority over the contracts (ExtendFootprintTTL is permissionless); the user must authorize that account separately.'};
}
module.exports = {staticKeys, describe, row, inventory, prepareExtension, VIEWS, CODES, V, R, CONTROLLER, KEEPER};
if (require.main === module) {
  const [mode, ...args] = process.argv.slice(2), opt = n => { const i = args.indexOf(n); return i < 0 ? null : args.splice(i, 2)[1]; };
  const url = opt('--rpc') || 'https://soroban-rpc.mainnet.stellar.gateway.fm', rpc = new S.rpc.Server(url, {timeout: 60});
  Promise.resolve().then(async () => {
    if (mode === 'inventory') {
      const out = args[0]; assert(out && !fs.existsSync(out), 'usage: inventory NEW_OUTPUT.json [--deps FULL_CONFIG.json] [--rpc URL]');
      const depsFile = opt('--deps'), deps = depsFile ? JSON.parse(fs.readFileSync(depsFile)).dependencies : null;
      const inv = await inventory(rpc, {deps}); fs.writeFileSync(out, json(inv), {flag: 'wx'});
      const tsv = ['liveUntilLedger\tledgersLeft\tapproxDays\townerRole\ttype\tcontract\tlabel\tsources', ...inv.rows.map(r => [r.liveUntilLedger, r.ledgersLeft, r.approxDaysLeft, r.ownerRole, r.type + (r.durability ? ':' + r.durability : ''), r.contract || r.hash, r.label, r.sources.join(',')].join('\t'))].join('\n') + '\n';
      fs.writeFileSync(out.replace(/\.json$/, '.tsv'), tsv, {flag: 'wx'});
      return {status: inv.status, latestLedger: inv.latestLedger, counts: inv.counts, earliest: inv.earliest, failedViews: inv.failedViews.length, out};
    }
    if (mode === 'prepare-extension') {
      const source = opt('--source'), extendTo = Number(opt('--extend-to') || '3110000'), inv = JSON.parse(fs.readFileSync(args[0])), roles = (opt('--roles') || 'vault,receipt').split(',');
      const below = Number(opt('--below-ledgers-left') || '3110400'), types = (opt('--types') || 'data,code').split(','), cap = opt('--max-fee-stroops');
      const keysXdr = inv.rows.filter(r => r.present && roles.includes(r.ownerRole) && r.ledgersLeft < below && types.includes(r.type)).map(r => r.keyXdr);
      assert(keysXdr.length, 'no key selected'); return prepareExtension(rpc, {source, keysXdr: keysXdr.slice(0, 40), extendTo, ...(cap ? {maxFeeStroops: Number(cap)} : {})});
    }
    throw new Error('usage: inventory NEW_OUTPUT.json [--deps FULL_CONFIG.json] [--rpc URL] | prepare-extension INVENTORY.json --source G... [--roles vault,receipt[,controller,venue:pool,...]] [--types data,code] [--extend-to LEDGERS] [--below-ledgers-left N] [--max-fee-stroops N] [--rpc URL]');
  }).then(v => console.log(json(v))).catch(e => { console.error(e.message); process.exitCode = 1; });
}
