// Ledger-sourced builder for the Vault-B retry (16 Nov 2026). Builds ONE unsigned transaction with the owner's Ledger
// account (GC4T5M…T7FP) as source, after a fresh quote, the open-command check and a fresh simulation taken right before
// it is written. It never signs, never sends and never reads keys: the operator signs on the device with
//   stellar tx sign --sign-with-ledger --rpc-url "$RPC_URL" --network-passphrase "$NET" F.unsigned.xdr > F.signed.xdr
// checks it with `verify-signed`, sends it with `stellar tx send`, and reads the outcome with `status`.
//
//   read                                       owner units/account, receipt state, open commands, wallet, both quotes
//   prepare-close --out F [--deadline-seconds N]   stage_prepare_close(owner, ALL units, min_swap, deadline)   (A3)
//   prepare-open  --out F [--deadline-seconds N]   stage_open(owner, 800 PYUSD, swap_in, min_swap, min_shares, reserve, deadline) (B7)
//   prepare-step <method> <id> --out F         Ledger fallback for a permissionless id call (A4/A5, B8–B11 if the keeper stops)
//   verify-signed F.unsigned.xdr F.signed.xdr  same transaction hash; exactly one valid owner signature
//   status <hash>                              RPC + Horizon outcome (read-only)
//   job <id>                                   one command's kind/phase/deadline (read-only)
//
// Fixed parameters (operator decisions, never widened here): quote tolerance 20 bps, reserve = minimum + 50%,
// principal exactly 8,000,000,000 raw, full exit only, deadline <= 3600 s, host CPU stop 97M, 40-ledger validity window.
'use strict';
const fs = require('node:fs');
const crypto = require('node:crypto');
const assert = require('node:assert/strict');
const S = require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');

const PINS = Object.freeze({
  network: S.Networks.PUBLIC, rpc: 'https://soroban-rpc.mainnet.stellar.gateway.fm', horizon: 'https://horizon.stellar.org',
  owner: 'GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP',
  receipt: 'CCGKTKPAWQHKI3TMX4RHLLZ3WYFELHERVNWOWMAZUHTKIAU2ENMHXBCI', vault: 'CD6N2EQJN47SYLSIYUK3YVBKD4YISATZ2CZZJQZCXKNNVB4HVR6AWL63',
  fundingAsset: 'CCCRWH6Q3FNP3I2I57BDLM5AFAT7O6OF6GKQOC6SSJNDAVRZ57SPHGU2',   // PYUSD SAC
  receiptWasm: '140dcaab08680be910f9ea65f6682b72c1c4029bd991bca9397b516af9b66823',
  vaultWasm: 'e66380303e50c6177492d1c9e880edcc84b4258da2cd46be46a1b3f8fe12e3e9',
});
const FIXED = Object.freeze({principalRaw: 8000000000n, toleranceBps: 20, reserveMarginBps: 5000, slippageBps: 100, divergenceBps: 200,
  maxDeadlineSeconds: 3600, defaultDeadlineSeconds: 3000, cpuStop: 97000000, window: 40, bidStroops: 10000n, maxFeeStroops: 5000000n,
  minXlmAfterFeeStroops: 10000000n});
const STEP_METHODS = Object.freeze(['stage_withdraw', 'stage_close_convert', 'stage_convert', 'stage_mint', 'stage_invest', 'stage_finalise', 'stage_cancel']);
const u = x => BigInt(String(x));
const json = v => JSON.stringify(v, (_, x) => typeof x === 'bigint' ? String(x) : x, 2);
const addr = a => S.nativeToScVal(a, {type: 'address'});
const floor20 = x => { const n = u(x) * BigInt(10000 - FIXED.toleranceBps) / 10000n; return n > 0n ? n : 1n; };

// ---------------------------------------------------------------- pure checks (unit-tested)
function checkCloseQuote(q, {units, ledger}) {
  assert.equal(q.valid, true, 'close quote not valid');
  assert.equal(u(q.shares), u(units), 'close quote is not for the full position');
  assert.equal(Number(q.ledger), ledger, 'close quote is not from the simulated ledger');
  assert(u(q.swap_out) > 0n && u(q.total) > 0n, 'empty close quote');
  assert.equal(u(q.min_swap), floor20(q.swap_out), 'min_swap is not the 20 bps floor of swap_out');
  assert.equal(Number(q.vault_slippage_bps), FIXED.slippageBps); assert.equal(Number(q.vault_divergence_bps), FIXED.divergenceBps);
}
function checkOpenQuote(q, {ledger}) {
  assert(q.valid === true && q.admissible === true, 'open quote not valid/admissible');
  assert.equal(q.oracle_fresh, true, 'oracle not fresh'); assert.equal(q.deposits_paused, false, 'vault deposits paused');
  assert.equal(q.composition_dirty, false, 'composition dirty'); assert.equal(q.in_band, true, 'pool out of band');
  assert.equal(Number(q.ledger), ledger, 'open quote is not from the simulated ledger');
  const amount = FIXED.principalRaw;
  assert(u(q.swap_in) > 0n && u(q.swap_in) < amount && u(q.swap_out) > 0n && u(q.shares) > 0n, 'degenerate open quote');
  assert.equal(u(q.min_swap), floor20(q.swap_out), 'min_swap is not the 20 bps floor');
  assert.equal(u(q.min_shares), floor20(q.shares), 'min_shares is not the 20 bps floor');
  assert.equal(u(q.reserve), u(q.min_reserve) + (u(q.min_reserve) * BigInt(FIXED.reserveMarginBps) + 9999n) / 10000n, 'reserve is not minimum + 50%');
  assert.equal(Number(q.vault_slippage_bps), FIXED.slippageBps); assert.equal(Number(q.vault_divergence_bps), FIXED.divergenceBps);
}
function checkIdle(st, commands) {
  assert.equal(Number(st.open_jobs), 0, 'an open command exists (open_jobs > 0)');
  assert.equal(commands.length, 0, 'owner has open commands');
}
function checkDeadline(deadline, closeTime, timeoutSeconds) {
  assert(deadline > BigInt(closeTime) && deadline <= BigInt(closeTime + FIXED.maxDeadlineSeconds), 'deadline outside (now, now+3600]');
  assert(BigInt(closeTime + timeoutSeconds) <= deadline, 'envelope validity must end before the command deadline');
}
const fnOf = inv => { const f = inv.function(); assert.equal(f.switch().name, 'sorobanAuthorizedFunctionTypeContractFn', 'unexpected authorized function type'); return f.contractFn(); };
const contractOf = f => S.Address.fromScAddress(f.contractAddress()).toString();
// Authorization: exactly one entry, the transaction source (owner) as credentials, the root equal to the operation itself.
function checkRootAuth(auth, op) {
  assert.equal(auth.length, 1, 'exactly one authorization entry required');
  assert.equal(auth[0].credentials().switch().name, 'sorobanCredentialsSourceAccount', 'only source-account (owner) authorization is allowed');
  const root = auth[0].rootInvocation();
  assert.equal(fnOf(root).toXDR('base64'), op.body().invokeHostFunctionOp().hostFunction().invokeContract().toXDR('base64'), 'authorized root differs from the operation');
  return root;
}
function checkCloseAuth(auth, op) { assert.equal(checkRootAuth(auth, op).subInvocations().length, 0, 'stage_prepare_close must not authorize anything nested'); }
// stage_open: root -> vault.stage_accept_funds(receipt, id, job) -> PYUSD.transfer(owner, vault, principal + reserve). Nothing else.
function checkOpenAuth(auth, op, {id, principal, reserve, owner = PINS.owner}) {
  const root = checkRootAuth(auth, op), subs = root.subInvocations();
  assert.equal(subs.length, 1, 'stage_open must authorize exactly one nested call');
  const accept = fnOf(subs[0]);
  assert.equal(contractOf(accept), PINS.vault); assert.equal(accept.functionName().toString(), 'stage_accept_funds');
  const aArgs = accept.args(); assert.equal(aArgs.length, 3);
  assert.equal(S.scValToNative(aArgs[0]), PINS.receipt, 'accept_funds for another receipt');
  assert.equal(u(S.scValToNative(aArgs[1])), u(id), 'accept_funds for another command id');
  const job = S.scValToNative(aArgs[2]);
  assert.equal(job.owner, owner); assert.equal(u(job.a), u(principal)); assert.equal(u(job.reserve), u(reserve));
  const inner = subs[0].subInvocations(); assert.equal(inner.length, 1, 'accept_funds must authorize exactly the funding transfer');
  assert.equal(inner[0].subInvocations().length, 0);
  const t = fnOf(inner[0]);
  assert.equal(contractOf(t), PINS.fundingAsset, 'transfer of a token other than PYUSD'); assert.equal(t.functionName().toString(), 'transfer');
  const [from, to, amount] = t.args().map(a => S.scValToNative(a));
  assert.equal(from, owner); assert.equal(to, PINS.vault, 'funds would go somewhere other than the vault');
  assert.equal(u(amount), u(principal) + u(reserve), 'transfer amount is not principal + reserve');
}
function checkResources(data, minResourceFee) {
  const r = data.resources(), fp = r.footprint(), keys = [...fp.readOnly(), ...fp.readWrite()].map(k => k.toXDR('base64'));
  assert(Number(r.instructions()) <= FIXED.cpuStop, `declared host CPU ${r.instructions()} above the 97M stop`);
  assert(keys.length <= 100 && new Set(keys).size === keys.length, 'more than 100 or duplicate transaction keys');
  assert(fp.readWrite().length <= 50, 'more than 50 writes');
  assert(data.ext().switch() === 0 || data.ext().v1().archivedSorobanEntries().length === 0, 'archived entries: restore first (outside this tool)');
  assert(u(data.resourceFee().toString()) >= u(minResourceFee));
  return {cpu: Number(r.instructions()), keys: keys.length, writes: fp.readWrite().length, readBytes: r.diskReadBytes(), writeBytes: r.writeBytes()};
}

// ---------------------------------------------------------------- chain
function chain(rpc = new S.rpc.Server(PINS.rpc, {timeout: 60})) {
  const view = async (contract, method, args = []) => {
    const tx = new S.TransactionBuilder(new S.Account(PINS.owner, '0'), {fee: '100', networkPassphrase: PINS.network})
      .addOperation(new S.Contract(contract).call(method, ...args)).setTimeout(0).build();
    const sim = await rpc.simulateTransaction(tx);
    if (!S.rpc.Api.isSimulationSuccess(sim)) throw Error(`view ${method} failed`);
    return {value: S.scValToNative(sim.result.retval), ledger: sim.latestLedger};
  };
  const codeHash = async c => {
    const k = S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({contract: new S.Address(c).toScAddress(), key: S.xdr.ScVal.scvLedgerKeyContractInstance(), durability: S.xdr.ContractDataDurability.persistent()}));
    const r = await rpc.getLedgerEntries(k); return r.entries[0].val.contractData().val().instance().executable().wasmHash().toString('hex');
  };
  return {rpc, view, codeHash};
}
async function snapshot(c) {
  const n = await c.rpc.getNetwork(); assert.equal(n.passphrase, PINS.network);
  assert.equal(await c.codeHash(PINS.receipt), PINS.receiptWasm, 'receipt code differs from the pilot pin');
  assert.equal(await c.codeHash(PINS.vault), PINS.vaultWasm, 'vault code differs from the pilot pin');
  const o = addr(PINS.owner), v = async (ct, m, a) => (await c.view(ct, m, a)).value;
  const [units, account, state, commands, config, receiptPaused, pyusd] = await Promise.all([
    v(PINS.receipt, 'stage_units', [o]), v(PINS.receipt, 'stage_account', [o]), v(PINS.receipt, 'stage_state'),
    v(PINS.receipt, 'stage_commands', [o]), v(PINS.receipt, 'stage_config'), v(PINS.receipt, 'stage_deposits_paused'),
    v(PINS.fundingAsset, 'balance', [o])]);
  const acc = await c.rpc.getAccount(PINS.owner), h = await (await fetch(`${PINS.horizon}/accounts/${PINS.owner}`)).json();
  const native = h.balances.find(b => b.asset_type === 'native');
  return {units, account, state, commands, config, receiptPaused, wallet: {pyusdRaw: String(pyusd), xlm: native.balance,
    xlmSellingLiabilities: native.selling_liabilities, subentries: h.subentry_count}, sequence: acc.sequenceNumber()};
}

async function build(c, {op, check, timeoutSeconds = 240}) {
  const acc = await c.rpc.getAccount(PINS.owner), latest = await c.rpc.getLatestLedger();
  const tx = new S.TransactionBuilder(acc, {fee: String(FIXED.bidStroops), networkPassphrase: PINS.network}).addOperation(op)
    .setLedgerbounds(latest.sequence + 1, latest.sequence + FIXED.window).setTimeout(timeoutSeconds).build();
  const sim = await c.rpc.simulateTransaction(tx);   // the simulation right before the Ledger signature
  assert(S.rpc.Api.isSimulationSuccess(sim), 'simulation failed: ' + String(sim.error || '').split('\n')[0].slice(0, 200));
  assert(!sim.restorePreamble, 'restoration needed: outside this tool');
  check(sim.result.auth || [], tx.operations.length === 1 ? tx.toEnvelope().v1().tx().operations()[0] : null, sim);
  const prepared = S.rpc.assembleTransaction(tx, sim).build();
  const data = prepared.toEnvelope().v1().tx().ext().sorobanData(), metrics = checkResources(data, sim.minResourceFee);
  assert.equal(u(prepared.fee), FIXED.bidStroops + u(data.resourceFee().toString()), 'fee is not bid + resource fee');
  assert(u(prepared.fee) <= FIXED.maxFeeStroops, `fee ${prepared.fee} above the 0.5 XLM cap`);
  assert.equal(prepared.signatures.length, 0); assert.equal(prepared.source, PINS.owner);
  return {prepared, sim, metrics, simulatedLedger: sim.latestLedger, validLedgers: {min: latest.sequence + 1, maxExclusive: latest.sequence + FIXED.window}};
}
function write(out, res, summary) {
  assert(out && !fs.existsSync(out) && !fs.existsSync(out.replace(/\.xdr$/, '') + '.json'), '--out FILE.unsigned.xdr required and must not exist');
  fs.writeFileSync(out, res.prepared.toXDR() + '\n', {mode: 0o600, flag: 'wx'});
  const record = {status: 'UNSIGNED_PREPARED_SIGN_ON_LEDGER', signing: false, submission: false, file: out, hash: res.prepared.hash().toString('hex'),
    source: PINS.owner, sequence: res.prepared.sequence, maxFeeStroops: res.prepared.fee, validLedgers: res.validLedgers,
    simulatedLedger: res.simulatedLedger, metrics: res.metrics, ...summary,
    next: `stellar tx sign --sign-with-ledger --rpc-url ${PINS.rpc} --network-passphrase "${PINS.network}" ${out} > ${out.replace(/\.unsigned\.xdr$|\.xdr$/, '')}.signed.xdr`};
  fs.writeFileSync(out.replace(/\.xdr$/, '') + '.json', json(record) + '\n', {mode: 0o600, flag: 'wx'});
  return record;
}
const deadlineOf = (closeTime, seconds) => { assert(Number.isSafeInteger(seconds) && seconds > 240 && seconds <= FIXED.maxDeadlineSeconds); return BigInt(closeTime + seconds); };

async function prepareClose(c, {out, deadlineSeconds = FIXED.defaultDeadlineSeconds}) {
  const s = await snapshot(c); checkIdle(s.state, s.commands);
  const units = u(s.units); assert(units > 0n, 'no units to close'); assert.equal(u(s.account.available_units), units, 'units are locked');
  const qv = await c.view(PINS.vault, 'stage_quote_close', [S.nativeToScVal(units, {type: 'u128'}), S.nativeToScVal(FIXED.toleranceBps, {type: 'u32'})]);
  checkCloseQuote(qv.value, {units, ledger: qv.ledger});
  const deadline = deadlineOf(Number(qv.value.timestamp), deadlineSeconds); checkDeadline(deadline, Number(qv.value.timestamp), 240);
  const op = new S.Contract(PINS.receipt).call('stage_prepare_close', addr(PINS.owner), S.nativeToScVal(units, {type: 'u128'}),
    S.nativeToScVal(u(qv.value.min_swap), {type: 'u128'}), S.nativeToScVal(deadline, {type: 'u64'}));
  const res = await build(c, {op, check: (auth, o, sim) => { checkCloseAuth(auth, o); assert.equal(u(S.scValToNative(sim.result.retval)), u(s.state.next_id), 'command id is not next_id'); }});
  return write(out, res, {step: 'A3', call: 'stage_prepare_close', commandId: String(s.state.next_id), args: {owner: PINS.owner, shares: String(units), min_swap: String(qv.value.min_swap), deadline: String(deadline),
    deadlineIso: new Date(Number(deadline) * 1000).toISOString()}, quote: qv.value, walletBefore: s.wallet});
}
async function prepareOpen(c, {out, deadlineSeconds = FIXED.defaultDeadlineSeconds}) {
  const s = await snapshot(c); checkIdle(s.state, s.commands);
  assert.equal(s.receiptPaused, false, 'receipt deposits paused');
  assert(u(s.state.principal_backing) + FIXED.principalRaw <= u(s.config.backing_cap), 'backing cap: principal_backing + 800 exceeds backing_cap (close first)');
  const amount = FIXED.principalRaw;
  const qv = await c.view(PINS.vault, 'stage_quote_open', [S.nativeToScVal(amount, {type: 'u128'}), S.nativeToScVal(FIXED.toleranceBps, {type: 'u32'}), S.nativeToScVal(FIXED.reserveMarginBps, {type: 'u32'})]);
  const q = qv.value; checkOpenQuote(q, {ledger: qv.ledger});
  assert(u(s.wallet.pyusdRaw) >= amount + u(q.reserve), `wallet PYUSD ${s.wallet.pyusdRaw} below 800 + reserve ${q.reserve}`);
  const deadline = deadlineOf(Number(q.timestamp), deadlineSeconds); checkDeadline(deadline, Number(q.timestamp), 240);
  const op = new S.Contract(PINS.receipt).call('stage_open', addr(PINS.owner), ...[amount, u(q.swap_in), u(q.min_swap), u(q.min_shares), u(q.reserve)].map(x => S.nativeToScVal(x, {type: 'u128'})),
    S.nativeToScVal(deadline, {type: 'u64'}));
  const res = await build(c, {op, check: (auth, o, sim) => {
    checkOpenAuth(auth, o, {id: s.state.next_id, principal: amount, reserve: q.reserve});
    assert.equal(u(S.scValToNative(sim.result.retval)), u(s.state.next_id), 'command id is not next_id');
  }});
  const xlmStroops = BigInt(Math.round(Number(s.wallet.xlm) * 1e7));
  assert(xlmStroops - u(res.prepared.fee) >= FIXED.minXlmAfterFeeStroops, 'owner XLM too low for the fee');
  return write(out, res, {step: 'B7', call: 'stage_open', commandId: String(s.state.next_id), args: {owner: PINS.owner, amount: String(amount), swap_in: String(q.swap_in), min_swap: String(q.min_swap),
    min_shares: String(q.min_shares), reserve: String(q.reserve), deadline: String(deadline), deadlineIso: new Date(Number(deadline) * 1000).toISOString()},
    pays: {pyusdRaw: String(amount + u(q.reserve)), to: PINS.vault}, quote: q, walletBefore: s.wallet,
    keeperMustFinishBy: 'convert, mint, 3 consecutive observations, invest and finalise before the deadline; otherwise cancel at phase 3'});
}
async function prepareStep(c, {method, id, out}) {
  assert(STEP_METHODS.includes(method), 'method outside the Ledger fallback list'); assert(/^[0-9]+$/.test(String(id)));
  const job = (await c.view(PINS.receipt, 'stage_job', [S.nativeToScVal(u(id), {type: 'u64'})])).value;
  assert.equal(job.owner, PINS.owner, 'job belongs to another owner');
  const op = new S.Contract(PINS.receipt).call(method, S.nativeToScVal(u(id), {type: 'u64'}));
  const res = await build(c, {op, check: auth => assert.equal(auth.length, 0, 'permissionless step unexpectedly requires authorization')});
  return write(out, res, {step: 'fallback', call: method, commandId: String(id), jobBefore: {kind: Number(job.kind), phase: Number(job.phase), deadline: String(job.deadline)}});
}
function verifySigned(unsignedText, signedText) {
  const a = S.TransactionBuilder.fromXDR(unsignedText.trim(), PINS.network), b = S.TransactionBuilder.fromXDR(signedText.trim(), PINS.network);
  assert.equal(b.hash().toString('hex'), a.hash().toString('hex'), 'signed transaction differs from the prepared one');
  assert.equal(a.signatures.length, 0); assert.equal(b.signatures.length, 1, 'exactly one signature expected');
  const kp = S.Keypair.fromPublicKey(PINS.owner);
  assert(kp.verify(b.hash(), b.signatures[0].signature()), 'signature is not the owner Ledger key');
  return {status: 'SIGNED_OK', hash: b.hash().toString('hex'), sequence: b.sequence, maxFeeStroops: b.fee, validLedgers: b.ledgerBounds};
}
async function status(c, hash) {
  assert(/^[0-9a-f]{64}$/.test(hash));
  const r = await c.rpc.getTransaction(hash), h = await fetch(`${PINS.horizon}/transactions/${hash}`).then(x => x.ok ? x.json() : null);
  return {rpc: {status: r.status, ledger: r.ledger ?? null, returnValue: r.returnValue ? S.scValToNative(r.returnValue) : null},
    horizon: h ? {successful: h.successful, ledger: h.ledger, feeChargedStroops: h.fee_charged, source: h.source_account} : 'NOT_FOUND', latestLedger: r.latestLedger};
}
const PHASES = {0: 'created', 1: 'funded', 2: 'converted', 3: 'minted', 4: 'finalised', 5: 'invested', 10: 'close locked', 11: 'withdrawn (mixture held)', 12: 'closed (paid)', 99: 'cancelled'};
async function job(c, id) {
  assert(/^[0-9]+$/.test(String(id)));
  const j = (await c.view(PINS.receipt, 'stage_job', [S.nativeToScVal(u(id), {type: 'u64'})])).value;
  return {id: String(id), kind: Number(j.kind) === 0 ? 'open' : 'close', phase: Number(j.phase), phaseName: PHASES[Number(j.phase)] ?? 'other',
    owner: j.owner, a: String(j.a), b: String(j.b), shares: String(j.shares), reserve: String(j.reserve), deadline: String(j.deadline),
    deadlineIso: new Date(Number(j.deadline) * 1000).toISOString(), signing: false};
}
async function read(c) {
  const s = await snapshot(c), units = u(s.units);
  const close = units > 0n ? (await c.view(PINS.vault, 'stage_quote_close', [S.nativeToScVal(units, {type: 'u128'}), S.nativeToScVal(FIXED.toleranceBps, {type: 'u32'})])).value : null;
  const open = (await c.view(PINS.vault, 'stage_quote_open', [S.nativeToScVal(FIXED.principalRaw, {type: 'u128'}), S.nativeToScVal(FIXED.toleranceBps, {type: 'u32'}), S.nativeToScVal(FIXED.reserveMarginBps, {type: 'u32'})])).value;
  return {...s, closeQuote: close, openQuote: open, signing: false, submission: false};
}

module.exports = {PINS, FIXED, floor20, checkCloseQuote, checkOpenQuote, checkIdle, checkDeadline, checkRootAuth, checkCloseAuth, checkOpenAuth, checkResources, verifySigned,
  sourceSha256: () => crypto.createHash('sha256').update(fs.readFileSync(__filename)).digest('hex')};
if (require.main === module) {
  const [mode, ...args] = process.argv.slice(2), opt = n => { const i = args.indexOf(n); return i < 0 ? undefined : args[i + 1]; };
  const ds = opt('--deadline-seconds') ? Number(opt('--deadline-seconds')) : undefined, c = mode === 'verify-signed' ? null : chain();
  const run = {
    read: () => read(c),
    'prepare-close': () => prepareClose(c, {out: opt('--out'), deadlineSeconds: ds}),
    'prepare-open': () => prepareOpen(c, {out: opt('--out'), deadlineSeconds: ds}),
    'prepare-step': () => prepareStep(c, {method: args[0], id: args[1], out: opt('--out')}),
    'verify-signed': async () => verifySigned(fs.readFileSync(args[0], 'utf8'), fs.readFileSync(args[1], 'utf8')),
    status: () => status(c, args[0]),
    job: () => job(c, args[0]),
  }[mode];
  if (!run) { console.error('usage: read | prepare-close --out F | prepare-open --out F | prepare-step <method> <id> --out F | verify-signed U S | status <hash> | job <id>'); process.exit(1); }
  run().then(r => console.log(json(r))).catch(e => { console.error('REFUSED: ' + e.message); process.exitCode = 1; });
}
