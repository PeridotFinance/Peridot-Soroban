// READ-ONLY seed simulation against CURRENT Mainnet state, before activation (user condition 3, 15 Nov 2026).
// No signer, no key, no journal, no sendTransaction; never writes a config. Uses the pilot-ceremony-v3 adapter and
// gate. Because the configs may still be unreviewed, the REVIEW STATUS (only) is substituted in memory so the reads
// can run; this program can prepare nothing.
//
// RPC simulateTransaction runs on current state only, and the receipt is not active yet: stage_open cannot be
// simulated "as if activation had applied" (it traps in stg_config). This tool therefore records:
//  (1) that limitation, with the actual simulated stage_open error;
//  (2) the exact step-20 activation simulation figures (from operator-deployment-draft-v3_1 full-release);
//  (3) fresh stage_quote_open for 800 PYUSD (and 400) at 20 bps / 5,000 bps on one ledger, wallet fit, and
//      the pre-activation open-command check.
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const S = require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const P = require('./pilot-ceremony-v3.cjs');
const ROOT = path.resolve(__dirname, '../..');
const json = v => JSON.stringify(v, (_, x) => typeof x === 'bigint' ? String(x) : x, 2) + '\n';
const pyusd = raw => { const n = BigInt(raw); return `${n / 10000000n}.${String(n % 10000000n).padStart(7, '0')}`; };
async function run(fullConfigFile, journalDir, {url = 'https://soroban-rpc.mainnet.stellar.gateway.fm', scratch}) {
  // In-memory review substitution in a scratch directory (never the release folder).
  fs.mkdirSync(scratch, {recursive: true});
  const full = JSON.parse(fs.readFileSync(fullConfigFile)), fullCopy = path.join(scratch, 'full-review-standin.json');
  fs.writeFileSync(fullCopy, JSON.stringify({...full, status: 'REVIEWED_UNSIGNED_CEREMONY', note: 'READ-ONLY STAND-IN: status substituted for reads only'}));
  const pilot = {...P.makeConfig(fullCopy, journalDir), status: 'REVIEWED_UNSIGNED_PYUSD_PILOT_V3', rpcUrl: url};
  const input = P.loadInputs(pilot, {allowPendingActivation: true}); assert(input.activationPending, 'activation already applied: use pilot-ceremony-v3 inspect');
  const rpc = P.paced(new S.rpc.Server(url, {timeout: 60})), deps = P.adapter(input, pilot, rpc);
  const snap = await deps.preActivation(), gate = P.preActivationGate(input, pilot, snap);
  // 400 quote and the stage_open attempt, on a fresh ledger each (read-only).
  const A = input.plan.admin, V = input.plan.proposedInstances.vault.contract, R = input.plan.proposedInstances.receipt.contract;
  const sim = async (c, m, args) => P.retrying(async () => {
    const acc = await rpc.getLedgerEntries(S.xdr.LedgerKey.account(new S.xdr.LedgerKeyAccount({accountId: new S.Address(A).toScAddress().accountId()})));
    const tx = new S.TransactionBuilder(new S.Account(A, acc.entries[0].val.account().seqNum().toString()), {fee: '1000', networkPassphrase: S.Networks.PUBLIC})
      .addOperation(new S.Contract(c).call(m, ...args)).setTimeout(0).build();
    return rpc.simulateTransaction(tx);
  }, {log: () => {}});
  const qa = a => [S.nativeToScVal(BigInt(a), {type: 'u128'}), S.nativeToScVal(20, {type: 'u32'}), S.nativeToScVal(5000, {type: 'u32'})];
  const q400 = await sim(V, 'stage_quote_open', qa('4000000000'));
  const quote400 = S.rpc.Api.isSimulationSuccess(q400) ? S.scValToNative(q400.result.retval) : {error: String(q400.error).split('\n')[0]};
  const q = snap.quoteFull, deadline = BigInt(snap.closeTime + 3000);
  const open = await sim(R, 'stage_open', [new S.Address(A).toScVal(), S.nativeToScVal(8000000000n, {type: 'u128'}), S.nativeToScVal(BigInt(q.swap_in), {type: 'u128'}),
    S.nativeToScVal(BigInt(q.min_swap), {type: 'u128'}), S.nativeToScVal(BigInt(q.min_shares), {type: 'u128'}), S.nativeToScVal(BigInt(q.reserve), {type: 'u128'}), S.nativeToScVal(deadline, {type: 'u64'})]);
  const openResult = S.rpc.Api.isSimulationSuccess(open) ? {status: 'UNEXPECTED_SUCCESS'} : {status: 'FAILED_AS_EXPECTED_BEFORE_ACTIVATION', error: String(open.error).split('\n')[0], latestLedger: open.latestLedger};
  const sims = fs.readdirSync(path.dirname(fullConfigFile)).filter(f => /^step20-simulation-/.test(f)).sort();
  const step20 = sims.length ? JSON.parse(fs.readFileSync(path.join(path.dirname(fullConfigFile), sims.at(-1)))).step20 : null;
  const reserveCheck = BigInt(q.reserve) === BigInt(q.min_reserve) + (BigInt(q.min_reserve) * 5000n + 9999n) / 10000n;
  const floor = x => { const n = BigInt(x) * 9980n / 10000n; return n > 0n ? n : 1n; };
  return {status: 'READ_ONLY_SEED_SIMULATION', signing: false, submission: false,
    limitation: 'RPC simulateTransaction cannot model the post-activation state: stage_open traps before activation (no StgV1Config). Figures below are the activation simulation, the fresh quote and the wallet fit; the seed itself is simulated by pilot-ceremony-v3 prepare right after activation is CONFIRMED.',
    ledger: snap.ledger, closeTime: snap.closeTime, account: snap.account,
    activationSimulation: step20 && {ledger: step20.ledger, sequence: step20.sequence, declaredInstructions: step20.declaredInstructionsFromRpcHostPreflight, label: step20.label,
      resourceFeeStroops: step20.resourceFeeStroops, maxFeeStroops: step20.maxFeeStroops, footprint: step20.footprint, diskReadBytes: step20.diskReadBytes, writeBytes: step20.writeBytes},
    quote800: {...q, checks: {validAndAdmissible: q.valid === true && q.admissible === true, minSwapIs20bpsFloor: String(floor(q.swap_out)) === String(q.min_swap),
      minSharesIs20bpsFloor: String(floor(q.shares)) === String(q.min_shares), reserveIsMinPlus50pct: reserveCheck},
      reserveWithMarginPyusd: pyusd(q.reserve), minReservePyusd: pyusd(q.min_reserve), swapInPyusd: pyusd(q.swap_in)},
    wallet: {pyusdRaw: snap.values.ownerFundingBalance, pyusd: pyusd(snap.values.ownerFundingBalance), needRaw: String(8000000000n + BigInt(q.reserve)),
      need: pyusd(8000000000n + BigInt(q.reserve)), headroom: pyusd(BigInt(snap.values.ownerFundingBalance) - 8000000000n - BigInt(q.reserve)), fits: gate.wallet.fits},
    decision: gate.seedPlanDecision, preActivationGate: {ok: gate.ok, findings: gate.findings, commandsPossible: gate.commandsPossible, nextSteps: gate.nextSteps},
    quote400, stageOpenAttempt: openResult,
    reviewStandIn: 'status substituted in a scratch copy for these reads only; the release-folder configs remain UNREVIEWED'};
}
module.exports = {run};
if (require.main === module) {
  const [full, journal, out, ...rest] = process.argv.slice(2), i = rest.indexOf('--rpc'), url = i < 0 ? undefined : rest[i + 1], j = rest.indexOf('--scratch');
  assert(full && journal && out && !fs.existsSync(out) && j >= 0, 'usage: seed-simulation-v3.cjs FULL_CONFIG JOURNAL_DIR NEW_OUTPUT.json --scratch DIR [--rpc URL]');
  run(path.resolve(full), path.resolve(journal), {url, scratch: path.resolve(rest[j + 1])}).then(r => { fs.writeFileSync(out, json(r), {flag: 'wx'});
    console.log(json({ledger: r.ledger, activation: r.activationSimulation, quote800: {valid: r.quote800.valid, admissible: r.quote800.admissible, swap_in: r.quote800.swap_in, min_swap: r.quote800.min_swap, min_shares: r.quote800.min_shares, min_reserve: r.quote800.min_reserve, reserve: r.quote800.reserve, checks: r.quote800.checks},
      wallet: r.wallet, decision: r.decision.available, gate: r.preActivationGate, stageOpenAttempt: r.stageOpenAttempt, quote400: r.quote400}));
  }).catch(e => { console.error(e.message); process.exitCode = 1; });
}
