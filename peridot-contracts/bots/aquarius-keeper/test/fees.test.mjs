import assert from "node:assert/strict";
import test from "node:test";
import { feeCoverage } from "../src/fees.mjs";

const account = (overrides = {}) => ({
  balances: [{ asset_type: "native", balance: "1.0109868", selling_liabilities: "0.0000000" }],
  subentry_count: 0, num_sponsoring: 0, num_sponsored: 0, ...overrides,
});

test("September26 keeper balance cannot cover any of the three refresh fees", () => {
  for (const fee of ["126929", "126875"]) {
    assert.deepEqual(feeCoverage(account(), 5_000_000, fee), {
      available: 109868n, required: BigInt(fee), reserve: 10000000n, sufficient: false,
    });
  }
});

test("fee boundary is exact to one stroop", () => {
  assert.equal(feeCoverage(account(), 5_000_000, "109868").sufficient, true);
  assert.equal(feeCoverage(account(), 5_000_000, "109869").sufficient, false);
});

test("reserve accounts for subentries, sponsorship and selling liabilities", () => {
  const a = account({ subentry_count: 3, num_sponsoring: 2, num_sponsored: 1,
    balances: [{ asset_type: "native", balance: "10.0000000", selling_liabilities: "6.9900000" }] });
  assert.equal(feeCoverage(a, 5_000_000, "100001").available, 100000n);
  assert.equal(feeCoverage(a, 5_000_000, "100001").sufficient, false);
});

test("malformed inputs and impossible sponsorship never authorize signing", () => {
  for (const a of [account({ balances: [] }), account({ num_sponsored: 3 }),
    account({ num_sponsoring: undefined }), account({ subentry_count: -1 }),
    account({ balances: [{ asset_type: "native", balance: "NaN", selling_liabilities: "0.0000000" }] })]) {
    assert.throws(() => feeCoverage(a, 5_000_000, "100"));
  }
  for (const fee of ["0", "-1", "1.5", undefined, 100]) {
    assert.throws(() => feeCoverage(account(), 5_000_000, fee));
  }
  for (const reserve of [0, -1, NaN, "5000000"]) {
    assert.throws(() => feeCoverage(account(), reserve, "100"));
  }
});
