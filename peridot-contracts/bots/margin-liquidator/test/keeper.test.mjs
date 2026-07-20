import assert from "node:assert/strict";
import test from "node:test";

import { Keypair, nativeToScVal } from "@stellar/stellar-sdk";

import {
  MarginLiquidationKeeper,
  applyLifecycleEvents,
  chooseLiquidationMinOut,
  enumTag,
} from "../src/keeper.mjs";

const symbol = (value) => nativeToScVal(value, { type: "symbol" });
const u64 = (value) => nativeToScVal(BigInt(value), { type: "u64" });

test("enumTag decodes Soroban enum vectors", () => {
  assert.equal(enumTag(["Open"]), "Open");
  assert.equal(enumTag("Liquidated"), "Liquidated");
  assert.equal(enumTag(null), null);
});

test("chooseLiquidationMinOut applies quote slippage without crossing oracle floor", () => {
  assert.equal(
    chooseLiquidationMinOut({ pool_estimated_out: 5_000n, oracle_min_out: 4_465n }, 100),
    4_950n,
  );
  assert.equal(
    chooseLiquidationMinOut({ pool_estimated_out: 4_500n, oracle_min_out: 4_465n }, 100),
    4_465n,
  );
  assert.throws(
    () => chooseLiquidationMinOut({ pool_estimated_out: 4_000n, oracle_min_out: 4_465n }, 100),
    /below oracle floor/,
  );
});

test("applyLifecycleEvents maintains the active position index", () => {
  const events = [
    { topic: [symbol("position_created"), symbol("owner"), u64(2)] },
    { topic: [symbol("position_removed"), symbol("owner"), u64(1)] },
  ];
  assert.deepEqual([...applyLifecycleEvents(new Set(["1"]), events)], ["2"]);
});

test("syncEvents paginates without skipping a full RPC page", async () => {
  const firstPage = Array.from({ length: 100 }, (_, index) => ({
    ledger: 101,
    topic: [symbol("position_created"), symbol("owner"), u64(index + 1)],
  }));
  const calls = [];
  const client = {
    async latestLedger() {
      return { sequence: 105 };
    },
    async getEvents(request) {
      calls.push(request);
      if (calls.length === 1) {
        return { events: firstPage, cursor: "page-1", latestLedger: 105 };
      }
      return {
        events: [
          { ledger: 106, topic: [symbol("position_removed"), symbol("owner"), u64(1)] },
        ],
        cursor: "page-2",
        latestLedger: 106,
      };
    },
  };
  const state = { initialized: true, activePositionIds: [], lastEventLedger: 100 };
  let saves = 0;
  const keeper = new MarginLiquidationKeeper(
    { controllerId: "C".repeat(56) },
    client,
    state,
    async () => {
      saves += 1;
    },
  );

  await keeper.syncEvents();

  assert.equal(calls.length, 2);
  assert.equal(calls[0].startLedger, 101);
  assert.equal(calls[0].endLedger, 105);
  assert.equal(calls[1].cursor, "page-1");
  assert.equal(state.activePositionIds.length, 99);
  assert.equal(state.lastEventLedger, 106);
  assert.equal(saves, 1);
});

test("initialize does not persist a partial position index", async () => {
  const client = {
    async latestLedger() {
      return { sequence: 105 };
    },
    async read(method) {
      assert.equal(method, "get_position");
      throw new Error("temporary RPC failure");
    },
  };
  const state = { initialized: false, activePositionIds: [], lastEventLedger: null };
  let saves = 0;
  const keeper = new MarginLiquidationKeeper(
    { bootstrapMaxPositionId: 1n, eventStartLedger: null },
    client,
    state,
    async () => {
      saves += 1;
    },
    { info() {}, warn() {}, error() {} },
  );

  await assert.rejects(() => keeper.initialize(), /bootstrap incomplete/);
  assert.equal(state.initialized, false);
  assert.deepEqual(state.activePositionIds, []);
  assert.equal(saves, 0);
});

test("keeper executes the complete split liquidation state machine", async () => {
  const liquidator = Keypair.random().publicKey();
  let pendingReads = 0;
  const submissions = [];
  const client = {
    async read(method) {
      if (method === "get_position") return { status: ["Open"] };
      if (method === "get_perps_position") return { side: ["Long"] };
      if (method === "get_health_factor") return 900_000n;
      if (method === "get_pending_liquidation") {
        pendingReads += 1;
        return {
          stage: [pendingReads === 1 ? "Started" : "CollateralConverted"],
          liquidator,
          repay_amount: BigInt(Math.floor(Date.now() / 1_000) + 300),
        };
      }
      if (method === "preview_liquidation_v3") {
        return { pool_estimated_out: 5_000n, oracle_min_out: 4_465n };
      }
      throw new Error(`unexpected read ${method}`);
    },
    async submit(method, args) {
      submissions.push({ method, args });
    },
  };
  const config = {
    publicKey: liquidator,
    dryRun: false,
    slippageBps: 100,
  };
  const state = { activePositionIds: ["7"] };
  const keeper = new MarginLiquidationKeeper(config, client, state, async () => {}, {
    info() {},
    warn() {},
    error() {},
  });

  await keeper.processPosition(7n);

  assert.deepEqual(
    submissions.map(({ method }) => method),
    ["begin_liquidation_v3", "swap_liquidation_v3", "finish_liquidation_v3"],
  );
});
