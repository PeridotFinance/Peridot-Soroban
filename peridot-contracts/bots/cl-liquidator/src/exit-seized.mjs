// Manual tool (LAUNCH P9): the CL liquidator exits the vault units it received from seizures (after L5 sync_seized).
// Run ON the liquidator host as the service user, with the cl-liquidator service STOPPED (it takes the same
// <dataDir>/run.lock, so it refuses while the bot runs: the bot and this tool share the liquidator's sequence number).
//
//   node src/exit-seized.mjs --config config/mainnet.local.json show
//   node src/exit-seized.mjs --config ... prepare-close [--units N]        # default: all stage_units(liquidator)
//   node src/exit-seized.mjs --config ... withdraw --id N                  # in-kind PYUSD + USDC to the liquidator
//   node src/exit-seized.mjs --config ... emergency-exit --units N --accept-reward-waiver   # fallback only
//   node src/exit-seized.mjs --config ... emergency-pay --id N                                 # fallback only
//
// Primary route (Testnet 03-liquidate.sh): stage_prepare_close(liquidator, units, min_swap = 0, deadline) locks the
// units; stage_withdraw(id) then pays the two-legged mixture in kind (no swap, no keeper). Fallback when the close path
// is unavailable: stage_emergency_exit(liquidator, units, accept_reward_waiver) then stage_emergency_pay(id) (no oracle,
// pool, reward, keeper or pause dependency; a reward waiver forfeits unprovable AQUA). Each command is a dry run
// (simulation from the liquidator address, nothing signed) unless CONFIRM_EXIT=yes, which signs with executorKeyFile
// and submits ONCE through the same guarded sender as the bot (allowlist, source-only auth, 97M stop, fee cap,
// daily budget, code pins). Every command but `show` refuses unless BOTH the PYUSD and USDC trustlines exist
// (src/trustlines.mjs): without PYUSD stage_withdraw / stage_emergency_pay revert with SAC #13 and the command stays held.
import {fileURLToPath} from 'node:url';
import {loadSdk} from './chain.mjs';
import {makeReader} from './chain.mjs';
import {loadConfig} from './config.mjs';
import {isMainnet, makeRpc, makeSender, openBudget, acquireRunLock, startChecks, NEVER_TOUCH, scArgs, trustlineStatus, requireTrustlines} from './mainnet.mjs';

export const PHASE = Object.freeze({CLOSE_LOCKED: 10, EMERGENCY_HELD: 21});
export const MAX_DEADLINE_SEC = 3600;

/** Pure: the exact call for a command, given chain facts. Throws `refused:` for anything outside the liquidator's own exit. */
export function exitCall(cfg, cmd, {units, job, now, opts = {}}) {
  const r = cfg.contracts.receipt, me = cfg.liquidator;
  if (NEVER_TOUCH.includes(r)) throw Error('refused: pilot/retired receipt');
  const ownJob = kind => {
    if (!job) throw Error('refused: unknown command id');
    if (String(job.owner) !== me) throw Error('refused: command does not belong to the liquidator');
    if (Number(job.kind) !== kind) throw Error('refused: wrong command kind');
  };
  switch (cmd) {
    case 'prepare-close': {
      const n = opts.units == null ? BigInt(units) : BigInt(opts.units);
      if (n <= 0n || n > BigInt(units)) throw Error(`refused: units ${n} outside 1..${units}`);
      return {contract: r, fn: 'stage_prepare_close', args: {owner: me, shares: n, min_swap: 0n, deadline: BigInt(now + 3000)}};
    }
    case 'withdraw':
      ownJob(1); if (Number(job.phase) !== PHASE.CLOSE_LOCKED) throw Error(`refused: close command is in phase ${job.phase}, not ${PHASE.CLOSE_LOCKED} (locked)`);
      if (BigInt(job.min_swap) !== 0n) throw Error('refused: only in-kind closes (min_swap 0) are handled here');
      return {contract: r, fn: 'stage_withdraw', args: {id: BigInt(opts.id)}};
    case 'emergency-exit': {
      if (opts.acceptRewardWaiver !== true) throw Error('refused: emergency-exit needs --accept-reward-waiver (explicit)');
      const n = BigInt(opts.units ?? -1);
      if (n <= 0n || n > BigInt(units)) throw Error(`refused: units ${n} outside 1..${units}`);
      return {contract: r, fn: 'stage_emergency_exit', args: {owner: me, shares: n, accept_reward_waiver: true}};
    }
    case 'emergency-pay':
      ownJob(2); if (Number(job.phase) !== PHASE.EMERGENCY_HELD) throw Error(`refused: emergency command is in phase ${job.phase}, not ${PHASE.EMERGENCY_HELD} (held)`);
      return {contract: r, fn: 'stage_emergency_pay', args: {id: BigInt(opts.id)}};
    default: throw Error(`unknown command ${cmd}`);
  }
}

/** Allowlist for makeSender: exactly the one call computed by exitCall. */
export function exitAllow(expected) {
  return (cfg, contract, fn, args) => {
    const norm = o => JSON.stringify(o, (_, v) => (typeof v === 'bigint' ? String(v) : v));
    if (contract !== expected.contract || fn !== expected.fn || norm(args) !== norm(expected.args)) throw Error('refused: call differs from the planned exit');
    if (fn === 'stage_prepare_close' && BigInt(args.deadline) > BigInt(Math.floor(Date.now() / 1000) + MAX_DEADLINE_SEC)) throw Error('refused: deadline above 1 h');
  };
}

async function main() {
  const a = process.argv.slice(2), opt = n => (a.includes(n) ? a[a.indexOf(n) + 1] : undefined);
  const cfg = loadConfig(opt('--config') ?? (() => { throw Error('--config required'); })());
  if (!isMainnet(cfg)) throw Error('exit-seized.mjs is for the Mainnet key-file signer (Testnet: prototypes/cl-collateral-group/testnet/03-liquidate.sh)');
  const cmd = a.find(x => ['show', 'prepare-close', 'withdraw', 'emergency-exit', 'emergency-pay'].includes(x));
  if (!cmd) throw Error('usage: exit-seized.mjs --config F show | prepare-close [--units N] | withdraw --id N | emergency-exit --units N --accept-reward-waiver | emergency-pay --id N');
  const release = acquireRunLock(cfg.dataDir); process.on('exit', release);
  const S = loadSdk(), rpc = makeRpc(cfg, {S}), r = makeReader(cfg), c = cfg.contracts, me = r.addr(cfg.liquidator);
  await startChecks(cfg, {rpc, view: r.view, S});
  const {now} = await r.ledgerNow();
  const units = BigInt(await r.view(c.receipt, 'stage_units', [me]));
  const facts = {units: String(units), beneficial: String(await r.view(c.vault, 'stage_beneficial', [me])),
    commands: await r.view(c.receipt, 'stage_commands', [me]),
    pyusd: String(await r.view(c.pyusd, 'balance', [me]).catch(() => 'no trustline')), usdc: String(await r.view(c.usdc, 'balance', [me]).catch(() => 'no trustline'))};
  const show = o => console.log(JSON.stringify(o, (_, v) => (typeof v === 'bigint' ? String(v) : v), 1));
  if (cmd === 'show') {
    facts.trustlines = await trustlineStatus(cfg, rpc, cfg.liquidator, S);
    const q = units > 0n ? await r.simulate(c.vault, 'stage_quote_close', [r.u128(units), S.nativeToScVal(20, {type: 'u32'})]) : null;
    return show({liquidator: cfg.liquidator, ...facts, closeQuote20bps: q ? (q.ok ? q.retval : q.error) : null});
  }
  // Both trustlines before ANY exit step: without PYUSD the in-kind payout reverts (SAC #13) and the command stays held.
  facts.trustlines = await requireTrustlines(cfg, rpc, cfg.liquidator, S);
  const id = opt('--id');
  const job = id != null ? await r.view(c.receipt, 'stage_job', [r.u64(id)]).catch(() => null) : null;
  const call = exitCall(cfg, cmd, {units, job, now, opts: {units: opt('--units'), id, acceptRewardWaiver: a.includes('--accept-reward-waiver')}});
  const sim = await r.simulate(call.contract, call.fn, scArgs(S, call));
  show({cmd, call, simulation: sim.ok ? {retval: sim.retval, metrics: sim.metrics} : {error: sim.error}, before: facts});
  if (process.env.CONFIRM_EXIT !== 'yes') { console.log('dry run: set CONFIRM_EXIT=yes to sign and submit once'); return; }
  if (!sim.ok) throw Error('refused: simulation failed');
  const send = makeSender(cfg, {rpc, S, account: cfg.liquidator, keyFile: cfg.executorKeyFile, kind: 'liquidator', allow: exitAllow(call), budget: openBudget(cfg.dataDir, cfg)});
  const res = await send(call.contract, call.fn, call.args, {onSigned: hash => console.log(JSON.stringify({signed: true, hash}))});
  show({result: res});
  if (!res.ok) process.exitCode = 1;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) main().catch(e => { console.error('exit-seized refused:', String(e.message).slice(0, 300)); process.exitCode = 1; });
