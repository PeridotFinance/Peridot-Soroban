import assert from "node:assert/strict";
import test from "node:test";
import { nativeToScVal, scValToNative, StrKey } from "@stellar/stellar-sdk";
import { harvestDecision } from "../src/harvest.mjs";
import { StellarClient } from "../src/stellar.mjs";

const vault = StrKey.encodeContract(Buffer.alloc(32, 1));
const token = StrKey.encodeContract(Buffer.alloc(32, 2));
const pool = StrKey.encodeContract(Buffer.alloc(32, 3));
function event(emitter, topics, data, successful = true) {
  return {
    inSuccessfulContractCall: () => successful,
    event: () => ({
      contractId: () => StrKey.decodeContract(emitter),
      type: () => ({ name: "contract" }),
      body: () => ({ v0: () => ({
        topics: () => topics.map(x => nativeToScVal(x, {
          type: StrKey.isValidContract(x) ? "address" : "symbol",
        })),
        data: () => nativeToScVal(data),
      }) }),
    }),
  };
}
const transfer = (amount, incoming = true, successful = true) =>
  event(token, ["transfer", incoming ? pool : vault, incoming ? vault : pool], amount, successful);

test("SDK decodes real Soroban address return values and transfer topics to strings", () => {
  const encoded = nativeToScVal(token, { type: "address" });
  assert.equal(encoded.switch().name, "scvAddress");
  assert.equal(scValToNative(encoded), token);
  const topics = transfer(1n).event().body().v0().topics();
  assert.equal(topics[1].switch().name, "scvAddress");
  assert.equal(scValToNative(topics[2]), vault);
});

test("uses settlement value plus idle cash and accepts the exact threshold", () => {
  assert.equal(harvestDecision([transfer(4_999n)], vault, token, 5_000n, 10_000n).ready, false);
  assert.equal(harvestDecision([transfer(5_000n)], vault, token, 5_000n, 10_000n).ready, true);
});
test("retains pre-deployment peak after rewards are reinvested", () => {
  const result = harvestDecision([transfer(6_000n), transfer(11_000n, false)], vault, token, 5_000n, 10_000n);
  assert.equal(result.ready, true);
  assert.equal(result.peak, 11_000n);
});
test("does not count AQUA amounts, unrelated transfers, or rolled-back proceeds", () => {
  const events = [transfer(50_000n, true, false), event(pool, ["transfer", pool, vault], 1_000_000n), event(token, ["transfer", pool, token], 50_000n)];
  assert.equal(harvestDecision(events, vault, token, 0n, 10_000n).ready, false);
});
test("reports a blocked conversion and defers it without claiming rewards", () => {
  const events = [event(vault, ["harvest_skipped"], { reason: "price_guard", reward_amount: 50_000n, reward_token: pool })];
  const result = harvestDecision(events, vault, token, 9_999n, 10_000n);
  assert.equal(result.ready, false);
  assert.equal(result.skips[0].reason, "price_guard");
});
test("rejects missing evidence and invalid settlement accounting", () => {
  assert.throws(() => harvestDecision([], vault, token, 0n, 10_000n), /diagnostic events/);
  assert.throws(() => harvestDecision([transfer(1n, false)], vault, token, 0n, 10_000n), /balance mismatch/);
  assert.throws(() => harvestDecision([transfer(1n)], vault, token, 0n, 0n), /invalid harvest threshold/);
});
test("live execute defers below threshold before preparation or signing", async () => {
  const client = Object.create(StellarClient.prototype);
  client.navTarget = () => null; // Freshness has its own client regression suite.
  client.config = { dryRun: false, harvestMinUnderlyingRaw: 10_000n };
  client.logger = { info() {}, warn() {} };
  client.read = async (_id, method) => ({ get_underlying: token, get_last_harvest: 0n, get_params: { harvest_cooldown: 3600n }, balance: 100n })[method];
  client.buildTransaction = async () => ({});
  client.server = {
    simulateTransaction: async () => ({ result: {}, events: [transfer(50n)] }),
    prepareTransaction: async () => { throw new Error("must not prepare/sign"); },
  };
  assert.equal((await client.execute(vault, "harvest")).deferred, true);
});

test("on-chain cooldown defers without attempting harvest simulation", async () => {
  const client = Object.create(StellarClient.prototype);
  client.navTarget = () => null;
  client.read = async (_id, method) => method === "get_last_harvest"
    ? BigInt(Math.floor(Date.now() / 1000)) : { harvest_cooldown: 3600n };
  client.buildTransaction = async () => { throw new Error("must not build"); };
  assert.equal((await client.execute(vault, "harvest")).reason, "cooldown");
});
