import { scValToNative, StrKey } from "@stellar/stellar-sdk";

export const NATIVE = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA";
export const USDC = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75";
export const PYUSD = "CCCRWH6Q3FNP3I2I57BDLM5AFAT7O6OF6GKQOC6SSJNDAVRZ57SPHGU2";
// Independent Stellar-asset feeds quoted in Circle USDC, not stablecoin aliases.
export const FEE_ORACLE = "CALI2BYU2JE6WVRUFYTS6MSBNEHGJ35P4AVCZYF3B6QOE3QKOB2PLE6M";
const SCALE = 100_000_000_000_000n;

// A conservative lower bound: only successful strategy-emitted reward sales.
// Existing idle underlying, paired residue, principal churn and LP fees do NOT
// count. Pair fees alone may therefore defer an otherwise worthwhile harvest.
export function convertedRewards(events, vaultId) {
  if (!Array.isArray(events) || !events.length) throw new Error("missing reward evidence");
  let total = 0n;
  const seen = new Set();
  for (const d of events) {
    if (!d.inSuccessfulContractCall()) continue;
    const e = d.event();
    if (e.type().name !== "contract" || !e.contractId() || StrKey.encodeContract(e.contractId()) !== vaultId) continue;
    const b = e.body().v0();
    if (scValToNative(b.topics()[0]) !== "harvested") continue;
    const v = scValToNative(b.data());
    if (!StrKey.isValidContract(v?.reward_token) || seen.has(v.reward_token) ||
        typeof v.reward_amount !== "bigint" || v.reward_amount <= 0n ||
        typeof v.underlying_out !== "bigint" || v.underlying_out < 0n) {
      throw new Error("invalid reward conversion evidence");
    }
    seen.add(v.reward_token);
    total += v.underlying_out;
  }
  return total;
}

export function freshFeePrice(value, now) {
  if (!Number.isSafeInteger(now) || typeof value?.price !== "bigint" || value.price <= 0n ||
      typeof value.timestamp !== "bigint" || value.timestamp <= 0n ||
      value.timestamp > BigInt(now) || BigInt(now) + 90n - value.timestamp > 600n) {
    throw new Error("fee price unavailable or stale");
  }
  return value.price;
}

export function harvestProfitability(rewards, fee, underlying, prices, now) {
  if (typeof rewards !== "bigint" || rewards < 0n || typeof fee !== "string" || !/^[1-9]\d*$/.test(fee)) {
    throw new Error("invalid harvest economic inputs");
  }
  let valueXlm;
  if (underlying === NATIVE) valueXlm = rewards;
  else {
    if (underlying !== USDC && underlying !== PYUSD) throw new Error("unsupported settlement asset");
    const xlm = freshFeePrice(prices?.xlm, now);
    const asset = underlying === USDC ? SCALE : freshFeePrice(prices?.asset, now);
    if (underlying === PYUSD && (prices.asset.timestamp - prices.xlm.timestamp > 300n ||
        prices.xlm.timestamp - prices.asset.timestamp > 300n)) throw new Error("fee price timestamps diverge");
    valueXlm = rewards * asset / xlm;
  }
  // All supported assets have 7 decimals. Round proceeds down, full maximum
  // prepared fee +25% up, and discount simulated proceeds 5%. No refund assumed.
  const conservativeXlm = valueXlm * 9500n / 10000n;
  const requiredXlm = (BigInt(fee) * 12500n + 9999n) / 10000n;
  return { ready: conservativeXlm > requiredXlm, conservativeXlm, requiredXlm };
}
