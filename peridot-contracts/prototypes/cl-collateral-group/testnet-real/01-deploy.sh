#!/usr/bin/env bash
# Real-pool Testnet stack (TESTNET ONLY, resumable; state in target/clg/testnet-real, never the 18 Nov state).
# Venue: a FRESH instance of the Aquarius concentrated pool code 155a17b9... (the code the Aquarius
# Testnet router CBCFTQSP...6GD deploys, and the Mainnet PYUSD/USDC pool code), initialised with
# initialize_all and clg-admin in every role, plus fresh instances of the Aquarius Testnet plane,
# config-storage and boost-feed code. No WASM is uploaded for the venue. Tokens are classic-asset SACs
# (as on Mainnet). PYUSD feed = Reflector-ABI fixture (we must move it); USDC price for the controller =
# the REAL Testnet Reflector external feed CCYOZJ...RN63 (Other(USDC), base USD), as CAFJ on Mainnet.
source "$(dirname "$0")/lib.sh"
ADM=$(addr clg-admin); ISS=$(addr clgr-iss)
for k in bor1 bor2 liq sup; do put "$(echo $k | tr a-z A-Z)" "$(addr clgr-$k)"; done
put ADM "$ADM"; put ISS "$ISS"
POOL_HASH=155a17b9929ffb1f9e84bd6ef5c00a4d613c1ab5f4ad4c502d84515250cc2907
PLANE_HASH=421b1b8550242ce40627c8f8c8472a59f3f18109126f571abbb494f3ce6f3382
CONFIG_HASH=758bdb9683495f07a96c1dbdd4f7775c45205691eea317d22b97aeb82fa4960f
FEED_HASH=2c36ae240a5bca0e1e2fe3347ffabe85651c1c613bc36ab60df5881d0e18266c
EXT_ORACLE=CCYOZJCOPG34LLQQ7N24YXBM7LL62R7ONMZ3G6WZAAYPB5OYKOMJRN63
put EXT_ORACLE "$EXT_ORACLE"
# ---- SAC tokens
for c in PYUSD USDC AQUA BOOST; do
  if [[ -z "${!c:-}" ]]; then
    id=$(stellar contract id asset --asset "$c:$ISS" --network $NET 2>/dev/null | tail -1)
    stellar contract asset deploy --asset "$c:$ISS" --source clgr-iss --network $NET >/dev/null 2>"$TN/last.err" || grep -q -i "exist" "$TN/last.err" || { cat "$TN/last.err"; exit 1; }
    put "$c" "$id"; jlog "{\"t\":\"$(now)\",\"kind\":\"sac\",\"asset\":\"$c:$ISS\",\"id\":\"$id\"}"
  fi
done
for who in clg-admin clgr-bor1 clgr-bor2 clgr-liq clgr-sup; do for c in PYUSD USDC AQUA; do
  classic "TRUST_${who//-/_}_$c" "trust $who $c" "$who" change-trust --line "$c:$ISS"
done; done
once MINT_ADM_PYUSD "mint admin PYUSD 1M" "$PYUSD" clgr-iss mint --to "$ADM" --amount 10000000000000
once MINT_ADM_USDC "mint admin USDC 1M" "$USDC" clgr-iss mint --to "$ADM" --amount 10000000000000
once MINT_ADM_AQUA "mint admin AQUA 10k" "$AQUA" clgr-iss mint --to "$ADM" --amount 100000000000
for k in BOR1 BOR2; do once "MINT_${k}_PYUSD" "mint $k PYUSD 2000" "$PYUSD" clgr-iss mint --to "${!k}" --amount 20000000000; done
once MINT_SUP_USDC "mint supplier USDC 20k" "$USDC" clgr-iss mint --to "$SUP" --amount 200000000000
once MINT_LIQ_USDC "mint liquidator USDC 5k" "$USDC" clgr-iss mint --to "$LIQ" --amount 50000000000
# ---- Aquarius venue (fresh instances of on-chain code)
deploy_hash PLANE "$PLANE_HASH" clg-admin
once PLANE_INIT "plane init_admin" "$PLANE" clg-admin init_admin --account "$ADM"
deploy_hash CONFIGS "$CONFIG_HASH" clg-admin -- --admin "$ADM" --emergency_admin "$ADM"
deploy_hash BFEED "$FEED_HASH" clg-admin -- --admin "$ADM" --operations_admin "$ADM" --emergency_admin "$ADM"
# The pool deploys its reward vault (and gauges) from code hashes staged in config storage (#2137 otherwise).
# Values copied from the Aquarius Testnet config storage CDD3G6M2...FA46 (identical to Mainnet code).
once CFG_VAULTWASM "config VaultWASM=803527d4 (Aquarius Testnet value)" "$CONFIGS" clg-admin set_value --admin "$ADM" --key '{"vec":[{"symbol":"VaultWASM"}]}' --value '{"bytes":"803527d495ef4ade60eba3a7f96c7610f9abbaebe46d34d83d4ca466510aa66f"}'
once CFG_GAUGEWASM "config GaugeWASM=802127e3" "$CONFIGS" clg-admin set_value --admin "$ADM" --key '{"vec":[{"symbol":"GaugeWASM"}]}' --value '{"bytes":"802127e382da37aa26d0be2ef16c053c0606ecceed9daea603d826ead3e168c7"}'
deploy_hash POOL "$POOL_HASH" clg-admin
once POOL_INIT_ALL "pool initialize_all (fee 10, spacing 20)" "$POOL" clg-admin initialize_all --admin "$ADM" \
  --privileged_addrs "[\"$ADM\",\"$ADM\",\"$ADM\",\"$ADM\",[\"$ADM\"],\"$ADM\"]" --router "$ADM" \
  --tokens "[\"$PYUSD\",\"$USDC\"]" --fee 10 --tick_spacing 20 --protocol_fee_fraction 0 \
  --reward_config "[\"$AQUA\",\"$BOOST\",\"$BFEED\"]" --plane "$PLANE" --config_storage_addr "$CONFIGS"
# Wide background liquidity owned by the admin (swap depth for price moves), then a real AQUA schedule.
once POOL_LP_WIDE "admin LP [-1000,1000] 20k/20k" "$POOL" clg-admin deposit_position --sender "$ADM" --tick_lower -1000 --tick_upper 1000 \
  --desired_amounts '["200000000000","200000000000"]' --min_liquidity 0
# The convert swap of a 500 PYUSD open moved the thin wide position to tick -25 (outside [-20,20)), so the
# investment band refused; depth near par is added (rebalanced first with one 500 USDC swap, journalled).
once POOL_LP_DEEP "admin LP [-100,100] 200k/200k (depth near par)" "$POOL" clg-admin deposit_position --sender "$ADM" --tick_lower -100 --tick_upper 100 --desired_amounts '["2000000000000","2000000000000"]' --min_liquidity 0
RV=$(view "$POOL" get_vault_addr | tr -d '"'); put RVAULT "$RV"
once FUND_RV "fund pool reward vault 2,000 AQUA" "$AQUA" clg-admin transfer --from "$ADM" --to "$RV" --amount 20000000000
once POOL_REWARDS "set_rewards_config 0.002 AQUA/s for 3 days" "$POOL" clg-admin set_rewards_config --admin "$ADM" --expired_at $(( $(date +%s) + 259200 )) --tps 20000
# ---- PYUSD feed (Reflector-ABI fixture, base Stellar(USDC)), as CALI
deploy ORACLE "$ART/cl_group_testnet_oracle.wasm" clg-admin -- --admin "$ADM" --base "{\"Stellar\":\"$USDC\"}"
once ORACLE_PYUSD "oracle PYUSD 0.9998" "$ORACLE" clg-admin set_price --asset "{\"Stellar\":\"$PYUSD\"}" --price 99980000000000
# ---- Candidates
deploy VAULT "$ART/vault/cl_share_vault.wasm" clg-admin
once VAULT_INIT "vault init" "$VAULT" clg-admin initialize --admin "$ADM" --pool "$POOL" --underlying_index 0 --oracle "$ORACLE"
once VAULT_REWARD "vault reward" "$VAULT" clg-admin set_primary_reward_token --admin_addr "$ADM" --reward_token "\"$AQUA\""
once VAULT_MAX_DEPLOY "vault max deploy" "$VAULT" clg-admin set_max_deploy --admin_addr "$ADM" --max_deploy 100000000000
deploy CTRL "$ART/ctrl/simple_peridottroller.wasm" clg-admin
once CTRL_INIT "ctrl init" "$CTRL" clg-admin initialize --admin "$ADM"
once CTRL_ORACLE "ctrl oracle = Testnet Reflector external (CAFJ analogue)" "$CTRL" clg-admin set_oracle --oracle "$EXT_ORACLE"
once CTRL_USDC_SYMBOL "ctrl USDC -> Other(USDC)" "$CTRL" clg-admin set_oracle_asset_symbol --token "$USDC" --symbol '"USDC"'
deploy RECEIPT "$ART/receipt/receipt_vault.wasm" clg-admin
once RECEIPT_INIT "receipt init" "$RECEIPT" clg-admin initialize --token_address "$PYUSD" --supply_yearly_rate_scaled 0 --borrow_yearly_rate_scaled 0 --admin "$ADM"
once RECEIPT_BIND_CTRL "receipt bind ctrl" "$RECEIPT" clg-admin set_peridottroller --peridottroller "$CTRL"
once RECEIPT_RESERVE_100 "receipt reserve 100%" "$RECEIPT" clg-admin set_min_cash_reserve_bps --admin "$ADM" --bps 10000
once RECEIPT_FLOOR_CAP "receipt floor cap" "$RECEIPT" clg-admin set_supply_cap --cap 10000000
once RECEIPT_FLOOR_SEED "receipt floor seed" "$RECEIPT" clg-admin seed_minimum_supply --admin "$ADM" --seed_assets 10000000
once VAULT_BIND_CUSTODY "vault bind custody" "$VAULT" clg-admin stage_bind --admin_addr "$ADM" --receipt "$RECEIPT"
once VAULT_GUARDIAN "vault guardian" "$VAULT" clg-admin stage_configure_guardian --admin_addr "$ADM" --guardian "$ADM"
once VAULT_INVEST_KEEPER "vault invest keeper" "$VAULT" clg-admin stage_set_invest_keeper --admin_addr "$ADM" --keeper "$ADM"
once RECEIPT_STGV1_ACTIVATE "receipt StgV1 activate" "$RECEIPT" clg-admin stage_activate --admin "$ADM" --vault "$VAULT" --guardian "$ADM" --backing_cap 1000000000000
once RECEIPT_STGV2_ACTIVATE "receipt StgV2 activate" "$RECEIPT" clg-admin stage2_activate --admin "$ADM" --c "{\"controller\":\"$CTRL\",\"pool\":\"$POOL\",\"oracle\":\"$ORACLE\",\"usdc\":\"$USDC\"}"
deploy USDCM "$ART/cash/receipt_vault.wasm" clg-admin
once USDC_MARKET_INIT "usdc market init" "$USDCM" clg-admin initialize --token_address "$USDC" --supply_yearly_rate_scaled 0 --borrow_yearly_rate_scaled 0 --admin "$ADM"
once USDC_RESERVE_10 "usdc reserve 10%" "$USDCM" clg-admin set_min_cash_reserve_bps --admin "$ADM" --bps 1000
once USDC_BORROWING_ON "usdc borrowing on" "$USDCM" clg-admin set_cash_borrowing_enabled --admin "$ADM" --enabled true
once USDC_STATIC_8 "usdc static 8%" "$USDCM" clg-admin set_borrow_rate --yearly_rate_scaled 80000
once USDC_STATIC_ON "usdc static on" "$USDCM" clg-admin enable_static_rates --admin "$ADM"
once USDC_SEED "usdc seed" "$USDCM" clg-admin seed_minimum_supply --admin "$ADM" --seed_assets 10000
once USDC_BIND_CTRL "usdc bind ctrl" "$USDCM" clg-admin set_peridottroller --peridottroller "$CTRL"
once CTRL_LIST_USDC "ctrl list usdc" "$CTRL" clg-admin add_market --market "$USDCM"
once CTRL_LIST_RECEIPT "ctrl list receipt" "$CTRL" clg-admin add_market --market "$RECEIPT"
once CTRL_SLEEVE "ctrl sleeve" "$CTRL" clg-admin group_set_sleeve --market "$RECEIPT"
once CTRL_CF_50 "ctrl CF 50%" "$CTRL" clg-admin set_market_cf --market "$RECEIPT" --cf_scaled 500000
once CTRL_LIQ_CACHE_ONLY "ctrl liq cache-only" "$CTRL" clg-admin group_set_liq_cache_only --on true
once CTRL_BORROW_PAUSE_RECEIPT "ctrl borrow pause receipt" "$CTRL" clg-admin set_pause_borrow --market "$RECEIPT" --paused true
once SUPPLIER_DEPOSIT "supplier deposit 20,000 USDC" "$USDCM" clgr-sup deposit --user "$SUP" --amount 200000000000
echo "deployed:"; grep -E '^[A-Z0-9]+=C' "$STATE"
