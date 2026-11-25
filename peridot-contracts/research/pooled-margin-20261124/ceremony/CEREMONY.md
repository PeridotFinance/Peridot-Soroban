# Pooled margin v2 — Mainnet ceremony list (WP8, 25 Nov 2026)

Status: **PROPOSED, NOT APPROVED.** Nothing here has been signed, sent, uploaded or deployed. The only network use was
read-only `simulateTransaction`, `getLedgerEntries` and Horizon GETs.

Owner rules this list implements: nothing deploys without a ceremony list; every upload and deploy step is simulated on
live Mainnet before it goes into a plan (and again by `step` right before the Ledger signs); the admin signs on Ledger
index 0; custody-only (both receipts' MarginController hook stays **unset**: the tool refuses `set_margin_controller` and
every receipt / controller margin hook, as the operation and anywhere in an authorization tree); no deposit pause on the
cash markets (the tool has no such switch and refuses `*pause_deposit*`).

| Item | Value |
|---|---|
| Contract | margin-controller v2, WP6 build `b1cb1cb21f02de1b1a9047d31fa60ffc066c2ddedc94c4dff2fcbdb0a7b1766f` (67,488 B), `stellar contract build --package margin-controller` at 35a5a15 |
| Frozen copy | `target/margin-freeze/b1cb1cb2/margin_controller.wasm` (git-ignored; sha256 + size checked before upload) |
| Margin id | `CA6Q7HULJTGFMTB7YLJWFKPQVIIWAP46PQHAY3H5V2EZ2R7HBCW6N7NZ` (admin + salt `peridot-margin-v2-mainnet-1:margin-controller`; the fork deploy produced the same id) |
| Cash markets (live, unchanged) | controller `CBMRS7LK…7RK` (code f41e5260), USDC receipt `CDW3GDNK…Y3F`, XLM receipt `CBHMG4CN…WMS` (both 5630a701), pool `CBBMQBNH…BUCV` |
| Pilot parameters | 5× max both sides (frontend default 3×); fees 0.10 % / 0.10 %; funding +2 % APR; bonus 3 % + 1 %; slippage 1.5 %; exit cooldown 72 h; caps $25 / position, $100 side equity, $500 side OI, $1 minimum; seed $750 USDC pTokens |
| Ledger signatures | **16** by the admin (index 0): M1–M6, M9–M12, M15, M16, C1–C4. M7, M8, M13, M14 equal the constructor defaults and are read-back only (a transaction is prepared only if a read-back differs) |
| Admin XLM | 210.59 XLM on 25 Nov. M1 needs up to 111.85 XLM (93.20 XLM simulated); everything else < 1 XLM in fees; the long canary locks ~41 XLM until C2 |
| Files | `margin-ceremony.cjs` (builder), `margin-mainnet.json` (pins), `margin-run.sh` (sign/send), `margin-ceremony.test.cjs` (13 offline tests), `SIZING.md` |
| Run / evidence dir | `/Users/joshua/Peridot/margin-v2-launch-20261125/` (outside the repo) |

## 0. Owner decisions needed before M2

1. **Guardian key.** `signers.guardian.publicKey` is `null`, so M2 is refused. Recommended: the cash pause key
   `GBSK3FYCGNYZLHHPSYRJNE53NZXDFL6ED24KLT3GUUETLFKDAUD7PQX3` (pause-only, on the pause host, already the cash
   controller's PauseGuardian). The margin guardian can only **pause** opens; the contract refuses its unpause. It is
   a constructor argument, so it needs no extra transaction; `set_guardian` can rotate it later.
2. **Keeper dry-stage length.** The gate requires `minDryStageHours: 2` (proposal).
3. **WP7 dependencies.** The cash liquidator must exclude the margin pooled account (alert only). `neverTouch` in its pins
   is a list of *receipts*, so adding the margin id there does nothing; WP7 has to add a per-borrower exclusion
   (`node margin-ceremony.cjs exclusion-fragment` prints the id). The margin keeper pass must run live on the Droplet.

## 1. Simulation results (25 Nov 2026, ledger 65664653)

Live Mainnet (`simulate-all`), before the deploy:

| Step | Instructions | Keys | Writes | Min resource fee | Max fee (padded) |
|---|---|---|---|---|---|
| M1 upload | 57.84 M | 1 | 1 | 93.2041 XLM | 111.8469 XLM (cap 120) |
| proxy: the seed's nested pToken transfer, admin → margin id, 750 pT | 22.47 M | 46 | 8 | 0.0707 XLM | 0.0869 XLM |
| proxy: FB1 USDC receipt deposit (1 USDC) | 15.52 M | 44 | 11 | 0.0041 XLM | 0.0069 XLM |

Deploy-dependent steps run on the margin fork (`harness/margin-fork`, test `wp8_ceremony_sequence_on_fork`: Mainnet
ledger 65662216, protocol-30 host, the real controller, receipts, Reflector and Aquarius pool). The admin and guardian are
the **real G-accounts**: their live entries come from `tools/fetch-wp8-entries.cjs`. Each call's recorded authorization
tree is asserted: exactly one authorizer, the expected root, and only the expected transfers. The fees below are
non-refundable estimates from the live fee settings; rent is set by the live simulation at `prepare`.

| Step | Instr | Keys | Writes | Class limit | Est. fee (non-refundable) |
|---|---|---|---|---|---|
| M3 / M4 set_market XLM / USDC | 5.1 / 5.3 M | 36 | 2 | setter ≤ 100 keys, 100 M | 0.0009 XLM |
| M5 enter_markets | 6.3 M | 16 | 3 | setter | 0.0011 |
| M6 set_pool | 3.1 M | 6 | 2 | setter | 0.0009 |
| M7–M14 setters (each) | 1.7–1.8 M | 4 | 2 | setter | 0.0008–0.0009 |
| FB1 USDC deposit (fallback) | 14.5 M | 46 | 12 | setter | 0.0028 |
| M15 backstop_deposit $750 seed | 22.6 M | 52 | 12 | release ≤ 130 keys, 200 M | 0.0030 (+ rent ≈ 0.07 live) |
| M16 unpause | 1.8 M | 4 | 2 | setter | 0.0009 |
| C1 canary long open, $5 at 3× | 141.8 M | 112 | 35 | release | 0.0108 |
| C2 canary long swap-close | 139.8 M | 107 | 35 | release | 0.0108 |
| C3 canary short open | 123.2 M | 109 | 32 | release | 0.0097 |
| C4 canary short swap-close | 116.8 M | 106 | 33 | release | 0.0095 |
| K1 pause (guardian / admin), K7 resume | 1.8–1.9 M | 4 | 2 | setter | 0.0009 |
| K2 reconcile | 9.2 M | 72 | 1 | liquidation (85 % of live) | — |
| K4 admin_recapitalize | 21.9 M | 50 | 9 | release | — |
| K5 admin_resolve_impairment | 9.6 M | 72 | 2 | liquidation | — |
| R3 / R4 backstop exit request / withdraw | 2.2 / 63.5 M | 6 / 97 | 4 / 16 | setter / release | — |

The fork does not measure the M2 deploy, because the deployer call runs in the test host. It ran there and produced the
pinned id with opens paused. The live simulation at `prepare` checks the host's returned id against the pin and refuses
otherwise. K3 `settle_backstop_debt` has no bad debt to settle in this sequence; the WP5/WP6 tables measure it at
89.6 M (USDC) and 198.0 M (XLM, worst). Every row is within its class limit. Canary results on the fork:

- Long: 40.24 XLM collateral, 9.945 USDC borrowed, notional $14.94, liq LTV 0.833.
- Short: 4.998 USDC collateral, 80.08 XLM borrowed.
- The open fee was 0.0149 pT per open. Ledger == pToken balance after each canary.

Raw outputs, outside the repo, in `/Users/joshua/Peridot/margin-v2-launch-20261125/sims/`:

| File | sha256 |
|---|---|
| `simulate-all-20261125.json` | `89c9c3b18159967817afbc96551f6470ade42e721ac8af81381d2ba034a1ecbb` |
| `fork-wp8-ceremony.json` | `cad7f76b5a9012ee8cbcf58386482116283b7fd701025130aa62b4bc82fb81fb` |

Re-run the simulations (read-only):

```sh
cd /Users/joshua/Peridot/Peridot-Soroban/.wt-margin-assess/peridot-contracts/harness/margin-fork/tools && node fetch-wp8-entries.cjs --out ../.scratch/wp8-entries.json
```

```sh
cd /Users/joshua/Peridot/Peridot-Soroban/.wt-margin-assess/peridot-contracts/harness/margin-fork && cargo test --test margin wp8_ceremony_sequence_on_fork -- --test-threads=1
```

```sh
cd /Users/joshua/Peridot/Peridot-Soroban/.wt-margin-assess/peridot-contracts/research/pooled-margin-20261124/ceremony && node margin-ceremony.cjs simulate-all --fork ../../../harness/margin-fork/.scratch/wp8-ceremony.json --out /Users/joshua/Peridot/margin-v2-launch-20261125/sims/simulate-all-$(date -u +%Y%m%dT%H%M%SZ).json
```

## 2. How every step runs

Open a dedicated terminal (zsh) and load the helpers. They redefine `step`, `gate` and `check`, so never load them in a
terminal that has another ceremony's `*-run.sh` sourced.

```sh
cd /Users/joshua/Peridot/Peridot-Soroban/.wt-margin-assess/peridot-contracts/research/pooled-margin-20261124/ceremony && source margin-run.sh
```

`step <name> prepare <stepId>` does, in order:

1. A fresh live simulation. `prepare` first refuses unless the preflight still holds (code pins, hooks unset) and every
   earlier step reads back done.
2. It writes `<name>.unsigned.xdr/.json`.
3. Ledger signature. Read the device screen: the function name, the margin id, and the arguments.
4. `verify-signed`: same hash, exactly one valid signature by the source.
5. Send, and require `SUCCESS`.

A step whose `.send.json` already says SUCCESS is skipped. On any failure nothing more is sent.

Live status of all steps at any time:

```sh
check
```

**Failure handling, every step:** if `prepare` refuses, read the reason. Nothing was written to chain. Fix the cause and
re-run the same `step` line (stale files are renamed automatically). If the send fails or is unknown, run the step's
read-back before anything else. Never re-sign a step whose read-back says done. If a send result is unknown, check it:

```sh
mc status <tx-hash>
```

## 3. Gates before any transaction

### G0 preflight (read-only)

Live code still equals the pins (controller f41e5260, receipts 5630a701), both receipts' `get_margin_controller` are
`null`, neither cash market is deposit-paused, and the admin holds ≥ 120 XLM. 25 Nov: all true; the admin held 210.59 XLM
and 2,940 USDC pT.

```sh
gate preflight
```

Failure: stop. Changed code means the plan was simulated against other code; re-run §1 and review before going on.

## 4. The ordered list

### M1 upload the WASM

- Signer: admin, Ledger index 0. No authorization entry (upload).
- Checks: sha256 `b1cb1cb2…766f` and 67,488 B of the frozen copy (refused otherwise); the code is not yet on chain.
- Sim 25 Nov: 57.84 M instructions, min fee 93.2041 XLM, max 111.8469 XLM (refused above 120).

```sh
step m01-upload prepare upload
```

Read-back: the code entry exists.

```sh
rb upload
```

Failure: an upload that did not land changes nothing; re-run. Bytes that are already on chain: the step refuses ("already
on chain"), go on to M2.

### M2 deploy with the constructor

- Signer: admin, index 0. Authorization: exactly the contract creation (root `CreateContractV2HostFn`), with the pinned
  wasm hash, the salt and the constructor arguments (admin, controller `CBMRS7LK…`, guardian, USDC SAC `CCW67TSZ…`, XLM SAC
  `CAS3J7GY…`).
- `prepare` also refuses unless the host's returned id is `CA6Q7HUL…N7NZ`.
- Prerequisite: owner decision §0.1 written into `margin-mainnet.json` (guardian). Opens start **paused**; the
  constructor sets fees, slippage, headroom and backstop params to the pilot values.

```sh
step m02-deploy prepare deploy
```

Read-back: the code hash is the pin, and the instance holds Admin, Guardian, Ctrl, Usdc and Xlm as configured.

```sh
rb deploy
```

Failure: if the contract exists with another guardian or code, stop. The id is fixed by the salt, so a mistake needs a
new salt and a new ceremony list.

### M3 set_market XLM

Signer: admin, index 0. Root-only authorization. On chain the contract checks the receipt's `get_underlying_token`.

```sh
step m03-market-xlm prepare set_market.XLM
```

```sh
rb set_market.XLM
```

### M4 set_market USDC

```sh
step m04-market-usdc prepare set_market.USDC
```

```sh
rb set_market.USDC
```

### M5 enter_markets (XLM, USDC)

The margin contract enters both cash markets as its pooled account (the controller's `enter_market` for its own address).

```sh
step m05-enter prepare enter_markets
```

Read-back: the controller's `get_user_markets(margin)` contains both receipts.

```sh
rb enter_markets
```

### M6 set_pool

Aquarius `CBBMQBNH…BUCV`; the contract stores its `get_tokens`.

```sh
step m06-pool prepare set_pool
```

```sh
rb set_pool
```

### M7 set_fees (read-back only)

The constructor already holds 1,000 / 1,000 / 20,000 / 30,000 / 10,000 (0.10 %, 0.10 %, +2 % APR, 3 %, 1 %). Expected
`done: true`; `prepare` refuses with "already done (constructor default)". Only if the read-back is false, run the step.

```sh
rb set_fees
```

```sh
step m07-fees prepare set_fees
```

### M8 set_max_slippage (read-back only, 15,000 = 1.5 %)

```sh
rb set_max_slippage
```

```sh
step m08-slippage prepare set_max_slippage
```

### M9 set_risk XLM (long side), 5×

The contract re-checks solvency on chain: 0.90 × 1.04 × 1.015 = 0.950 < 1.

```sh
step m09-risk-xlm prepare set_risk.XLM
```

```sh
rb set_risk.XLM
```

### M10 set_risk USDC (short side), 5×

```sh
step m10-risk-usdc prepare set_risk.USDC
```

```sh
rb set_risk.USDC
```

### M11 set_caps XLM (long): $25 / $100 / $500 / $1

```sh
step m11-caps-xlm prepare set_caps.XLM
```

```sh
rb set_caps.XLM
```

### M12 set_caps USDC (short): $25 / $100 / $500 / $1

```sh
step m12-caps-usdc prepare set_caps.USDC
```

```sh
rb set_caps.USDC
```

### M13 set_headroom (read-back only)

1.25 / 1.10 / ±20 %, derived close reserve, coefficients 3.0 / 2.1 / 4.6.

```sh
rb set_headroom
```

```sh
step m13-headroom prepare set_headroom
```

### M14 set_backstop_params (read-back only)

Cooldown 259,200 s (72 h), window 172,800 s (48 h), minimum deposit 0.1 pT.

```sh
rb set_backstop_params
```

```sh
step m14-bsparams prepare set_backstop_params
```

### Gate: seed balance

The admin must hold ≥ 750.0000000 USDC pTokens. 25 Nov: 2,940 pT, so FB1 is not needed.

```sh
gate seed-balance
```

### FB1 (fallback only) mint USDC pTokens

Only if the seed-balance gate fails. The admin deposits the missing USDC into the USDC receipt. Authorization: root plus
exactly one USDC SAC transfer (admin → USDC receipt, ≤ the amount). The step refuses if the balance is already enough.

```sh
step fb1-mint prepare mint.usdc-pt
```

### M15 $750 backstop seed

- Signer: admin, index 0. `backstop_deposit(admin, 7,500,000,000, min_shares 7,500,000,000)`.
- Authorization: root plus exactly one USDC-receipt `transfer(admin → margin, ≤ 750 pT)`.
- The first deposit is admin-only; it mints 1:1 shares plus 1e6 dead shares held by the contract.
- The pTokens move from the admin's existing supply: USDC market supply and borrowable cash do not change.

```sh
step m15-seed prepare backstop_deposit.seed
```

Read-back: `get_backstop().ptokens` ≥ 750 pT and `get_backer(admin)` ≥ 750 pT.

```sh
rb backstop_deposit.seed
```

### Gate: config

Every read-back M2–M14 is true; hooks are unset; not Impaired; no backstop bad-debt shares; opens still paused; backstop
value ≥ the full-pilot-book requirement $625.

```sh
gate config
```

### Gate: keeper (HARD gate, operator only)

The unpause (M16), the canary and every later resume are **refused** unless this gate passes at `prepare` time. It has two
parts.

**Operator gate file.** The **operator** writes `/Users/joshua/Peridot/margin-v2-launch-20261125/KEEPER-GATE.json` by hand
after checking the Droplet. Agents never write or edit it. The tool checks:

- `marginController` is the margin id.
- `keeper.host` is `droplet`, with a real host name (anything that looks like the Mac is refused).
- `keeper.mode` is `live`, `dryRun: false`, and `commit` is a 40-hex git commit.
- A dry stage of ≥ 2 h with 0 errors, ending before `liveSince`.
- Alerts tested within 7 days: liquidatable > 10 min, Impaired, keeper down.
- The cash-liquidator exclusion of the margin account is live (with its commit).
- A named operator (agent-like names are refused) and the exact attestation sentence.
- The file was written within 24 h and after `liveSince`. Placeholders are refused.

**On-chain checks:**

- The keeper account `GDQARPHR…5B5G` sent a transaction within the last 15 min.
- It holds USDC ≥ the largest long debt ($100).
- It holds XLM ≥ the largest short debt plus 20 XLM. 25 Nov: 502 USDC and 2,608 XLM, against needs of 99.9 USDC and 840.7 XLM.

Print the template (stdout only; the tool never writes the file):

```sh
mc gate-template
```

Print the exclusion WP7 must carry:

```sh
mc exclusion-fragment
```

Check the gate:

```sh
gate keeper
```

Failure: do not unpause. Fix the keeper or alerts, then have the operator rewrite the file.

### M16 unpause opens (LAST M-step)

- Signer: admin, index 0. `set_opens_paused(admin, false)`. Refused unless the keeper and config gates pass.
- From here anyone with the contract id can open, within the pilot caps. Do not publish the id before the canary is clean.

```sh
step m16-unpause prepare unpause
```

Read-back: `opens_paused()` is false.

```sh
rb unpause
```

Failure: if it did not land, opens stay paused and nothing else happened. Re-check the gates and re-run.

### C1 canary long open ($5 equity at 3×)

- Signer: admin, index 0. `open(admin, XLM, USDC, collateral, borrow, 1, 0.1 USDC)`.
- The amounts come from the live controller prices at `prepare`: about 41 XLM collateral and 9.95 USDC borrowed.
- Authorization: root plus exactly one XLM SAC transfer (admin → margin, ≤ the collateral). The pool swap is
  authorized by the contract itself.
- Keeper gate re-checked.

```sh
step c1-long-open prepare canary.long.open
```

```sh
rb canary.long.open
```

### C2 canary long swap-close

- `close(admin, id, 1)`. The id is found from the admin's single open long.
- The 1.5 % oracle floor applies. If the close is refused with `InsufficientSwapOutput` or a slippage error during a fast
  move, wait one Reflector round and re-run. Or use the no-swap fallback in §5 (close-with-repay).

```sh
step c2-long-close prepare canary.long.close
```

```sh
rb canary.long.close
```

### C3 canary short open ($5 at 3×: ~5 USDC collateral, ~80 XLM debt)

The admin needs ≥ 5 USDC in the wallet (5.68 on 25 Nov).

```sh
step c3-short-open prepare canary.short.open
```

```sh
rb canary.short.open
```

### C4 canary short swap-close

```sh
step c4-short-close prepare canary.short.close
```

```sh
rb canary.short.close
```

### Gate: canary

- No open admin position.
- Both side totals are back to 0 positions.
- `Ledger(asset)` equals the pToken balance for both receipts.
- Backstop NAV ≥ the seed (the canary fees went in).

```sh
gate canary
```

Failure: pause opens (K1) and investigate before telling the team.

## 5. Kill switches and K-steps

None of these is blocked by earlier steps. Each prepares ONE unsigned transaction from a fresh live simulation.
Close, close_with_repay, liquidate, claim and settle are never paused by the contract. Pausing opens never traps a trader.

**K1 pause opens, guardian (pause host key, not the Ledger).** Prepare on the Mac, sign on the pause host, send from the
Mac:

```sh
hstep k1-pause-g emergency pause-opens guardian
```

```sh
hsend k1-pause-g
```

**K1 pause opens, admin (Ledger index 0):**

```sh
kstep k1-pause-a emergency pause-opens admin
```

**K2 reconcile.** Permissionless. Sets Impaired and pauses opens on a pToken deficit (e.g. a cash-controller seizure of
the pooled account). The keeper simulates it every cycle; this is the manual path.

```sh
kstep k2-reconcile emergency reconcile
```

**K3 settle backstop bad debt.** The amount is self-sized: the tool reads `settle_max_ptokens` and refuses at 0 (nothing to
settle, or a pooled shortfall: do K4 first). Repeat until `get_bs_debt_shares` is 0 for both assets. Opens stay refused
(#25) until both are exactly 0.

```sh
kstep k3-settle-usdc-1 emergency settle usdc
```

```sh
kstep k3-settle-xlm-1 emergency settle xlm
```

**K4 recapitalize the backstop** (USDC pTokens admin → backstop, no shares; works while Impaired). The amount is in raw
pTokens; for example, 100 pT:

```sh
kstep k4-recap emergency recapitalize 1000000000
```

**K5 resolve an impairment.** Book a USDC pToken deficit against the backstop (first argument). Optionally restore XLM
pTokens from the admin (second argument; adds one XLM-receipt transfer to the authorization). Impaired clears only if
both ledgers are covered and NAV ≥ 0, so recapitalize first. For example, charge 280.84 pT and restore none:

```sh
kstep k5-resolve emergency resolve 2808400000 0
```

**K6 lower the caps of one side** (usd7 values: per position, side equity, side OI, minimum). For example, long side
$10 / $20 / $100 / $1:

```sh
kstep k6-caps-xlm emergency set-caps xlm 100000000 200000000 1000000000 10000000
```

**K7 resume opens** (admin). Refused unless the keeper gate passes, the contract is not Impaired and there are no backstop
bad-debt shares:

```sh
kstep k7-resume emergency resume-opens
```

**Rotate the guardian:**

```sh
kstep kg-guardian emergency set-guardian <G-address>
```

**Canary fallback: close with repay** (no swap; the admin pays the debt from the wallet). For example, position 1:

```sh
kstep c2b-long-cwr emergency close-with-repay 1
```

**Owner claim** after a liquidation of an admin position:

```sh
kstep kc-claim-xlm emergency claim xlm
```

**TTL keep-alive** (permissionless; instance plus the listed positions):

```sh
kstep kt-bump emergency bump-ttl 1 2
```

**Impairment drill order** (K-steps):

1. K1 pause opens.
2. Let the keeper liquidate whatever is past its liq LTV.
3. K2 reconcile.
4. K3 settles, repeated.
5. K4 recapitalize if NAV < 0 or settles fail with `PooledShortfall` (#35).
6. K3 again.
7. K5 resolve.
8. The keeper gate is re-checked.
9. K7 resume.

## 6. Rollback

The margin contract has no "undeploy". The lenders' markets never call into it (the hook stays unset), so rollback means
making it inert and recovering the seed:

1. **R1** K1 pause opens (guardian or admin).
2. **R2** Let every position exit. Close, close_with_repay, liquidate and claim keep working, and the keeper liquidates.
   Wait until `get_side` shows 0 open positions on both sides and both bad-debt share counts are 0 (K3 if not).
3. **R3** Request the seed's exit (the shares are in `get_backer(admin)`; the 1e6 dead shares stay in the contract):

```sh
kstep r3-bs-request emergency bs-request <shares>
```

4. **R4** After the 72 h cooldown, within the 48 h window, withdraw at NAV. The pool check passes with the book empty.
   The second argument is the minimum pTokens accepted:

```sh
kstep r4-bs-withdraw emergency bs-withdraw <min-ptokens>
```

Cancel a pending request:

```sh
kstep r3c-bs-cancel emergency bs-cancel
```

5. **Code fix instead of retirement.** Use the 24 h upgrade timelock: propose (a new proposal replaces a pending one),
   then execute after the ETA. Upgrades need their own ceremony list and a live simulation of the new bytes first.

```sh
kstep u1-propose emergency propose-upgrade <sha256>
```

```sh
kstep u2-execute emergency execute-upgrade <sha256>
```

Nothing in the rollback touches the cash markets: no deposit pause, no CF change, no hook.
