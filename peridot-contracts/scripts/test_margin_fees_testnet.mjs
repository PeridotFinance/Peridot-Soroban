#!/usr/bin/env node
// Isolated fee-release validation. Never imports a production signing key.
// This recorded fixture requires its three peridot-fees-20260916 CLI aliases,
// builds under target/margin-fees-testnet, and margin-liquidator npm dependencies.
// Modes: deploy, probe, matrix, extras, ownership, audit. Mutations additionally require
// CONFIRM_TESTNET=ISOLATED_MARGIN_FEES; audit is read-only. Preserve state.json
// when resuming; never run two processes against the same fixture concurrently.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync, writeFileSync, appendFileSync, existsSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { MarginLiquidationKeeper } from '../bots/margin-liquidator/src/keeper.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const require = createRequire(resolve(root, 'bots/margin-liquidator/package.json'));
const S = require('@stellar/stellar-sdk');
const mode = process.argv[2] ?? 'inspect';
// The remediation fixture uses new contracts/state but the same dedicated test
// signers. Never overwrite the evidence or bindings of the original deployment.
const entitlements = process.env.FEE_ENTITLEMENTS === 'true';
const aliases = 'peridot-fees-20260916';
const run = entitlements ? 'peridot-fee-entitlements-20260916' : aliases;
const candidate = entitlements ? process.env.CANDIDATE_COMMIT : 'e1104c701f557601929fe0f1c75939b68a8e8ac2';
assert.match(candidate ?? '', /^[a-f0-9]{40}$/, 'CANDIDATE_COMMIT must identify the tested build');
const dir = resolve(root, entitlements ? 'target/margin-fee-entitlements-testnet' : 'target/margin-fees-testnet');
mkdirSync(dir, { recursive: true });
const stateFile = resolve(dir, 'state.json');
const state = existsSync(stateFile) ? JSON.parse(readFileSync(stateFile)) : {
  run, sourceCommit: candidate, ids: {}, done: {}, results: [],
};
assert.equal(state.run, run);
assert.equal(state.sourceCommit, candidate);
const rpcUrl = 'https://soroban-testnet.stellar.org';
const server = new S.rpc.Server(rpcUrl);
const stringify = v => JSON.stringify(v, (_, x) => typeof x === 'bigint' ? x.toString() : x);
const save = () => writeFileSync(stateFile, stringify(state) + '\n');
const log = entry => {
  const line = stringify({ time: new Date().toISOString(), ...entry });
  console.log(line);
  appendFileSync(resolve(dir, 'transactions.jsonl'), line + '\n');
};
const sleep = ms => new Promise(r => setTimeout(r, ms));
const actors = Object.fromEntries(['admin', 'alice', 'bob'].map(role => [role, {
  alias: `${aliases}-${role}`,
  address: execFileSync('stellar', ['keys', 'address', `${aliases}-${role}`], {
    encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'],
  }).trim(),
}]));
const publicSource = actors.admin.address;
const specs = new Map();
let poolBalanceDependencies;
const U = 10_000_000n;
const E = 1_000_000n;
const P = 100_000_000_000_000n;
const sharedPool = 'CCMNSENXDBNJSY72BDIPH5CCXLLHBKZ4LXTRKDLKZN4UI2NJFQLWTLD6';
const shared = new Set([sharedPool,
  'CAKKHUGHP67UA4F42QOYPKNGRSBJEOE62MGDXA2UURTEYFOQGSMIRUFO',
  'CDMXPWG55776NECXQMWNBXMEQXZUAWA2AJBCQS7SU7SA64XHMO3KB3O6',
  'CCEW6NSPCV7XUEQV75ZMII5HK5DGXK5JP2QOTGLV4UFLDBPEKRGO4Y4B',
  'CB32OVY4AADCHQT3DLKJYW5QVTWY5MOX7BBNZFT3SDHZ5HPSDDEA2THJ',
]);
let networkLimits;

async function loadNetworkLimits() {
  const keys = [1, 2, 15].map(id => S.xdr.LedgerKey.configSetting(
    new S.xdr.LedgerKeyConfigSetting({ configSettingId: S.xdr.ConfigSettingId.fromValue(id) })));
  const response = await server.getLedgerEntries(...keys);
  const settings = new Map(response.entries.map(e => {
    const setting = e.val.configSetting();
    return [setting.switch().value, setting.value()];
  }));
  const limits = {
    instructions: Number(settings.get(1).txMaxInstructions().toString()),
    readEntries: settings.get(15).txMaxFootprintEntries(),
    writeEntries: settings.get(2).txMaxWriteLedgerEntries(),
    diskReadBytes: settings.get(2).txMaxDiskReadBytes(),
    writeBytes: settings.get(2).txMaxWriteBytes(),
  };
  log({ networkLimits: limits, ledger: response.latestLedger });
  return limits;
}

async function retryRead(fn) {
  for (let i = 0; ; i++) {
    try { return await fn(); } catch (error) {
      if (i === 3) throw error;
      await sleep(500 * 2 ** i);
    }
  }
}
async function spec(id) {
  if (!specs.has(id)) specs.set(id, S.contract.Spec.fromWasm(await retryRead(() => server.getContractWasmByContractId(id))));
  return specs.get(id);
}
async function simulate(op, role = 'admin') {
  const account = await retryRead(() => server.getAccount(actors[role].address));
  const tx = new S.TransactionBuilder(account, { fee: '10000', networkPassphrase: S.Networks.TESTNET })
    .addOperation(op).setTimeout(180).build();
  const sim = await retryRead(() => server.simulateTransaction(tx));
  return { tx, sim };
}
async function poolBalanceKeys() {
  if (poolBalanceDependencies) return poolBalanceDependencies;
  const keys = new Set();
  for (const token of state.poolTokens) {
    const { sim } = await simulate(new S.Contract(token).call('balance', new S.Address(state.ids.pool).toScVal()));
    assert.ok(!S.rpc.Api.isSimulationError(sim), 'pool balance preflight failed');
    const footprint = sim.transactionData.build().resources().footprint();
    for (const key of [...footprint.readOnly(), ...footprint.readWrite()]) keys.add(key.toXDR('base64'));
  }
  poolBalanceDependencies = keys;
  return keys;
}
async function simulateOpenWithCompleteFootprint(op, role, method) {
  for (let attempt = 0; attempt < 4; attempt++) {
    const result = await simulate(op, role);
    if (method !== 'begin_open_position_v3' || S.rpc.Api.isSimulationError(result.sim)) return result;
    const footprint = result.sim.transactionData.build().resources().footprint();
    const recorded = new Set([...footprint.readOnly(), ...footprint.readWrite()].map(key => key.toXDR('base64')));
    const missing = [...await poolBalanceKeys()].filter(key => !recorded.has(key));
    if (missing.length === 0) return result;
    log({ incompletePoolFootprint: method, missingKeys: missing.length, attempt, simulationLedger: result.sim.latestLedger });
    // A pool quote can omit its balance dependencies on one ledger, then read
    // them during execution. Do not sign that envelope or retry a paid failure.
    let advanced = false;
    for (let n = 0; n < 30; n++) {
      await sleep(1000);
      if ((await server.getLatestLedger()).sequence > result.sim.latestLedger) { advanced = true; break; }
    }
    assert.ok(advanced, 'Testnet ledger did not advance');
  }
  throw new Error('pool balance footprint remains incomplete; no transaction signed');
}
async function call(id, method, args = {}, { role = 'admin', label, negative = false } = {}) {
  const abi = await spec(id);
  const op = new S.Contract(id).call(method, ...abi.funcArgsToScVals(method, args));
  if (label && state.done[label]) return state.done[label].value;
  const { tx, sim } = await simulateOpenWithCompleteFootprint(op, role, method);
  if (negative) {
    assert.ok(S.rpc.Api.isSimulationError(sim), `${method} unexpectedly succeeded`);
    log({ check: label ?? method, expectedFailure: sim.error });
    return;
  }
  if (S.rpc.Api.isSimulationError(sim)) {
    log({ failed: label ?? method, contract: id, error: sim.error, events: sim.events?.map(e => e.toXDR('base64')) });
    throw new Error(`${label ?? method}: ${sim.error}`);
  }
  const value = sim.result ? S.scValToNative(sim.result.retval) : null;
  if (!label) return value;
  assert.ok(Object.values(state.ids).includes(id) && !shared.has(id), 'mutation outside isolated deployment');
  return submit(tx, sim, role, label, value);
}
async function submit(tx, sim, role, label, value) {
  assert.equal((await server.getNetwork()).passphrase, S.Networks.TESTNET);
  for (const auth of sim.result?.auth ?? []) {
    assert.equal(auth.credentials().switch().name, 'sorobanCredentialsSourceAccount', 'unexpected extra signer');
  }
  const prepared = S.rpc.assembleTransaction(tx, sim).build();
  const envelope = prepared.toEnvelope();
  const body = envelope.v1().tx();
  const data = body.ext().sorobanData();
  const oldFee = BigInt(data.resourceFee().toString());
  const resourceFee = oldFee * 2n + 1_000_000n;
  assert.ok(resourceFee < 3_000n * U, 'unexpected testnet transaction cost');
  data.resourceFee(S.xdr.Int64.fromString(resourceFee.toString()));
  body.fee(body.fee() + Number(resourceFee - oldFee));
  const resources = data.resources();
  const footprint = resources.footprint();
  const resourceSummary = {
    instructions: resources.instructions(),
    readEntries: footprint.readOnly().length + footprint.readWrite().length,
    writeEntries: footprint.readWrite().length,
    diskReadBytes: resources.diskReadBytes(),
    writeBytes: resources.writeBytes(),
  };
  log({ simulated: label, resources: resourceSummary, minResourceFee: sim.minResourceFee });
  if (mode === 'probe') return value;
  assert.equal(process.env.CONFIRM_TESTNET, 'ISOLATED_MARGIN_FEES', 'explicit isolated testnet confirmation required');
  // Real Aquarius and lending dependencies exceed the mock harness footprint.
  // Keep headroom below current network limits, not the old 100-entry assumption.
  const ceilings = { instructions: 100_000_000, readEntries: 200, writeEntries: 80 };
  for (const [name, actual] of Object.entries(resourceSummary)) {
    const ceiling = Math.min(networkLimits[name], ceilings[name] ?? networkLimits[name]);
    assert.ok(actual <= ceiling, `${label}: ${name}=${actual} exceeds ${ceiling}`);
  }
  const padded = new S.Transaction(envelope, S.Networks.TESTNET);
  const signed = execFileSync('stellar', ['tx', 'sign', '--network', 'testnet',
    '--sign-with-key', actors[role].alias, padded.toXDR()], {
    encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], timeout: 30000,
  }).trim();
  const finalTx = S.TransactionBuilder.fromXDR(signed, S.Networks.TESTNET);
  assert.equal(finalTx.hash().toString('hex'), padded.hash().toString('hex'));
  const hash = finalTx.hash().toString('hex');
  log({ submitting: label, hash, resources: resourceSummary, minResourceFee: sim.minResourceFee });
  const sent = await server.sendTransaction(finalTx);
  if (!['PENDING', 'DUPLICATE'].includes(sent.status)) throw new Error(`${label} rejected: ${sent.status}`);
  for (let i = 0; i < 90; i++) {
    const result = await retryRead(() => server.getTransaction(hash));
    if (result.status === 'SUCCESS') {
      const actual = result.returnValue ? S.scValToNative(result.returnValue) : value;
      state.done[label] = { hash, ledger: result.ledger, value: actual, resources: resourceSummary };
      save();
      log({ confirmed: label, ...state.done[label] });
      return actual;
    }
    if (result.status !== 'NOT_FOUND') throw new Error(`${label} failed on-chain: ${result.status} ${result.resultXdr?.toXDR('base64')}`);
    await sleep(1000);
  }
  throw new Error(`Uncertain submission ${hash}; inspect before retrying ${label}`);
}
async function deploy(name, wasmHash) {
  if (state.ids[name]) return state.ids[name];
  const salt = S.hash(Buffer.from(`${run}:${name}`));
  const preimage = S.xdr.ContractIdPreimage.contractIdPreimageFromAddress(
    new S.xdr.ContractIdPreimageFromAddress({ address: new S.Address(publicSource).toScAddress(), salt }));
  const operation = S.Operation.invokeHostFunction({ func: S.xdr.HostFunction.hostFunctionTypeCreateContract(
    new S.xdr.CreateContractArgs({ contractIdPreimage: preimage,
      executable: S.xdr.ContractExecutable.contractExecutableWasm(Buffer.from(wasmHash, 'hex')) })), auth: [] });
  const { tx, sim } = await simulate(operation);
  if (S.rpc.Api.isSimulationError(sim)) throw new Error(`${name} deploy: ${sim.error}`);
  const id = await submit(tx, sim, 'admin', `deploy:${name}`, S.scValToNative(sim.result.retval));
  assert.ok(!shared.has(id));
  state.ids[name] = id;
  save();
  return id;
}
async function upload(name) {
  const label = `upload:${name}`;
  const wasm = readFileSync(resolve(dir, 'wasm32v1-none/release', `${name}.wasm`));
  const hash = S.hash(wasm).toString('hex');
  if (!state.done[label]) {
    const { tx, sim } = await simulate(S.Operation.uploadContractWasm({ wasm }));
    if (S.rpc.Api.isSimulationError(sim)) throw new Error(`${label}: ${sim.error}`);
    await submit(tx, sim, 'admin', label, hash);
  }
  state.hashes ??= {};
  if (state.hashes[name]) assert.equal(state.hashes[name], hash, 'artifact changed during run');
  state.hashes[name] = hash;
  save();
  return hash;
}
const write = (id, method, args, label, role = 'admin') => call(id, method, args, { label, role });
async function instance(id) {
  const key = S.xdr.LedgerKey.contractData(new S.xdr.LedgerKeyContractData({
    contract: new S.Address(id).toScAddress(), key: S.xdr.ScVal.scvLedgerKeyContractInstance(),
    durability: S.xdr.ContractDataDurability.persistent(),
  }));
  const data = (await server.getLedgerEntries(key)).entries[0].val.contractData().val().instance();
  return { hash: data.executable().wasmHash().toString('hex'),
    fields: Object.fromEntries((data.storage() ?? []).map(e => [S.scValToNative(e.key())[0], S.scValToNative(e.val())])) };
}
async function setup() {
  const tokenHash = await upload('mock_token');
  const usd = await deploy('usd', tokenHash);
  const base = await deploy('base', tokenHash);
  for (const [id, symbol] of [[usd, 'FUSD'], [base, 'FBASE']]) {
    await write(id, 'initialize', { name: `Isolated margin fee ${symbol}`, symbol, decimals: 7 }, `init:${symbol}`);
    for (const [role, actor] of Object.entries(actors)) await write(id, 'mint', { to: actor.address, amount: 10_000_000n * U }, `mint:${symbol}:${role}`);
  }
  const admin = publicSource;
  const ctrl = await deploy('controller', await upload('simple_peridottroller'));
  await write(ctrl, 'initialize', { admin }, 'init:controller');
  const vaultHash = await upload('receipt_vault');
  const uv = await deploy('usdVault', vaultHash);
  const bv = await deploy('baseVault', vaultHash);
  const model = await deploy('model', await upload('jump_rate_model'));
  await write(model, 'initialize', { base: 50000n, multiplier: 200000n, jump: 4000000n, kink: 800000n, admin }, 'init:model');
  for (const [vault, asset, name] of [[uv, usd, 'usd'], [bv, base, 'base']]) {
    await write(vault, 'initialize', { token_address: asset, supply_yearly_rate_scaled: 0n, borrow_yearly_rate_scaled: 0n, admin }, `init:${name}Vault`);
    await write(ctrl, 'add_market', { market: vault }, `market:${name}`);
    await write(vault, 'set_peridottroller', { peridottroller: ctrl }, `controller:${name}`);
    await write(ctrl, 'set_market_cf', { market: vault, cf_scaled: 900000n }, `cf:${name}`);
    await write(vault, 'set_interest_model', { model }, `rate:${name}`);
    await write(ctrl, 'set_price_fallback', { token: asset, price: [P, P] }, `price:${name}`);
    await write(vault, 'deposit', { user: admin, amount: 1_000_000n * U }, `liquidity:${name}`);
  }
  const template = await instance(sharedPool);
  const share = await instance(await call(sharedPool, 'share_id'));
  const pool = await deploy('pool', template.hash);
  state.poolTokens = [base, usd].sort((a, b) => Buffer.compare(new S.Address(a).toScAddress().toXDR(), new S.Address(b).toScAddress().toXDR()));
  state.poolId = S.hash(Buffer.from(`${run}:pool-binding`)).toString('hex');
  save();
  await write(pool, 'initialize_all', { admin,
    privileged_addrs: [admin, admin, admin, admin, [], admin], router: admin,
    lp_token_wasm_hash: Buffer.from(share.hash, 'hex'), tokens: state.poolTokens,
    fees_config: [30, 5000], reward_config: [template.fields.RewardToken, template.fields.RewardBoostToken, template.fields.RewardBoostFeed],
    plane: template.fields.Plane, config_storage: template.fields.ConfigStorage,
  }, 'init:pool');
  await write(pool, 'deposit', { user: admin, desired_amounts: [1_000_000n * U, 1_000_000n * U], min_shares: 1n }, 'liquidity:pool');
  const adapter = await deploy('adapter', await upload('swap_adapter'));
  await write(adapter, 'initialize', { admin, router: template.fields.Router }, 'init:adapter');
  const margin = await deploy('margin', await upload('margin_controller'));
  for (const [name, id] of [['margin', margin], ['pool', pool]]) await write(adapter, 'set_pool_allowed', { admin, pool: id, allowed: true }, `allow:${name}`);
  await write(adapter, 'set_pool_binding', { admin, pool_id: Buffer.from(state.poolId, 'hex'), pool, allowed: true }, 'bind:pool');
  await write(margin, 'initialize', { admin, peridottroller: ctrl, swap_adapter: adapter, max_leverage: 5n }, 'init:margin');
  for (const [vault, asset, name] of [[uv, usd, 'usd'], [bv, base, 'base']]) {
    await write(margin, 'set_market', { admin, asset, vault }, `margin-market:${name}`);
    await write(vault, 'set_margin_controller', { admin, margin_controller: margin }, `margin-bind:${name}`);
  }
  await write(ctrl, 'set_margin_liquidation_ctrl', { controller: margin, allowed: true }, 'allow:liquidation');
  for (const side of ['Long', 'Short']) {
    const pair = { admin, margin_asset: usd, base_asset: base, side: { tag: side } };
    await write(margin, 'set_perps_pair_config', { ...pair, max_leverage: 5n, maintenance_margin_scaled: 100000n, liquidation_incentive_scaled: 10000n }, `pair:${side}`);
    await write(margin, 'set_perps_pair_execution_config', { ...pair, config: { max_open_deviation_scaled: 50000n, open_slippage_scaled: 20000n, close_slippage_scaled: 20000n, liquidation_slippage_scaled: 50000n } }, `execution:${side}`);
    await write(margin, 'set_perps_pair_exit_config', { ...pair, config: { max_close_deviation_scaled: 150000n, max_liq_deviation_scaled: 250000n } }, `exit:${side}`);
  }
  await write(margin, 'set_open_fee_bps', { admin, fee_bps: 10n }, 'fee:open');
  await write(margin, 'set_close_fee_bps', { admin, fee_bps: 20n }, 'fee:close');
  for (const role of ['alice', 'bob']) {
    const user = actors[role].address;
    await write(uv, 'deposit', { user, amount: 20000n * U }, `fund:${role}`, role);
    await write(margin, 'transfer_spot_to_margin', { user, asset: usd, ptoken_amount: 19000n * U }, `margin:${role}`, role);
  }
  log({ setupComplete: true, ids: state.ids, actors, hashes: state.hashes });
}
async function quote(assetIn, amount) {
  const in_idx = state.poolTokens.indexOf(assetIn);
  assert.notEqual(in_idx, -1);
  return BigInt(await call(state.ids.pool, 'estimate_swap', { in_idx, out_idx: 1 - in_idx, in_amount: amount }));
}
async function begin(label, role, side, margin, leverage) {
  const { usd, base, usdVault, margin: mc, pool } = state.ids;
  const user = actors[role].address;
  const rate = BigInt(await call(usdVault, 'get_exchange_rate'));
  const trade = margin * rate / E * BigInt(leverage);
  const input = side === 'Long' ? usd : base;
  const min = (await quote(input, trade)) * 9800n / 10000n;
  const position_id = BigInt(await write(mc, 'begin_open_position_v3', { user, margin_asset: usd, base_asset: base,
    margin_ptokens: margin, leverage: BigInt(leverage), side: { tag: side }, pool_tokens: state.poolTokens,
    pool_id: Buffer.from(state.poolId, 'hex'), pool, amount_with_slippage: min }, `${label}:begin`, role));
  return position_id;
}
async function open(label, role, side, margin, leverage) {
  if (state.done[`${label}:finish`]) {
    const id = BigInt(state.done[`${label}:begin`].value);
    assert.equal(await call(state.ids.margin, 'get_position', { position_id: id }), null);
    return id;
  }
  const before = await call(state.ids.margin, 'get_undistributed_margin_fees', { vault: state.ids.usdVault });
  const position_id = await begin(label, role, side, margin, leverage);
  const mc = state.ids.margin;
  const user = actors[role].address;
  await write(mc, 'swap_open_position_v3', { user, position_id }, `${label}:swap`, role);
  await write(mc, 'activate_open_position_v3', { user, position_id }, `${label}:activate`, role);
  const after = await call(mc, 'get_undistributed_margin_fees', { vault: state.ids.usdVault });
  if (!state.done[`${label}:open-checked`]) {
    assert.equal(BigInt(after.ptokens) - BigInt(before.ptokens), margin * BigInt(leverage) * 10n / 10000n);
    state.done[`${label}:open-checked`] = { value: true }; save();
  }
  assert.ok(BigInt(await call(mc, 'get_health_factor', { position_id })) > E);
  return position_id;
}
async function close(label, role, side, position_id, { deferFinish = false } = {}) {
  const { margin: mc, baseVault, usdVault, base, usd } = state.ids;
  const user = actors[role].address;
  if (state.done[`${label}:finish`]) {
    assert.equal(await call(mc, 'get_position', { position_id }), null);
    assert.equal(BigInt(await call(side === 'Long' ? usdVault : baseVault, 'get_margin_borrow_balance', { position_id })), 0n);
    return;
  }
  state.closeRounds ??= {};
  let round = state.closeRounds[label] ?? 0;
  const previous = await call(mc, 'get_pending_perps_close', { position_id });
  if (previous && BigInt(previous.debt_amount) === 0n && BigInt(previous.received_debt_asset) === 0n
      && BigInt(previous.expires_at) < BigInt(Math.floor(Date.now() / 1000) - 10)) {
    await write(mc, 'expire_close_position_v3', { position_id }, `${label}:expire:${round}`, role === 'alice' ? 'bob' : 'alice');
    assert.equal((await call(mc, 'get_position', { position_id })).status[0], 'Open');
    state.closeRounds[label] = ++round;
    save();
    log({ expiredPreSwapRecovered: position_id, retryRound: round });
  }
  const attempt = round ? `${label}:retry:${round}` : label;
  await write(mc, 'prepare_close_position_v3', { user, position_id }, `${attempt}:prepare`, role);
  const pending = await call(mc, 'get_pending_perps_close', { position_id });
  const amount = BigInt(pending.collateral_underlying);
  if (side === 'Long') {
    await write(mc, 'swap_close_position_v3', { user, position_id, amount_with_slippage: (await quote(base, amount)) * 9800n / 10000n }, `${attempt}:swap-close`, role);
  } else {
    const debt = BigInt(await call(baseVault, 'get_margin_borrow_balance', { position_id }));
    // The contract requires 10 bps ABOVE post-accrual debt. Quoting exactly
    // 10 bps on the stored index races interest during search/signing.
    const min = debt + (debt * 20n + 9999n) / 10000n + 100n;
    let lo = 1n, hi = amount;
    assert.ok(await quote(usd, hi) >= min, 'short cannot repay from position');
    while (lo < hi) { const mid = (lo + hi) / 2n; if (await quote(usd, mid) >= min) hi = mid; else lo = mid + 1n; }
    await write(mc, 'swap_close_short_position_v3', { user, position_id, swap_amount_in: lo, min_debt_out: min }, `${attempt}:swap-close`, role);
  }
  if (deferFinish) return;
  const terms = await call(mc, 'get_position_fee_terms', { position_id });
  const feesBefore = await call(mc, 'get_undistributed_margin_fees', { vault: usdVault });
  await write(mc, 'finish_close_position_v3', { position_id }, `${label}:finish`, role === 'alice' ? 'bob' : 'alice');
  const feesAfter = await call(mc, 'get_undistributed_margin_fees', { vault: usdVault });
  assert.equal(BigInt(feesAfter.underlying) - BigInt(feesBefore.underlying), BigInt(terms.close_fee_underlying));
  assert.equal(await call(mc, 'get_position', { position_id }), null);
  assert.equal(BigInt(await call(side === 'Long' ? usdVault : baseVault, 'get_margin_borrow_balance', { position_id })), 0n);
}
async function distribution(label) {
  const { margin: mc, usdVault, usd } = state.ids;
  const pending = await call(mc, 'get_undistributed_margin_fees', { vault: usdVault });
  const distributed = BigInt(await write(mc, 'distribute_margin_fees', { vault: usdVault }, `${label}:distribute`, 'bob'));
  let claimed = 0n;
  for (const role of ['alice', 'bob']) {
    const user = actors[role].address;
    const expected = BigInt(await call(mc, 'get_claimable_margin_fees', { user, asset: usd }));
    const balance = BigInt(await call(mc, 'get_margin_balance_ptokens', { user, asset: usd }));
    const actual = BigInt(await write(mc, 'claim_margin_fees', { user, asset: usd }, `${label}:claim:${role}`, role));
    assert.equal(actual, expected);
    assert.equal(BigInt(await call(mc, 'get_margin_balance_ptokens', { user, asset: usd })), balance + actual);
    assert.equal(BigInt(await call(mc, 'get_claimable_margin_fees', { user, asset: usd })), 0n);
    claimed += actual;
  }
  assert.ok(claimed <= distributed, 'fee claims exceed the funded batch');
  assert.ok(distributed - claimed <= 4n, 'unexpected distribution dust');
  assert.equal(BigInt(await call(mc, 'distribute_margin_fees', { vault: usdVault })), 0n);
  log({ feeReconciliation: label, pending, distributed, claimed, dust: distributed - claimed });
}
async function matrix() {
  for (const side of ['Long', 'Short']) for (const leverage of [2, 3, 4, 5]) for (const amount of [U / 10n, 10n * U, 500n * U]) {
    const label = `matrix:${side}:${leverage}:${amount}`;
    if (state.results.includes(label)) continue;
    const role = leverage % 2 ? 'bob' : 'alice';
    const id = await open(label, role, side, amount, leverage);
    await close(label, role, side, id);
    await distribution(label);
    state.results.push(label); save();
    log({ passed: label, position_id: id });
  }
}

async function ownership() {
  assert.ok(entitlements, 'ownership regression is only for the new isolated fixture');
  const label = 'ownership:late-entry-and-early-exit';
  if (state.results.includes(label)) return;
  const { margin: mc, usdVault, usd } = state.ids;
  const bob = actors.bob.address;
  const alice = actors.alice.address;
  if (!state.ownership) {
    await distribution('ownership:baseline');
    state.ownership = { bobPrincipal: String(await call(mc, 'get_margin_balance_ptokens', { user: bob, asset: usd })) };
    save();
  }
  const bobPrincipal = BigInt(state.ownership.bobPrincipal);
  await write(mc, 'transfer_margin_to_spot', { user: bob, asset: usd, ptoken_amount: bobPrincipal }, `${label}:bob-exit`, 'bob');
  const id = await open(label, 'alice', 'Short', 10n * U, 5);
  await close(label, 'alice', 'Short', id);
  if (!state.ownership.alicePrincipal) {
    state.ownership.alicePrincipal = String(await call(mc, 'get_margin_balance_ptokens', { user: alice, asset: usd }));
    save();
  }
  const alicePrincipal = BigInt(state.ownership.alicePrincipal);
  // Alice leaves before conversion; Bob enters after both fees were charged.
  await write(mc, 'transfer_margin_to_spot', { user: alice, asset: usd, ptoken_amount: alicePrincipal }, `${label}:alice-exit`, 'alice');
  await write(mc, 'transfer_spot_to_margin', { user: bob, asset: usd, ptoken_amount: bobPrincipal }, `${label}:bob-enter`, 'bob');
  const distributed = BigInt(await write(mc, 'distribute_margin_fees', { vault: usdVault }, `${label}:distribute`, 'bob'));
  assert.ok(distributed > 0n);
  assert.equal(BigInt(await call(mc, 'get_claimable_margin_fees', { user: bob, asset: usd })), 0n);
  const earned = BigInt(await write(mc, 'claim_margin_fees', { user: alice, asset: usd }, `${label}:alice-claim`, 'alice'));
  assert.ok(earned > 0n && earned <= distributed && distributed - earned <= 4n);
  assert.equal(BigInt(await call(mc, 'get_claimable_margin_fees', { user: alice, asset: usd })), 0n);
  await write(mc, 'transfer_spot_to_margin', { user: alice, asset: usd, ptoken_amount: alicePrincipal }, `${label}:alice-return`, 'alice');
  state.results.push(label); save();
  log({ passed: label, position_id: id, distributed, originalProviderClaimed: earned, lateDepositorClaimed: 0 });
}

async function extras() {
  const { margin: mc, usd, base, usdVault: uv, baseVault: bv, controller } = state.ids;
  const alice = actors.alice.address;
  const free = () => call(mc, 'get_margin_balance_ptokens', { user: alice, asset: usd }).then(BigInt);
  if (!state.results.includes('cancel-refund')) {
    const before = await free();
    const position_id = await begin('cancel-refund', 'alice', 'Long', 10n * U, 5);
    assert.equal(before - await free(), 10n * U + 10n * U * 5n / 1000n);
    await write(mc, 'cancel_pending_open_v3', { user: alice, position_id }, 'cancel-refund:cancel', 'alice');
    assert.equal(await free(), before);
    assert.equal(await call(mc, 'get_position_fee_terms', { position_id }), null);
    state.results.push('cancel-refund'); save(); log({ passed: 'cancel-refund' });
  }
  if (!state.results.includes('repay-add-recover-release')) {
    const label = 'repay-add-recover-release';
    const id = await open(label, 'alice', 'Short', 100n * U, 5);
    const args = { user: alice, position_id: id };
    const position = await call(mc, 'get_position', { position_id: id });
    await write(mc, 'add_position_collateral_v3', { ...args, position_ptokens: 20n * U }, `${label}:add`, 'alice');
    assert.equal(BigInt((await call(mc, 'get_position', { position_id: id })).collateral_ptokens), BigInt(position.collateral_ptokens) + 20n * U);
    const debt = BigInt(await call(bv, 'get_margin_borrow_balance', { position_id: id }));
    await write(mc, 'repay_margin_position_v3', { ...args, amount: debt / 4n }, `${label}:partial`, 'alice');
    const reduced = BigInt(await call(bv, 'get_margin_borrow_balance', { position_id: id }));
    assert.ok(reduced < debt && reduced > debt / 2n);
    const beforeFees = await call(mc, 'get_undistributed_margin_fees', { vault: uv });
    await write(mc, 'prepare_close_position_v3', args, `${label}:prepare`, 'alice');
    await call(mc, 'swap_close_short_position_v3', { ...args, swap_amount_in: 1n, min_debt_out: debt * 100n }, { role: 'alice', negative: true });
    await write(mc, 'cancel_close_position_v3', args, `${label}:restore`, 'alice');
    assert.deepEqual(await call(mc, 'get_undistributed_margin_fees', { vault: uv }), beforeFees);
    assert.equal((await call(mc, 'get_position', { position_id: id })).status[0], 'Open');
    // The vault caps repayment before lazy accrual; a second payment may be
    // needed for interest dust. Never assume the requested amount cleared debt.
    for (let n = 0; n < 4; n++) {
      const remaining = BigInt(await call(bv, 'get_margin_borrow_balance', { position_id: id }));
      if (!remaining) break;
      await write(mc, 'repay_margin_position_v3', { ...args, amount: remaining + U }, `${label}:full:${n}`, 'alice');
    }
    assert.equal(BigInt(await call(bv, 'get_margin_borrow_balance', { position_id: id })), 0n);
    await write(mc, 'release_debt_free_position_v3', args, `${label}:release`, 'alice');
    assert.equal(await call(mc, 'get_position', { position_id: id }), null);
    assert.deepEqual(await call(mc, 'get_undistributed_margin_fees', { vault: uv }), beforeFees);
    await distribution(label);
    state.results.push(label); save(); log({ passed: label });
  }
  if (!state.results.includes('siblings')) {
    // Keep Bob's swapped close outstanding while Alice exercises eight siblings.
    // This spends the expiry interval doing useful work rather than idling.
    const delayedId = await open('delayed-close', 'bob', 'Long', 10n * U, 5);
    await close('delayed-close', 'bob', 'Long', delayedId, { deferFinish: true });
    const ids = [];
    for (let n = 0; n < 8; n++) ids.push(await open(`siblings:${n}`, 'alice', n % 2 ? 'Short' : 'Long', U, 2 + n % 4));
    const unsettled = ids.filter((_, n) => !state.done[`siblings:${n}:finish`]).length;
    assert.equal((await call(mc, 'get_user_positions', { user: alice })).length, unsettled);
    const bobId = await open('siblings:bob', 'bob', 'Short', 10n * U, 5);
    await close('siblings:bob', 'bob', 'Short', bobId);
    for (let n = 0; n < ids.length; n++) await close(`siblings:${n}`, 'alice', n % 2 ? 'Short' : 'Long', ids[n]);
    assert.equal((await call(mc, 'get_user_positions', { user: alice })).length, 0);
    await distribution('siblings');
    state.results.push('siblings'); save(); log({ passed: 'siblings' });
  }
  if (!state.results.includes('delayed-close')) {
    const label = 'delayed-close';
    const id = BigInt(state.done[`${label}:begin`].value);
    const pending = await call(mc, 'get_pending_perps_close', { position_id: id });
    const waitMs = Math.max(0, Number(pending.expires_at) * 1000 - Date.now() + 10000);
    assert.ok(waitMs < 330000, 'unexpected close deadline');
    if (waitMs) { log({ waitingForExpiredCloseMs: waitMs }); await sleep(waitMs); }
    await close(label, 'bob', 'Long', id);
    const settlement = await retryRead(() => server.getTransaction(state.done[`${label}:finish`].hash));
    assert.ok(BigInt(settlement.createdAt) > BigInt(pending.expires_at), 'settlement did not exercise expiry');
    log({ expiredCloseSettled: id, expiresAt: pending.expires_at, settledAt: settlement.createdAt });
    await distribution(label);
    state.results.push(label); save(); log({ passed: label });
  }
  if (!state.results.includes('liquidation')) {
    const id = await open('liquidation', 'alice', 'Long', 100n * U, 5);
    await call(mc, 'begin_liquidation_v3', { liquidator: actors.bob.address, position_id: id }, { role: 'bob', negative: true });
    const initialPosition = await call(mc, 'get_position', { position_id: id });
    const baseAmount = BigInt(initialPosition.collateral_ptokens) * BigInt(await call(bv, 'get_exchange_rate')) / E;
    const debt = BigInt(await call(uv, 'get_margin_borrow_balance', { position_id: id }));
    // Long: equity = base * price - debt; maintenance = 10% * base * price.
    const boundary = debt * P * 10n / (baseAmount * 9n);
    await write(controller, 'set_price_fallback', { token: base, price: [boundary * 1001n / 1000n, P] }, 'liquidation:above-boundary');
    const above = BigInt(await call(mc, 'get_health_factor', { position_id: id }));
    assert.ok(above > E);
    await write(controller, 'set_price_fallback', { token: base, price: [boundary * 999n / 1000n, P] }, 'liquidation:price');
    assert.ok(BigInt(await call(mc, 'get_health_factor', { position_id: id })) < E);
    log({ liquidationBoundary: 'Long', priceScaled: boundary, scale: P, healthAbove: above });
    const feesBefore = await call(mc, 'get_undistributed_margin_fees', { vault: uv });
    const position = await call(mc, 'get_position', { position_id: id });
    const underlying = BigInt(position.collateral_ptokens) * BigInt(await call(bv, 'get_exchange_rate')) / E;
    const args = { liquidator: actors.bob.address, position_id: id };
    await write(mc, 'begin_liquidation_v3', args, 'liquidation:liquidate-begin', 'bob');
    await write(mc, 'swap_liquidation_v3', { ...args, amount_with_slippage: (await quote(base, underlying)) * 9500n / 10000n }, 'liquidation:liquidate-swap', 'bob');
    await write(mc, 'finish_liquidation_v3', args, 'liquidation:liquidate-finish', 'bob');
    assert.equal(await call(mc, 'get_position', { position_id: id }), null);
    assert.equal(BigInt(await call(uv, 'get_margin_borrow_balance', { position_id: id })), 0n);
    assert.deepEqual(await call(mc, 'get_undistributed_margin_fees', { vault: uv }), feesBefore);
    await write(controller, 'set_price_fallback', { token: base, price: [P, P] }, 'liquidation:restore-price');
    await distribution('liquidation');
    state.results.push('liquidation'); save(); log({ passed: 'liquidation' });
  }
  if (!state.results.includes('withdraw-fees')) {
    const before = BigInt(await call(usd, 'balance', { who: actors.bob.address }));
    await write(mc, 'transfer_margin_to_spot', { user: actors.bob.address, asset: usd, ptoken_amount: U }, 'withdraw-fees:spot', 'bob');
    await write(uv, 'withdraw', { user: actors.bob.address, ptoken_amount: U }, 'withdraw-fees:wallet', 'bob');
    assert.ok(BigInt(await call(usd, 'balance', { who: actors.bob.address })) >= before + U);
    state.results.push('withdraw-fees'); save(); log({ passed: 'withdraw-fees' });
  }
  if (!state.results.includes('keeper-liquidation')) {
    const label = 'keeper-liquidation';
    const id = await open(label, 'alice', 'Short', 100n * U, 5);
    const position = await call(mc, 'get_position', { position_id: id });
    const quoteAmount = BigInt(position.collateral_ptokens) * BigInt(await call(uv, 'get_exchange_rate')) / E;
    const debt = BigInt(await call(bv, 'get_margin_borrow_balance', { position_id: id }));
    // Short: equity = quote collateral - base debt * price; maintenance = 10% of debt value.
    const boundary = quoteAmount * P * 10n / (debt * 11n);
    await write(controller, 'set_price_fallback', { token: base, price: [boundary * 999n / 1000n, P] }, `${label}:below-boundary`);
    const below = BigInt(await call(mc, 'get_health_factor', { position_id: id }));
    assert.ok(below > E);
    await write(controller, 'set_price_fallback', { token: base, price: [boundary * 1001n / 1000n, P] }, `${label}:price`);
    assert.ok(BigInt(await call(mc, 'get_health_factor', { position_id: id })) < E);
    log({ liquidationBoundary: 'Short', priceScaled: boundary, scale: P, healthBelow: below });
    const before = await call(mc, 'get_undistributed_margin_fees', { vault: uv });
    const abi = await spec(mc);
    const argsObject = (name, args) => Object.fromEntries(abi.getFunc(name).inputs()
      .map((input, i) => [input.name().toString(), S.scValToNative(args[i])]));
    const client = {
      read: (name, args = []) => call(mc, name, argsObject(name, args)),
      submit: (name, args = []) => write(mc, name, argsObject(name, args), `${label}:${name}`, 'bob'),
    };
    const logger = Object.fromEntries(['info', 'warn', 'error'].map(level => [level, (message, data) => log({ level, message, ...data })]));
    const keeper = new MarginLiquidationKeeper({ publicKey: actors.bob.address, dryRun: false, slippageBps: 100 },
      client, { activePositionIds: [id.toString()] }, async () => {}, logger);
    await keeper.processPosition(id);
    assert.equal(await call(mc, 'get_position', { position_id: id }), null);
    assert.equal(BigInt(await call(bv, 'get_margin_borrow_balance', { position_id: id })), 0n);
    assert.deepEqual(await call(mc, 'get_undistributed_margin_fees', { vault: uv }), before);
    await write(controller, 'set_price_fallback', { token: base, price: [P, P] }, `${label}:restore-price`);
    await distribution(label);
    state.results.push(label); save(); log({ passed: label });
  }
}

async function audit() {
  assert.equal(state.results.filter(x => x.startsWith('matrix:')).length, 24, 'matrix incomplete');
  for (const label of ['cancel-refund', 'repay-add-recover-release', 'siblings', 'delayed-close', 'liquidation', 'withdraw-fees', 'keeper-liquidation']) {
    assert.ok(state.results.includes(label), `${label} incomplete`);
  }
  for (const [idName, artifact] of Object.entries({ margin: 'margin_controller', adapter: 'swap_adapter',
    controller: 'simple_peridottroller', usdVault: 'receipt_vault', baseVault: 'receipt_vault', model: 'jump_rate_model' })) {
    assert.equal((await instance(state.ids[idName])).hash, state.hashes[artifact], `${idName} code changed`);
  }
  const mc = state.ids.margin;
  for (const role of ['alice', 'bob']) assert.deepEqual(await call(mc, 'get_user_positions', { user: actors[role].address }), []);
  for (const [vault, asset] of [[state.ids.usdVault, state.ids.usd], [state.ids.baseVault, state.ids.base]]) {
    let free = 0n, unclaimed = 0n;
    for (const role of ['alice', 'bob']) {
      const args = { user: actors[role].address, asset };
      free += BigInt(await call(mc, 'get_margin_balance_ptokens', args));
      unclaimed += BigInt(await call(mc, 'get_claimable_margin_fees', args));
    }
    const backing = BigInt(await call(vault, 'get_ptoken_balance', { user: mc }));
    const pending = await call(mc, 'get_undistributed_margin_fees', { vault });
    const cash = BigInt(await call(asset, 'balance', { who: mc }));
    assert.ok(backing >= free + unclaimed + BigInt(pending.ptokens), 'unbacked margin shares');
    assert.equal(BigInt(pending.ptokens), 0n);
    assert.equal(BigInt(pending.underlying), 0n);
    assert.equal(cash, 0n, 'stranded controller underlying');
    const borrowed = BigInt(await call(vault, 'get_total_borrowed'));
    const badDebt = BigInt(await call(vault, 'get_total_bad_debt'));
    assert.equal(badDebt, 0n, 'unexpected test bad debt');
    log({ finalBacking: vault, backing, free, unclaimed, shareDust: backing - free - unclaimed, cash, borrowed, badDebt });
  }
  const count = BigInt(await call(mc, 'get_position_counter'));
  const opened = new Set(Object.entries(state.done).filter(([label, tx]) => label.endsWith(':begin') && tx.value !== null
    && state.done[`${label.slice(0, -6)}:swap`]).map(([, tx]) => String(tx.value)));
  for (let id = 1n; id <= count; id++) {
    const args = { position_id: id };
    assert.equal(await call(mc, 'get_position', args), null);
    assert.equal(await call(mc, 'get_position_fee_terms', args), null);
    for (const getter of ['get_pending_perps_open', 'get_pending_perps_open_execution',
      'get_pending_perps_close', 'get_perps_position', 'get_pending_liquidation', 'get_liquidation_takeover_after']) {
      assert.equal(await call(mc, getter, args), null, `${getter} retained state for position ${id}`);
    }
    // Only the debt vault creates a snapshot; reading a never-initialized
    // namespace intentionally fails closed. Require a retained zero snapshot
    // for every executed open, rather than treating a failed getter as zero.
    const keys = [state.ids.usdVault, state.ids.baseVault].map(vault => S.xdr.LedgerKey.contractData(
      new S.xdr.LedgerKeyContractData({ contract: new S.Address(vault).toScAddress(),
        key: S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol('MarginBorrowSnapshots'), S.nativeToScVal(id, { type: 'u64' })]),
        durability: S.xdr.ContractDataDurability.persistent() })));
    const snapshots = (await retryRead(() => server.getLedgerEntries(...keys))).entries;
    assert.equal(snapshots.length, opened.has(id.toString()) ? 1 : 0, 'unexpected debt snapshot count');
    for (const entry of snapshots) assert.equal(BigInt(S.scValToNative(entry.val.contractData().val()).principal), 0n);
  }
  const confirmed = Object.entries(state.done).filter(([, x]) => x.hash);
  const peaks = {};
  const liquidationStarts = new Map();
  const liquidationFinishes = [];
  for (const [label, tx] of confirmed) {
    const result = await retryRead(() => server.getTransaction(tx.hash));
    assert.equal(result.status, 'SUCCESS', `${label} is not independently confirmed`);
    const events = result.events.contractEventsXdr.flat().map(event => ({
      contract: event.contractId() ? S.StrKey.encodeContract(event.contractId()) : null,
      topics: event.body().v0().topics().map(S.scValToNative),
      data: S.scValToNative(event.body().v0().data()),
    }));
    for (const event of events.filter(event => event.contract === mc)) {
      const [name, id, liquidator] = event.topics;
      if (name === 'liquidation_started') liquidationStarts.set(String(id), event.data);
      if (name !== 'liquidation_finished') continue;
      const initial = liquidationStarts.get(String(id));
      assert.ok(initial, 'missing liquidation start event');
      assert.equal(liquidator, actors.bob.address);
      assert.equal(event.data.owner, actors.alice.address);
      assert.equal(BigInt(event.data.bad_debt), 0n);
      assert.ok(BigInt(event.data.repaid) >= BigInt(initial.debt_amount));
      assert.equal(BigInt(event.data.incentive), BigInt(initial.debt_amount) / 100n);
      const asset = label.startsWith('keeper-liquidation:') ? state.ids.base : state.ids.usd;
      const transfers = events.filter(e => e.contract === asset && e.topics[0] === 'transfer' && e.topics[1] === mc);
      assert.equal(transfers.filter(e => e.topics[2] === liquidator).reduce((sum, e) => sum + BigInt(e.data), 0n),
        BigInt(event.data.incentive), 'liquidator was not paid the recorded incentive');
      liquidationFinishes.push({ positionId: id, hash: tx.hash, asset, ...event.data });
    }
    for (const [key, value] of Object.entries(tx.resources)) {
      if (!peaks[key] || value > peaks[key].value) peaks[key] = { value, label };
    }
  }
  assert.equal(liquidationFinishes.length, 2, 'both liquidation directions must settle');
  const successfulHashes = new Set(confirmed.map(([, tx]) => tx.hash));
  const attempts = new Map(readFileSync(resolve(dir, 'transactions.jsonl'), 'utf8').trim().split('\n')
    .map(line => JSON.parse(line)).filter(entry => entry.submitting).map(entry => [entry.hash, entry]));
  const failedAttempts = [];
  for (const [hash, attempt] of attempts) {
    if (successfulHashes.has(hash)) continue;
    const result = await retryRead(() => server.getTransaction(hash));
    // Retain the diagnosed, rolled-back first attempt in the final report.
    // A new or unconfirmed failure must stop the audit, not disappear from it.
    assert.equal(hash, 'b0b019562456fa5c1e0a5f860a9cc49a23593b6bab5f7b3f0c4ee0c44614b389', 'unreviewed submission attempt');
    assert.equal(result.status, 'FAILED', 'submission outcome remains uncertain');
    failedAttempts.push({ hash, label: attempt.submitting, ledger: result.ledger,
      reason: 'Pool token instance absent from simulated footprint; execution trapped and rolled back' });
  }
  log({ auditPassed: true, sourceCommit: state.sourceCommit, scenarios: state.results, clearedPositions: count,
    confirmedTransactions: confirmed.length, failedAttempts, liquidationFinishes, resourcePeaks: peaks });
}

assert.equal((await server.getNetwork()).passphrase, S.Networks.TESTNET, 'refusing non-testnet RPC');
networkLimits = await loadNetworkLimits();
if (mode === 'deploy') await setup();
else if (mode === 'probe') await begin('probe', 'alice', 'Long', U / 10n, 2);
else if (mode === 'matrix') await matrix();
else if (mode === 'extras') await extras();
else if (mode === 'ownership') await ownership();
else if (mode === 'audit') await audit();
else if (mode === 'inspect') log({ state, actors });
else throw new Error(`Unknown mode ${mode}`);
