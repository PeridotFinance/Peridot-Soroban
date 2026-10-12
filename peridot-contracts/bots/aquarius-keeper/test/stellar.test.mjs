import assert from "node:assert/strict";
import test from "node:test";
import { xdr } from "@stellar/stellar-sdk";
import { navIsFresh, StellarClient, transactionFailure } from "../src/stellar.mjs";

function fixture(t) {
  let now = 1_000_000;
  t.mock.method(Date, "now", () => now * 1000);
  const calls = [];
  const c = Object.create(StellarClient.prototype);
  c.config = { dryRun: false, keypair: {}, targets: [{ vaultId: "vault", marketId: "market" }] };
  c.logger = { info() {}, warn() {} };
  c.canPayFee = async () => true;
  c.read = async (_id, method) => method === "get_params"
    ? { nav_root_max_age: 300n } : 999_980n;
  c.buildTransaction = async () => { calls.push("build"); return {}; };
  c.server = {
    prepareTransaction: async () => { calls.push("prepare"); return {
      timeBounds: { maxTime: String(Math.floor(Date.now()/1000)+60) },
      sign() { calls.push("sign"); },
    }; },
    sendTransaction: async () => { calls.push("send"); return { status: "PENDING", hash: "hash" }; },
  };
  c.waitForTransaction = async () => { calls.push("confirm"); return { status: "SUCCESS", ledger: 1 }; };
  return { c, calls, advance: seconds => { now += seconds; } };
}

test("NAV freshness includes transaction lifetime and safety margin", () => {
  assert.equal(navIsFresh(1000n, 300n, 1209n), true);
  assert.equal(navIsFresh(1000n, 300n, 1210n), false);
  for (const bad of [0n, -1n, 1201n, undefined, "1000", 1000]) {
    assert.equal(navIsFresh(bad, 300n, 1200n), false);
  }
  for (const bad of [0n, -1n, 90n, undefined, 300]) {
    assert.equal(navIsFresh(1200n, bad, 1200n), false);
  }
});

test("stale NAV blocks all dependent actions before building or signing", async t => {
  const { c, calls } = fixture(t);
  c.read = async (_id, method) => method === "get_params" ? { nav_root_max_age: 300n } : 999_000n;
  for (const [id, method] of [["vault", "harvest"], ["vault", "rebalance"], ["market", "refresh_boosted_underlying"]]) {
    assert.equal((await c.execute(id, method)).reason, "stale_nav");
  }
  assert.deepEqual(calls, []);
});

test("SUCCESS with fallback NAV is deferred even though the transaction confirmed", async t => {
  const { c, calls } = fixture(t);
  c.read = async (_id, method) => method === "get_params" ? { nav_root_max_age: 300n } : 999_000n;
  const r = await c.execute("vault", "refresh_nav_root");
  assert.equal(r.reason, "stale_nav");
  assert.equal(r.hash, "hash");
  assert.equal(calls.filter(x => x === "send").length, 1);
});

test("oracle recovery with a genuinely fresh timestamp permits the next maintenance call", async t => {
  const { c, calls } = fixture(t);
  assert.equal((await c.execute("vault", "refresh_nav_root")).hash, "hash");
  assert.equal((await c.execute("market", "refresh_boosted_underlying")).hash, "hash");
  assert.equal(calls.filter(x => x === "send").length, 2);
});

test("NAV aging during preparation blocks signing", async t => {
  const { c, calls, advance } = fixture(t);
  c.server.prepareTransaction = async () => { advance(190); return { sign() { calls.push("sign"); } }; };
  assert.equal((await c.execute("market", "refresh_boosted_underlying")).reason, "stale_nav");
  assert.deepEqual(calls, ["build"]);
});

test("unconfigured dependent targets fail closed", async t => {
  const { c, calls } = fixture(t);
  await assert.rejects(c.execute("other", "rebalance"), /not a configured/);
  assert.deepEqual(calls, []);
});

test("confirmation timeout never triggers an automatic resubmission", async t => {
  const { c, calls } = fixture(t);
  c.waitForTransaction = async () => { throw new Error("confirmation timed out: hash"); };
  await assert.rejects(c.execute("market", "refresh_boosted_underlying"), /timed out/);
  assert.equal(calls.filter(x => x === "send").length, 1);
});

test("confirmed failures report bounded metadata, not signed SDK objects, and do not retry", async t => {
  const { c, calls } = fixture(t);
  const result = { status: "FAILED", txHash: "hash", ledger: 1, envelopeXdr: { secret: "x".repeat(100_000) }, diagnosticEventsXdr: ["y".repeat(100_000)] };
  assert.equal(transactionFailure(result), '{"status":"FAILED","hash":"hash","ledger":1}');
  c.waitForTransaction = async () => result;
  await assert.rejects(c.execute("market", "refresh_boosted_underlying"), error => error.message.length < 200 && !error.message.includes("envelope"));
  assert.equal(calls.filter(x => x === "send").length, 1);
});

test("submission failures decode errorResult without serializing the envelope", () => {
  const errorResult = new xdr.TransactionResult({
    feeCharged: xdr.Int64.fromString("0"),
    result: xdr.TransactionResultResult.txInsufficientBalance(),
    ext: new xdr.TransactionResultExt(0),
  });
  const summary = JSON.parse(transactionFailure({ status: "ERROR", hash: "h", errorResult,
    envelopeXdr: "never log this" }));
  assert.deepEqual(summary, { status: "ERROR", hash: "h", code: "txInsufficientBalance" });
});

test("insufficient fee balance defers before signing or submission", async t => {
  const { c, calls } = fixture(t);
  c.canPayFee = async () => false;
  assert.equal((await c.execute("vault", "refresh_nav_root")).reason, "insufficient_fee_balance");
  assert.deepEqual(calls, ["build", "prepare"]);
});

test("unavailable fee data fails closed without signing", async t => {
  const { c, calls } = fixture(t);
  c.canPayFee = async () => { throw new Error("fee balance unavailable"); };
  await assert.rejects(c.execute("vault", "refresh_nav_root"), /fee balance unavailable/);
  assert.deepEqual(calls, ["build", "prepare"]);
});

test("NAV is rechecked after a slow balance probe", async t => {
  const { c, calls, advance } = fixture(t);
  c.canPayFee = async () => { advance(190); return true; };
  assert.equal((await c.execute("market", "refresh_boosted_underlying")).reason, "stale_nav");
  assert.deepEqual(calls, ["build", "prepare"]);
});

function feeFixture(t, balance = "1.0109868") {
  const { c, calls } = fixture(t);
  c.config.publicKey = "keeper";
  delete c.canPayFee;
  const account = { account_id: "keeper", balances: [{ asset_type: "native", balance,
    selling_liabilities: "0.0000000" }], subentry_count: 0, num_sponsoring: 0, num_sponsored: 0 };
  const ledger = { closed_at: new Date(Date.now()).toISOString(), base_reserve_in_stroops: 5_000_000 };
  const warnings = [];
  c.logger.warn = (...args) => warnings.push(args);
  c.horizon = {
    loadAccount: async () => account,
    ledgers: () => ({ order: () => ({ limit: () => ({ call: async () => ({ records: [ledger] }) }) }) }),
  };
  c.server.prepareTransaction = async () => ({ fee: "126929",
    timeBounds: { maxTime: String(Math.floor(Date.now()/1000)+60) },
    sign() { calls.push("sign"); },
  });
  return { c, calls, account, ledger, warnings };
}

test("real fee probe prevents the observed failure and automatically recovers after funding", async t => {
  const { c, calls, account, warnings } = feeFixture(t);
  assert.equal((await c.execute("vault", "refresh_nav_root")).reason, "insufficient_fee_balance");
  assert.equal(calls.includes("sign"), false);
  assert.equal(warnings[0][1].availableStroops, "109868");
  account.balances[0].balance = "6.0109868";
  assert.equal((await c.execute("vault", "refresh_nav_root")).hash, "hash");
  assert.equal(calls.filter(x => x === "send").length, 1);
});

test("low spendable balance warns before exhaustion but still permits affordable maintenance", async t => {
  const { c, warnings } = feeFixture(t, "1.9000000");
  assert.equal(await c.canPayFee({ fee: "126929" }), true);
  assert.equal(warnings[0][1].sufficient, true);
});

test("wrong account, stale, future and missing reserve data refuse signing", async t => {
  const { c, calls, account, ledger } = feeFixture(t, "10.0000000");
  account.account_id = "other";
  await assert.rejects(c.execute("vault", "refresh_nav_root"), /account mismatch/);
  account.account_id = "keeper";
  for (const when of [Date.now()-61000, Date.now()+6000, NaN]) {
    ledger.closed_at = Number.isFinite(when) ? new Date(when).toISOString() : "bad";
    await assert.rejects(c.execute("vault", "refresh_nav_root"), /stale or invalid/);
  }
  ledger.closed_at = new Date(Date.now()).toISOString();
  delete ledger.base_reserve_in_stroops;
  await assert.rejects(c.execute("vault", "refresh_nav_root"), /invalid base reserve/);
  assert.equal(calls.includes("sign"), false);
});

test("slow fee probes cannot sign a nearly expired maintenance transaction", async t => {
  const { c, calls, advance } = fixture(t);
  c.canPayFee = async () => { advance(50); return true; };
  assert.equal((await c.execute("vault", "refresh_nav_root")).reason, "transaction_expiring");
  assert.deepEqual(calls, ["build", "prepare"]);
});

test("missing expiry fails closed before signing", async t => {
  const { c, calls } = fixture(t);
  c.server.prepareTransaction = async () => ({ sign() { calls.push("sign"); } });
  await assert.rejects(c.execute("vault", "refresh_nav_root"), /finite expiry/);
  assert.equal(calls.includes("sign"), false);
});
