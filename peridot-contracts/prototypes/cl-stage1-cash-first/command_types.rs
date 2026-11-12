// ISOLATED 1x prototype. No production ABI adoption.
#[soroban_sdk::contracttype]
#[derive(Clone, Debug)]
pub struct StageJob {
    pub owner: Address,
    pub kind: u32, // 0 open; 1 close
    pub phase: u32, // open 0 prepared,1 funded,2 converted,3 minted,4 terminal; close 10 locked,11 mixture,12 terminal
    pub deadline: u64,
    pub a: u128,
    pub b: u128,
    pub reserve: u128,
    pub swap_in: u128,
    pub min_swap: u128,
    pub min_shares: u128,
    pub shares: u128,
}
