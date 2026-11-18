# Cash-borrowing operations — 18 Nov 2026 (ops subagent report, saved by the lead)

Nothing was signed, sent or deployed. No keys, secrets or env vars were read. Network use: read-only simulation only.
Tests: research tools 12/12 (`node --test tools/tests/*.cjs`), bots/cash-liquidator 29/29 (`npm test`), re-run by the lead.
`simulate-all` (throwaway pause key): 34 steps succeed at ledger 65541442, 0.514 XLM resource fee in total.

## Changes
- `research/cash-borrowing-activation-20261117/`:
  - tools/ceremony.cjs, tests, params.proposed.json, CEREMONY.md, PLAN.md, and the new read-only tools/oracle-bounds.cjs.
  - Step 31 `controller.set_pause_guardian(params.pauseGuardian.publicKey)` (Ledger) with read-back. It refuses a missing key, the admin and the liquidator. The ceremony is now 34 signatures.
  - `caps` check on prepare: cap × close factor × 1.10 ≤ inventory.
  - `set_oracle` and gate G1 check the live feed against the D1 pins. G2 requires 2,522 XLM net and separate roles.
  - K2 uses the admin `set_pause_borrow`. Added `unpause-borrow` and `revoke-guardian`. `renew-cl-pause` is unchanged.
- `bots/cash-liquidator/`:
  - Pause watcher (src/pause.mjs, pause-main.mjs): builds `pause_borrow_g` only, on USDC/XLM/EURC; dry run by default.
  - Keys: keyfile.mjs and keygen.mjs (0600 file, public key printed only).
  - trustlines.mjs: operator one-shot.
  - Permissionless `cache_price` refresher every Reflector round, fee-capped, 3 XLM/day budget; it never blocks liquidation.
  - XLM inventory excludes the fee floor and reserve. Low-inventory alert. Refuses an admin or guardian key.
- This folder: funding.md, frontend-pyusd-copy.md with frontend-fresh-cash-markets.pyusd.patch (not applied), droplet/, evidence/.

## D1 (live read at ledger 65541367)
- Feed `CAFJZQWSED6YAWZU3GWRTOCNPPCGBN32L7QV43XX5LZLFTK6JLN34DLN`: Reflector external CEX/DEX, base `Other(USD)`, 14 decimals, 300 s resolution.
- USDC→Other(USDC) 1.00094; XLM→Other(XLM) 0.14165; EURC→Other(EURC) 1.09733.
- PYUSD: `None`, so it stays deposit-only.
- Staleness: controller k = 2, so a price older than 600 s gives `None` and blocks borrows and liquidations.
- Fiat EUR feed `CBKGPWGK…JZC` is used by the watcher only.

## Caps vs liquidator inventory (close factor 50%, buffer 1.10)
| Asset | Cap (was → now) | Cap × 0.5 × 1.1 | Inventory |
|---|---|---|---|
| USDC | 1,000 → 900 | 495 | 500 |
| XLM | 5,000 → 4,500 | 2,475 | 2,500 |
| EURC | 500 → 450 | 247.5 | 250 |

Assumptions:
- The per-market cap bounds total principal, so one liquidation round covers both the worst single position and a correlated move.
- Seized collateral is not swapped back during an incident.
- The operator tops up within 24 h.

## Funding (~$1,148)
| When | Account | Amount |
|---|---|---|
| T0: after the bootstrap prints the key | liquidator | 25 XLM, then trustlines |
| P0: after the pause bootstrap | pause key | 10 XLM |
| T1: after the 48–72 h soak, before DRY_RUN=false and gate G2 | liquidator | 500 USDC + 250 EURC + 2,600 XLM |

## Open items
1. Rates TODO (jump-rate measurement pending).
2. Sign off the lower caps, the 1.10 buffer and the one-round assumption.
3. Watcher thresholds: 50/100 bps, 900 s stale, 2 confirmations, 20 clean cycles. Should a stale feed alone pause?
4. The guardian is controller-wide: the hot key can pause redeem, liquidation and deposit on any market, including PYUSD and the CL receipt. The watcher code uses borrow only. `revoke-guardian` is the fix.
5. Refresher cost is estimated at 1.3–1.7 XLM/day; verify `feeCharged` live.
6. Fill both public keys after the bootstraps; pin Node v22 and its SHA-256; build the tarball from a reviewed commit.
7. Not rehearsed on Testnet: step 31, K2/K4/K5 and the watcher (simulated read-only on Mainnet only).
8. Still to do: alerting and a dead-man check, a security scan before live signing, the CL vault link target, and addresses.md after step 31.
