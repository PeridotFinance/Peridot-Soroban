// Exact native fee headroom, including sponsorship and selling liabilities.
// This is a pre-signing guard, not a replacement for network validation.
function stroops(value) {
  if (typeof value !== "string" || !/^\d+\.\d{7}$/.test(value)) {
    throw new Error("invalid native balance or liability");
  }
  return BigInt(value.replace(".", ""));
}

function count(value) {
  if (!Number.isSafeInteger(value) || value < 0) throw new Error("invalid reserve count");
  return BigInt(value);
}

export function feeCoverage(account, baseReserveStroops, feeStroops) {
  const native = account.balances.filter(b => b.asset_type === "native");
  if (native.length !== 1) throw new Error("missing or duplicate native balance");
  if (!Number.isSafeInteger(baseReserveStroops) || baseReserveStroops <= 0) {
    throw new Error("invalid base reserve");
  }
  if (typeof feeStroops !== "string" || !/^[1-9]\d*$/.test(feeStroops)) {
    throw new Error("invalid prepared transaction fee");
  }
  const entries = 2n + count(account.subentry_count) + count(account.num_sponsoring)
    - count(account.num_sponsored);
  if (entries < 0n) throw new Error("invalid sponsored reserve");
  const reserve = entries * BigInt(baseReserveStroops);
  const available = stroops(native[0].balance) - reserve - stroops(native[0].selling_liabilities);
  const required = BigInt(feeStroops);
  return { available, required, reserve, sufficient: available >= required };
}
