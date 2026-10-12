import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { promisify } from "node:util";
import test from "node:test";
import { Keypair, Networks, StrKey } from "@stellar/stellar-sdk";
import pg from "pg";
import { PostgresJournal } from "../src/journal.mjs";

const exec = promisify(execFile);
test("real PostgreSQL: durable intent, duplicate worker, rolling cap, restart and failure fences", {
  skip: process.env.PG_INTEGRATION_TEST !== "true", timeout: 60000,
}, async t => {
  const root = await mkdtemp(join(tmpdir(), "peridot-margin-pg-"));
  const data = join(root, "data");
  let started = false;
  const journals = [];
  let admin;
  const config = {
    networkPassphrase: Networks.TESTNET, publicKey: Keypair.random().publicKey(),
    controllerId: StrKey.encodeContract(Buffer.alloc(32, 3)),
    maxTransactionFeeStroops: 100n, maxFee24hStroops: 150n,
  };
  // Isolated local Unix socket only, no TCP listener or cloud/database credentials.
  const options = { host: root, port: 55439, database: "postgres", user: "keeper_test", ssl: false };
  const open = async (cfg = config) => {
    const j = new PostgresJournal(new pg.Client(options), cfg); journals.push(j); return j.start();
  };
  const intent = (n, fee = "100") => ({ hash: n.toString(16).padStart(64, "0"), sequence: String(n),
    maxTime: "1900000000", method: "begin_liquidation_v3", fee });
  try {
    await exec("initdb", ["-D", data, "-U", "keeper_test", "--auth=trust", "--no-locale"]);
    await exec("pg_ctl", ["-D", data, "-l", join(root, "server.log"), "-o", `-h '' -k ${root} -p 55439`, "-w", "start"]);
    started = true;
    admin = new pg.Client(options); await admin.connect();
    let j = await open();
    await t.test("same signing account cannot be run by two workers or controllers", async () => {
      await assert.rejects(open(), /another keeper/);
      await assert.rejects(open({ ...config, controllerId: StrKey.encodeContract(Buffer.alloc(32, 4)) }), /another keeper/);
    });
    await t.test("committed intent survives process connection shutdown", async () => {
      await j.intent(intent(1));
      await j.close();
      j = await open();
      assert.equal((await j.pending()).hash, intent(1).hash);
      await assert.rejects(j.intent(intent(2)), /unresolved/);
      await j.resolve(intent(1).hash, "SUCCESS");
      assert.equal(await j.pending(), null);
    });
    await t.test("rolling fee cap counts full signed maxima across restart", async () => {
      await assert.rejects(j.intent(intent(2)), /24-hour fee cap/);
      assert.equal(await j.pending(), null, "failed reservation must roll back");
      await j.intent(intent(2, "50"));
      await j.resolve(intent(2).hash, "SUCCESS");
      await j.close(); j = await open();
      await assert.rejects(j.intent(intent(3, "1")), /24-hour fee cap/);
    });
    await t.test("wrong resolution cannot mark a different hash complete", async () => {
      await assert.rejects(j.resolve(intent(10).hash, "SUCCESS"), /resolution mismatch/);
    });
    await t.test("only records older than 24 hours leave the rolling fee budget", async () => {
      await admin.query("UPDATE margin_keeper_intents SET created_at=clock_timestamp()-interval '25 hours' WHERE scope=$1", [j.scope]);
      await j.intent(intent(3));
      await j.resolve(intent(3).hash, "FAILED");
      await j.close(); j = await open();
      await assert.rejects(j.pending(), /failed transaction/);
      await assert.rejects(j.intent(intent(4)), /failed transaction/);
    });
    await t.test("lost database connection prevents further use", async () => {
      const separate = await open({ ...config, publicKey: Keypair.random().publicKey() });
      const pid = (await separate.query("SELECT pg_backend_pid() AS pid")).rows[0].pid;
      await admin.query("SELECT pg_terminate_backend($1)", [pid]);
      await new Promise(resolve => setTimeout(resolve, 50));
      await assert.rejects(separate.assertHealthy(), /journal/);
    });
  } finally {
    for (const j of journals) await j.close().catch(() => {});
    await admin?.end().catch(() => {});
    if (started) await exec("pg_ctl", ["-D", data, "-m", "immediate", "-w", "stop"]);
    await rm(root, { recursive: true, force: true });
  }
});
