#!/usr/bin/env bash
# Step 1: deploy + configure the CL group stack on TESTNET (resumable: existing ids are kept).
# Fixtures: mock-token PYUSD/USDC/AQUA/GAUGE, Reflector-ABI test oracle, concentrated-pool fixture.
# Candidates (Testnet init-admin builds of the reviewed sources): vault v2, StgV2 receipt, group
# controller (G0-G10), accepted cash market as the static-rate USDC lending market.
source "$(dirname "$0")/lib.sh"
ADM=$(addr clg-admin); LIQ=$(addr clg-liq); SUP=$(addr clg-sup)
put ADM "$ADM"; put LIQ "$LIQ"; put SUP "$SUP"
MT=$ART/mocks/mock_token.wasm
for t in PYUSD USDC AQUA GAUGE; do
  deploy "$t" "$MT" clg-admin
  [[ -n "$(view "${!t}" decimals 2>/dev/null)" ]] || once INIT_T "init $t" "${!t}" clg-admin initialize --name "\"t$t\"" --symbol "\"t$t\"" --decimals 7
done
deploy ORACLE "$ART/cl_group_testnet_oracle.wasm" clg-admin -- --admin "$ADM" --base "{\"Stellar\":\"$USDC\"}"
[[ -n "${PRICE_SET:-}" ]] || { once ORACLE_PYUSD_0_9998 "oracle PYUSD 0.9998" "$ORACLE" clg-admin set_price --asset "{\"Stellar\":\"$PYUSD\"}" --price 99980000000000; put PRICE_SET 1; }
deploy POOL "$ART/cl_group_testnet_pool.wasm" clg-admin
if [[ -z "${POOL_INIT:-}" ]]; then
  once POOL_INIT "pool init" "$POOL" clg-admin initialize --token0 "$PYUSD" --token1 "$USDC" --tick_spacing 20 --fee_bps 5
  once POOL_REWARDS "pool rewards" "$POOL" clg-admin set_reward_tokens --aqua "$AQUA" --gauge "$GAUGE"
  once MINT_PYUSD_ADMIN "mint PYUSD admin" "$PYUSD" clg-admin mint --to "$ADM" --amount 1000000000000
  once MINT_USDC_ADMIN "mint USDC admin" "$USDC" clg-admin mint --to "$ADM" --amount 1000000000000
  once POOL_RESERVES_20K_20K "pool reserves 20k/20k" "$POOL" clg-admin donate --from "$ADM" --amount0 200000000000 --amount1 200000000000
  once MINT_AQUA_POOL "mint AQUA pool" "$AQUA" clg-admin mint --to "$POOL" --amount 100000000000
  put POOL_INIT 1
fi
deploy VAULT "$ART/vault/cl_share_vault.wasm" clg-admin
if [[ -z "${VAULT_INIT:-}" ]]; then
  once VAULT_INIT "vault init" "$VAULT" clg-admin initialize --admin "$ADM" --pool "$POOL" --underlying_index 0 --oracle "$ORACLE"
  once VAULT_REWARD "vault reward" "$VAULT" clg-admin set_primary_reward_token --admin_addr "$ADM" --reward_token "\"$AQUA\""
  once VAULT_MAX_DEPLOY "vault max deploy" "$VAULT" clg-admin set_max_deploy --admin_addr "$ADM" --max_deploy 10000000000
  put VAULT_INIT 1
fi
deploy CTRL "$ART/ctrl/simple_peridottroller.wasm" clg-admin
[[ -n "${CTRL_INIT:-}" ]] || { once CTRL_INIT "ctrl init" "$CTRL" clg-admin initialize --admin "$ADM"; once CTRL_ORACLE "ctrl oracle" "$CTRL" clg-admin set_oracle --oracle "$ORACLE"; put CTRL_INIT 1; }
deploy RECEIPT "$ART/receipt/receipt_vault.wasm" clg-admin
if [[ -z "${RECEIPT_INIT:-}" ]]; then
  once RECEIPT_INIT "receipt init" "$RECEIPT" clg-admin initialize --token_address "$PYUSD" --supply_yearly_rate_scaled 0 --borrow_yearly_rate_scaled 0 --admin "$ADM"
  once RECEIPT_BIND_CTRL "receipt bind ctrl" "$RECEIPT" clg-admin set_peridottroller --peridottroller "$CTRL"
  once RECEIPT_RESERVE_100 "receipt reserve 100%" "$RECEIPT" clg-admin set_min_cash_reserve_bps --admin "$ADM" --bps 10000
  once RECEIPT_FLOOR_CAP "receipt floor cap" "$RECEIPT" clg-admin set_supply_cap --cap 10000000
  once RECEIPT_FLOOR_SEED "receipt floor seed" "$RECEIPT" clg-admin seed_minimum_supply --admin "$ADM" --seed_assets 10000000
  once VAULT_BIND_CUSTODY "vault bind custody" "$VAULT" clg-admin stage_bind --admin_addr "$ADM" --receipt "$RECEIPT"
  once VAULT_GUARDIAN "vault guardian" "$VAULT" clg-admin stage_configure_guardian --admin_addr "$ADM" --guardian "$ADM"
  once VAULT_INVEST_KEEPER "vault invest keeper" "$VAULT" clg-admin stage_set_invest_keeper --admin_addr "$ADM" --keeper "$ADM"
  once RECEIPT_STGV1_ACTIVATE "receipt StgV1 activate" "$RECEIPT" clg-admin stage_activate --admin "$ADM" --vault "$VAULT" --guardian "$ADM" --backing_cap 100000000000
  once RECEIPT_STGV2_ACTIVATE "receipt StgV2 activate" "$RECEIPT" clg-admin stage2_activate --admin "$ADM" --c "{\"controller\":\"$CTRL\",\"pool\":\"$POOL\",\"oracle\":\"$ORACLE\",\"usdc\":\"$USDC\"}"
  put RECEIPT_INIT 1
fi
deploy USDCM "$ART/cash/receipt_vault.wasm" clg-admin
if [[ -z "${USDCM_INIT:-}" ]]; then
  once USDC_MARKET_INIT "usdc market init" "$USDCM" clg-admin initialize --token_address "$USDC" --supply_yearly_rate_scaled 0 --borrow_yearly_rate_scaled 0 --admin "$ADM"
  once USDC_RESERVE_10 "usdc reserve 10%" "$USDCM" clg-admin set_min_cash_reserve_bps --admin "$ADM" --bps 1000
  once USDC_BORROWING_ON "usdc borrowing on" "$USDCM" clg-admin set_cash_borrowing_enabled --admin "$ADM" --enabled true
  once USDC_STATIC_8 "usdc static 8%" "$USDCM" clg-admin set_borrow_rate --yearly_rate_scaled 80000
  once USDC_STATIC_ON "usdc static on" "$USDCM" clg-admin enable_static_rates --admin "$ADM"
  once USDC_SEED "usdc seed" "$USDCM" clg-admin seed_minimum_supply --admin "$ADM" --seed_assets 10000
  once USDC_BIND_CTRL "usdc bind ctrl" "$USDCM" clg-admin set_peridottroller --peridottroller "$CTRL"
  put USDCM_INIT 1
fi
if [[ -z "${GROUP_INIT:-}" ]]; then
  once CTRL_LIST_USDC "ctrl list usdc" "$CTRL" clg-admin add_market --market "$USDCM"
  once CTRL_LIST_RECEIPT "ctrl list receipt" "$CTRL" clg-admin add_market --market "$RECEIPT"
  once CTRL_SLEEVE "ctrl sleeve" "$CTRL" clg-admin group_set_sleeve --market "$RECEIPT"
  once CTRL_CF_50 "ctrl CF 50%" "$CTRL" clg-admin set_market_cf --market "$RECEIPT" --cf_scaled 500000
  once CTRL_LIQ_CACHE_ONLY "ctrl liq cache-only" "$CTRL" clg-admin group_set_liq_cache_only --on true
  once CTRL_BORROW_PAUSE_RECEIPT "ctrl borrow pause receipt" "$CTRL" clg-admin set_pause_borrow --market "$RECEIPT" --paused true
  put GROUP_INIT 1
fi
if [[ -z "${SUPPLY_INIT:-}" ]]; then
  once MINT_USDC_SUPPLIER "mint USDC supplier" "$USDC" clg-admin mint --to "$SUP" --amount 50000000000
  once SUPPLIER_DEPOSIT_5_000 "supplier deposit 5,000" "$USDCM" clg-sup deposit --user "$SUP" --amount 50000000000
  once MINT_USDC_LIQUIDATOR "mint USDC liquidator" "$USDC" clg-admin mint --to "$LIQ" --amount 20000000000
  put SUPPLY_INIT 1
fi
echo "deployed:"; grep -E '^[A-Z]+=C' "$STATE"
