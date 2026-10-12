import assert from "node:assert/strict";
import { Address, hash, scValToNative } from "@stellar/stellar-sdk";

export class KeeperSafetyError extends Error {
  constructor(message) { super(message); this.name = "KeeperSafetyError"; this.fatal = true; }
}

export function requireSafe(condition, message) {
  if (!condition) throw new KeeperSafetyError(message);
}

export function assertKeeperCall(config, method, args) {
  const callerMethods = new Map([
    ["begin_liquidation_v3", 2], ["swap_liquidation_v3", 3],
    ["finish_liquidation_v3", 2], ["liquidate_position_v3", 2],
  ]);
  if (callerMethods.has(method)) {
    requireSafe(args.length === callerMethods.get(method), "unexpected keeper arguments");
    requireSafe(scValToNative(args[0]) === config.publicKey, "wrong liquidation beneficiary");
    requireSafe(args[1].switch().name === "scvU64" && scValToNative(args[1]) > 0n, "invalid position ID");
    if (method === "swap_liquidation_v3") {
      requireSafe(args[2].switch().name === "scvU128" && scValToNative(args[2]) > 0n, "invalid swap floor");
    }
  } else if (method === "expire_close_position_v3") {
    requireSafe(args.length === 1 && args[0].switch().name === "scvU64" && scValToNative(args[0]) > 0n,
      "invalid close recovery arguments");
  } else if (method === "distribute_margin_fees") {
    requireSafe(args.length === 1 && config.feeDistributionVaults.includes(scValToNative(args[0])),
      "fee vault not approved");
  } else {
    throw new KeeperSafetyError("method not allowed for keeper signing");
  }
}

export function assertPrepared(config, original, prepared, now = Date.now()) {
  requireSafe(prepared.source === config.publicKey && prepared.sequence === original.sequence,
    "prepared source/sequence changed");
  requireSafe(prepared.signatures.length === 0 && prepared.operations.length === 1, "unexpected envelope shape");
  requireSafe(prepared.memo.type === "none", "unexpected memo");
  const op = prepared.operations[0];
  requireSafe(op.type === "invokeHostFunction" && !op.source, "unexpected operation");
  requireSafe(op.func.switch().name === "hostFunctionTypeInvokeContract", "unexpected host function");
  const call = op.func.invokeContract();
  requireSafe(Address.fromScAddress(call.contractAddress()).toString() === config.controllerId, "wrong controller");
  requireSafe(op.func.toXDR().equals(original.operations[0].func.toXDR()), "prepared call changed");
  requireSafe((op.auth ?? []).every(entry => entry.credentials().switch().name === "sorobanCredentialsSourceAccount"),
    "non-source authorization not allowed");
  requireSafe((op.auth ?? []).every(entry => {
    const root = entry.rootInvocation().function();
    return root.switch().name === "sorobanAuthorizedFunctionTypeContractFn" && root.contractFn().toXDR().equals(call.toXDR());
  }), "unexpected authorization root");
  requireSafe(prepared.toEnvelope().v1().tx().ext().switch() === 1, "Soroban resource data missing");
  requireSafe(JSON.stringify(prepared.timeBounds) === JSON.stringify(original.timeBounds), "prepared time bounds changed");
  const remaining = BigInt(prepared.timeBounds.maxTime) - BigInt(Math.floor(now / 1000));
  requireSafe(remaining >= 15n && remaining <= 120n, "unsafe transaction validity window");
  const fee = BigInt(prepared.fee);
  requireSafe(fee > 0n && fee <= config.maxTransactionFeeStroops, "transaction fee cap exceeded");
  return fee;
}

export function decimalStroops(value) {
  assert.match(value, /^\d+\.\d{7}$/);
  const [whole, fraction] = value.split(".");
  return BigInt(whole) * 10_000_000n + BigInt(fraction);
}

export function assertFeeBalance(config, account, ledger, fee, now = Date.now()) {
  requireSafe(account.account_id === config.publicKey, "wrong fee account");
  const age = now - Date.parse(ledger.closed_at);
  requireSafe(Number.isFinite(age) && age >= -10000 && age <= 60000, "stale fee-account ledger");
  const native = account.balances.find(balance => balance.asset_type === "native");
  requireSafe(Boolean(native), "native fee balance missing");
  const entries = 2n + BigInt(account.subentry_count) + BigInt(account.num_sponsoring ?? 0) - BigInt(account.num_sponsored ?? 0);
  requireSafe(entries >= 0n && BigInt(ledger.base_reserve_in_stroops) > 0n, "invalid account reserve");
  const reserve = entries * BigInt(ledger.base_reserve_in_stroops);
  const available = decimalStroops(native.balance) - decimalStroops(native.selling_liabilities ?? "0.0000000") - reserve;
  requireSafe(available >= fee + config.minFeeBalanceStroops, "insufficient keeper fee balance");
}

export async function verifyController(server, config) {
  requireSafe((await server.getNetwork()).passphrase === config.networkPassphrase, "RPC network mismatch");
  const wasm = await server.getContractWasmByContractId(config.controllerId);
  requireSafe(hash(wasm).toString("hex") === config.expectedControllerHash, "controller code hash mismatch");
}
