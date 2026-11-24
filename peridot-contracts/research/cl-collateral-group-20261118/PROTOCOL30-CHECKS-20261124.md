# Protocol 30 (live on Mainnet since 23 Nov 2026): checks before G45 (24 Nov 2026)

Read-only. Nothing signed, sent or deployed. Verdict: **G45 may proceed** (no panic or misprice found).

## 1. Live per-transaction limits

Command (CLI 26.1.0; CLI 30.0.1 returned byte-identical JSON, `--internal` and XDR; the CLI 26 "protocol 30 > 26"
warning is cosmetic):

```
stellar network settings --rpc-url https://soroban-rpc.mainnet.stellar.gateway.fm \
  --network-passphrase "Public Global Stellar Network ; September 2015" --output json-formatted
```

Output (per-tx fields):

```
contract_compute_v0.tx_max_instructions = 400000000
contract_compute_v0.tx_memory_limit = 41943040
contract_ledger_cost_v0.tx_max_disk_read_entries = 200
contract_ledger_cost_v0.tx_max_disk_read_bytes = 200000
contract_ledger_cost_v0.tx_max_write_ledger_entries = 200
contract_ledger_cost_v0.tx_max_write_bytes = 132096
contract_events_v0.tx_max_contract_events_size_bytes = 16384
contract_bandwidth_v0.tx_max_size_bytes = 132096
contract_ledger_cost_ext_v0.tx_max_footprint_entries = 400
```

| Limit | Live | 85 % ceiling |
|---|---|---|
| Instructions | 400 M | 340 M |
| Memory | 41,943,040 B (40 MiB) | 35.65 MB |
| Disk-read entries | 200 | 170 |
| Disk-read bytes | 200,000 | 170,000 |
| Write entries | 200 | 170 |
| Write bytes | 132,096 | 112,281 |
| Footprint entries | 400 | 340 |
| Tx size | 132,096 B | 112,281 B |
| Events size | 16,384 B | 13,926 B |

"400 keys / 400 M" is correct for footprint entries and instructions, but **disk-read and write entries are 200**
(85 % = 170). Internal rule unchanged: 100 entries / 100 M, except the approved margin release (130 keys / 200 M);
everything ≤ 85 % of live.

## 2. CAP-88 (whole-second ledger timestamps may repeat; never go backwards)

CAP text: `closeTimeMs` strictly increases by ≥ 1 ms; `closeTime` = floor(ms/1000) may equal the previous ledger's;
Soroban `ledger_timestamp` returns the whole-second value. Ledger sequence unchanged. Last 200 Mainnet ledgers: all
5 s apart (no repeat observed yet).

**Contracts** (matched to deployed hashes): cash controller f41e5260, cash receipts and group USDC market 5630a701,
group controller 642421f1, CL receipt StgV2 424454b9, CL vault v2 6fd8599a, PYUSD pilot vault e6638030 and receipt
140dcaab, margin (undeployed) at 8b60040 — **all safe**:
- interest accrual uses saturating elapsed time; elapsed 0 accrues 0 and does not advance `LastUpdateTime`;
- reward accrual gated by `dt > 0`; no division by elapsed time anywhere;
- no timestamp used as a storage key (boundaries, segments, seizures, positions are counter-keyed);
- every `now − last` saturating or guarded; future-dated oracle prices return None / `OracleStale` (fail closed);
- 24 h timelocks, cooldowns, 60 s checkpoint, 600 s oracle age, D3 chain behave correctly at elapsed 0.
Residual (not blocking): the vault's 3-ledger investment guard covers less wall-clock time if ledgers get faster.

**Keepers:**
- CL liquidator + heartbeat (3171b5c): safe. `>=`/`<=` comparisons; gate and heartbeat on wall clock; live send path
  uses getAccount / simulate / send / raw-JSON getTransaction only.
- Cash liquidator and pause watcher: safe (raw-JSON latest ledger).
- `clg-ceremony.cjs`: safe because `cash-borrowing-activation-20261117/tools/lib.cjs` overrides `getLatestLedger` with
  a raw call. **Keep that override.**
- Aquarius Mainnet keeper: safe.
- **Cash TTL keeper: broken since protocol 30** — not CAP-88 but XDR: SDK 16.3.x `getLatestLedger()` throws
  `unknown StellarValueType member for value 3`. Hourly job failing since 16:00 UTC 24 Nov. Not blocking: least TTL
  left ~114 days (code entries, ~18 Mar 2027). Fix after go-live: raw `getLatestLedger` at
  `scripts/cash_ttl_keeper.cjs:149`, then run `quote`.
- Aquarius Testnet depth reporter: same XDR issue (Testnet only).
- Optional hardening: override `getLatestLedger` in the CL bot's `makeRpc` too (listed in HB_RPC_METHODS, never called).

## 3. CAP-84 (muxed contract addresses, strkey `W…`; `M…` = muxed accounts from CAP-67)

A muxed address cannot reach any `Address` parameter (the SDK conversion traps before the function body). Only the
cash/CL receipt pToken `transfer(to: MuxedAddress)` accepts M/W; it credits the base address (balances, health,
custody, borrow flag, rewards use `to.address()`) and emits `{amount, to_muxed_id}`. All deposit, withdraw, borrow,
repay, liquidate, enter_market, claim, margin entrypoints take `Address` → M/W rejected. Keepers' event parsing
accepts G/C only. **Affects G45: no.** Frontend: accept only G (and C where intended) as the user account.

## 4. CLI 30.0.1

Installed at `/Users/joshua/Peridot/tools/stellar-cli-30.0.1/bin/stellar` (official arm64 asset, sha256 matches the
GitHub digest, `gh attestation verify` passed). Default `stellar` on PATH stays 26.1.0 for the ceremony.
- Identical on: network settings, `contract fetch` (f41e52…, 6fd859…, 424454…), `contract read`, `contract info meta`,
  simulation (`get_price_usd(USDC)`, `checkpoint_nav` → #2108), `tx decode`.
- Differences: `contract info interface` JSON key `type_` → `type`; `tx simulate` total fee no longer double-counts the
  resource fee (v28 fix; our scripts don't call it); `contract build` embeds `cliver`, so **wasm hashes change** —
  never rebuild pinned/scanned artifacts with 30.
- `tx sign --sign-with-ledger` and `tx send` output unchanged (from source; not device-tested).
- Switch proposal (after go-live): Testnet Ledger rehearsal on 30 (sign, send, upload, build) → update `type_`
  parsers → re-pin/rescan anything rebuilt → switch PATH.

**SDK:** soroban-sdk 30.0.0 (released 24 Nov, needs CLI ≥ 30). Workspace is on 25.3.x. **Margin release builds on the
workspace SDK 25.3.x**; SDK 30 later as a whole-workspace upgrade on its own branch with full tests and a scan.

Agent notes: `.scratch`-equivalent scratchpad (`cap88/`, `cap84/`, `cli30/`, `netcfg/`) of the 24 Nov session.
