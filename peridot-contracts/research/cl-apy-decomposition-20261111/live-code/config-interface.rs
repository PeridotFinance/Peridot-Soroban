#[soroban_sdk::contractargs(name = "Args")]
#[soroban_sdk::contractclient(name = "Client")]
pub trait Contract {
    fn version(env: soroban_sdk::Env) -> u32;
    fn get_value(
        env: soroban_sdk::Env,
        key: soroban_sdk::Val,
    ) -> Option<soroban_sdk::Val>;
    fn set_value(
        env: soroban_sdk::Env,
        admin: soroban_sdk::Address,
        key: soroban_sdk::Val,
        value: soroban_sdk::Val,
    );
    fn __constructor(
        env: soroban_sdk::Env,
        admin: soroban_sdk::Address,
        emergency_admin: soroban_sdk::Address,
    );
    fn apply_upgrade(
        env: soroban_sdk::Env,
        admin: soroban_sdk::Address,
    ) -> soroban_sdk::BytesN<32>;
    fn contract_name(env: soroban_sdk::Env) -> soroban_sdk::Symbol;
    fn commit_upgrade(
        env: soroban_sdk::Env,
        admin: soroban_sdk::Address,
        new_wasm_hash: soroban_sdk::BytesN<32>,
    );
    fn revert_upgrade(env: soroban_sdk::Env, admin: soroban_sdk::Address);
    fn get_emergency_mode(env: soroban_sdk::Env) -> bool;
    fn get_future_address(
        env: soroban_sdk::Env,
        role_name: soroban_sdk::Symbol,
    ) -> soroban_sdk::Address;
    fn set_emergency_mode(
        env: soroban_sdk::Env,
        emergency_admin: soroban_sdk::Address,
        value: bool,
    );
    fn get_value_updated_at(env: soroban_sdk::Env, key: soroban_sdk::Val) -> u64;
    fn apply_transfer_ownership(
        env: soroban_sdk::Env,
        admin: soroban_sdk::Address,
        role_name: soroban_sdk::Symbol,
    );
    fn commit_transfer_ownership(
        env: soroban_sdk::Env,
        admin: soroban_sdk::Address,
        role_name: soroban_sdk::Symbol,
        new_address: soroban_sdk::Address,
    );
    fn revert_transfer_ownership(
        env: soroban_sdk::Env,
        admin: soroban_sdk::Address,
        role_name: soroban_sdk::Symbol,
    );
}
#[soroban_sdk::contracterror(export = false)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum AccessControlError {
    RoleNotFound = 101,
    Unauthorized = 102,
    AdminAlreadySet = 103,
    BadRoleUsage = 104,
    AnotherActionActive = 2906,
    NoActionActive = 2907,
    ActionNotReadyYet = 2908,
}
#[soroban_sdk::contracterror(export = false)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum Error {
    AnotherActionActive = 2906,
    NoActionActive = 2907,
    ActionNotReadyYet = 2908,
}
#[soroban_sdk::contracterror(export = false)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum MathError {
    NumberOverflow = 510,
}
#[soroban_sdk::contracterror(export = false)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum StorageError {
    AlreadyInitialized = 201,
    ValueNotInitialized = 501,
    ValueMissing = 502,
    ValueConversionError = 503,
}

