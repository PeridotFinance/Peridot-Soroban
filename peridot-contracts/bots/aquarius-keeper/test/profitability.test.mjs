import assert from "node:assert/strict";
import test from "node:test";
import { Account, Keypair, Contract, TransactionBuilder, Networks, SorobanDataBuilder, nativeToScVal, StrKey } from "@stellar/stellar-sdk";
import { convertedRewards, harvestProfitability, freshFeePrice, NATIVE, USDC, PYUSD } from "../src/profitability.mjs";
import { StellarClient } from "../src/stellar.mjs";
const vault = StrKey.encodeContract(Buffer.alloc(32, 1));
const reward = StrKey.encodeContract(Buffer.alloc(32, 2));
const other = StrKey.encodeContract(Buffer.alloc(32, 3));
const now = 1_000_000;
const price = (p = 20_000_000_000_000n, timestamp = BigInt(now)) => ({ price: p, timestamp });
function event(emitter, topics, data, success = true) {
  return { inSuccessfulContractCall: () => success, event: () => ({
    type: () => ({ name: "contract" }), contractId: () => StrKey.decodeContract(emitter),
    body: () => ({ v0: () => ({topics: () => topics.map(t => nativeToScVal(t, {type: StrKey.isValidContract(t) ? "address" : "symbol"})), data: () => nativeToScVal(data)}) }),
  }) };
}
const harvested = (n, emitter = vault, success = true) => event(emitter, ["harvested"], {
  reward_token: reward, reward_amount: 1_000_000n, underlying_out: n,
}, success);

test("only successful strategy reward conversions count, not idle, fees or principal transfers", () => {
  assert.equal(convertedRewards([harvested(7n), harvested(900n, other), harvested(900n, vault, false),
    event(NATIVE, ["transfer", other, vault], 999_999_999n), event(vault, ["harvest_skipped"], {})], vault), 7n);
  assert.equal(convertedRewards([event(NATIVE, ["transfer", other, vault], 999_999_999n)], vault), 0n);
  assert.throws(() => convertedRewards([harvested(1n), harvested(1n)], vault), /invalid/);
  assert.throws(() => convertedRewards([harvested(-1n)], vault), /invalid/);
  assert.throws(() => convertedRewards([], vault), /missing/);
});
test("observed September27 uneconomic harvest is deferred despite settlement dust threshold", () => {
  const d = harvestProfitability(7279n, "329646", NATIVE, null, now);
  assert.equal(d.ready, false);
});
test("uses exact conservative integer boundaries without floating point", () => {
  assert.equal(harvestProfitability(100n, "76", NATIVE, null, now).ready, false); //95 ==95
  assert.equal(harvestProfitability(102n, "76", NATIVE, null, now).ready, true);
  assert.equal(harvestProfitability(102n, "77", NATIVE, null, now).ready, false); //96 <ceil96.25
  assert.equal(harvestProfitability(10n**30n, "100000", NATIVE, null, now).ready, true);
  for (const fee of ["0", "-1", "1.1", 100, undefined]) assert.throws(() => harvestProfitability(1n, fee, NATIVE, null, now));
});
test("USDC fee comparison uses oracle base units; PYUSD uses actual feed not dollar parity", () => {
  const p = { xlm: price(), asset: price(50_000_000_000_000n) };
  assert.equal(harvestProfitability(100_000n, "300000", USDC, p, now).ready, true);
  assert.equal(harvestProfitability(100_000n, "300000", PYUSD, p, now).ready, false);
  assert.throws(() => harvestProfitability(100_000n, "300000", other, p, now));
});
test("invalid, missing, future, stale and divergent price samples fail closed", () => {
  for (const p of [null, price(0n), price(-1n), price(1n, BigInt(now+1)), price(1n, BigInt(now-511)), {price: 1, timestamp:BigInt(now)}]) assert.throws(() => freshFeePrice(p, now));
  assert.equal(freshFeePrice(price(1n, BigInt(now-510)), now), 1n);
  assert.throws(() => harvestProfitability(1n, "100", PYUSD, {xlm:price(), asset:price(1n,BigInt(now-301))}, now));
});

function fixture(t, proceeds = 1_000_000n, underlying = NATIVE) {
  t.mock.method(Date, "now", () => now*1000);
  const calls = [];
  const c = Object.create(StellarClient.prototype);
  c.config = { dryRun: false, keypair: Keypair.random(), harvestMinUnderlyingRaw: 10_000n };
  c.logger = { info() {}, warn() {} };
  c.navTarget = () => null;
  c.read = async (_id, method) => ({get_underlying:underlying, get_last_harvest:0n, get_params:{harvest_cooldown:3600n}, balance:0n})[method];
  const tx = new TransactionBuilder(new Account(c.config.keypair.publicKey(), "1"), {networkPassphrase:Networks.PUBLIC,fee:"100000"})
    .addOperation(new Contract(vault).call("harvest")).setTimeout(60).build();
  c.buildTransaction = async () => tx;
  c.harvestPrices = async () => ({xlm:price(),asset:price(100_000_000_000_000n)});
  c.canPayFee = async () => {calls.push("fee"); return true;};
  c.server = {
    simulateTransaction: async () => ({_parsed:true, latestLedger:1, minResourceFee:"200000", transactionData:new SorobanDataBuilder().setResourceFee("200000"), result:{auth:[],retval:nativeToScVal(0n)}, events:[
      // Peak settlement deliberately exceeds the dust gate even if rewards don't.
      event(underlying,["transfer",other,vault],10_000_000n), harvested(proceeds),
    ]}),
    prepareTransaction: async () => { throw new Error("must use exact harvest simulation"); },
    sendTransaction: async p => {calls.push("send");assert.equal(p.signatures.length,1);return {status:"PENDING",hash:"hash"};},
  };
  c.waitForTransaction = async () => ({status:"SUCCESS",ledger:1});
  return {c,calls,tx};
}
test("dust-ready but unprofitable harvest never signs or submits", async t => {
  const {c,calls,tx}=fixture(t,7279n);
  assert.equal((await c.execute(vault,"harvest")).reason,"unprofitable_harvest");
  assert.equal(tx.signatures.length,0);assert.deepEqual(calls,[]);
});
test("profitable harvest signs exact assembled simulation and submits once", async t => {
  const {c,calls}=fixture(t);
  assert.equal((await c.execute(vault,"harvest")).hash,"hash");
  assert.deepEqual(calls,["fee","send"]);
});
test("dry run applies identical economics without signing", async t => {
  const {c,calls}=fixture(t,7279n);c.config.dryRun=true;
  assert.equal((await c.execute(vault,"harvest")).reason,"unprofitable_harvest");assert.deepEqual(calls,[]);
});
test("unavailable fee oracle defers optional harvest without failure or signing", async t => {
  const {c,calls}=fixture(t,1_000_000n,USDC);
  c.harvestPrices=async()=>{throw new Error("offline");};
  assert.equal((await c.execute(vault,"harvest")).reason,"harvest_economics_unavailable");assert.deepEqual(calls,[]);
});
test("oracle expiry during fee probe is checked again immediately before signing", async t => {
  const {c,calls}=fixture(t,1_000_000n,USDC);
  c.harvestPrices=async()=>({xlm:price(20_000_000_000_000n,BigInt(now-509))});
  c.canPayFee=async()=>{t.mock.method(Date,"now",()=> (now+2)*1000);return true;};
  assert.equal((await c.execute(vault,"harvest")).reason,"harvest_economics_unavailable");assert.deepEqual(calls,[]);
});
test("oracle denomination mismatch is rejected", async () => {
  const c=Object.create(StellarClient.prototype);
  c.read=async(_id,m)=>m==="base"?["Other","USD"]:m==="decimals"?14:price();
  await assert.rejects(c.harvestPrices(USDC),/denomination/);
});
