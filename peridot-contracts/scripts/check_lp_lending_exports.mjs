// Read-only validation of the compiled receipt ABI; never signs or submits.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const path = process.argv[2];
assert(path, 'usage: node scripts/check_lp_lending_exports.mjs <receipt.wasm>');
const module = new WebAssembly.Module(readFileSync(path));
const actual = WebAssembly.Module.exports(module)
  .filter(({ kind }) => kind === 'function').map(({ name }) => name).sort();
const expected = `
__constructor accept_admin activate allowance approve balance borrow bump_ttl
claim_pool_rewards compound decimals deposit get_account_snapshot get_admin
get_boosted_vault get_exchange_rate get_ptoken_balance get_total_borrowed
get_total_ptokens get_total_underlying get_underlying_token get_user_balance
get_user_borrow_balance initialize_rewards is_active lp_version name payout_rewards
propose_upgrade_wasm rebalance_idle_cash recycle refresh_boosted_underlying
register_reward repay repay_max repay_on_behalf reward_earned reward_reserved
seize set_admin set_boosted_vault set_borrow_cap set_idle_cash_buffer_bps
set_interest_model set_peridottroller set_supply_cap settle_reserved symbol
target_collateral_factor total_supply transfer transfer_from update_interest
upgrade_wasm withdraw withdraw_with_minimum
`.trim().split(/\s+/).sort();
assert.deepEqual(actual, expected, 'receipt ABI differs from reviewed allowlist');
console.log(`PASS: ${actual.length} explicit LP lending exports; no inherited selectors`);
