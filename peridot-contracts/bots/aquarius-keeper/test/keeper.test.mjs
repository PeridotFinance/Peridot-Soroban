import assert from "node:assert/strict";
import test from "node:test";

import { Keypair } from "@stellar/stellar-sdk";

import { AquariusKeeper } from "../src/keeper.mjs";

function target(label) {
  return {
    label,
    vaultId: `vault-${label}`,
    marketId: `market-${label}`,
  };
}

function config(overrides = {}) {
  return {
    publicKey: Keypair.random().publicKey(),
    targets: [target("XLM"), target("PYUSD"), target("USDC")],
    runHarvest: true,
    harvestOnStart: true,
    harvestIntervalMs: 3_600_000,
    harvestRetryMs: 300_000,
    pollIntervalMs: 1_200_000,
    dryRun: true,
    ...overrides,
  };
}

function logger() {
  return { info() {}, warn() {}, error() {} };
}

test("fallback NAV skips dependent actions without a restart loop and other targets continue", async () => {
  const calls = [];
  const client = { async execute(id, method) {
    calls.push([id, method]);
    if (id === "vault-XLM" && method === "refresh_nav_root") return { deferred: true, reason: "stale_nav" };
  } };
  const keeper = new AquariusKeeper(config({ runHarvest: false }), client, logger());
  assert.equal(await keeper.runCycle(), 0);
  assert.deepEqual(calls, [["vault-XLM", "refresh_nav_root"], ["vault-PYUSD", "refresh_nav_root"],
    ["market-PYUSD", "refresh_boosted_underlying"], ["vault-USDC", "refresh_nav_root"], ["market-USDC", "refresh_boosted_underlying"]]);
});

test("failed NAV refresh stops the target before any dependent signing", async () => {
  const calls = [];
  const client = { async execute(_id, method) { calls.push(method); throw new Error("RPC failure"); } };
  const keeper = new AquariusKeeper(config({ targets: [target("XLM")] }), client, logger());
  assert.equal(await keeper.runCycle(), 1);
  assert.deepEqual(calls, ["refresh_nav_root"]);
});

test("XLM checks hourly while all caches and stable ranges keep twenty-minute cadence", async () => {
  let now = 0;
  const reads = [];
  const writes = [];
  const client = {
    async read(id) { reads.push(id); now += 17; return false; },
    async execute(id, method) { writes.push([id, method]); now += 31; },
  };
  const keeper = new AquariusKeeper(config({ runHarvest: false, runRebalance: true }), client, logger(), () => now);
  for (const time of [0, 1_200_000, 2_400_000, 3_600_000]) {
    now = time;
    assert.equal(await keeper.runCycle(), 0);
  }
  assert.equal(reads.filter(id => id === "vault-XLM").length, 2);
  assert.equal(reads.filter(id => id === "vault-PYUSD").length, 4);
  assert.equal(reads.filter(id => id === "vault-USDC").length, 4);
  assert.equal(writes.length, 24);
  now = 10_800_000; // Missed intervals produce one check, not catch-up calls.
  await keeper.runCycle();
  assert.equal(reads.filter(id => id === "vault-XLM").length, 3);
});

test("XLM read failures retry next cycle but failed transactions wait for next hourly slot", async () => {
  let now = 0;
  let reads = 0;
  let rebalances = 0;
  const client = {
    async read() { if (++reads === 1) throw new Error("RPC unavailable"); return true; },
    async execute(_id, method) {
      if (method === "rebalance") { ++rebalances; throw new Error("guard failed"); }
    },
  };
  const keeper = new AquariusKeeper(config({ targets: [target("XLM")], runHarvest: false, runRebalance: true }), client, logger(), () => now);
  assert.equal(await keeper.runCycle(), 1);
  now = 1_200_000;
  assert.equal(await keeper.runCycle(), 1);
  now = 2_400_000;
  assert.equal(await keeper.runCycle(), 0);
  assert.equal(rebalances, 1);
  now = 3_600_000;
  assert.equal(await keeper.runCycle(), 1);
  assert.equal(rebalances, 2);
});

test("services all targets serially and harvests only when due", async () => {
  let now = 1_000_000;
  const calls = [];
  const client = {
    async execute(contractId, method) {
      calls.push([contractId, method]);
    },
  };
  const keeper = new AquariusKeeper(config(), client, logger(), () => now);

  assert.equal(await keeper.runCycle(), 0);
  assert.deepEqual(
    calls.map(([, method]) => method),
    [
      "refresh_nav_root",
      "harvest",
      "refresh_boosted_underlying",
      "refresh_nav_root",
      "harvest",
      "refresh_boosted_underlying",
      "refresh_nav_root",
      "harvest",
      "refresh_boosted_underlying",
    ],
  );

  calls.length = 0;
  now += 300_000;
  assert.equal(await keeper.runCycle(), 0);
  assert.deepEqual(
    calls.map(([, method]) => method),
    [
      "refresh_nav_root",
      "refresh_boosted_underlying",
      "refresh_nav_root",
      "refresh_boosted_underlying",
      "refresh_nav_root",
      "refresh_boosted_underlying",
    ],
  );
});

test("continues the cycle and schedules a short retry after harvest failure", async () => {
  let now = 1_000_000;
  let harvestAttempts = 0;
  const client = {
    async execute(_contractId, method) {
      if (method === "harvest") {
        harvestAttempts += 1;
        throw new Error("route unavailable");
      }
    },
  };
  const keeper = new AquariusKeeper(
    config({ targets: [target("XLM")] }),
    client,
    logger(),
    () => now,
  );

  assert.equal(await keeper.runCycle(), 1);
  assert.equal(harvestAttempts, 1);
  now += 299_999;
  assert.equal(await keeper.runCycle(), 0);
  assert.equal(harvestAttempts, 1);
  now += 1;
  assert.equal(await keeper.runCycle(), 1);
  assert.equal(harvestAttempts, 2);
});

test("simulates the rebalance check and submits only targets that need it", async () => {
  const calls = [];
  const client = {
    async read(contractId, method) {
      calls.push(["read", contractId, method]);
      return contractId === "vault-PYUSD";
    },
    async execute(contractId, method) {
      calls.push(["execute", contractId, method]);
    },
  };
  const keeper = new AquariusKeeper(
    config({
      targets: [target("XLM"), target("PYUSD")],
      runHarvest: false,
      runRebalance: true,
    }),
    client,
    logger(),
  );

  assert.equal(await keeper.runCycle(), 0);
  assert.deepEqual(calls, [
    ["execute", "vault-XLM", "refresh_nav_root"],
    ["read", "vault-XLM", "needs_rebalance"],
    ["execute", "market-XLM", "refresh_boosted_underlying"],
    ["execute", "vault-PYUSD", "refresh_nav_root"],
    ["read", "vault-PYUSD", "needs_rebalance"],
    ["execute", "vault-PYUSD", "rebalance"],
    ["execute", "market-PYUSD", "refresh_boosted_underlying"],
  ]);
});

test("counts a failed rebalance check and still refreshes the market cache", async () => {
  const methods = [];
  const client = {
    async read() {
      throw new Error("simulation unavailable");
    },
    async execute(_contractId, method) {
      methods.push(method);
    },
  };
  const keeper = new AquariusKeeper(
    config({
      targets: [target("XLM")],
      runHarvest: false,
      runRebalance: true,
    }),
    client,
    logger(),
  );

  assert.equal(await keeper.runCycle(), 1);
  assert.deepEqual(methods, ["refresh_nav_root", "refresh_boosted_underlying"]);
});

test("a threshold deferral is healthy and still refreshes caches and checks ranges", async () => {
  let now = 1_000_000;
  const calls = [];
  const client = {
    async read() { return false; },
    async execute(_id, method) {
      calls.push(method);
      return method === "harvest" ? { deferred: true } : {};
    },
  };
  const keeper = new AquariusKeeper(config({ targets: [target("XLM")], runRebalance: true }), client, logger(), () => now);
  assert.equal(await keeper.runCycle(), 0);
  assert.deepEqual(calls, ["refresh_nav_root", "harvest", "refresh_boosted_underlying"]);
  now += 1_200_000;
  assert.equal(await keeper.runCycle(), 0);
  assert.equal(calls.filter(method => method === "harvest").length, 2);
});
