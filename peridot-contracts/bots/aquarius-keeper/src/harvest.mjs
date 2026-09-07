import { scValToNative, StrKey } from "@stellar/stellar-sdk";

// Reconstruct the settlement balance during the simulated harvest. Its peak
// includes existing idle cash, converted rewards and claimed pool fees, before
// deploy_idle spends them. Failed nested calls must not contribute proceeds.
export function harvestDecision(events, vaultId, underlyingId, idle, minimum) {
  if (!Array.isArray(events) || events.length === 0) {
    throw new Error("harvest simulation did not provide diagnostic events");
  }
  if (typeof idle !== "bigint" || idle < 0n || typeof minimum !== "bigint" || minimum < 10_000n) {
    throw new Error("invalid harvest threshold or idle balance");
  }
  let balance = idle;
  let peak = idle;
  let observed = false;
  const skips = [];
  for (const diagnostic of events) {
    if (!diagnostic.inSuccessfulContractCall()) continue;
    const event = diagnostic.event();
    if (event.type().name !== "contract" || !event.contractId()) continue;
    const emitter = StrKey.encodeContract(event.contractId());
    const body = event.body().v0();
    const topics = body.topics().map(scValToNative);
    if (emitter === vaultId && topics[0] === "harvest_skipped") {
      skips.push(scValToNative(body.data()));
    }
    if (emitter !== underlyingId || topics[0] !== "transfer") continue;
    if (topics[1] !== vaultId && topics[2] !== vaultId) continue;
    const amount = scValToNative(body.data());
    if (typeof amount !== "bigint" || amount < 0n) throw new Error("invalid settlement transfer");
    observed = true;
    if (topics[1] === vaultId) balance -= amount;
    if (topics[2] === vaultId) balance += amount;
    if (balance < 0n) throw new Error("harvest simulation balance mismatch");
    if (balance > peak) peak = balance;
  }
  return { ready: observed && peak >= minimum, peak, minimum, skips };
}
