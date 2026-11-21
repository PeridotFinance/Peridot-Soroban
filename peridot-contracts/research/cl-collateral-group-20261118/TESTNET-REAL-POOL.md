# CL collateral group: Testnet on a real concentrated pool (21 Nov 2026, stream A)

Everything below ran on Testnet only, with Testnet-only keys (`clg-admin` plus new `clgr-*` identities). Nothing was signed,
sent, deployed or upgraded on Mainnet. Key material was never read: the CLI signs, and the JS tools only build and simulate.

This report closes the BUILD-REPORT §4 item 6 gap (stand-in pool, cash-backed units, CF-cut shortfall, dry-run bot).

## 0. Results

| # | Item | Result |
|---|---|---|
| 1 | Units backed by a **real concentrated position** | **PASS**. A fresh instance of the Aquarius concentrated-pool code `155a17b9…` (§1). The vault holds liquidity in [−20, 20] and the pool's own snapshot shows the range. |
| 2 | Price-driven shortfall (pool and oracle moved, not CF), NAV-path L1a → L3 → L4 → L5 | **PASS** on R1 (18 Nov candidates) and on **R2 (current release candidates)**. The bot did both. The L4 AQUA claim was real and non-zero, and the beneficiary moved. |
| 3 | Real-time D3 fallback liquidation (≥ 10 min continuously blocked) | **PASS** on R1 (bot) and R2 (manual liquidator). L1b = 97.94 % of NAV. |
| 4 | Bot live soak with `watch` heartbeat | **Defect found and fixed.** Old cadence 25–125 s broke the chain 7× in 20 min. On the **final code `5f48b0d`**, over a 31.9 min out-of-range window, both bots measured **p95 15 s / max 15 s** (0 gaps > 20 s). A heartbeat-only process with a **dead primary RPC** also held max 15 s. RPC and host outages are in §4. |
| 5 | External USDC feed (CAFJ analogue) | **PASS**. The controller priced USDC from the **real Testnet Reflector external feed** `CCYOZJ…RN63` `Other(USDC)` (base USD, 14 dp, 300 s). L2 cached 1.00180 and 1.00166. |
| 6 | Rows vs the 100-entry internal limit, stop at 85 | **STOP: the position-backed in-kind `stage_withdraw` is 91 entries (60 + 31) and 91.3M CPU** (1911b142…). Reported to the lead. Otherwise no measured stage is above 85. The maximum is the emergency exit at **84** (58 + 26), which does not supersede the 89 on the Mainnet capture. L3 = 77, borrow = 71, open-mint = 75. `prepare_close` with debt = 77. Fee-on L3 was not run on the real pool. See §5. |
| 7 | Upgrade path (coordinator add-on) | **Proposed on all three R2 contracts.** Cancel/re-propose and every before-ETA refusal pass. **Execute is pending the ETA, 2026-11-22 ≈ 12:44 UTC** (§6). |
| 8 | Real auth, hashes per stage (add-on) | Every tx was signed, with no `mock_all_auths` (§3). |
| 9 | Trustline retry (add-on) | **PASS**. Pay without a PYUSD trustline fails with SAC #13, and the command stays HELD (phase 21). After the trustline is added, the pay succeeds (phase 22, exact amounts). |

**No functional contract defect was found. There is one resource STOP:** the position-backed in-kind `stage_withdraw` measured 91 entries and 91.3M CPU (§7.0). The other defects are operational (bot, fees), §7.

## 1. Venue and stacks

**Pool.** No existing Testnet pool was usable: a usable one needs PYUSD as token0, a price near par inside tick [−20, 20), and our control of the tokens and the oracle.

The Aquarius Testnet router `CBCFTQSP…6GD` reports `ConcentratedPoolHash = 155a17b9929f…2907`. That is the Mainnet PYUSD/USDC pool code from the 13 Nov capture.

We deployed a **fresh instance of that on-chain code** (no upload) and initialised it with `initialize_all`, with `clg-admin` in every role. This lets us run `set_rewards_config` and so get a real non-zero AQUA claim; a router-created pool's rewards belong to the router admin. The other venue pieces:
- Fresh instances of the Aquarius Testnet **plane** (`421b1b85…`), **config storage** (`758bdb96…`) and **boost feed** (`2c36ae24…`) code.
- Config storage was given `VaultWASM = 803527d4…` and `GaugeWASM = 802127e3…`, copied from the Aquarius Testnet config. Without them, `initialize_all` fails with `#2137 VaultWASMNotStaged`.
- Fee 10, spacing 20.
- Admin liquidity [−1000, 1000] 20k/20k plus [−100, 100] 200k/200k. Active L is about 4.1e14; Mainnet is 2.09e15.
- Reward vault funded with 2,000 tAQUA; tps 20,000 (0.002 AQUA/s).

Tokens are **classic-asset SACs**, as on Mainnet, issued by `clgr-iss`. PYUSD `CDHARH6F…FO6D` (token0), USDC `CD4D24BK…BUYN`, AQUA `CAMJF574…JIQE`.

**Feeds.**
- PYUSD: the Reflector-ABI fixture `CAKSIO7I…45Z5` (base `Stellar(USDC)`, as CALI). It must be movable, and it reads 2 keys, like CALI.
- USDC (controller): the real Testnet Reflector external feed `CCYOZJCO…RN63` via `set_oracle` and `set_oracle_asset_symbol(USDC, "USDC")`. This is the CAFJ path.

| | R1 (18 Nov candidates) | R2 (current release candidates) |
|---|---|---|
| Bytes | `target/clg/art/testnet` (f52547d era, Testnet admin): vault 18e70ceb, receipt 4f8b78b7, ctrl cbd54f43 | `testnet-real/build-testnet.sh` = `build/canonical-build.sh` flags at **0eefd81** (includes 6e83fd7 upgradeability, 137de32 F1, F2), Testnet admin, receipt with `stg-mainnet-accepted,stg2-mainnet-accepted`: vault e6d7cbd1 125,615 B, receipt f2a5352f 125,585 B, ctrl 2e5b4f44 64,957 B |
| Vault | CCVEBG3M…KMWO | CDN32USY…LNZEG |
| Receipt | CCDLIRMI…4ELI | CA64PYSL…GX3W |
| Controller | CCWDJJLS…2VD6 | CBP3O3GF…SX3I7 |
| USDC market | CCCQZ6CW…FXTQ | CDGITXDO…DNTWT |

- Both stacks share the pool, tokens and feeds.
- The USDC market on both is the Testnet build of the accepted cash market (eb4c912a). Mainnet uses the on-chain 5630a701.
- Receipt and controller lengths match the release pins 424454b9/642421f1 exactly. The vault is 56 B larger than the 6fd8599a pin, which is exactly one G-strkey. The Testnet build contains both the Testnet admin and the pinned-Ledger literal `GC4T5MP5…7FP` (one occurrence each). The Mainnet build contains only the Ledger, since the two literals are equal there and dedupe. Not a discrepancy.

**Positions.** Each owner opened 1,000 PYUSD through `stage_open → convert → mint → 3 keeper observations on consecutive ledgers → stage_invest → finalise`:
- The observations are sent by `tn.mjs invest`. Simulation runs at the latest ledger, so one simulated footprint is reused with the next sequence.
- `stage_invest` is sent within 3 ledgers.
- Vault liquidity on R1 is 9.893e12 in [−20, 20].
- Each owner holds about 9.995e9 units. CF 50 %. Borrows were 499.0 USDC (99.86 % of the limit) and 493.7 USDC (98.8 %).

**Two notes from the open journey (not defects):**
- The convert swap of a 500-PYUSD open moved the thin wide position to tick −25, and the invest band refused. Depth near par was added.
- `stage_quote_open` returns `valid:false` when the post-swap price would cross the tick-0 boundary (pool exactly at par). The old `02-open.sh` ignored `valid` and opened with reserve 0, so investment then failed. That attempt was cancelled and withdrawn. The new script refuses an invalid quote.

## 2. Liquidations

**Price-driven shortfall.**
- PYUSD feed 0.9998 → 0.9945 (−53 bps, inside the 100 bps depeg guard).
- Pool swapped to tick −15 (spot 0.99850, 40 bps from the feed, inside the 50 bps band, tick inside [−20, 20)).
- NAV rate fell 999,807 → 995,792. BOR1's shortfall was 13.16 USDC.

**D3.**
- The pool was swapped to tick −31, out of range. `checkpoint_nav` returns 0 and extends the chain.
- Fallback refusals were observed live: `#2104` with the pool in band, `#2111` with no chain, `#2112` before 10:00, and `#2113` after a lapse.

| Path | Seized for 100 USDC repaid | Notes |
|---|---|---|
| R1 NAV (bot) | 1,084,563,843 units | Beneficiary moved BOR1 → liquidator. Vault reward totals: received 93,222 raw AQUA by L4. Liquidator pending was 0 at seizure and accrued 3,104 afterwards. |
| R1 D3 fallback (bot, after 10 min) | 1,107,378,832 units | L1b rate 975,276 (97.94 % of NAV 995,792) |
| R2 NAV (bot) | as R1 | |
| R2 D3 fallback (`clgr-liq3`, manual) | 1,107,406,084 units | L1b 975,252; the liquidator had no PYUSD trustline (§3) |

L5 returned `false` each time: no D6 boundary opened, because L4 ran within 60 s.

## 3. Real auth, hashes per stage, trustline retry

All transactions are signed Testnet transactions:
- `require_auth` on the borrower, liquidator, owner and admin is satisfied by real signatures (source-account credentials).
- The USDC repayment uses the cash market's `transfer_from` under a signed `approve`.
- The open pulls PYUSD with the owner's signed auth tree.
- No `mock_all_auths` anywhere.

| Stage | R1 NAV (bot) | R1 D3 (bot) | R2 NAV (bot) | R2 D3 (liq3) |
|---|---|---|---|---|
| APPROVE | 7ede5c6b996ba9f7ee281023fa8aaa0dc72f480c68cc8b91bc233e4c67e5ba60 | 88a1ab4083ef3ea0e67c68cb13d61bf2738c3741957d6624b644eb7a568fc46b | 577f94027f772fc1ad34ed220a2253e5c37e66f19ac0f6a3b398368a7353a620 | 8ccca8f614afe91e98c0bc616c0783e5b32b9654de4d2f44fcc233910d3adc91 |
| L1 | L1a a6fb2fa80d66539f8c47191781fa5ce8967b98a581b1684138ace49219fc1ae5 | L1b 3c9e0dcef6ced5a4d3d9c2ee1bed1f2eccbee8f73265182a156357a59b5b560d | L1a 4d2b6046b4f41011bc53032dab7edd95d073ed5a732b9ce3d6d6c6e00332a0b0 | L1b 029ad575af3d842e939dea6ada2806b78527d95a942f99530a7ee7ce2421ba0d |
| L2 | (cache still valid) | eb0057a08d9777b4380e3f1ab35c314880d5144c68426641e3cb632e917c7a27 | (cache still valid) | 1cd884a0b5732ce97b0e28ba3edbb92768d7e2b72a293b8ad8e18549f568bd5d |
| L3 | 1b6300ba8b5f17305373c0bbaaaa2bd12b339030634a64d79d3b157287cc0c6b | 7da889b4871bf55a52f36a5675b5cc7439bb8e6558675a7606a05f9007eb61df | 8df2b104fdd0db4d2b4900d2cd2deee9a400a886d17505743aa4fd60f63dcb98 | b9139632ceecaf70c7bf19f19b13b5785d536bc832f30e201cd247d4d6362ed9 |
| L4 | 2ea38a52c044460172f51d0abc59a28785a4c33a9afbf6963c1c86b5da851749 | 1c2d760aea57d247098b079cc6f2cd59370806cb8efd90d08f4655fd333c8ec3 | f1603a005c90503294a41d7d70ccc63a4d389af32a802608777c07f526beaca9 | 5f01c6f76bdbb5bd7812e507e23eaa074f4e541c29dea258cf5a8777375c5285 |
| L5 | 91c4881b1d66aad4d75fdb7e4f54ad7a25ecc018605a28ebcb9238b706c88c5e | 0f9a2e82fd18ab1e9ed624d66ac5afb5c683fa0ba89c2eeb7cc0b655ec5c1c67 | 80859d53d8cff401100a76c4c74047251201e41d08f0735bebfbcca5191b8921 | d4691d3dbf24865dab9151b3de799ee8048b5749d1b4b11ec5b55eaa33205f07 |

**Trustline retry** (R2, `06-manual-liq-trustline.sh`). `clgr-liq3` GCEY6NX5… held a USDC trustline only.
1. `stage_emergency_exit` on all 1,107,406,084 seized units, with waiver: c4f54c6329e129596f5ff39596581b8002913d3d45cad1e1975c4d62e69d0f9d. This gave job 2, phase 21 HELD, a = 1,097,821,037 PYUSD, b = 10,533,855 USDC.
2. `stage_emergency_pay(2)` reverted with **`Error(Contract, #13)`** (SAC trustline missing). It was simulated because the CLI does not broadcast a failing simulation; a reverted tx changes no state either way. **Job still phase 21 HELD, amounts unchanged.**
3. PYUSD trustline added: 42feacf2d51d8c3d351d726ccb7eb9e8fd662a1f5e99d10f480a183e3b945d26.
4. `stage_emergency_pay(2)`: 5b64f7ce32774116a8de1f6ccc4104ea6f2b57147e940964a6626d6f267eab49. **Phase 22 PAID.** The liquidator received exactly 1,097,821,037 PYUSD (0 → 1,097,821,037) and 10,533,855 USDC (9,000,000,000 → 9,010,533,855).

## 4. Bot soak and the D3 heartbeat

**Defect (bot):** the plan-loop heartbeat sends L1a only when the last one is ≥ 15 s old, a plan cycle takes about 12 s, and a CLI send takes 7 s or more.
- On-chain gaps over 08:17–08:37 (26 gaps): p50 30 s, p95 105 s, max 125 s, with 7 gaps over 60 s. The chain therefore restarted repeatedly and **the fallback was never admitted in 20 min**.
- Testnet ledgers were 5 s and RPC reads were fast. The slowness was in the CLI sends.

**Fix.** `bots/cl-liquidator/src/heartbeat.mjs` (final 5f48b0d):
- A fixed 12 s timer from the last submit, independent of the plan loop.
- Its own fee-only account, so there are no sequence collisions with the liquidator.
- SDK build and simulate, CLI `tx sign`, then RPC send without waiting for inclusion.
- The hash is journalled at intent.
- Hardening:
  - `TRY_AGAIN_LATER` keeps the local sequence and retries after 2 s;
  - resync only on `ERROR`;
  - inclusion fee 10,000 stroops;
  - ordered RPC failover (`heartbeatRpcUrls`);
  - **self-observation**: the heartbeat simulates `checkpoint_nav` itself through its failover RPC when the plan loop has been quiet for more than 20 s. Failover alone did not help, because the plan loop reads through the single primary RPC, its observation went stale, and the heartbeat then stopped after 60 s.
- CLI sends now journal the tx hash (`tx/` marker).
- Tests: `node --test test/*.test.mjs` 49/49 at 7e55b84, including 8 new heartbeat tests.

| Window (on-chain `checkpoint_nav` ledger close times, `gaps.py`) | n | p50 | p95 | max | > 20 s | > 60 s |
|---|---:|---:|---:|---:|---:|---:|
| Old bot, R1, 08:17–08:37 | 26 | 30 | 105 | 125 | 26 | 7 |
| Fixed timer, R1, 08:37–12:32 (whole 3 h 55 min, pre-hardening) | 1,150 | 10 | 15 | 70 | 7 | 2 |
| Fixed timer 4979f7b (pre-hardening), R1, 09:22–12:32 (3 h 10 min) | **949** | **10** | **15** | **15** | **0** | **0** |
| Fixed timer, R2, 12:45–12:57 (7e55b84 not yet deployed) | 38 | 10 | 15 | 220 | 1 | 1 |
| **Final code 5f48b0d, R1 bot, 16:14:05–16:46:00 (31.9 min out of range)** | **159** | **10** | **15** | **15** | **0** | **0** |
| **Final code 5f48b0d, R2 bot, same window** | **159** | **10** | **15** | **15** | **0** | **0** |
| Final code, heartbeat-only, **primary RPC dead** (127.0.0.1:9) + gateway.fm, no plan loop, 16:14–16:29 | 74 | 10 | 15 | 15 | 0 | 0 |

Causes of every gap over 20 s:
- 08:53–09:21: `TRY_AGAIN_LATER` → sequence resync → `txBAD_SEQ`. Fixed in 4a7b449.
- 12:49–12:53: soroban-testnet.stellar.org unreachable for about 220 s (`fetch failed`). Failover added in 7e55b84.
- 12:57–15:46: **the host machine was suspended** (no log line from either process). This is not counted as bot behaviour. Since then the bots run under `caffeinate`.

**Mainnet requirement:** run the heartbeat on an always-on host with at least two independent RPC providers.

**Soak totals (both bots live, `--watch --live`):**
- R1 ran 08:14–12:57 and 15:46 onward. R2 ran 12:41–12:57 and 15:46 onward.
- The R1 liquidator sent 52 CLI stages with **0 failures**. Included heartbeats: 1,151 on R1 (08:37–12:32) and 48 on R2 up to 15:52. Each cost about 0.0108 XLM (gaps.py fee totals: 1.24 XLM and 0.063 XLM).
- XLM spent at 16:10 UTC: liq 0.286, heartbeat 1.391, liq2 0.170, heartbeat2 0.128.
- Latency: heartbeat submit took 0.5–4.0 s (`sendMs` in the logs). At a 12 s period the on-chain gaps were 10 s or 15 s, i.e. two or three 5 s ledgers. The L1b → L3 window measured 10 s.
- **Misses and failures (log counts, 08:14–16:49):**
  - R1: 40 plan cycles `cycle failed` (all RPC `fetch failed`); 9 heartbeat send rejections (`TRY_AGAIN_LATER`/`ERROR`, all before 4a7b449); 10 heartbeat transport errors; 0 failed CLI stages out of 59.
  - R2: 37 cycles failed (`fetch failed`); 1 rejection; 1 transport error; 0 failed CLI stages out of 16.
- **Uptime.** The longest gap between consecutive log lines is 10,128 s (12:57:32–15:46:20, host suspended). Otherwise it is ≤ 90 s.
  - Scheduled runtime (sum of log intervals < 120 s): R1 ≈ 5.4 h by 16:24, crossing 6 h about 17:05. R2 ≈ 1.3 h.
  - **The ≥ 6 h target was met only as a sum of two segments (4 h 43 min + about 1 h 20 min), not as one continuous run.**
  - `caffeinate -i -s` does not survive a closed lid or battery power; Mainnet must run on a server.

## 5. Measured rows on real dependencies

Entries = |readOnly| + 2·|readWrite| (= keys + writes, the BUILD-REPORT metric). CPU and memory come from the `core_metrics` diagnostic events (actual use). Fees are `feeCharged` in XLM. Raw rows: `target/clg/testnet-real{,2}/rows*.jsonl` (outside git, from `tn.mjs measure-*`).

| Stage (R2 = release candidates unless noted) | Entries (keys+writes) | Keys | Writes | CPU | Mem | Fee XLM |
|---|---|---:|---:|---:|---:|---:|
| `stage_emergency_exit` (not entered, liquidator, waiver) | **84** (58+26) | 58 | 26 | 64,383,313 | 9,530,760 | 0.3599 |
| L3 `liquidate` on fallback | 77 (61+16) | 61 | 16 | 32,417,421 | 5,001,760 | 0.0455 |
| L3 `liquidate` on NAV | 77 (61+16) | 61 | 16 | 31,908,948 | 4,994,304 | 0.0532 |
| `stage_mint`, second owner (includes the real AQUA claim) | 75 (57+18) | 57 | 18 | 77,063,824 | 9,527,765 | 0.1093 |
| `borrow`, first (cold) | 71 (62+9) | 62 | 9 | 24,081,732 | 5,094,038 | 0.2104 |
| `borrow`, second | 63 (52+11) | 52 | 11 | 21,458,735 | 5,020,628 | 0.1598 |
| `stage_invest` (real `deposit_position`) | 58 (41+17) | 41 | 17 | 77,845,730 | 9,323,787 | 0.0625–0.3071 |
| `stage_open` | 37 (31+6) | 31 | 6 | 14,982,638 | 6,286,483 | 0.1279 |
| `stage_convert` (real swap) | 29 (20+9) | 20 | 9 | 34,232,186 | 6,961,153 | 0.0033–0.0236 |
| L5 `sync_seized` | 29–30 | 19–20 | 10 | 5,399,669 | 3,193,480 | 0.0427–0.0979 |
| L4 `checkpoint_rewards` (real claim) | 28 (21+7) | 21 | 7 | 9,835,650 | 4,822,533 | 0.0021 |
| `stage_finalise` | 26 (18+8) | 18 | 8 | 11,865,073 | 4,774,545 | 0.0169 |
| `stage_emergency_pay` | 21 (13+8) | 13 | 8 | 6,499,498 | 3,664,774 | 0.0022 |
| L1a `checkpoint_nav` (ok, or blocked observation) | 20 (19+1) | 19 | 1 | 13,670,328 | 5,316,095 | 0.0011 |
| L2 `cache_price(USDC)` via the real Reflector external feed | 17 (16+1) | 16 | 1 | 4,133,160 | 1,986,612 | 0.0007 |
| L1b `checkpoint_fallback` | 15 (14+1) | 14 | 1 | 7,467,895 | 4,086,952 | 0.0101 |
| **`stage_withdraw` in kind, position-backed (10 % close by an entered owner with debt)** | **91 (60+31)** | 60 | 31 | **91,308,680** | 10,697,873 | 0.1832 |
| `stage_prepare_close` 10 % with debt | 77 (66+11) | 66 | 11 | 29,430,961 | 6,422,934 | 0.0530 |
| `stage_withdraw` in kind, cash-backed units (R1) | 46 (30+16) | 30 | 16 | 14,316,886 | 5,070,494 | 0.0774 |

Comparison with the local rows on the Mainnet capture (BUILD-REPORT §2):
- L3 is 77 here vs 80 there; L1a 20 vs 20; L1b 15 vs 15; L4 28 vs 28–29; L5 29 vs 29; borrow cold 71 vs 75.
- The emergency exit is **84** on real dependencies vs 89 there. The Testnet figure does **not** supersede the Mainnet-capture row: the pool state and the caller differ (here a liquidator that holds only seized units). Open issue 1 stays open. 84 is within 1 of the 85 stop line.
- **Not measured on the real pool:** L3 with the liquidation fee on (84 locally) and `prepare_close` with debt (80 locally).
- CPU: open-mint and invest use **77–78M of the 100M instruction limit**. That is the tightest resource, not entries.
- R1 rows (18 Nov bytes) are identical within ±1 entry.

## 6. Upgrade path on Testnet (R2)

`testnet-real/05-upgrade.sh`. The targets are the `upgrade-marker` builds of the same sources (vault b68acb14, receipt 88c387dd, ctrl d5b551fe), uploaded to Testnet. The ETA is the 86,400 s constant; no short-timelock config exists.

| Step | Result | Tx |
|---|---|---|
| Vault execute, nothing pending | refused (sim) | – |
| Ctrl propose (A) | `get_pending_upgrade` = (d5b551fe…, 1795351441) | b7ff2bdd9b5a197b37adec3020bb8f7aaa44293c0f11f9db33f7ac2e2cd33011 |
| Ctrl cancel | pending = null | 0fd7f3da725c5fc7795b221dbfa1afd11b92e9549c90dc89777a881454086875 |
| Ctrl second cancel / execute the cancelled hash | both refused (sim) | – |
| Ctrl propose (live) | ETA 1795351451 | d35776889129985a3de4a8c68378821eacb8deef2888e2cc515d87ed467ea486 |
| Receipt propose | ETA 1795351456 | 8efdb079226a09704a358218adc1064fb1d23bc44bbe83b31ad28e4e206531e7 |
| Vault propose | ETA 1795351461 | 9e3cb8ea192dc6dd88f8b2d0361abe27afa4028ea9f9a10ad34d9bd05841da51 |
| Execute before ETA (ctrl, receipt, vault) | all refused (sim) | – |
| **Execute after ETA (2026-11-22 ≥ 12:45 UTC)** | **PENDING**: `CLG_TN_DIR=…/testnet-real2 bash 05-upgrade.sh execute` | – |

The execute step does the following:
1. Refuses the vault while it is unpaused, and the receipt without the controller pauses.
2. Upgrades the controller.
3. Pauses receipt deposit and redeem (borrow is already paused), then upgrades the receipt.
4. Calls `set_paused(true)` on the vault, upgrades it, then `set_paused(false)`.
5. Checks `upgrade_marker() == 2` on all three.
6. Diffs a 21-value state snapshot: units, beneficial, reward accounts, debts, position, idle, reward totals, configs, CF, oracle.
7. Unpauses, then runs a post-upgrade `checkpoint_nav`.

Caveat: "propose by a non-admin" passes in simulation because recording auth does not enforce. The refusal happens at signing. That refusal is covered by the compiled tests, not by this run.

## 7. Findings for the lead

0. **STOP: position-backed in-kind withdraw is 91 entries and 91.3M CPU** (1911b14283cf48a13da334f424e85e2618ae94e74f2e623d8bf04007f6115dba). Key counts by contract:
   - pool 21 (3 WordBitmap, 2 TickChunk, 2 TickRewardPlaneChunk, PositionV2, User, UserRewardState, RewardGlobalStatePacked, …);
   - vault 14;
   - AQUA 3 (the withdraw claims rewards), plane 2, PYUSD/USDC 3 each, code 6, trustlines 2.

   The pool part scales with the tick chunks and bitmap words touched. A Mainnet pool with more initialised ticks or reward-plane chunks can push this past 100 entries or 100M CPU, which makes a normal exit unexecutable. The emergency pair (84 + 21) remains as an exit. BUILD-REPORT has no row for this stage; the 18 Nov withdraw was cash-backed. Needs a harness row on the live Mainnet pool state before freeze.

1. **Bot D3 heartbeat cadence (defect, fixed).** See §4. Commits 4979f7b, 4a7b449, a17d2ab, 7e55b84.
2. **Testnet single-RPC and host outages break the chain.** With a broken chain the protocol degrades to *no* fallback liquidation, not a wrong one. For Mainnet: two or more RPC providers (configured), an always-on host, and alerting on `D3 chain lapsed`.
3. **Rent on the first touch of fresh code.** The first `checkpoint_nav` on freshly uploaded R2 code (3c5775d3…) charged **70.97 XLM**. Soroban meta: `rentFeeCharged` = 709,693,010 stroops, against 0 for the same call on R1 (a6fb2fa8…). This is TTL extension of code and instance entries, with 19 reads and 1 write of 1,740 B. The bot has no fee ceiling, so a keeper can pay a large rent bump unannounced. Recommendations:
   - a resource-fee ceiling in the bot;
   - extend the fresh Mainnet code TTLs inside the launch ceremony;
   - stream C should quote the Mainnet rent.
4. **Hash parse.** The CLI prints event payloads, for example the wasm hash in upgrade events, so "last 64-hex token" is wrong. Fixed in the bot and in `lib.sh`.
5. ~~Vault byte length~~ explained: +56 B is the second strkey literal in a non-Ledger-admin build (§1).
6. Emergency exit is at 84 entries and open/invest at 78 % CPU. These are the tightest margins.

## 8. Reproduction

Scripts are in `prototypes/cl-collateral-group/testnet-real/`:
- `01-deploy.sh` (venue and stack, resumable);
- `02-open.sh <BOR1|BOR2>` (open and invest);
- `px.sh` (swap, oracle);
- `05-upgrade.sh`;
- `06-manual-liq-trustline.sh`;
- `r2-prepare.sh` plus `build-testnet.sh <commit> <dest>` (R2);
- `tn.mjs` (`measure`, `measure-journal`, `measure-account`, `invest`);
- `gaps.py`.

State and append-only journals are in `target/clg/testnet-real{,2}/` (outside git). The 18 Nov `target/clg/testnet` state was not touched.

Bot configs are `bots/cl-liquidator/config/testnet-real{,2}.local.json` (git-ignored). Logs: `target/clg/testnet-real{,2}/bot/soak.log`.
