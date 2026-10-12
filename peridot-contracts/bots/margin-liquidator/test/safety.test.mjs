import assert from "node:assert/strict";
import test from "node:test";
import { Account, Contract, Keypair, Networks, SorobanDataBuilder, StrKey, TransactionBuilder, hash, rpc, xdr } from "@stellar/stellar-sdk";
import { assertFeeBalance, assertKeeperCall, assertPrepared } from "../src/safety.mjs";
import { StellarClient, scAddress, scU64, scU128 } from "../src/stellar.mjs";
import { databaseOptions, journalScope } from "../src/journal.mjs";
import { MarginLiquidationKeeper } from "../src/keeper.mjs";

const key = Keypair.random();
const controllerId = StrKey.encodeContract(Buffer.alloc(32, 2));
const wasm = Buffer.from("test controller");
const config = {
  networkPassphrase: Networks.TESTNET, controllerId, publicKey: key.publicKey(), keypair: key,
  rpcUrl: "https://example.org", horizonUrl: "https://example.org", rpcTimeoutMs: 1000,
  expectedControllerHash: hash(wasm).toString("hex"), maxTransactionFeeStroops: 1_000_000n,
  maxFee24hStroops: 10_000_000n, minFeeBalanceStroops: 20_000_000n, feeDistributionVaults: [controllerId],
};
const args = () => [scAddress(config.publicKey), scU64(1)];
const logger = { info() {}, warn() {}, error() {} };

function transaction(method = "begin_liquidation_v3", callArgs = args()) {
  return new TransactionBuilder(new Account(config.publicKey, "100"), { fee: "100", networkPassphrase: config.networkPassphrase })
    .addOperation(new Contract(controllerId).call(method, ...callArgs)).setTimeout(60).build();
}

function simulation(fee = 20000) {
  return { id: "1", latestLedger: 100, events: [], _parsed: true, minResourceFee: String(fee),
    transactionData: new SorobanDataBuilder().setResourceFee(fee),
    result: { auth: [], retval: xdr.ScVal.scvVoid() } };
}

function fixture(options = {}) {
  const calls = [];
  let pending = options.pending ?? null;
  let submitted;
  const journal = {
    async assertHealthy() { calls.push("journal-check"); if (pending && options.lockLost) throw new Error("lock lost"); },
    async pending() { return pending; },
    async intent(record) { calls.push("intent"); if (options.journalFails) throw Object.assign(new Error("database failed"), { fatal: true }); pending = record; },
    async resolve(h, status) { calls.push(status); assert.equal(h, pending.hash); if (status === "SUCCESS") pending = null; },
  };
  const c = new StellarClient({ ...config, dryRun: false, ...options.config }, logger, journal);
  c.buildTransaction = async (method, a) => transaction(method, a);
  c.server = {
    async getNetwork() { return { passphrase: options.network ?? Networks.TESTNET }; },
    async getContractWasmByContractId() { return options.wasm ?? wasm; },
    async simulateTransaction() { calls.push("simulate"); return options.simulation ?? simulation(); },
    async sendTransaction(tx) {
      calls.push("send"); submitted = tx;
      assert.ok(pending, "durable intent must precede send");
      assert.equal(tx.signatures.length, 1);
      if (options.lostResponse) throw new Error("connection lost");
      return { hash: tx.hash().toString("hex"), status: options.sendStatus ?? "PENDING" };
    },
    async getTransaction() { return options.lookup ?? { status: "NOT_FOUND" }; },
  };
  c.horizon = {
    async loadAccount() { return { account_id: config.publicKey, sequence: options.sequence ?? "100", subentry_count: 0,
      balances: [{ asset_type: "native", balance: options.balance ?? "10.0000000" }] }; },
    ledgers() { return { order() { return this; }, limit() { return this; }, async call() {
      return { records: [{ closed_at: new Date().toISOString(), base_reserve_in_stroops: 5_000_000 }] };
    } }; },
    transactions() { return { transaction() { return this; }, async call() {
      if (options.history) return options.history;
      throw new Error("404");
    } }; },
  };
  c.waitForTransaction = async () => {
    if (options.timeout) throw new Error("timed out");
    return { status: options.status ?? "SUCCESS", envelopeXdr: submitted.toEnvelope(), ledger: 101 };
  };
  return { c, calls, journal, pending: () => pending };
}

test("allowed keeper calls constrain method, position, beneficiary and fee vault", () => {
  for (const m of ["begin_liquidation_v3", "finish_liquidation_v3", "liquidate_position_v3"]) assertKeeperCall(config, m, args());
  assertKeeperCall(config, "swap_liquidation_v3", [...args(), scU128(1)]);
  assertKeeperCall(config, "expire_close_position_v3", [scU64(1)]);
  assertKeeperCall(config, "distribute_margin_fees", [scAddress(controllerId)]);
  assert.throws(() => assertKeeperCall(config, "upgrade_wasm", []), /not allowed/);
  assert.throws(() => assertKeeperCall(config, "begin_liquidation_v3", [scAddress(Keypair.random().publicKey()), scU64(1)]), /beneficiary/);
  assert.throws(() => assertKeeperCall(config, "begin_liquidation_v3", [scAddress(config.publicKey), scU64(0)]), /position/);
  assert.throws(() => assertKeeperCall(config, "distribute_margin_fees", [scAddress(Keypair.random().publicKey())]), /not approved/);
  assert.throws(() => assertKeeperCall(config, "swap_liquidation_v3", [...args(), scU128(0)]), /floor/);
});

test("prepared transaction validates real SDK resource fees, body and expiry", () => {
  const tx = transaction();
  const prepared = rpc.assembleTransaction(tx, simulation()).build();
  assert.equal(assertPrepared(config, tx, prepared), 20100n);
  assert.throws(() => assertPrepared({ ...config, maxTransactionFeeStroops: 20000n }, tx, prepared), /fee cap/);
  assert.throws(() => assertPrepared(config, transaction("finish_liquidation_v3"), prepared), /call changed/);
  assert.throws(() => assertPrepared(config, tx, tx), /resource data/);
  assert.throws(() => assertPrepared(config, tx, prepared, Date.now() + 50000), /validity/);
  prepared.sign(key);
  assert.throws(() => assertPrepared(config, tx, prepared), /shape/);
});

test("native fee balance accounts for reserve, sponsorship and selling liabilities", () => {
  const account = { account_id: config.publicKey, subentry_count: 2, num_sponsoring: 2, num_sponsored: 1,
    balances: [{ asset_type: "native", balance: "5.0000000", selling_liabilities: "0.4000000" }] };
  const ledger = { closed_at: new Date().toISOString(), base_reserve_in_stroops: 5000000 };
  assertFeeBalance(config, account, ledger, 1000000n);
  assert.throws(() => assertFeeBalance(config, account, ledger, 1000001n), /insufficient/);
  assert.throws(() => assertFeeBalance(config, account, { ...ledger, closed_at: "2000-01-01T00:00:00Z" }, 1n), /stale/);
});

test("successful submission records intent before signing and confirms exact envelope", async () => {
  const f = fixture();
  const result = await f.c.submit("begin_liquidation_v3", args());
  assert.equal(result.status, "SUCCESS");
  assert.ok(f.calls.indexOf("intent") < f.calls.indexOf("send"));
  assert.equal(f.pending(), null);
});

test("prepared source-auth roots cannot authorize a different operation", () => {
  const tx = transaction();
  const sim = simulation();
  sim.result.auth = [new xdr.SorobanAuthorizationEntry({
    credentials: xdr.SorobanCredentials.sorobanCredentialsSourceAccount(),
    rootInvocation: new xdr.SorobanAuthorizedInvocation({
      function: xdr.SorobanAuthorizedFunction.sorobanAuthorizedFunctionTypeContractFn(
        xdr.InvokeContractArgs.fromXDR(tx.operations[0].func.invokeContract().toXDR())),
      subInvocations: [],
    }),
  })];
  assertPrepared(config, tx, rpc.assembleTransaction(tx, sim).build());
  sim.result.auth[0].rootInvocation().function().contractFn().functionName("other");
  assert.throws(() => assertPrepared(config, tx, rpc.assembleTransaction(tx, sim).build()), /authorization root/);
});

test("loss of journal connection after intent prevents signing and send", async () => {
  const f = fixture({ lockLost: true });
  await assert.rejects(f.c.submit("begin_liquidation_v3", args()), /signing stopped/);
  assert.ok(f.pending());
  assert.ok(!f.calls.includes("send"));
});

test("fee cap includes refundable fee padding and inclusion fee", async () => {
  const f = fixture({ config: { maxTransactionFeeStroops: 35100n } });
  await f.c.submit("begin_liquidation_v3", args());
  const tooSmall = fixture({ config: { maxTransactionFeeStroops: 35099n } });
  await assert.rejects(tooSmall.c.submit("begin_liquidation_v3", args()), /fee cap/);
  assert.ok(!tooSmall.calls.includes("intent") && !tooSmall.calls.includes("send"));
});

for (const [name, options] of Object.entries({
  wrongNetwork: { network: Networks.PUBLIC }, wrongCode: { wasm: Buffer.from("changed") },
  oversizedFee: { simulation: simulation(1_000_001) }, noBalance: { balance: "1.0000000" },
  journalFailure: { journalFails: true }, restoration: { simulation: { ...simulation(), restorePreamble: {} } },
  sequenceChanged: { sequence: "101" },
})) test(`${name} blocks signing and submission`, async () => {
  const f = fixture(options);
  await assert.rejects(f.c.submit("begin_liquidation_v3", args()));
  assert.ok(!f.calls.includes("send"));
});

for (const [name, options] of Object.entries({ lostResponse: { lostResponse: true },
  timeout: { timeout: true }, rejected: { sendStatus: "ERROR" }, retryLater: { sendStatus: "TRY_AGAIN_LATER" },
  chainFailure: { status: "FAILED" },
})) test(`${name} retains intent and prohibits another send`, async () => {
  const f = fixture(options);
  await assert.rejects(f.c.submit("begin_liquidation_v3", args()), /signing stopped/);
  assert.ok(f.pending());
  await assert.rejects(f.c.submit("finish_liquidation_v3", args()), /stopped/);
  assert.equal(f.calls.filter(c => c === "send").length, 1);
});

test("restart reconciles confirmed intent without sending again", async () => {
  const tx = rpc.assembleTransaction(transaction(), simulation()).build();
  const pending = { hash: tx.hash().toString("hex"), sequence: tx.sequence };
  const f = fixture({ pending, lookup: { status: "SUCCESS", envelopeXdr: tx.toEnvelope() } });
  assert.equal(await f.c.reconcile(), true);
  assert.equal(f.pending(), null);
  assert.ok(!f.calls.includes("send"));
});

test("RPC history miss can reconcile from verified Horizon envelope", async () => {
  const tx = rpc.assembleTransaction(transaction(), simulation()).build();
  const pending = { hash: tx.hash().toString("hex"), sequence: tx.sequence };
  const f = fixture({ pending, history: { hash: pending.hash, successful: true, envelope_xdr: tx.toXDR() } });
  assert.equal(await f.c.reconcile(), true);
  assert.equal(f.pending(), null);
});

test("missing/expired history is never treated as permission to retry", async () => {
  const f = fixture({ pending: { hash: "a".repeat(64), sequence: "101", max_time: "1" } });
  await assert.rejects(f.c.reconcile(), /operator review/);
  assert.ok(f.pending());
  assert.ok(!f.calls.includes("simulate") && !f.calls.includes("send"));
});

test("wrong confirmation envelope does not clear the journal", async () => {
  const f = fixture({ pending: { hash: "a".repeat(64), sequence: "101" },
    lookup: { status: "SUCCESS", envelopeXdr: transaction().toEnvelope() } });
  await assert.rejects(f.c.reconcile(), /operator review/);
  assert.ok(f.pending());
});

test("dry-run submission never needs a journal or signs", async () => {
  const f = fixture({ config: { dryRun: true, keypair: null } });
  f.c.journal = null;
  assert.equal((await f.c.submit("begin_liquidation_v3", args())).dryRun, true);
  assert.deepEqual(f.calls, ["simulate"]);
});

test("database options enforce TLS verification and reject connection overrides", () => {
  assert.equal(databaseOptions("postgres://user:password@host/db").ssl.rejectUnauthorized, true);
  assert.equal(databaseOptions("postgres://user:password@host/db", "certificate").ssl.ca, "certificate");
  assert.throws(() => databaseOptions("postgres://host/db?sslmode=no-verify"));
  assert.equal(journalScope(config), journalScope({ ...config, controllerId: "other" }));
  assert.notEqual(journalScope(config), journalScope({ ...config, networkPassphrase: Networks.PUBLIC }));
});

test("fatal signing error propagates out of per-position catch", async () => {
  const keeper = new MarginLiquidationKeeper({ maxPositionsPerCycle: 2 }, {},
    { activePositionIds: ["1", "2"] }, async () => {}, logger);
  keeper.initialize = async () => {};
  keeper.syncEvents = async () => {};
  let count = 0;
  keeper.processPosition = async () => { count++; throw Object.assign(new Error("stop"), { fatal: true }); };
  await assert.rejects(keeper.runCycle(), /stop/);
  assert.equal(count, 1);
});
