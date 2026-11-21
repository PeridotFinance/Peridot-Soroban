"""Materialise the StgV2 receipt: <base> (the run-5 staged receipt tree: 4c98bdd3 receipt + StgV1
overlay, option B) + staged_v2.rs + d3_chain.rs + additive hooks. No accepted line is removed.

usage (from peridot-contracts/):
  python3 -I prototypes/cl-collateral-group/receipt-v2/apply-stgv2.py --base <tree> --out <tree>
<tree> holds peridot-contracts/ and openzeppelin-stellar-contracts/. Writes <out>/stgv2.diff.
No build, network or signing."""
from pathlib import Path
import argparse, hashlib, json, shutil, subprocess
HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
D3 = HERE.parent / 'd3-chain/src/d3_chain.rs'
REL = 'peridot-contracts/contracts/receipt-vault'
# StgV1 staged.rs as frozen in run-5 (receipt 140dcaab when built at the run-5 path).
STAGED_SHA = '522e34022bbf5f4fb8c2527d04af7688a31608f107ef5adf9061b65a275c75bf'
p = argparse.ArgumentParser(); p.add_argument('--base', type=Path, required=True); p.add_argument('--out', type=Path, required=True)
a = p.parse_args()
base, out = a.base.resolve(), a.out.resolve()
assert out.is_relative_to(ROOT / 'target'), out
src0 = base / REL
assert hashlib.sha256((src0 / 'src/staged.rs').read_bytes()).hexdigest() == STAGED_SHA, 'base staged.rs drifted'
if out.exists(): shutil.rmtree(out)
shutil.copytree(base, out, ignore=shutil.ignore_patterns('target'))
dst = out / REL
shutil.copy2(HERE / 'staged_v2.rs', dst / 'src/staged_v2.rs')
shutil.copy2(D3, dst / 'src/d3_chain.rs')

def patch(path, edits, prepend=False):
    s = path.read_text()
    for anchor, add, count in edits:
        assert s.count(anchor) == count, (path.name, anchor[:80], s.count(anchor))
        s = s.replace(anchor, (add + anchor) if prepend else (anchor + add))
    path.write_text(s)

G = '        #[cfg(not(feature = "lp-engine"))]\n'
def ret(expr): return G + f'        if staged_v2::stg2_active(&env) {{ return {expr}; }}\n'
patch(dst / 'src/contract.rs', [
    ('    pub fn get_account_snapshot(env: Env, user: Address) -> (u128, u128, u128, Address) {\n', ret('staged_v2::stg2_account_snapshot(&env, &user)'), 1),
    ('    pub fn get_exchange_rate(env: Env) -> u128 {\n', ret('staged_v2::stg2_exchange_rate(&env)'), 1),
    ('    pub fn get_ptoken_balance(env: Env, user: Address) -> u128 {\n', ret('staged_v2::stg2_units(&env, &user)'), 1),
    ('    pub fn get_underlying_token(env: Env) -> Address {\n', ret('staged_v2::stg2_numeraire(&env)'), 1),
    ('    pub fn get_total_ptokens(env: Env) -> u128 {\n', ret('staged_v2::stg2_total_units(&env)'), 1),
    ('    pub fn get_available_liquidity(env: Env) -> u128 {\n', ret('0'), 1),
    ('    pub fn get_user_borrow_balance(env: Env, user: Address) -> u128 {\n', ret('0'), 1),
    ('    pub fn get_total_borrowed(env: Env) -> u128 {\n', ret('0'), 1),
    ('    pub fn update_interest(env: Env) {\n', ret('()'), 1),
    ('    pub fn seize(\n        env: Env,\n        borrower: Address,\n        liquidator: Address,\n        ptoken_amount: u128,\n        ctx: Option<SeizeContext>,\n    ) {\n',
     ret('staged_v2::stg2_seize(&env, borrower, liquidator, ptoken_amount, ctx)'), 1),
    ('    pub fn set_peridottroller(env: Env, peridottroller: Address) {\n',
     G + '        assert!(!staged_v2::stg2_active(&env), "StgV2: controller binding is fixed");\n', 1),
])
# Upgradeability (21 Nov 2026, UPGRADEABILITY.md): cancel path, pending view and events, same 24 h
# propose/execute shape as the cash markets (.wt-cash-upgrade receipt-vault). Additive anchors only.
patch(dst / 'src/contract.rs', [
    ('    }\n\n    /// Admin: execute a staged upgrade once timelock has elapsed.\n',
     '        env.events().publish((Symbol::new(&env, "upgrade_proposed"),), (new_wasm_hash, execute_after));\n', 1),
    ('        env.deployer().update_current_contract_wasm(new_wasm_hash);\n',
     '        env.events().publish((Symbol::new(&env, "upgraded"),), new_wasm_hash.clone());\n', 1),
    ('    /// Admin: transfer admin to new address\n', '''    /// Admin: cancel a staged upgrade (removes the pending hash and ETA).
    pub fn cancel_upgrade(env: Env) {
        let _ = ensure_initialized(&env);
        let admin: Address = env.storage().persistent().get(&DataKey::Admin).expect("admin not set");
        admin.require_auth();
        let persistent = env.storage().persistent();
        let h: soroban_sdk::BytesN<32> = persistent.get(&DataKey::PendingUpgradeHash).expect("pending upgrade not set");
        persistent.remove(&DataKey::PendingUpgradeHash);
        persistent.remove(&DataKey::PendingUpgradeEta);
        env.events().publish((Symbol::new(&env, "upgrade_cancelled"),), h);
    }

    pub fn get_pending_upgrade(env: Env) -> Option<(soroban_sdk::BytesN<32>, u64)> {
        let persistent = env.storage().persistent();
        let h: Option<soroban_sdk::BytesN<32>> = persistent.get(&DataKey::PendingUpgradeHash);
        h.map(|h| (h, persistent.get(&DataKey::PendingUpgradeEta).unwrap()))
    }

''', 1),
], prepend=True)
c = dst / 'src/contract.rs'
c.write_text(c.read_text() + '\n// StgV2 CL collateral class (prototype overlay; see prototypes/cl-collateral-group/receipt-v2).\n'
    '#[cfg(not(feature = "lp-engine"))]\n#[path = "staged_v2.rs"]\nmod staged_v2;\n#[cfg(not(feature = "lp-engine"))]\npub use staged_v2::*;\n'
    + '// Test-only v2\' build (`--features stg2-upgrade-marker`) for the upgrade state-survival test.\n'
    '#[cfg(all(feature = "stg2-upgrade-marker", not(feature = "lp-engine")))]\n#[contractimpl]\nimpl ReceiptVault {\n    pub fn upgrade_marker(_env: Env) -> u32 {\n        2\n    }\n}\n')
st = dst / 'src/staged.rs'
# Exit guard: prepare-close and emergency exit both start from the owner's available units.
patch(st, [('        stg_deadline_ok(&e, deadline);\n        let available = stg_units(&e, &owner);\n', '        super::staged_v2::stg2_exit_check(&e, &owner, available, shares);\n', 1),
            ('        assert!(shares > 0, "invalid emergency exit");\n        let available = stg_units(&e, &owner);\n', '        super::staged_v2::stg2_emergency_check(&e, &owner);\n', 1)])
st.write_text(st.read_text() + '''
// ---- StgV2 accessors (appended; base code above unchanged) ----
pub(super) fn stg_vault(e: &Env) -> Address { stg_config(e).vault }
pub(super) fn stg_units_pub(e: &Env, owner: &Address) -> u128 { stg_units(e, owner) }
pub(super) fn stg_set_units_pub(e: &Env, owner: &Address, n: u128) { stg_set_units(e, owner, n) }
pub(super) fn stg_total_units(e: &Env) -> u128 { stg_state(e).total_units }
/// No StgV2 path runs inside a staged command (reentrancy domain shared with StgV1).
pub(super) fn stg_require_idle(e: &Env) {
    assert!(!e.storage().instance().get::<_, bool>(&StgKey::StgV1Busy).unwrap_or(false), "staged reentry");
}
''')
patch(dst / 'Cargo.toml', [('stg-mainnet-accepted = []\n',
    '# Explicit Mainnet acceptance of the StgV2 collateral class. Default builds refuse activation on Mainnet.\nstg2-mainnet-accepted = []\n# Measurement-only D3 chain seeding (never a deployment candidate).\nstg2-measure = []\n# Test-only v2-prime marker build (upgrade state-survival test). Never a release feature.\nstg2-upgrade-marker = []\n', 1)])
diff = subprocess.run(['diff', '-ruN', str(src0), str(dst)], capture_output=True, text=True).stdout.replace(str(base) + '/', 'base/').replace(str(out) + '/', 'stgv2/')
(out / 'stgv2.diff').write_text(diff)
removed = [l for l in diff.splitlines() if l.startswith('-') and not l.startswith('---')]
assert not removed, 'StgV2 overlay must not remove any accepted line'
pins = {str(f.relative_to(out)): hashlib.sha256(f.read_bytes()).hexdigest() for f in sorted(dst.joinpath('src').glob('*.rs'))}
(out / 'stgv2-pins.json').write_text(json.dumps(pins, indent=1) + '\n')
print(json.dumps({'added_lines': sum(1 for l in diff.splitlines() if l.startswith('+') and not l.startswith('+++')), 'removed_lines': len(removed)}))
