# Fresh lending ceremony — operator progress and remaining commands

**Release gate passed by operator decision accepting 4c98bdd3 on 2026-11-11.** Both original mediums are resolved. Almanax HIGH90f942e8 is recorded as **not applicable to this fresh deployment**: populated legacy markets are retired and will never be upgraded. Its raw scanner record is preserved. Current simulations, signatures and readbacks below remain required; this decision does not assert that deployment has occurred. No old contract is invoked or reused. The release is one fresh default controller plus four fresh default receipts, using canonical USDC, XLM, PYUSD and EURC assets. No PERI deployment, margin controller, strategy, oracle, router or rate-model binding. Initial rates and debt are zero. The rate WASM is reviewed and preserved but need not be deployed for this supply-only configuration.

The controller can list a future CL receipt with `add_market`. That is a later Ledger action, with separate accounting and no lending-cash routing. Do not set maintenance margin or enable borrowing.

## Confirmed operator progress — 2026-11-12

Funding, both code uploads and all five creations have completed. **Do not repeat steps 1–3.** `confirmed-instances-20261112.json` records the five successful creation transactions, total charged fee **0.0891585 XLM**, and remaining deployer balance **117.9017249 XLM**. Pinned salts and IDs remain in the historical `fresh-instance-plan.json`; its original pre-deployment measurements are preserved.

At ledger **65437466**, public RPC confirms all five installed executable hashes match the accepted clean artifacts and all five are uninitialized. `initialization-preflight-20261112.json` records independent unsigned Ledger-source simulations: controller 3,046,567 declared CPU / 15 unique keys / 11 writes; each receipt 5,878,116 / 35 / 23. Estimated combined initialization fee including nominal inclusion: **2.3797473 XLM**. These are preflights, not applied initialization. Only source-account authorization was requested; no restoration preamble. Actual commands must refresh sequence and simulation separately before each operator signature.

**Initialization subsequently confirmed at ledgers 65437507–65437529.** All five Ledger-source transactions succeeded; total actual fee **2.0695530 XLM**. `ledger-initialization-transactions-20261112.json` preserves transaction identities and fees; `fresh-cash-initialized-20261112.json` verifies actual raw storage, Ledger admins, canonical assets, zero debt/cash, disabled borrowing and absent external bindings. Do not repeat initialize. Guardian assignment, policy configuration, listing, seed deposits and withdrawal checks remain outstanding.

Seventeen independent unsigned configuration preflights passed (`configuration-preflight-20261112.json`): maximum declared CPU 13,265,344, 52 unique keys and 4 writes (maxima across rows). The four borrow-pause setters require listing first and will be simulated after each `add_market` by the operator CLI. These preflights are not a sequential applied configuration. `check-fresh-cash-state.cjs guardian` checks the intermediate guardian phase; `configured` checks all four bindings, durable policy and supported-market/pause maps before seeds. The Ledger wallet has 32.0821629 XLM, zero USDC/EURC and no PYUSD trustline at the current readback; seed assets must be funded before the deposit ceremony. Refresh the dated XLM/EURC dollar conversions before that ceremony.

## Configuration confirmed; wallet funding pending — 2026-11-12 09:18 UTC

All 21 configuration transactions succeeded (actual total fee **0.4617483 XLM**). Independent raw readback at ledger **65437805** verifies Ledger admin/guardian, all four fresh-controller bindings/listings, 10000 bps reserve, borrowing disabled, no pending authorities/upgrades and no external allocation. Do not repeat configuration. Transaction identities are in `ledger-configuration-transactions-20261112.json`.

The Ledger wallet currently holds **29.5508616 XLM**, zero USDC/EURC and no PYUSD trustline. Before market seeding, fund that wallet with **101 USDC, 101 PYUSD, 91.2062548 EURC**, and a suggested **700 additional XLM**. The latter leaves approximately **38.8749738 XLM** after the XLM seed/deposit, before remaining fees and the account's locked reserve. It is operating balance, not supplier capital. Do not transfer these assets directly to receipt contracts.

Create the canonical PYUSD trustline first: issuer `GDQE7IXJ4HUHV6RQHIUPRJSEZE4DRS5WY577O2FY6YQ5LVWZ7JZTU2V5`, whose Mainnet SAC derivation exactly matches the bound PYUSD asset. The prepared CLI build-only example has zero signatures and is not submitted; rebuild with the current source sequence before operator signing. Confirm the resulting trustline is authorized before sending PYUSD. No permanent seed or ordinary deposit has been submitted.

## Funding verified; permanent seeds next — 2026-11-12 09:32 UTC

Ledger funding now reads **728.5406217 XLM, 101 USDC, 101.0611141 PYUSD and 92 EURC**, with the canonical PYUSD trustline authorized. `funded-ledger-preseed-20261112.json` records the public balances. No additional funding is currently needed for the approved seed/deposit amounts. The preceding funding shortfall is historical and resolved.

Fresh configuration and all four actual SAC balances/share totals/NAV were checked again: zero underlying, zero shares/NAV, exchange rate 1000000, CL custody false. Four independent unsigned `seed_minimum_supply` simulations passed at ledger **65437981** with exact permanent amounts and source-account authorization. Maximum declared CPU 8,733,583 / 37 unique keys / 6 writes; total nominal fee **0.3830017 XLM**. `permanent-seed-preflight-20261112.json` retains unsigned authorization trees. No seed transaction has been applied at this checkpoint. The empty-state and pre-seed configuration scripts are phase-specific: do not interpret their expected failures after seeding as a deployed-state regression.

## Permanent seeds confirmed; ordinary deposits next — 2026-11-12 09:39 UTC

All four permanent seeds succeeded at ledgers **65438011, 65438021, 65438027 and 65438031**. Exact wallet debits (including **0.3324090 XLM** actual fees), actual underlying balances, NAV, receipt-owned permanent shares, zero Ledger shares and initial exchange rates reconcile. Do not repeat `seed_minimum_supply`. Evidence: `seed-verification-deposit-preflight-20261112.json`.

Independent unsigned ordinary-deposit simulations passed for **100 USDC, 685.6758878 XLM, 100 PYUSD and 90.2062548 EURC** at ledgers 65438052–65438053. Maximum declared CPU **14,401,142**, **44 unique keys**, **8 writes**; combined nominal fees **0.2887482 XLM**. Only source-account authorization is requested; no restoration is required. No ordinary deposit has yet been applied at this checkpoint. The first verification attempt's history-filter error included other-source incoming funding; its failure is preserved and the verifier now filters by exact source account before comparing sequence numbers.

## Ordinary deposits confirmed; withdrawal checks next — 2026-11-12

All four ordinary deposits succeeded. Wallet debits reconcile exactly including **0.2506673 XLM** in actual fees. Owner shares match the approved raw deposits, permanent receipt-owned shares remain unchanged, and actual cash/NAV/total shares match their sum. Exchange rates remain 1000000, debt zero, borrowing disabled. Evidence and transaction identities: `deposit-verification-withdraw-preflight-20261112.json`.

Four independent unsigned withdrawal simulations passed at ledgers **65438101–65438102** for **10 USDC, 68.5675887 XLM, 10 PYUSD and 9.0206254 EURC**. Exact raw share burns equal the expected raw payouts at the verified 1:1 NAV/share ratio. Maximum declared CPU **18,422,584**, **45 unique keys**, **7 writes**; combined nominal fees **0.0136744 XLM**. Source-account authorization only, no restoration. These withdrawal transactions have not yet been applied; operator signatures and final wallet/share/cash readbacks remain required before top-ups.

## Ceremony complete — 2026-11-12

All four live withdrawal checks are now confirmed. Exact payouts, raw receipt burns, cash/NAV, exchange rates and unchanged permanent floors reconcile, including **0.0115753 XLM** actual withdrawal fees. Final roles, durable safeguards, code hashes and absent external bindings pass the post-withdraw audit. See `LIVE-CEREMONY-COMPLETE-20261112.md`, `fresh-deployment-manifest.json` and `final-withdrawal-verification-20261112.json`. **Do not replay any completed ceremony transaction below.** Further top-ups require separately supplied operator amounts; the separate tracker is ready.

## Operator inputs and artifact seal

Ledger public account: `GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP`. Confirm it on the device. Fresh deployer public account: `GDBS26DYO2FMMLXGFPZ6JX3ZTHJBM2FVAAKD34J7K6ZWVKGLLANZTOYE`, supplied by the operator. Local alias: `peridot-cash-fresh-20261111`. Unique salts remain operator inputs. Market amounts are filled below; the gas quote is recorded in step 1. Its private key stays on the operator's host and is never provided here.

| Market | Underlying SAC | Permanent seed, raw units | Withdrawable seed deposit, raw units |
|---|---|---:|---:|
| USDC | CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75 | 10000000 (1 USDC) | 1000000000 (100 USDC) |
| XLM | CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA | 50000000 (5 XLM) | 6856758878 (685.6758878 XLM) |
| PYUSD | CCCRWH6Q3FNP3I2I57BDLM5AFAT7O6OF6GKQOC6SSJNDAVRZ57SPHGU2 | 10000000 (1 PYUSD) | 1000000000 (100 PYUSD) |
| EURC | CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV | 10000000 (1 EURC) | 902062548 (90.2062548 EURC) |

All four use seven decimals. The minimum permanent seed at the initialized 1:1 rate is **1,000 raw units = 0.0001000 underlying units**. All assets passed to `seed_minimum_supply` are permanently allocated to nonredeemable receipt-owned shares. This is separate from the operator's withdrawable deposit. Do not accidentally pass the full deposit to the seed method.

The permanent amounts are approved: **1 USDC, 5 XLM, 1 PYUSD and 1 EURC**. Withdrawable deposits are **100 USDC, 685.6758878 XLM, 100 PYUSD and 90.2062548 EURC**. The XLM/EURC amounts use [Coinbase XLM/USD](https://api.coinbase.com/v2/prices/XLM-USD/spot) $0.1458415 and [Coinbase EURC/USD](https://api.coinbase.com/v2/prices/EURC-USD/spot) $1.1085705777270018, fetched 2026-11-12 09:18:20 UTC. Current raw responses and exact Decimal quantities are pinned in `preseed-funding-20261112.json`; the earlier `seed-price-quotes.json` and `approved-seed-amounts.json` remain historical evidence. Quantities round down to seven decimals; these are approximate dollar values, not exchange guarantees. USDC/PYUSD are the exact token amounts specified by the operator, not a claim of exact market parity. Recheck quotes before signing if execution is delayed, and record any refreshed quantity explicitly. No asset swap is included.

The four withdrawal checks use 10 USDC, 68.5675887 XLM, 10 PYUSD and 9.0206254 EURC at the same quote snapshot. They are additional transactions after all four deposits; each returns assets from the withdrawable allocation, never from the permanent seed. Fees and account reserves require extra XLM outside these allocations.

Build only with `stellar contract build`, default features and the Ledger initializer environment variables. `medium-final-build-hashes.json` is the current artifact seal; the earlier `build-hashes.json` and `medium-fixed-build-hashes.json` are historical checkpoints. Any production source change invalidates this seal and requires rebuilding/rescanning.

| Artifact | SHA-256 |
|---|---|
| receipt, 89,033 bytes | 4229b7ef9f40028a20e7bace2484f330dbbea625aa98f29f453861e0c498f79e |
| controller, 63,285 bytes | 847894a65fb59675c74707ea8899633a7638aa2716a183b0b2efe35a8bc2b902 |
| rate model, 7,644 bytes, not deployed | 4f57459de70963abb7b6bc4c06ad5eae9d7f8dc0ef3ef59f023241705033126f |

## Command forms and signature protocol

These are operator commands. Funding, uploads and creations are now confirmed above; subsequent configuration and funding steps have not been executed. Use a new output directory, one unique `STEP` per transaction, and stop on every command error. Never overwrite an envelope or retry an unknown submission.

```sh
NET='Public Global Stellar Network ; September 2015'
LEDGER_PUBLIC='GC4T5MP5ZKILC633IUL6GHWGSIUV6JREN7BKOM6X7XTSVYLNKWZIT7FP'
DEPLOYER_PUBLIC='GDBS26DYO2FMMLXGFPZ6JX3ZTHJBM2FVAAKD34J7K6ZWVKGLLANZTOYE'
FRESH_DEPLOYER_ALIAS='peridot-cash-fresh-20261111'
# Fill: RPC_URL, DEPLOYER_PUBLIC, CONTROLLER, four fresh RECEIPT IDs,
# unique deployment salts and sealed WASM paths. Seed amounts are selected below.
build_call() {
  : "${STEP:?unique step required}" "${CONTRACT:?fresh contract required}"
  test ! -e "$STEP.unsigned.xdr" || return 2
  stellar contract invoke --id "$CONTRACT" --source-account "$LEDGER_PUBLIC" \
    --rpc-url "$RPC_URL" --network-passphrase "$NET" --build-only -- "$@" \
    > "$STEP.unsigned.xdr"
}
read_call() {
  stellar contract invoke --id "$CONTRACT" --source-account "$LEDGER_PUBLIC" \
    --rpc-url "$RPC_URL" --network-passphrase "$NET" --send=no -- "$@"
}
key_xdr() {
  node -e 'const S=require("./bots/margin-liquidator/node_modules/@stellar/stellar-sdk"); console.log(S.xdr.ScVal.scvVec([S.xdr.ScVal.scvSymbol(process.argv[1])]).toXDR("base64"))' "$1"
}
read_key() {
  stellar contract read --id "$CONTRACT" --key-xdr "$(key_xdr "$1")" \
    --durability persistent --output json --rpc-url "$RPC_URL" --network-passphrase "$NET"
}
```

Select one market at a time; `RECEIPT_*` must be populated with the verified **new** IDs from step 3. The helper refuses unknown market names. Run steps 5–10 for each market, completing each signature/readback before the next transaction. After all four deposits succeed, run step 11 once per market. No loop submits transactions automatically.

```sh
select_market() {
  MARKET="$1"
  case "$MARKET" in
    USDC) RECEIPT="${RECEIPT_USDC:?fresh USDC receipt required}"; ASSET=CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75; LOCKED_SEED_RAW=10000000; DEPOSIT_RAW=1000000000; WITHDRAW_RAW=100000000 ;;
    XLM) RECEIPT="${RECEIPT_XLM:?fresh XLM receipt required}"; ASSET=CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA; LOCKED_SEED_RAW=50000000; DEPOSIT_RAW=6856758878; WITHDRAW_RAW=685675887 ;;
    PYUSD) RECEIPT="${RECEIPT_PYUSD:?fresh PYUSD receipt required}"; ASSET=CCCRWH6Q3FNP3I2I57BDLM5AFAT7O6OF6GKQOC6SSJNDAVRZ57SPHGU2; LOCKED_SEED_RAW=10000000; DEPOSIT_RAW=1000000000; WITHDRAW_RAW=100000000 ;;
    EURC) RECEIPT="${RECEIPT_EURC:?fresh EURC receipt required}"; ASSET=CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV; LOCKED_SEED_RAW=10000000; DEPOSIT_RAW=902062548; WITHDRAW_RAW=90206254 ;;
    *) return 2 ;;
  esac
  CONTRACT="$RECEIPT"
}
```

For each financial/configuration envelope, the operator must:

1. Decode and verify network, source, sequence, method, argument amounts, contract IDs and complete authorization tree.
2. Simulate against the current public ledger and retain the result. Include restoration preparation where required, then resimulate. Reject over-budget rows; no manual omission of keys or auth entries.
3. Review assembled fee/resource limits. The source must match the intended signer. Use source-account authorization for Ledger calls; detached authorization, if returned, requires its own reviewed Ledger signing path and must not be silently omitted.
4. Sign the exact assembled envelope, submit once, then read transaction hash, success status, charged fee, source sequence/balance and the step-specific contract values. An unknown status stops the next step until reconciled.

Installed CLI forms (checked with `--help`):

```sh
stellar tx simulate --source-account "$SIGNER_PUBLIC" --rpc-url "$RPC_URL" \
  --network-passphrase "$NET" "$STEP.unsigned.xdr" > "$STEP.simulated.xdr"
# Inspect the assembled envelope and simulation before continuing.
# Ledger steps only:
stellar tx sign --sign-with-ledger --rpc-url "$RPC_URL" --network-passphrase "$NET" \
  "$STEP.simulated.xdr" > "$STEP.signed.xdr"
# Deployer steps only: operator-owned fresh identity alias, never a pasted secret.
stellar tx sign --sign-with-key "$FRESH_DEPLOYER_ALIAS" --rpc-url "$RPC_URL" \
  --network-passphrase "$NET" "$STEP.simulated.xdr" > "$STEP.signed.xdr"
# Use exactly one applicable signature command above, then operator submission:
stellar tx send --rpc-url "$RPC_URL" --network-passphrase "$NET" "$STEP.signed.xdr"
```

These examples do not authorize an agent signature or submission. Do not use `--auto-sign`, skip simulation, or interpret a source signature as detached Soroban authorization.

A failed or unavailable raw-key read is not proof of absence. Check instance/code liveness and the retained fresh initialization transaction, and resolve any archival/restoration indication before accepting None. `read_key` uses enum-key XDR, not the CLI symbol-key shorthand. Any RPC error stops the readback.

## Ordered ceremony

**1. Deployer gas.** The operator creates/funds a fresh throwaway account. Public read at 2026-11-11 18:33 UTC confirms the fresh deployer does not yet exist. At ledger 65427198, unsigned final-code simulations quote controller upload **89.9792066 XLM** and receipt upload **119.3289309 XLM**, totaling **209.3081375 XLM**, including 100 stroops nominal inclusion per operation. Both code hashes are currently absent. Creation simulations fail specifically with `Wasm does not exist`; five instance fees remain unquoted until upload. These are unsigned host estimates with placeholder source sequence, not executable transactions or full resource qualification. Recommended initial deployer funding: **300 XLM** = quoted uploads + **1 XLM** current new-account reserve + **89.6918625 XLM** contingency/instance budget. This remainder is a funding buffer, not a measured five-instance fee. Re-simulate each creation after the uploads and stop for insufficient balance; unused funds remain in the deployer. `fresh-deployment-fee-quote.json` preserves all seven results. The Ledger account read 32.0821729 XLM and cannot currently fund 300 XLM; fund the deployer from another operator wallet or replenish Ledger first. Configuration and market seed fees are paid separately by Ledger. Read its public signer weights, thresholds, balance and sequence. Abort for an unexpected signer, wrong address or inadequate balance. Gas is separate from supplier capital and permanent seed.

For a not-yet-existing deployer account, the operator may build the funding transaction as follows, then inspect/sign it with the funding Ledger and submit once. `GAS_STROOPS` is the reviewed amount in stroops, not whole XLM. Existing accounts use a payment instead of create-account; do not retry create-account after it succeeds.

```sh
stellar tx new create-account --source-account "$LEDGER_PUBLIC" \
  --destination "$DEPLOYER_PUBLIC" --starting-balance "$GAS_STROOPS" --build-only \
  --rpc-url "$RPC_URL" --network-passphrase "$NET" > 01-deployer-funding.unsigned.xdr
```

This classic operation has no Soroban simulation step. Verify the decoded amount and source reserve/sequence, use the Ledger signature protocol, then read the new account before uploads.

**Funding complete — do not repeat step 1.** Treasury transaction `6ba8f1d665b553b5cacd18dd0ab59ede404f48b52514422406197c667b598929` succeeded at ledger 65427468; the deployer read back **300.0001000 XLM** and sequence 281008835320086528. Resubmitting the older failed Ledger envelope `0190372b...` returned TxBadSeq and is not a new funding requirement. Public confirmation is saved in `deployer-funded-6ba8f1d6.json`.

Unsigned funded-source CLI preflight confirms the controller quote unchanged at 89.9792066 XLM. Receipt quote at ledger 65427535 remains 119.3289309 XLM. Prepare each upload from a fresh account sequence after the preceding transaction lands. No operator upload has yet been observed at this checkpoint.

**2. Two code uploads — deployer signatures.** One each for controller and receipt:

```sh
stellar contract upload --wasm "$WASM" --optimize=false --build-only \
  --source-account "$DEPLOYER_PUBLIC" --rpc-url "$RPC_URL" --network-passphrase "$NET" \
  > "$STEP.unsigned.xdr"
```

Compare local bytes with the seal before signing; read uploaded code hash/TTL after success. Abort on any mismatch. No automatic rebuild or changed optimization at upload.

**Both code uploads confirmed.** Controller transaction `5fb9f13f6d421ce18ea67bec5001f1b087387d2a1fdd45169ac1efd8aafe7fb7` succeeded at ledger 65427574, charging **78.2436225 XLM**. Receipt transaction `1409139551c20649289f18edbd4de2bb3205ea59a063a9bedff7179696af80f0` succeeded at ledger 65427604, charging **103.7655941 XLM**. Fresh-deployer balance is **117.9908834 XLM**, sequence **281008835320086530**. Fetched public code bytes exactly match both sealed local WASMs. Do not repeat the uploads.

Five independent unsigned creation simulations at ledger 65427628 pass, totaling **0.1046850 XLM** with nominal inclusion. These are fresh empty instances, not initialization/configuration. Expected IDs were independently derived by the CLI and matched simulator return values; each was absent before deployment. Use the pinned salts below. Build each transaction only after the previous one completes, so source sequence is fresh. Proposed resource-fee allowance is **0.1 XLM per creation**, plus inclusion fee; this is an allowance, not the quoted charge. No contract administration is assigned to the deployer by these creations.

| Label | Pinned salt | Expected ID — NOT YET DEPLOYED at this checkpoint |
|---|---|---|
| controller | `d30c38265e4717f9a1569788dc57d55c7d321b6504a7e3b96c5389cf1ec80c93` | `CBMRS7LK6HIZHHPXZTRJLVWZ4HOKJDMZAZ3FKDNFHZKGHXEGN72IJ7RK` |
| USDC | `1393ee404f539db850e249c19d49a675c6e50394a127683a25adc0e327deac97` | `CDW3GDNKLUEPPJLEUVGLJSKGYLB3YYDQBUY7NBHJILGQWNN55AYFHY3F` |
| XLM | `1ce59528d7905dbfb1bea8a3f5207d5c5a3e868ca4c1c88807d7f022157dd9cf` | `CBHMG4CNJO2CRGADUMIO3UJ45C5MTWGCBFECMR6QE2TDDCDBB3N24WMS` |
| PYUSD | `74c3f848cef9deed9c3d96f9915f450145bab22d29c4d78ccb0a2e5db9ebb542` | `CD6OTGA6J6UIEKXTXA5CLPLSJA4VQ5HEOYP3CA375CZEHI3KMIL2OYZC` |
| EURC | `19a24b1dc4380e232896349b0b87bd313f2a58637aa7190116ed2fe814bcb43e` | `CAXWP5M2SF74DW22EQAYOXR4ZHN2FO2HGL737KQ2WFPEZTA4Z3W5E2M4` |

**3. Five fresh instances — deployer signatures.** One controller, then USDC, XLM, PYUSD and EURC receipts; unique recorded salts:

```sh
stellar contract deploy --wasm-hash "$HASH" --salt "$SALT_HEX32" --build-only \
  --source-account "$DEPLOYER_PUBLIC" --rpc-url "$RPC_URL" --network-passphrase "$NET" \
  > "$STEP.unsigned.xdr"
```

Read each instance executable/hash/TTL and record the new ID. Abort if any ID is from the retired registry or any executable differs. Deployment itself grants no contract role to its source.

**4. Controller initialization and guardian — two Ledger signatures.** With `CONTRACT=$CONTROLLER`, separate steps:

```sh
CONTRACT="$CONTROLLER"
STEP=04-controller-init
build_call initialize --admin "$LEDGER_PUBLIC"
STEP=04-controller-guardian
build_call set_pause_guardian --guardian "$LEDGER_PUBLIC"
```

Read `get_admin`, `read_key PauseGuardian`, absent `PendingAdmin`, `PendingUpgradeHash` and `PendingUpgradeEta`, `get_oracle` None, and absent `PeridotToken`. Abort unless both authorities are exactly Ledger and no pending authority or external dependency exists. Do not call `set_oracle` or configure PERI.

**5. Receipt initialization — one Ledger signature per market.** Set `MARKET` to USDC, XLM, PYUSD or EURC, then that market's fresh `CONTRACT` and canonical `ASSET`:

```sh
STEP="05-$MARKET-init"
build_call initialize --token_address "$ASSET" --supply_yearly_rate_scaled 0 \
  --borrow_yearly_rate_scaled 0 --admin "$LEDGER_PUBLIC"
```

Read `get_admin`, `get_underlying_token`, `get_total_ptokens`, `get_total_borrowed`, `get_total_underlying`, `get_min_cash_reserve_bps`, `get_cash_borrowing_enabled`, `get_boosted_vault`, `get_margin_controller`; raw `RatesReady`, `InterestModel`, `PendingAdmin`, `PendingUpgradeHash` and `PendingUpgradeEta`. Expected: Ledger, exact asset, all totals zero, Some(0), false, no strategy/margin/model, RatesReady false, no pending admin or upgrade. Abort otherwise.

**6. Durable cash policy — two Ledger signatures per market.**

```sh
STEP="06-$MARKET-reserve"
build_call set_min_cash_reserve_bps --admin "$LEDGER_PUBLIC" --bps 10000
STEP="06-$MARKET-borrow-disabled"
build_call set_cash_borrowing_enabled --admin "$LEDGER_PUBLIC" --enabled false
```

Read Some(10000) and false. There is no separate flash-disable setter/getter: the sealed flash implementation calls the same durable check. Preserve the local expiry/lift/flash test evidence and code identity. Holder withdrawals are not prohibited by this reserve floor.

**7. Controller registration and binding — three Ledger signatures per market.**

```sh
CONTRACT="$CONTROLLER"
STEP="07-$MARKET-list"
build_call add_market --market "$RECEIPT"
STEP="07-$MARKET-pause"
build_call set_pause_borrow --market "$RECEIPT" --paused true
CONTRACT="$RECEIPT"
STEP="07-$MARKET-bind"
build_call set_peridottroller --peridottroller "$CONTROLLER"
```

Controller readbacks: `is_market_supported --market "$RECEIPT"` true, `get_market_cf --market "$RECEIPT"` zero, `is_borrow_paused --market "$RECEIPT"` true, `is_deposit_paused --market "$RECEIPT"` false, and `is_redeem_paused --market "$RECEIPT"` false. Receipt `read_key Peridottroller` must equal the fresh controller; debt zero and durable switch false. **Do not call `set_market_cf(...,0)`**: zero is the unconfigured default, not a valid setter value. The controller pause expires after 72 hours; the durable receipt switch does not.

**8. Pre-seed readback — no signature.** Re-read every admin/guardian, pending authority and pending upgrade; no throwaway address may hold any role. Every receipt must have absent BoostedVault, MarginController and InterestModel, `get_cl_custody` false, zero debt/supply/managed cash and zero `get_total_reserves`, `get_total_admin_fees` and `get_total_bad_debt`, and zero underlying SAC balance. Inspect initialized SupplyCap/BorrowCap (explicit zero means uncapped, not a borrowing switch). No external allocation or reward module is configured. An unexpected donation/balance is an abort requiring reconciliation. Maintenance margin stays absent.

**9. Permanent seed — one Ledger signature per market.**

```sh
STEP="09-$MARKET-permanent-seed"
build_call seed_minimum_supply --admin "$LEDGER_PUBLIC" --seed_assets "$LOCKED_SEED_RAW"
```

Read `get_minimum_supply`, `get_ptoken_balance --user "$RECEIPT"`, `get_total_ptokens`, `get_total_underlying`, `get_exchange_rate` (expected 1000000), underlying balance and Ledger debit. All share totals equal the floor and cash equals the funded seed at the initial rate. Ledger receives no withdrawable shares for this operation. Abort on mismatch.

**10. Withdrawable seed deposit — one Ledger signature per market.**

```sh
STEP="10-$MARKET-deposit"
build_call deposit --user "$LEDGER_PUBLIC" --amount "$DEPOSIT_RAW"
```

Read Ledger receipt balance, self-held floor, `get_exchange_rate` (expected 1000000), total NAV/shares/cash, exact underlying debit, zero debt, durable borrowing false, no strategy, and all roles, absence of pending upgrades, CL custody false and zero reserves/admin fees/bad debt again. Abort on foreign authorization/recipient or any unexpected delta. Use the approved amounts selected above; the permanent allocation and ordinary deposit must remain separate.

**11. Withdraw approximately $10 — one additional Ledger signature per market, after all deposits.** Call `select_market USDC`, `select_market XLM`, `select_market PYUSD`, then `select_market EURC` in separate reviewed iterations. Record these reads immediately before each withdrawal:

```sh
read_call get_ptoken_balance --user "$LEDGER_PUBLIC"
read_call get_ptoken_balance --user "$RECEIPT"
read_call get_total_ptokens
read_call get_total_underlying
read_call get_exchange_rate
CONTRACT="$ASSET" read_call balance --id "$LEDGER_PUBLIC"
CONTRACT="$ASSET" read_call balance --id "$RECEIPT"
```

`withdraw(user, ptoken_amount)` burns exact receipt shares; it does not accept a desired underlying payout. In this fresh zero-rate, zero-debt configuration, require `get_total_underlying == get_total_ptokens` and `get_exchange_rate == 1000000`, then the approved raw underlying amount equals the raw shares to burn. **Abort and re-quote if these identities differ**; do not derive a burn from the truncated exchange-rate quote. For a later non-1:1 quote, the exact calculation is ceiling(target_underlying × total_shares / supplier_NAV), with payout floor(burn × supplier_NAV / total_shares), validated by fresh simulation.

```sh
# Only after the exact 1:1 checks and sufficient owner-share readback:
PTOKEN_BURN_RAW="$WITHDRAW_RAW"
STEP="11-$MARKET-withdraw-check"
build_call withdraw --user "$LEDGER_PUBLIC" \
  --ptoken_amount "$PTOKEN_BURN_RAW"
```

Inspect, simulate, sign with Ledger and submit once using the signature protocol. Read transaction status/meta/fee, then repeat all seven reads above. Require the owner's receipt shares to fall by exactly `PTOKEN_BURN_RAW`; the receipt's underlying balance and supplier NAV to fall by the simulated payout; permanent self-held shares unchanged; exchange rate still 1,000,000; debt still zero and borrowing false. Issued-asset wallet balance must rise by the payout. For XLM, reconcile the wallet increase **net of the actual charged transaction fee**, separately accounting for any reserve/sponsorship changes shown in metadata. Do not mistake the XLM fee for a withdrawal shortfall. Unexpected deltas, failed status or missing readbacks abort the remaining ceremony until reconciled; never repeat an unknown withdrawal.

Expected values at the pinned quotes, assuming only the listed seed/deposit/withdraw operations:

| Market | Shares burned = underlying paid, raw | Operator receipt balance after, raw | Market underlying after, raw | Exchange rate |
|---|---:|---:|---:|---:|
| USDC | 100000000 | 900000000 | 910000000 | 1000000 |
| XLM | 685675887 | 6171082991 | 6221082991 | 1000000 |
| PYUSD | 100000000 | 900000000 | 910000000 | 1000000 |
| EURC | 90206254 | 811856294 | 821856294 | 1000000 |

Other suppliers' intervening transactions must be reconciled from transaction metadata rather than treated as this operator's balance changes. The permanent seed remains in the market totals. Record each market's successful deposit hash, successful withdrawal hash, read ledger and before/after balances/rate; **all four markets must pass both operations** before top-ups.

**12. Final manifest and keepalive.** Record the fresh controller/four receipts, sealed hashes, authority reads, all None bindings, cash policies and seed transaction hashes. The deployer has no role or pending role. Do not discard its local key or drain its gas before every submission is reconciled. Operator key disposal is separate from contract-role evidence.

Use `scripts/prepare_cash_keepalive.sh` only with the verified fresh manifest. It builds unsigned receipt keepalive envelopes, renewing persistent core keys (including caps) and the instance. Monitor code, instance and controller policy TTLs too. Cold entries require restoration and re-simulation; keepalive is not a guarantee of perpetual availability. Never run the legacy strategy keeper.

Ledger-only `reduce_reserves`, `reduce_admin_fees` and strategy binding remain accepted powers, explicitly **not to be used in supply-only mode**. No borrowed-cash or allocation activation is part of this ceremony. Later CL listing is a separate admin action and readiness decision.


## Future upgrades — required before any proposal or signature

Any future upgrade of the **new** controller or receipt markets requires a passing storage-compatibility test against a snapshot of their **actual live state first**. Pin the ledger, current WASM hashes and candidate WASM hashes; apply the candidate locally to that snapshot and verify storage decoding, admin/guardian/pending roles, cash policies, controller/asset bindings, locked minimum supply, owner shares, NAV, debt/reserve/fee accounting and deposit/withdraw/recovery behavior. Include archived entries and restoration where present. A fresh empty-state fixture or matching ABI alone is insufficient. Preserve the test report before any live upgrade proposal; no migration may silently reset safeguards or erase claims. Retired legacy contracts will never be upgraded.

## Follow-on order

1. Complete all four deposit/withdraw checks and final readbacks above.
2. Receive the operator's remaining-capital amounts; prepare a **separate top-up command list** against these verified new addresses. `TOP-UPS-PENDING.md` records this pending step; no amounts or executable top-up envelopes are supplied yet.
3. After that, prepare the DefiLlama adapter change using only the new market addresses and drop the retired addresses. Cash-market TVL comes from underlying contract balances; no receipt/underlying double count. Any later CL Aquarius position is separately marked double-counted. Do not open the adapter PR without instruction.
