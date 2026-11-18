'use strict';
// keeper-inspect-v4.cjs against an injected fake RPC. Read-only; public identifiers only.
const test = require('node:test');
const assert = require('node:assert/strict');
const S = require('../../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const {inspect} = require('../keeper-inspect-v4.cjs');
const publicKey = byte => S.StrKey.encodeEd25519PublicKey(Buffer.alloc(32, byte));
const contract = byte => S.StrKey.encodeContract(Buffer.alloc(32, byte));
function fixture(options = {}) {
  const config = {readOnly: true, network: S.Networks.PUBLIC, rpcUrl: 'https://rpc.invalid', admin: publicKey(1), keeperPublicAddress: 'keeper' in options ? options.keeper : publicKey(2),
    receipt: contract(1), vault: contract(2), fundingAsset: contract(3), wasmSha256: {receipt: '1'.repeat(64), vault: '2'.repeat(64)}, pilotOwners: [publicKey(1)]};
  // boundaries[i] = interval id of boundary i; sealed = set of sealed interval ids.
  const world = {boundaries: options.boundaries ?? [], sealed: new Set(options.sealed ?? []), claim: options.claim ?? true, commands: options.commands ?? [], holder: options.holder ?? publicKey(9)};
  const calls = [], addr = a => new S.Address(a).toScVal(), sym = (o) => S.xdr.ScVal.scvMap(Object.entries(o).map(([k, v]) => new S.xdr.ScMapEntry({key: S.xdr.ScVal.scvSymbol(k), val: v})));
  const instance = (id, hash) => ({val: {contractData: () => ({contract: () => new S.Address(id).toScAddress(), val: () => ({instance: () => ({executable: () => S.xdr.ContractExecutable.contractExecutableWasm(Buffer.from(hash, 'hex'))})})})}});
  let latest = 0;
  const rpc = {getNetwork: async () => ({passphrase: config.network}),
    getLatestLedger: async () => ({sequence: options.ledgerMoves && ++latest > 1 ? 101 : 100, closeTime: '1000'}),
    getLedgerEntries: async () => ({latestLedger: 100, entries: options.absent ? [] : [instance(config.receipt, config.wasmSha256.receipt), instance(config.vault, config.wasmSha256.vault)]}),
    getAccount: async id => new S.Account(id, '7'),
    simulateTransaction: async tx => {
      assert.equal(tx.signatures.length, 0); const fn = tx.operations[0].func.invokeContract(), method = fn.functionName().toString(); calls.push(method);
      const ok = retval => ({_parsed: true, latestLedger: 100, transactionData: new S.SorobanDataBuilder().setResources(15247953, 0, 0), result: {auth: [], retval}});
      const id = () => Number(S.scValToNative(fn.args()[0]));
      switch (method) {
        case 'stage_config': return ok(sym({vault: addr(config.vault), guardian: addr(config.admin)}));
        case 'stage_receipt': return ok(addr(config.receipt));
        case 'get_admin': case 'stage_guardian': return ok(addr(config.admin));
        case 'stage_investment_keeper': return ok(addr(config.keeperPublicAddress ?? publicKey(2)));
        case 'stage_investment_guard': return ok(S.xdr.ScVal.scvVoid());
        case 'stage_funding_asset': return ok(addr(config.fundingAsset));
        case 'stage_commands': return ok(S.xdr.ScVal.scvVec(world.commands.map(c => sym({id: S.nativeToScVal(BigInt(c.id), {type: 'u64'}),
          job: sym({kind: S.nativeToScVal(c.kind, {type: 'u32'}), phase: S.nativeToScVal(c.phase, {type: 'u32'}), deadline: S.nativeToScVal(BigInt(c.deadline), {type: 'u64'})})}))));
        case 'reward_boundary': return ok(id() < world.boundaries.length ? sym({holder: addr(world.holder), interval: S.nativeToScVal(BigInt(world.boundaries[id()]), {type: 'u64'}), waived: S.xdr.ScVal.scvBool(false)}) : S.xdr.ScVal.scvVoid());
        case 'reward_interval': return ok(options.missingInterval ? S.xdr.ScVal.scvVoid() : sym({sealed: S.xdr.ScVal.scvBool(world.sealed.has(id())), segments: S.nativeToScVal(1, {type: 'u32'})}));
        case 'checkpoint_rewards':
          assert.equal(fn.args().length, 0); assert.equal(S.Address.fromScAddress(fn.contractAddress()).toString(), config.vault);
          if (world.claim === 'error') return {_parsed: true, latestLedger: 100, error: 'host error'};
          return ok(S.xdr.ScVal.scvBool(world.claim === true));
        default: throw Error('unexpected read ' + method);
      }
    }};
  return {config, rpc, calls, world};
}
test('no boundary: nothing to checkpoint, the plan is the ordinary one and no checkpoint is simulated', async () => {
  const f = fixture(); const r = await inspect(f.config, f.rpc);
  assert.equal(r.status, 'IDLE'); assert.deepEqual(r.rewards, {open: false, lastBoundaryId: null, interval: null, checkpointClaimable: null});
  assert(!f.calls.includes('checkpoint_rewards')); assert(!f.calls.includes('reward_interval')); assert.equal(r.signing, false); assert.equal(r.submission, false);
});
test('emergency exit by ANY holder leaves an unsealed interval: checkpoint_rewards is planned first, also with no listed command', async () => {
  const f = fixture({boundaries: [0], commands: []}); const r = await inspect(f.config, f.rpc); // holder publicKey(9) is not a pilot owner
  assert.equal(r.status, 'UNSIGNED_PLAN_REQUIRES_FRESH_SIMULATION'); assert.equal(r.rewards.open, true); assert.equal(r.rewards.checkpointClaimable, true);
  assert.equal(r.rewards.lastBoundaryId, '0'); assert.equal(r.rewards.interval, '0'); assert.equal(r.rewards.checkpointInstructions, 15247953);
  assert.deepEqual(r.plan.plans.map(p => p.method), ['checkpoint_rewards']); assert.equal(r.plan.plans[0].target, f.config.vault); assert.equal(r.plan.rewardCheckpoint.status, 'PLANNED_FIRST');
  const g = fixture({boundaries: [0], commands: [{id: 4, kind: 2, phase: 21, deadline: 0}, {id: 5, kind: 1, phase: 10, deadline: 5000}]});
  assert.deepEqual((await inspect(g.config, g.rpc)).plan.plans.map(p => p.method), ['checkpoint_rewards', 'stage_emergency_pay', 'stage_withdraw']);
});
test('the newest boundary decides: sealed intervals need nothing, a later exit reopens', async () => {
  let f = fixture({boundaries: [0, 0, 1, 1, 2], sealed: [0, 1, 2]}); let r = await inspect(f.config, f.rpc);
  assert.equal(r.rewards.open, false); assert.equal(r.rewards.lastBoundaryId, '4'); assert.equal(r.rewards.interval, '2'); assert.equal(r.status, 'IDLE'); assert(!f.calls.includes('checkpoint_rewards'));
  f = fixture({boundaries: [0, 0, 1, 1, 2, 3], sealed: [0, 1, 2]}); r = await inspect(f.config, f.rpc);
  assert.equal(r.rewards.open, true); assert.equal(r.rewards.interval, '3'); assert.deepEqual(r.plan.plans.map(p => p.method), ['checkpoint_rewards']);
  const many = fixture({boundaries: Array.from({length: 41}, (_, i) => i), sealed: Array.from({length: 40}, (_, i) => i)}); r = await inspect(many.config, many.rpc);
  assert.equal(r.rewards.lastBoundaryId, '40'); assert.equal(r.rewards.open, true); assert(many.calls.filter(m => m === 'reward_boundary').length <= 14);
});
test('NEGATIVE venue claim unavailable (false return or failed simulation): no checkpoint is planned, only release steps', async () => {
  for (const claim of [false, 'error']) {
    const f = fixture({boundaries: [0], claim, commands: [{id: 4, kind: 2, phase: 21, deadline: 0}, {id: 5, kind: 1, phase: 10, deadline: 5000}]}); const r = await inspect(f.config, f.rpc);
    assert.equal(r.rewards.open, true); assert.equal(r.rewards.checkpointClaimable, false);
    assert.deepEqual(r.plan.plans.map(p => p.method), ['stage_emergency_pay']); assert.equal(r.plan.rewardCheckpoint.status, 'UNAVAILABLE_HOLDING_OTHER_STEPS');
    assert.deepEqual(r.plan.rewardCheckpoint.held, [{method: 'stage_withdraw', args: ['5']}]);
  }
});
test('NEGATIVE incoherent reads fail closed: ledger change, missing interval record, undeployed pair; keeper address pending plans nothing', async () => {
  let f = fixture({boundaries: [0], ledgerMoves: true}); assert.equal((await inspect(f.config, f.rpc)).status, 'LEDGER_CHANGED_READ_AGAIN');
  f = fixture({boundaries: [0], missingInterval: true}); await assert.rejects(inspect(f.config, f.rpc), /boundary without an interval record/);
  f = fixture({absent: true}); assert.equal((await inspect(f.config, f.rpc)).status, 'NOT_DEPLOYED_OR_ARCHIVED');
  f = fixture({boundaries: [0], keeper: null}); const r = await inspect(f.config, f.rpc);
  assert.equal(r.status, 'KEEPER_CONFIGURATION_PENDING'); assert.equal(r.plan, undefined); assert.equal(r.rewards.open, true);
  await assert.rejects(inspect({...f.config, readOnly: false}, f.rpc)); await assert.rejects(inspect({...f.config, network: S.Networks.TESTNET}, f.rpc));
});
