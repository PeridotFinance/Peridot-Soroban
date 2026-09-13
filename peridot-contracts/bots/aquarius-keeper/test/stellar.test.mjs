import assert from "node:assert/strict";
import test from "node:test";
import { navIsFresh, StellarClient, transactionFailure } from "../src/stellar.mjs";

function fixture(t) {
  let now = 1_000_000;
  t.mock.method(Date, "now", () => now * 1000);
  const calls = [];
  const c = Object.create(StellarClient.prototype);
  c.config = { dryRun: false, keypair: {}, targets: [{ vaultId: "vault", marketId: "market" }] };
  c.logger = { info() {}, warn() {} };
  c.read = async (_id, method) => method === "get_params"
    ? { nav_root_max_age: 300n } : 999_980n;
  c.buildTransaction = async () => { calls.push("build"); return {}; };
  c.server = {
    prepareTransaction: async () => { calls.push("prepare"); return { sign() { calls.push("sign"); } }; },
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
