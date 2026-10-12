import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import pg from "pg";
import { requireSafe, KeeperSafetyError } from "./safety.mjs";

export function journalScope(config) {
  // Lock the signing account, not just this controller: sequence numbers are shared.
  return createHash("sha256").update(JSON.stringify([config.networkPassphrase, config.publicKey])).digest("hex");
}

export function databaseOptions(connectionString, ca) {
  const url = new URL(connectionString);
  requireSafe(["postgres:", "postgresql:"].includes(url.protocol), "PostgreSQL URL required");
  requireSafe([...url.searchParams.keys()].length === 0, "database URL options are not allowed; use DATABASE_CA_FILE");
  return { connectionString, ssl: { rejectUnauthorized: true, ...(ca ? { ca } : {}) },
    connectionTimeoutMillis: 10000, query_timeout: 10000, statement_timeout: 10000,
    application_name: "peridot-stellar-margin-keeper" };
}

export class PostgresJournal {
  constructor(client, config) {
    this.client = client;
    this.config = config;
    this.scope = journalScope(config);
    this.lockId = BigInt.asIntN(64, BigInt(`0x${this.scope.slice(0, 16)}`)).toString();
    this.healthy = false;
    client.on("error", () => { this.healthy = false; });
    client.on("end", () => { this.healthy = false; });
  }

  async start() {
    await this.client.connect();
    const lock = await this.client.query("SELECT pg_try_advisory_lock($1::bigint) AS locked", [this.lockId]);
    requireSafe(lock.rows[0]?.locked === true, "another keeper holds the signing-account lock");
    this.healthy = true;
    await this.query(`CREATE TABLE IF NOT EXISTS margin_keeper_intents (
      scope text NOT NULL, hash text NOT NULL, controller text NOT NULL,
      sequence numeric(20,0) NOT NULL, max_time numeric(20,0) NOT NULL,
      method text NOT NULL, max_fee numeric(20,0) NOT NULL CHECK (max_fee > 0),
      status text NOT NULL CHECK (status IN ('INTENT','SUCCESS','FAILED')),
      created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
      PRIMARY KEY (scope, hash), UNIQUE (scope, sequence)
    )`);
    return this;
  }

  async query(sql, args) {
    requireSafe(this.healthy, "journal connection lost");
    try { return await this.client.query(sql, args); }
    catch { this.healthy = false; throw new KeeperSafetyError("durable journal operation failed"); }
  }

  async assertHealthy() { await this.query("SELECT 1"); }

  async pending() {
    const { rows } = await this.query(
      "SELECT * FROM margin_keeper_intents WHERE scope=$1 AND status <> 'SUCCESS' ORDER BY sequence", [this.scope]);
    requireSafe(rows.length <= 1, "multiple unresolved keeper intents");
    const row = rows[0];
    if (!row) return null;
    requireSafe(row.controller === this.config.controllerId, "journal controller mismatch");
    requireSafe(row.status === "INTENT", "failed transaction requires operator review");
    requireSafe(/^[a-f0-9]{64}$/.test(row.hash), "invalid journal hash");
    return row;
  }

  async intent(record) {
    requireSafe(/^[a-f0-9]{64}$/.test(record.hash), "invalid intent hash");
    requireSafe(BigInt(record.fee) > 0n && BigInt(record.fee) <= this.config.maxTransactionFeeStroops, "intent fee cap exceeded");
    // Single session owns the advisory lock. Commit the unsigned hash before signing.
    await this.query("BEGIN");
    try {
      requireSafe(!(await this.pending()), "unresolved transaction blocks new signing");
      const { rows } = await this.query(`SELECT COALESCE(SUM(max_fee),0)::text AS reserved
        FROM margin_keeper_intents WHERE scope=$1 AND created_at > clock_timestamp() - interval '24 hours'`, [this.scope]);
      requireSafe(BigInt(rows[0].reserved) + BigInt(record.fee) <= this.config.maxFee24hStroops, "24-hour fee cap exceeded");
      await this.query(`INSERT INTO margin_keeper_intents
        (scope,hash,controller,sequence,max_time,method,max_fee,status)
        VALUES ($1,$2,$3,$4,$5,$6,$7,'INTENT')`,
      [this.scope, record.hash, this.config.controllerId, record.sequence, record.maxTime, record.method, record.fee]);
      await this.query("COMMIT");
    } catch (error) {
      if (this.healthy) await this.query("ROLLBACK");
      throw error;
    }
  }

  async resolve(hash, status) {
    requireSafe(["SUCCESS", "FAILED"].includes(status), "invalid terminal status");
    const result = await this.query(`UPDATE margin_keeper_intents SET status=$3
      WHERE scope=$1 AND hash=$2 AND status='INTENT'`, [this.scope, hash, status]);
    requireSafe(result.rowCount === 1, "journal resolution mismatch");
  }

  async close() { this.healthy = false; await this.client.end(); }
}

export async function openJournal(config) {
  const ca = config.databaseCaPem ?? (config.databaseCaFile ? await readFile(config.databaseCaFile, "utf8") : undefined);
  const journal = new PostgresJournal(new pg.Client(databaseOptions(config.databaseUrl, ca)), config);
  try { return await journal.start(); }
  catch { await journal.close().catch(() => {}); throw new KeeperSafetyError("cannot acquire durable keeper journal"); }
}
