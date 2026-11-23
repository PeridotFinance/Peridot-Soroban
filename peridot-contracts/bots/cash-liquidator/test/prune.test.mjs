import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {pruneZeroDebt} from '../src/plan.mjs';
import {pruneBorrowers} from '../src/keeper.mjs';

test('pruneZeroDebt keeps nonzero, failed or unknown reads; drops all-zero', () => {
  const debts = new Map([['A', [0n, 0n]], ['B', [0n, 5n]], ['C', [0n, null]], ['E', []]]);
  assert.deepEqual(pruneZeroDebt(['A', 'B', 'C', 'D', 'E'], debts), {kept: ['B', 'C', 'D', 'E'], pruned: ['A']});
});

function fakeChain(debt, {marketsFail = new Set(), entered = {}} = {}) {
  const calls = [];
  return {calls, addr: a => a, async view(contract, method, args) {
    calls.push([contract, method, args[0]]);
    if (method === 'get_user_markets') { if (marketsFail.has(args[0])) throw Error('rpc'); return entered[args[0]] || []; }
    if (method === 'get_user_borrow_balance') { const v = debt[`${args[0]}@${contract}`]; if (v === 'fail') throw Error('rpc'); return String(v ?? 0); }
    throw Error('unexpected ' + method);
  }};
}

const cfgFor = dir => ({pruneEveryMs: 3_600_000, stateFile: path.join(dir, 'state.json'), pins: {controller: 'CTRL', markets: {USDC: {receipt: 'RU'}, XLM: {receipt: 'RX'}}}});

test('pruneBorrowers drops merged/repaid accounts, keeps debtors and read failures, checks entered non-pinned markets', async () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'prune-'));
  const cfg = cfgFor(dir);
  const st = {borrowers: ['T1', 'T2', 'D', 'F', 'G', 'P']};
  const c = fakeChain({'D@RX': 7, 'G@RU': 'fail', 'P@RP': 3}, {marketsFail: new Set(['F']), entered: {P: ['RP']}});
  const pruned = await pruneBorrowers(cfg, c, st, 10_000_000);
  assert.deepEqual(pruned, ['T1', 'T2']);
  assert.deepEqual(st.borrowers, ['D', 'F', 'G', 'P']);
  assert.equal(JSON.parse(fs.readFileSync(cfg.stateFile, 'utf8')).borrowers.length, 4);
  assert.ok(c.calls.some(([k, m, u]) => k === 'RP' && u === 'P'));
});

test('pruneBorrowers runs at most once per interval and is off at 0', async () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'prune-'));
  const st = {borrowers: ['T1'], lastPruneMs: 10_000_000};
  assert.deepEqual(await pruneBorrowers(cfgFor(dir), fakeChain({}), st, 10_000_000 + 1_000), []);
  assert.deepEqual(st.borrowers, ['T1']);
  assert.deepEqual(await pruneBorrowers({...cfgFor(dir), pruneEveryMs: 0}, fakeChain({}), {borrowers: ['T1']}, 1), []);
  assert.deepEqual(await pruneBorrowers(cfgFor(dir), fakeChain({}), st, 10_000_000 + 3_600_000), ['T1']);
});
