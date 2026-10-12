# Testnet legacy intent recovery

October6: SSH access is restored; the reporter stopped September26 at20:26UTC.
Its final public journal intent `f24bf11b...eca5a` has no submission receipt.
The previous11 intents succeeded. Horizon cannot find the last hash, and the
reporter sequence is still20594093306413067. The generic error did not record
the original cause. Do not claim that missing history proves no signature/send.

## Explicit operator recovery

`src/depth-recover-main.mjs` is separate from the continuous publisher. It accepts
only the reviewed Testnet manifest scope, reporter, legacy hash and confirmed
anchor. It requires `CONFIRM_DEPTH_TESTNET=ISOLATED_DEPTH_REPLAY` plus
`CONFIRM_DEPTH_RECOVERY_HASH` equal to the complete legacy hash. No Mainnet mode.

Stop the service first and verify no duplicate container. Keep the canonical
remote journal at `/var/lib/peridot-depth-testnet/publisher.jsonl`; preserve a
byte-identical backup. Never replace it with the historical laptop copy.

The command verifies network/code/source policy, a fresh closed ledger, the
confirmed anchor, absent original hash, and exact pre-recovery account sequence.
It then records an **unsigned** one-operation bump-sequence transaction in the
same exclusively locked, append-only journal before accessing the reporter key.
The operation advances ONLY the dedicated Testnet reporter sequence to
20594093306413069, making the legacy next-sequence transaction unusable. It sends
no assets and touches no oracle/admin configuration. Maximum fee0.001faucet XLM;
60-second transaction window. Secrets and signed envelopes are never persisted.

After durable recording it repeats the checks before signing. A lost response,
unknown/failed fence, changed account sequence, history outage or mismatch stops
the process with both records intact. Re-running reconciles the exact fence hash
only, never re-signs/rebuilds/resends. An expired, unincluded fence needs a separate
review; there is deliberately no automatic replacement.

Only verified successful inclusion of the exact fence envelope and exact resulting
account sequence permits appending `RETIRED` for the legacy hash. This means
**fenced off**, not successfully executed or an on-chain failed transaction.
The original intent, fence and retirement remain in the journal permanently.
The ordinary publisher refuses to run while an operator fence is unresolved.

This one-time recovery does not infer sequence/expiry for arbitrary legacy hashes.
The known sequence is tied to this specific confirmed anchor and reviewed stopped
reporter run. Future intents now persist source, sequence and maxTime before send;
these fields are evidence, not authorization for automatic timeout recovery.

## Diagnostics and resumption

The publisher emits fixed stages for verify, decision, preparation, pre-sign checks,
journal intent, send and confirmation. The RPC adapter emits `send_rpc` for thrown
transport errors and a decoded protocol result code for an explicit ERROR response.
No arbitrary exception text, XDR, headers or environment is logged. Errors still
stop safely; this does not relax simulation, oracle, freshness or fee guards.

After tests and Almanax review, deploy the reviewed image only to the isolated
Droplet. Run the recovery command using the existing root-only environment file
without exporting its contents. Independently verify the fence hash/account and
journal, then start exactly one reporter. It must collect a NEW30-minute healthy
window; this is not evidence of the original timely lost-response restart test.
Continuous availability, recovery after price moves, economic review and Mainnet
activation remain separately gated.

The recovery is intentionally manual. `Restart=no` and the1MiB journal bound stay
unchanged. Production Mainnet keeper v0.4.3 is not rebuilt or redeployed.
