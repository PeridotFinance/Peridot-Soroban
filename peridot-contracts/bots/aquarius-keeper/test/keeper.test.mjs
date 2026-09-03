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
    dryRun: true,
    ...overrides,
  };
}

function logger() {
  return { info() {}, warn() {}, error() {} };
}

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
