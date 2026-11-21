// HG4 Mainnet signer tests. Offline: fake RPC objects only, no network. The only keys are throwaway keys created in a
// temp directory by createKeyFile for the signing tests and deleted afterwards; no Mainnet key is touched or created.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {loadSdk, MAINNET, TESTNET} from '../src/chain.mjs';
import {normalize} from '../src/config.mjs';
import {failoverRpc} from '../src/heartbeat.mjs';
import {makeHeartbeatRpc, botAllow, makeSender, makeHeartbeatSigner, openBudget, openGate, gateBinding, acquireRunLock, readPins, assertPins,
  validateMainnetConfig, FORBIDDEN_SIGNERS, NEVER_TOUCH, CPU_STOP, trustlineStatus, requireTrustlines, startChecks} from '../src/mainnet.mjs';
import {loadKeyFile, createKeyFile} from '../src/keyfile.mjs';
import {exitCall, exitAllow, PHASE} from '../src/exit-seized.mjs';
import {trustlineAssets, buildTrustlineTx} from '../src/trustlines.mjs';
import {mainnetLiveMode, stageCall} from '../src/main.mjs';

const S = loadSdk();
const C = b => S.StrKey.encodeContract(Buffer.alloc(32, b)), Gk = b => S.StrKey.encodeEd25519PublicKey(Buffer.alloc(32, b));
const PINS = {receipt: 'a'.repeat(64), vault: 'b'.repeat(64), controller: 'c'.repeat(64), usdcMarket: 'd'.repeat(64)};
const tmp = t => { const d = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'clliq-main-'))); fs.chmodSync(d, 0o700); t.after(() => fs.rmSync(d, {recursive: true, force: true})); return d; };
function raw(over = {}) {
  return {networkPassphrase: MAINNET, rpcUrl: 'https://rpc.invalid',
    contracts: {receipt: C(1), vault: C(2), controller: C(3), usdcMarket: C(4), usdc: 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75', pyusd: 'CCCRWH6Q3FNP3I2I57BDLM5AFAT7O6OF6GKQOC6SSJNDAVRZ57SPHGU2', oracle: C(7)},
    pins: PINS, admin: 'GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP', team3: Gk(9), borrowers: [Gk(9)],
    liquidator: Gk(10), executorKeyFile: '/k/liq.secret', heartbeatAccount: Gk(11), heartbeatKeyFile: '/k/hb.secret', heartbeatRpcUrls: ['https://rpc-a.invalid', 'https://rpc-b.invalid'], repayCapRaw: '1000000000', dataDir: '/tmp/x', ...over};
}
const cfgOf = (over = {}) => normalize(raw(over));

// ------------------------------------------------------------------ config
test('mainnet config: example shape accepted; CLI aliases, placeholders, pilot/retired targets, missing pins refused', () => {
  const c = cfgOf();
  assert.equal(c.dryRunGateHours, 12); assert.equal(c.fees.maxFeeStroops, 21_000_000); assert.equal(c.fees.heartbeat.maxFeeStroops, 5_100_000);
  assert.equal(c.fees.resourceCeilingStroops.checkpoint_nav, 5_000_000); assert.equal(c.fees.resourceCeilingStroops.liquidate, 20_000_000); assert.equal(c.fees.heartbeat.resourceCeilingStroops, 5_000_000);
  assert.throws(() => cfgOf({executorIdentity: 'cl-liq'}), /Testnet-only/);
  assert.throws(() => cfgOf({heartbeatIdentity: 'cl-hb-main'}), /Testnet-only/);
  assert.throws(() => cfgOf({contracts: {...raw().contracts, receipt: 'PLACEHOLDER'}}), /contract id/);
  assert.throws(() => cfgOf({contracts: {...raw().contracts, receipt: 'CCGKTKPAWQHKI3TMX4RHLLZ3WYFELHERVNWOWMAZUHTKIAU2ENMHXBCI'}}), /pilot\/retired/);
  assert.throws(() => cfgOf({contracts: {...raw().contracts, vault: NEVER_TOUCH.at(-1)}}), /pilot\/retired/);
  assert.throws(() => cfgOf({pins: {...PINS, usdcMarket: undefined}}), /pins.usdcMarket/);
  assert.throws(() => cfgOf({executorKeyFile: 'rel/liq.secret'}), /executorKeyFile/);
  assert.throws(() => cfgOf({repayCapRaw: null}), /repayCapRaw/);
  const example = JSON.parse(fs.readFileSync(new URL('../config/mainnet.example.json', import.meta.url)));
  assert.throws(() => normalize(example), /liquidator must be a public/);
  assert.throws(() => normalize({...example, liquidator: Gk(10), heartbeatAccount: Gk(11)}), /heartbeatRpcUrls/);   // second RPC is a placeholder
  assert.doesNotThrow(() => normalize({...example, liquidator: Gk(10), heartbeatAccount: Gk(11), heartbeatRpcUrls: ['https://rpc-a.invalid', 'https://rpc-b.invalid']}));
});
test('mainnet config: role separation (admin, cash liquidator, cash pause key, keeper, T3, heartbeat = liquidator)', () => {
  for (const g of Object.keys(FORBIDDEN_SIGNERS)) {
    assert.throws(() => cfgOf({liquidator: g}), /refused: signer is/);
    assert.throws(() => cfgOf({heartbeatAccount: g}), /refused: signer is/);
  }
  assert.throws(() => cfgOf({liquidator: Gk(9)}), /T3|borrowers/);
  assert.throws(() => cfgOf({heartbeatAccount: Gk(10)}), /distinct/);
  assert.throws(() => cfgOf({heartbeatKeyFile: '/k/liq.secret'}), /distinct from executorKeyFile/);
  assert.throws(() => cfgOf({borrowers: [Gk(10)]}), /other than the bot accounts/);
});
test('mainnet config: fee caps, heartbeat fee, gate hours bounded', () => {
  assert.throws(() => cfgOf({fees: {maxFeeStroops: 60_000_000, dailyBudgetStroops: 100_000_000}}), /5 XLM/);
  assert.throws(() => cfgOf({fees: {heartbeat: {maxFeeStroops: 7_000_000}}}), /0.6 XLM/);
  assert.throws(() => cfgOf({fees: {heartbeat: {resourceCeilingStroops: 6_000_000}}}), /0.5 XLM/);
  assert.throws(() => cfgOf({fees: {resourceCeilingStroops: {liquidate: 30_000_000}}}), /exceeds maxFeeStroops/);
  assert.throws(() => cfgOf({fees: {resourceCeilingStroops: {transfer: 1}}}), /unknown call/);
  assert.equal(cfgOf({fees: {resourceCeilingStroops: {checkpoint_nav: 1_000_000}}}).fees.resourceCeilingStroops.liquidate, 20_000_000);
  assert.throws(() => cfgOf({heartbeatFeeStroops: 1_000_000, fees: {heartbeat: {maxFeeStroops: 1_000_000, resourceCeilingStroops: 500_000}}}), /must exceed the heartbeat inclusion fee/);
  assert.throws(() => cfgOf({dryRunGateHours: 1}), /2..72/);
  assert.equal(cfgOf({dryRunGateHours: 2}).dryRunGateHours, 2);
  assert.throws(() => cfgOf({secretKey: 'x'}), /key material/);
});
test('mainnet live mode needs BOTH --live and DRY_RUN=false', () => {
  assert.equal(mainnetLiveMode(false, {}), false);
  assert.equal(mainnetLiveMode(false, {DRY_RUN: 'true'}), false);
  assert.throws(() => mainnetLiveMode(true, {}), /DRY_RUN=false/);
  assert.throws(() => mainnetLiveMode(true, {DRY_RUN: 'no'}), /DRY_RUN=false/);
  assert.throws(() => mainnetLiveMode(false, {DRY_RUN: 'false'}), /ambiguous/);
  assert.equal(mainnetLiveMode(true, {DRY_RUN: 'false'}), true);
});

// ------------------------------------------------------------------ allowlist
test('bot allowlist: exactly the stage calls of main.mjs with bounded arguments', () => {
  const c = cfgOf(), call = (s, seq = 100) => stageCall(s, c, seq);
  for (const s of [{stage: 'L1a'}, {stage: 'L1b'}, {stage: 'L2'}, {stage: 'L4'}, {stage: 'L5', id: 3}, {stage: 'APPROVE', amount: 1_000_000_000n},
    {stage: 'L3', borrower: Gk(9), repay: 100n}]) { const x = call(s); assert.doesNotThrow(() => botAllow(c, x.contract, x.fn, x.args), s.stage); }
  const ap = call({stage: 'APPROVE', amount: 1_000_000_001n}); assert.throws(() => botAllow(c, ap.contract, ap.fn, ap.args), /outside/);
  const l3 = call({stage: 'L3', borrower: Gk(12), repay: 100n}); assert.throws(() => botAllow(c, l3.contract, l3.fn, l3.args), /outside/);
  const big = call({stage: 'L3', borrower: Gk(9), repay: 1_000_000_001n}); assert.throws(() => botAllow(c, big.contract, big.fn, big.args), /outside/);
  assert.throws(() => botAllow(c, c.contracts.usdc, 'approve', {from: c.liquidator, spender: C(5), amount: 1n, expiration_ledger: 1}), /outside/);
  assert.throws(() => botAllow(c, c.contracts.usdc, 'transfer', {from: c.liquidator, to: Gk(1), amount: 1n}), /not on the liquidator allowlist/);
  assert.throws(() => botAllow(c, c.contracts.controller, 'cache_price', {token: c.contracts.pyusd}), /outside/);
  assert.throws(() => botAllow(c, NEVER_TOUCH[0], 'checkpoint_nav', {}), /pilot/);
});

// ------------------------------------------------------------------ fake RPC
function fakeRpc(c, o = {}) {
  const sent = [], events = [];
  const auth = o.auth ?? ((contract, fn) => [new S.xdr.SorobanAuthorizationEntry({credentials: S.xdr.SorobanCredentials.sorobanCredentialsSourceAccount(),
    rootInvocation: new S.xdr.SorobanAuthorizedInvocation({function: S.xdr.SorobanAuthorizedFunction.sorobanAuthorizedFunctionTypeContractFn(new S.xdr.InvokeContractArgs({
      contractAddress: new S.Address(contract).toScAddress(), functionName: fn, args: []})), subInvocations: []})})]);
  const codeOf = o.code ?? (role => PINS[role]);
  const ids = ['receipt', 'vault', 'controller', 'usdcMarket'];
  return {sent, events,
    getNetwork: async () => ({passphrase: MAINNET, protocolVersion: 25}),
    getAccount: async a => new S.Account(a, '100'),
    simulateTransaction: async tx => {
      events.push('simulate');
      if (o.simError) return {error: 'HostError: Error(Contract, #2105)', _parsed: true, latestLedger: 1, events: []};
      const inv = tx.operations[0].func.invokeContract(), contract = S.Address.fromScAddress(inv.contractAddress()).toString(), fn = inv.functionName().toString();
      return {_parsed: true, latestLedger: 1, events: [], minResourceFee: String(o.resourceFee ?? 50_000), ...(o.restore ? {restorePreamble: {minResourceFee: '1', transactionData: new S.SorobanDataBuilder()}} : {}),
        transactionData: new S.SorobanDataBuilder().setResources(o.cpu ?? 30_000_000, 1000, 100).setResourceFee(String(o.resourceFee ?? 50_000)),
        result: {auth: o.noAuth ? [] : auth(contract, fn), retval: S.xdr.ScVal.scvBool(true)}};
    },
    getLedgerEntries: async (...keys) => ({latestLedger: 5, entries: keys.flatMap(k => {
      if (k.switch().name !== 'contractData') return [];
      const d = k.contractData(), id = S.Address.fromScAddress(d.contract()).toString(), role = ids.find(r => c.contracts[r] === id);
      const isInst = d.key().switch().name === 'scvLedgerKeyContractInstance';
      if (isInst) return [{val: {contractData: () => ({contract: () => d.contract(), key: () => d.key(), val: () => ({instance: () => ({executable: () => ({wasmHash: () => Buffer.from(codeOf(role), 'hex')})})})})}}];
      return (o.pending ?? []).includes(role) ? [{val: {contractData: () => ({contract: () => d.contract(), key: () => d.key(), val: () => S.xdr.ScVal.scvBytes(Buffer.alloc(32))})}}] : [];
    })}),
    sendTransaction: async tx => { events.push('send'); sent.push(tx); if (o.sendThrows) throw Error('socket hang up'); return {status: o.sendStatus ?? 'PENDING', hash: tx.hash().toString('hex')}; },
    getTransaction: async () => (o.never ? {status: 'NOT_FOUND'} : {status: o.final ?? 'SUCCESS', ledger: 6, feeCharged: 40_000n, returnValue: S.xdr.ScVal.scvBool(false)}),
  };
}
function senderFixture(t, o = {}) {
  const d = tmp(t), keyFile = path.join(d, 'liq.secret'), pub = createKeyFile(keyFile);
  const c = cfgOf({liquidator: pub, executorKeyFile: keyFile, dataDir: d, ...o.cfg});
  const rpc = fakeRpc(c, o), budget = openBudget(d, c), notes = [];
  let now = 0;
  const send = makeSender(c, {rpc, S, account: pub, keyFile, allow: botAllow, budget, clock: () => now, sleep: async ms => { now += ms; }, pollMs: 1000, pollTimeoutMs: 5000});
  const nav = () => send(c.contracts.receipt, 'checkpoint_nav', {}, {onSigned: h => { notes.push(h); rpc.events.push('journal'); }});
  return {c, rpc, budget, send, nav, notes, d, pub};
}

test('sender: simulate -> checks -> sign -> journal hash -> send once -> final; retval decoded; fee trued up', async t => {
  const f = senderFixture(t);
  const r = await f.nav();
  assert.equal(r.ok, true); assert.equal(r.unknown, false); assert.equal(r.stdout, 'false');
  assert.deepEqual(f.rpc.events, ['simulate', 'journal', 'send']); assert.equal(f.rpc.sent.length, 1);
  assert.equal(r.hash, f.notes[0]); assert.equal(f.rpc.sent[0].hash().toString('hex'), r.hash);
  assert.equal(f.rpc.sent[0].source, f.pub); assert.equal(f.rpc.sent[0].signatures.length, 1);
  assert.equal(f.budget.spent('liquidator'), 40_000n);
});
test('sender: every refusal before signing is a plain failure (nothing signed, nothing sent, nothing charged)', async t => {
  const nested = (contract, fn) => [new S.xdr.SorobanAuthorizationEntry({credentials: S.xdr.SorobanCredentials.sorobanCredentialsSourceAccount(),
    rootInvocation: new S.xdr.SorobanAuthorizedInvocation({function: S.xdr.SorobanAuthorizedFunction.sorobanAuthorizedFunctionTypeContractFn(new S.xdr.InvokeContractArgs({contractAddress: new S.Address(contract).toScAddress(), functionName: fn, args: []})),
      subInvocations: [new S.xdr.SorobanAuthorizedInvocation({function: S.xdr.SorobanAuthorizedFunction.sorobanAuthorizedFunctionTypeContractFn(new S.xdr.InvokeContractArgs({contractAddress: new S.Address(contract).toScAddress(), functionName: 'x', args: []})), subInvocations: []})]})})];
  const foreign = contract => [new S.xdr.SorobanAuthorizationEntry({credentials: S.xdr.SorobanCredentials.sorobanCredentialsAddress(new S.xdr.SorobanAddressCredentials({
    address: new S.Address(Gk(1)).toScAddress(), nonce: S.xdr.Int64.fromString('0'), signatureExpirationLedger: 1, signature: S.xdr.ScVal.scvVoid()})),
    rootInvocation: new S.xdr.SorobanAuthorizedInvocation({function: S.xdr.SorobanAuthorizedFunction.sorobanAuthorizedFunctionTypeContractFn(new S.xdr.InvokeContractArgs({contractAddress: new S.Address(contract).toScAddress(), functionName: 'checkpoint_nav', args: []})), subInvocations: []})})];
  const other = () => [new S.xdr.SorobanAuthorizationEntry({credentials: S.xdr.SorobanCredentials.sorobanCredentialsSourceAccount(),
    rootInvocation: new S.xdr.SorobanAuthorizedInvocation({function: S.xdr.SorobanAuthorizedFunction.sorobanAuthorizedFunctionTypeContractFn(new S.xdr.InvokeContractArgs({contractAddress: new S.Address(C(5)).toScAddress(), functionName: 'transfer', args: []})), subInvocations: []})})];
  for (const [o, re] of [[{cpu: CPU_STOP + 1}, /97M/], [{resourceFee: 1_000_000, mutate: c => { c.fees.maxFeeStroops = 1_000_000; }}, /above cap/], [{resourceFee: 4_200_000}, /ceiling 5000000/], [{auth: nested}, /nested/], [{auth: foreign}, /non-source/], [{auth: other}, /root differs/],
    [{restore: true}, /restore/], [{simError: true}, /simulation failed/], [{code: r => (r === 'vault' ? 'e'.repeat(64) : PINS[r])}, /vault code/],
    [{cfg: {fees: {dailyBudgetStroops: 21_000_000, maxFeeStroops: 21_000_000, resourceCeilingStroops: {checkpoint_nav: 20_000_000}}}, resourceFee: 15_000_000}, /daily fee budget/]]) {
    const f = senderFixture(t, o);
    o.mutate?.(f.c);   // defence in depth: the per-tx cap still applies if the config is changed after validation
    if (o.cfg?.fees?.dailyBudgetStroops) f.budget.reserve('liquidator', 5_000_000n);
    const before = f.budget.spent('liquidator');
    const r = await f.nav();
    assert.equal(r.ok, false); assert.equal(r.unknown, false, String(re)); assert.match(r.stderr, re);
    assert.equal(f.rpc.sent.length, 0); assert.equal(f.notes.length, 0); assert.equal(f.budget.spent('liquidator'), before);
  }
  const f = senderFixture(t);
  const r = await f.send(f.c.contracts.usdc, 'transfer', {from: f.pub, to: Gk(1), amount: 1n});
  assert.equal(r.ok, false); assert.equal(r.refused, true); assert.equal(f.rpc.events.length, 0);
});
test('sender: rejected send refunds; no final status or a throwing send is UNKNOWN (intent stays open, fee stays charged)', async t => {
  let f = senderFixture(t, {sendStatus: 'ERROR'}); let r = await f.nav();
  assert.deepEqual([r.ok, r.unknown], [false, false]); assert.equal(f.budget.spent('liquidator'), 0n); assert.equal(f.notes.length, 1);
  f = senderFixture(t, {never: true}); r = await f.nav();
  assert.deepEqual([r.ok, r.unknown], [false, true]); assert.equal(r.hash, f.notes[0]); assert(f.budget.spent('liquidator') > 0n);
  f = senderFixture(t, {sendThrows: true}); r = await f.nav();
  assert.deepEqual([r.ok, r.unknown], [false, true]);
  f = senderFixture(t, {final: 'FAILED'}); r = await f.nav();
  assert.deepEqual([r.ok, r.unknown], [false, false]); assert.match(r.stderr, /FAILED/);
});
test('sender: refused on a non-Mainnet config, and re-checks the network on every call', async t => {
  const f = senderFixture(t);
  assert.throws(() => makeSender({...f.c, networkPassphrase: TESTNET}, {rpc: f.rpc, account: f.pub, keyFile: 'x', allow: botAllow, budget: f.budget}), /non-Mainnet/);
  f.c.networkPassphrase = TESTNET;
  const r = await f.nav(); assert.equal(r.ok, false); assert.match(r.stderr, /network changed/); assert.equal(f.rpc.events.length, 0);
});
test('pins: code hash per role and pending upgrade proposals from one read', async () => {
  const c = cfgOf(), rpc = fakeRpc(c, {pending: ['receipt']});
  const p = await readPins(c, rpc, S);
  assert.deepEqual(p.pending, ['receipt']); assert.doesNotThrow(() => assertPins(c, p));
  assert.throws(() => assertPins(c, {code: {...p.code, controller: 'f'.repeat(64)}}), /controller code/);
  assert.throws(() => assertPins(c, {code: {...p.code, usdcMarket: undefined}}), /missing/);
});
test('start checks: a bot key that holds an on-chain role (guardian, keeper, admin) is refused', async () => {
  const c = cfgOf(), rpc = fakeRpc(c);
  const views = {stage_config: {guardian: c.admin}, stage_guardian: c.admin, stage_investment_keeper: 'GBHWPV6QTQIAPJI7WHLHWZJB4GELG3B5NAR2NJ27FQNKSIGYGBYFZON4', get_admin: c.admin};
  const ok = await startChecks(c, {rpc, S, view: async (_, m) => views[m]});
  assert.equal(ok.network, MAINNET);
  await assert.rejects(startChecks(c, {rpc, S, view: async (_, m) => (m === 'stage_guardian' ? c.heartbeatAccount : views[m])}), /holds the on-chain role vaultGuardian/);
  await assert.rejects(startChecks(c, {rpc: {...rpc, getNetwork: async () => ({passphrase: TESTNET})}, S, view: async (_, m) => views[m]}), /not Mainnet/);
});

// ------------------------------------------------------------------ heartbeat signer
test('heartbeat signer: only receipt.checkpoint_nav() from the heartbeat account, no auth, fee cap and daily budget', async t => {
  const d = tmp(t), keyFile = path.join(d, 'hb.secret'), pub = createKeyFile(keyFile);
  const c = cfgOf({heartbeatAccount: pub, heartbeatKeyFile: keyFile, dataDir: d, fees: {heartbeat: {maxFeeStroops: 100_000, dailyBudgetStroops: 100_000}}});
  const budget = openBudget(d, c), sign = makeHeartbeatSigner(c, {S, budget, rpc: fakeRpc(c)});
  const build = ({source = pub, contract = c.contracts.receipt, fn = 'checkpoint_nav', fee = '60000', ops = 1} = {}) => {
    const b = new S.TransactionBuilder(new S.Account(source, '7'), {fee, networkPassphrase: MAINNET});
    for (let i = 0; i < ops; i++) b.addOperation(new S.Contract(contract).call(fn));
    return b.setTimeout(45).build().toXDR();
  };
  const signed = S.TransactionBuilder.fromXDR(await sign(build()), MAINNET);
  assert.equal(signed.signatures.length, 1); assert(S.Keypair.fromPublicKey(pub).verify(signed.hash(), signed.signatures[0].signature()));
  for (const [o, re] of [[{source: Gk(10)}, /source/], [{fn: 'checkpoint_fallback'}, /only call/], [{contract: c.contracts.vault}, /only call/], [{ops: 2}, /one invokeHostFunction/], [{fee: '100001'}, /above cap/]])
    await assert.rejects(sign(build(o)), re);
  await assert.rejects(sign(build()), /daily fee budget/);   // 60000 + 60000 > 100000
  assert.throws(() => makeHeartbeatSigner({...c, networkPassphrase: TESTNET}, {S, budget}), /non-Mainnet/);
});

// ------------------------------------------------------------------ key files
test('key files: 0600, owner, no symlink, matching public key; createKeyFile never overwrites and prints only the public key', t => {
  const d = tmp(t), f = path.join(d, 'k.secret'), pub = createKeyFile(f);
  assert.match(pub, /^G[A-Z2-7]{55}$/);
  assert.equal(fs.statSync(f).mode & 0o777, 0o600);
  assert.equal(loadKeyFile(f, {expectedPublicKey: pub}).publicKey(), pub);
  assert.throws(() => loadKeyFile(f, {expectedPublicKey: Gk(3)}), /does not match/);
  assert.throws(() => createKeyFile(f), e => e.code === 'EEXIST');
  fs.chmodSync(f, 0o640); assert.throws(() => loadKeyFile(f), /too open/); fs.chmodSync(f, 0o600);
  fs.symlinkSync(f, path.join(d, 'link')); assert.throws(() => loadKeyFile(path.join(d, 'link')), /symlink/);
  assert.throws(() => loadKeyFile('rel.secret'), /absolute/);
  const open = path.join(d, 'open'); fs.mkdirSync(open, {mode: 0o755}); fs.chmodSync(open, 0o755);
  assert.throws(() => createKeyFile(path.join(open, 'x.secret')), /chmod 700/);
  try { loadKeyFile(path.join(d, 'missing')); } catch (e) { assert(!/S[A-Z2-7]{55}/.test(e.message)); }
});

// ------------------------------------------------------------------ dry-run gate, single instance
test('dry-run gate: needs dryRunGateHours of error-free cycles, resets on a failure, a gap or a changed binding; arms once', t => {
  const d = tmp(t), c = cfgOf({dataDir: d, dryRunGateHours: 2}); let now = 1_000_000_000;
  const g = openGate(d, c, {binding: 'B1', clock: () => now});
  assert.throws(() => g.requireArmed(), /no dry-run soak/);
  for (let i = 0; i < 360; i++) { g.record(true); now += 10_000; }                   // 1 h
  assert.throws(() => g.requireArmed(), /min < 2 h/);
  g.record(false);                                                                   // an error restarts the soak
  for (let i = 0; i < 721; i++) { g.record(true); now += 10_000; }
  now -= 10_000;
  assert.equal(g.requireArmed().armedAt, Math.floor(now / 1000));
  assert.equal(g.requireArmed().armedAt, Math.floor(now / 1000));                     // live restart: stays armed
  assert.throws(() => openGate(d, c, {binding: 'B2', clock: () => now}).requireArmed(), /changed since the soak/);
  const d2 = tmp(t), g2 = openGate(d2, c, {binding: 'B1', clock: () => now});
  for (let i = 0; i < 400; i++) { g2.record(true); now += 10_000; } now += 200_000; g2.record(true);   // gap > 120 s
  assert.equal(g2.load().resets, 1);
  const d3 = tmp(t), g3 = openGate(d3, c, {binding: 'B1', clock: () => now});
  for (let i = 0; i < 721; i++) { g3.record(true); now += 10_000; } now += 700_000;
  assert.throws(() => g3.requireArmed(), /more than 10 min ago/);
});
test('gate binding: covers config and sources, not borrowers / team3', () => {
  const a = gateBinding(cfgOf()), b = gateBinding(cfgOf({borrowers: [], team3: null}));
  assert.equal(a, b); assert.notEqual(a, gateBinding(cfgOf({repayCapRaw: '999'})));
});
test('run lock: a live holder refuses a second process; a dead holder is taken over', t => {
  const d = tmp(t);
  const rel = acquireRunLock(d, {pid: 111, alive: () => true});
  assert.throws(() => acquireRunLock(d, {pid: 222, alive: () => true}), /another cl-liquidator/);
  const rel2 = acquireRunLock(d, {pid: 333, alive: () => false}); rel2();
  rel();
  assert.equal(fs.existsSync(path.join(d, 'run.lock')), false);
});

// ------------------------------------------------------------------ trustlines
const tlEntry = n => {
  const A = {USDC: ['USDC', 'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN'], PYUSD: ['PYUSD', 'GDQE7IXJ4HUHV6RQHIUPRJSEZE4DRS5WY577O2FY6YQ5LVWZ7JZTU2V5']}[n];
  return {val: {trustLine: () => ({asset: () => new S.Asset(...A).toTrustLineXDRObject()})}};
};
test('trustlines: classic assets match the configured SACs; only missing lines are added; both are required', async () => {
  const c = cfgOf(), assets = trustlineAssets(c, S);
  assert.deepEqual(assets.map(a => a.name), ['USDC', 'PYUSD']);
  assert.throws(() => trustlineAssets(cfgOf({contracts: {...raw().contracts, pyusd: C(8)}}), S), /not the configured SAC/);
  const acc = new S.Account(c.liquidator, '5');
  const tx = buildTrustlineTx(acc, assets, MAINNET, new Set(['USDC:GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN']), S);
  assert.equal(tx.operations.length, 1); assert.equal(tx.operations[0].line.code, 'PYUSD'); assert.equal(tx.fee, '10000');
  assert.equal(buildTrustlineTx(acc, assets, MAINNET, new Set(assets.map(a => `${a.asset.code}:${a.asset.issuer}`)), S), null);
  const rpc = have => ({getLedgerEntries: async (...keys) => { assert.equal(keys.length, 2); return {latestLedger: 1, entries: have.map(tlEntry)}; }});
  assert.deepEqual(await trustlineStatus(c, rpc(['PYUSD']), c.liquidator, S), {USDC: false, PYUSD: true});
  await assert.rejects(requireTrustlines(c, rpc(['USDC']), c.liquidator, S), /missing: PYUSD/);
  await assert.rejects(requireTrustlines(c, rpc([]), c.liquidator, S), /missing: USDC, PYUSD/);
  assert.deepEqual(await requireTrustlines(c, rpc(['USDC', 'PYUSD']), c.liquidator, S), {USDC: true, PYUSD: true});
});

// ------------------------------------------------------------------ seized-unit exit tool
test('exit tool: only the liquidator\'s own in-kind close / emergency commands, exact calls, bounded deadline', () => {
  const c = cfgOf(), now = 1_800_000_000, me = c.liquidator;
  const pc = exitCall(c, 'prepare-close', {units: 500n, now});
  assert.deepEqual([pc.fn, pc.args.owner, pc.args.shares, pc.args.min_swap, pc.args.deadline], ['stage_prepare_close', me, 500n, 0n, BigInt(now + 3000)]);
  assert.equal(exitCall(c, 'prepare-close', {units: 500n, now, opts: {units: '200'}}).args.shares, 200n);
  assert.throws(() => exitCall(c, 'prepare-close', {units: 500n, now, opts: {units: '501'}}), /outside/);
  assert.throws(() => exitCall(c, 'prepare-close', {units: 0n, now}), /outside/);
  const job = (o = {}) => ({owner: me, kind: 1, phase: PHASE.CLOSE_LOCKED, min_swap: 0n, ...o});
  assert.equal(exitCall(c, 'withdraw', {units: 0n, now, job: job(), opts: {id: '4'}}).fn, 'stage_withdraw');
  for (const j of [job({owner: Gk(9)}), job({kind: 2}), job({phase: 12}), job({min_swap: 5n}), null])
    assert.throws(() => exitCall(c, 'withdraw', {units: 0n, now, job: j, opts: {id: '4'}}), /refused/);
  assert.throws(() => exitCall(c, 'emergency-exit', {units: 10n, now, opts: {units: '10'}}), /accept-reward-waiver/);
  assert.equal(exitCall(c, 'emergency-exit', {units: 10n, now, opts: {units: '10', acceptRewardWaiver: true}}).args.accept_reward_waiver, true);
  assert.equal(exitCall(c, 'emergency-pay', {units: 0n, now, job: job({kind: 2, phase: PHASE.EMERGENCY_HELD}), opts: {id: '7'}}).fn, 'stage_emergency_pay');
  assert.throws(() => exitCall(c, 'emergency-pay', {units: 0n, now, job: job({kind: 2, phase: 22}), opts: {id: '7'}}), /held/);
  const allow = exitAllow({...pc, args: {...pc.args, deadline: BigInt(Math.floor(Date.now() / 1000) + 100)}});
  assert.doesNotThrow(() => allow(c, pc.contract, pc.fn, {...pc.args, deadline: BigInt(Math.floor(Date.now() / 1000) + 100)}));
  assert.throws(() => allow(c, pc.contract, pc.fn, {...pc.args, shares: 1n}), /differs/);
  assert.throws(() => exitAllow(pc)(c, c.contracts.usdc, 'approve', {}), /differs/);
});

test('resource-fee ceiling (Testnet 70.97 XLM rent on a first checkpoint_nav): refused before signing for the bot and the heartbeat', async t => {
  // 70.97 XLM of rent on L1a: far above the 0.5 XLM checkpoint_nav ceiling; nothing signed, sent or charged, flagged `refused`.
  const f = senderFixture(t, {resourceFee: 709_700_000});
  const r = await f.nav();
  assert.deepEqual([r.ok, r.unknown, r.refused], [false, false, true]); assert.match(r.stderr, /resource fee \d+ above the checkpoint_nav ceiling/);
  assert.equal(f.rpc.sent.length, 0); assert.equal(f.notes.length, 0); assert.equal(f.budget.spent('liquidator'), 0n);
  // L3 just under its 2 XLM ceiling passes (padded fee 1.2 x + 0.001 XLM); just above refuses.
  const ok = senderFixture(t, {resourceFee: 16_000_000});
  const l3 = s => s.send(s.c.contracts.controller, 'liquidate', {borrower: s.c.borrowers[0], repay_market: s.c.contracts.usdcMarket, collateral_market: s.c.contracts.receipt, repay_amount: 100n, liquidator: s.pub});
  assert.equal((await l3(ok)).ok, true);
  const hi = senderFixture(t, {resourceFee: 17_000_000});
  assert.match((await l3(hi)).stderr, /liquidate ceiling 20000000/);
  // Heartbeat: an assembled checkpoint_nav whose resource fee exceeds 0.5 XLM is refused at signing.
  const d = tmp(t), keyFile = path.join(d, 'hb.secret'), pub = createKeyFile(keyFile);
  const c = cfgOf({heartbeatAccount: pub, heartbeatKeyFile: keyFile, dataDir: d});
  const sign = makeHeartbeatSigner(c, {S, budget: openBudget(d, c), rpc: fakeRpc(c)});
  const env = res => new S.TransactionBuilder(new S.Account(pub, '7'), {fee: '10000', networkPassphrase: MAINNET, sorobanData: new S.SorobanDataBuilder().setResourceFee(String(res)).build()})
    .addOperation(new S.Contract(c.contracts.receipt).call('checkpoint_nav')).setTimeout(45).build().toXDR();
  await assert.rejects(sign(env(709_700_000)), /resource fee 709700000 above the ceiling/);
  assert.equal(openBudget(d, c).spent('heartbeat'), 0n);
  assert.ok(await sign(env(60_000)));
});

// ------------------------------------------------------------------ heartbeat RPC failover (5f48b0d) on Mainnet
test('mainnet config: heartbeatRpcUrls needs >= 2 distinct https URLs without credentials', () => {
  for (const urls of [undefined, [], ['https://a.invalid'], ['https://a.invalid', 'https://a.invalid'], ['https://a.invalid', 'http://b.invalid'], ['https://a.invalid', 'https://u:p@b.invalid']])
    assert.throws(() => cfgOf({heartbeatRpcUrls: urls}), /heartbeatRpcUrls/, JSON.stringify(urls));
  assert.deepEqual(cfgOf({heartbeatRpcUrls: ['https://a.invalid', 'https://b.invalid', 'https://c.invalid']}).heartbeatRpcUrls.length, 3);
});
function servers(c, spec) {
  // spec[url] = {net: passphrase | 'throw', dead: bool, code: role => hash}
  const calls = [];
  const make = url => { const o = spec[url], base = fakeRpc(c, {code: o.code});
    return new Proxy({}, {get: (_, m) => async (...a) => { calls.push([new URL(url).host, m]);
      if (m === 'getNetwork') { if (o.net === 'throw') throw Error('fetch failed'); return {passphrase: typeof o.net === 'function' ? o.net() : o.net}; }
      if (o.dead) throw Error('fetch failed'); return base[m](...a); }}); };
  return {make, calls};
}
test('heartbeat RPC: start refused unless EVERY server reports Mainnet', async () => {
  const c = cfgOf();
  for (const bad of [{net: TESTNET}, {net: 'throw'}]) {
    const {make} = servers(c, {'https://rpc-a.invalid': {net: MAINNET}, 'https://rpc-b.invalid': bad});
    await assert.rejects(makeHeartbeatRpc(c, {S, make, failover: r => failoverRpc(r)}), /rpc-b.invalid is not a reachable Mainnet RPC/);
  }
  await assert.rejects(makeHeartbeatRpc({...c, networkPassphrase: TESTNET}, {S, make: () => ({}), failover: r => r}), /non-Mainnet/);
});
test('heartbeat RPC: a dead primary fails over; a server that later reports another network is skipped', async () => {
  const c = cfgOf(); let now = 0, bNet = MAINNET, aDead = false;
  const spec = {'https://rpc-a.invalid': {net: MAINNET, get dead() { return aDead; }}, 'https://rpc-b.invalid': {net: () => bNet}};
  const {make, calls} = servers(c, spec), events = [];
  const rpc = await makeHeartbeatRpc(c, {S, make, clock: () => now, recheckMs: 1000, failover: r => failoverRpc(r, e => events.push(e))});
  assert.equal((await rpc.getAccount(c.heartbeatAccount)).accountId(), c.heartbeatAccount);
  assert.equal(calls.at(-1)[0], 'rpc-a.invalid');
  aDead = true;                                                       // primary dies (transport failure)
  await rpc.getAccount(c.heartbeatAccount);
  assert.equal(calls.at(-1)[0], 'rpc-b.invalid'); assert.equal(events[0].msg, 'heartbeat RPC failover');
  now += 2000; bNet = TESTNET;                                        // secondary now answers another network: refused
  await assert.rejects(rpc.getAccount(c.heartbeatAccount));
  assert(calls.some(([h, m]) => h === 'rpc-b.invalid' && m === 'getNetwork'));
});
test('heartbeat signer re-checks the code pins through whichever heartbeat RPC is in use', async t => {
  const d = tmp(t), keyFile = path.join(d, 'hb.secret'), pub = createKeyFile(keyFile);
  const c = cfgOf({heartbeatAccount: pub, heartbeatKeyFile: keyFile, dataDir: d});
  const xdr = () => new S.TransactionBuilder(new S.Account(pub, '7'), {fee: '60000', networkPassphrase: MAINNET}).addOperation(new S.Contract(c.contracts.receipt).call('checkpoint_nav')).setTimeout(45).build().toXDR();
  // Primary dead, secondary serves a CHANGED receipt code: refused, nothing charged.
  let spec = {'https://rpc-a.invalid': {net: MAINNET, dead: true}, 'https://rpc-b.invalid': {net: MAINNET, code: r => (r === 'receipt' ? 'f'.repeat(64) : PINS[r])}};
  let rpc = await makeHeartbeatRpc(c, {S, make: servers(c, spec).make, failover: r => failoverRpc(r)});
  const budget = openBudget(d, c);
  await assert.rejects(makeHeartbeatSigner(c, {S, budget, rpc})(xdr()), /receipt code ffffffff differs/);
  assert.equal(budget.spent('heartbeat'), 0n);
  // Primary dead, secondary with the pinned code: signs (pins read through the secondary).
  spec = {'https://rpc-a.invalid': {net: MAINNET, dead: true}, 'https://rpc-b.invalid': {net: MAINNET}};
  const sv = servers(c, spec);
  rpc = await makeHeartbeatRpc(c, {S, make: sv.make, failover: r => failoverRpc(r)});
  assert.ok(await makeHeartbeatSigner(c, {S, budget, rpc})(xdr()));
  assert(sv.calls.some(([h, m]) => h === 'rpc-b.invalid' && m === 'getLedgerEntries'));
  await assert.rejects(makeHeartbeatSigner(c, {S, budget})(xdr()), /needs its RPC/);
});
