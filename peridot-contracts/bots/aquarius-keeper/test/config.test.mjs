import assert from "node:assert/strict";
import test from "node:test";

import { Keypair } from "@stellar/stellar-sdk";

import { MAINNET_TARGETS, loadConfig } from "../src/config.mjs";

test("requires a secret outside dry-run mode", () => {
  assert.throws(() => loadConfig({}), /KEEPER_SECRET is required/);
});

test("accepts a public key for dry-run mode", () => {
  const keypair = Keypair.random();
  const config = loadConfig({ DRY_RUN: "true", KEEPER_PUBLIC_KEY: keypair.publicKey() });
  assert.equal(config.publicKey, keypair.publicKey());
  assert.equal(config.keypair, null);
  assert.equal(config.targets, MAINNET_TARGETS);
  assert.equal(config.inclusionFeeStroops, 100_000);
  assert.equal(config.pollIntervalMs, 1_200_000);
  assert.equal(config.harvestIntervalMs, 21_600_000);
  assert.equal(config.maxConsecutiveFailedCycles, 3);
  assert.equal(config.runRebalance, false);
});

test("rejects a public key that does not match the secret", () => {
  const signer = Keypair.random();
  const other = Keypair.random();
  assert.throws(
    () =>
      loadConfig({
        KEEPER_SECRET: signer.secret(),
        KEEPER_PUBLIC_KEY: other.publicKey(),
      }),
    /does not match/,
  );
});

test("rejects unsafe cadence and non-mainnet configuration", () => {
  const keypair = Keypair.random();
  assert.throws(
    () =>
      loadConfig({
        DRY_RUN: "true",
        KEEPER_PUBLIC_KEY: keypair.publicKey(),
        POLL_INTERVAL_MS: "1",
      }),
    /POLL_INTERVAL_MS/,
  );
  assert.throws(
    () =>
      loadConfig({
        DRY_RUN: "true",
        KEEPER_PUBLIC_KEY: keypair.publicKey(),
        NETWORK_PASSPHRASE: "Test SDF Network ; September 2015",
      }),
    /pinned to Stellar Mainnet/,
  );
  assert.throws(
    () =>
      loadConfig({
        DRY_RUN: "true",
        KEEPER_PUBLIC_KEY: keypair.publicKey(),
        RPC_URL: "http://rpc.invalid",
      }),
    /RPC_URL must use HTTPS/,
  );
  assert.throws(
    () =>
      loadConfig({
        DRY_RUN: "true",
        KEEPER_PUBLIC_KEY: keypair.publicKey(),
        HORIZON_URL: "not a URL",
      }),
    /HORIZON_URL must be a valid HTTPS URL/,
  );
});
