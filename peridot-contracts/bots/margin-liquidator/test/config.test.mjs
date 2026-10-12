import assert from "node:assert/strict";
import test from "node:test";
import { Keypair } from "@stellar/stellar-sdk";
import { loadConfig } from "../src/config.mjs";

test("live config requires durable DB, hash pin, consistent key and bounded fees", () => {
  const saved = { ...process.env };
  try {
    const key = Keypair.random();
    delete process.env.LIQUIDATOR_PUBLIC_KEY;
    delete process.env.DATABASE_URL;
    delete process.env.EXPECTED_CONTROLLER_WASM_HASH;
    process.env.LIQUIDATOR_SECRET = key.secret();
    process.env.DRY_RUN = "false";
    assert.throws(loadConfig, /requires DATABASE_URL/);
    process.env.DATABASE_URL = "postgres://user:password@host/database";
    process.env.EXPECTED_CONTROLLER_WASM_HASH = "a".repeat(64);
    process.env.MAX_TRANSACTION_FEE_STROOPS = "1000000";
    process.env.MAX_FEE_24H_STROOPS = "10000000";
    assert.equal(loadConfig().maxTransactionFeeStroops, 1000000n);
    process.env.LIQUIDATOR_PUBLIC_KEY = Keypair.random().publicKey();
    assert.throws(loadConfig, /mismatch/);
    delete process.env.LIQUIDATOR_PUBLIC_KEY;
    process.env.MAX_FEE_24H_STROOPS = "999999";
    assert.throws(loadConfig, /below transaction cap/);
    delete process.env.MAX_FEE_24H_STROOPS;
    process.env.MAX_TRANSACTION_FEE_STROOPS = "0";
    assert.throws(loadConfig, /integer between/);
  } finally {
    for (const key of Object.keys(process.env)) if (!(key in saved)) delete process.env[key];
    Object.assign(process.env, saved);
  }
});
