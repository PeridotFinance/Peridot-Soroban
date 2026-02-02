# Peridot Finance: Smart Accounts Technical Architecture
## Soroban Implementation Specification v1.0

---

## EXECUTIVE SUMMARY

This document specifies the technical architecture for integrating Stellar's native Smart Accounts (contract accounts) into Peridot Finance's lending and leveraged margin trading protocol on Soroban. The implementation leverages Soroban's `CustomAccountInterface` to provide:

1. **Automated risk management** - Prevent liquidations through autonomous position adjustment
2. **Leverage controls** - Enforce max leverage ratios and concentration limits per account
3. **Session-based trading** - Enable high-frequency trading without constant wallet signatures
4. **Multi-signature governance** - Institutional-grade controls for large positions
5. **Protocol integration** - Seamless compatibility with existing Peridot vault contracts

**Key Architectural Principle**: Smart Accounts intercept `require_auth()` calls in existing Peridot contracts without requiring protocol modifications. All policy enforcement occurs at the account layer.

---

## SYSTEM ARCHITECTURE OVERVIEW

### High-Level Component Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                     USER INTERFACE LAYER                     │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│  │   Web App    │  │  Mobile App  │  │  Trading Bot │      │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘      │
└─────────┼──────────────────┼──────────────────┼─────────────┘
          │                  │                  │
          └──────────────────┼──────────────────┘
                             │
┌────────────────────────────┼─────────────────────────────────┐
│                    SMART ACCOUNT LAYER                        │
│  ┌─────────────────────────▼────────────────────────┐        │
│  │        Smart Account Factory Contract             │        │
│  └──────────┬──────────────────────────┬─────────────┘        │
│             │                          │                       │
│  ┌──────────▼──────────┐    ┌─────────▼────────────┐         │
│  │  BasicMarginAccount │    │  ProMarginAccount    │         │
│  │  - 3x max leverage  │    │  - 10x max leverage  │         │
│  │  - Auto-deleverage  │    │  - Session keys      │         │
│  │  - Single sig       │    │  - Custom policies   │         │
│  └──────────┬──────────┘    └──────────┬───────────┘         │
│             │                           │                      │
│             └───────────┬───────────────┘                      │
│                         │ require_auth() intercepted           │
└─────────────────────────┼──────────────────────────────────────┘
                          │
┌─────────────────────────▼──────────────────────────────────────┐
│                  PERIDOT PROTOCOL LAYER                         │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐         │
│  │ Vault/Lending│  │  Liquidator  │  │   Oracle     │         │
│  │   Contract   │  │   Contract   │  │   Contract   │         │
│  └──────────────┘  └──────────────┘  └──────────────┘         │
│                                                                 │
│  ┌──────────────────────────────────────────────────┐          │
│  │         Margin Trading / Perpetuals              │          │
│  │  - Position Management                           │          │
│  │  - Leverage Calculation                          │          │
│  │  - Interest Rate Updates                         │          │
│  └──────────────────────────────────────────────────┘          │
└─────────────────────────────────────────────────────────────────┘
                          │
┌─────────────────────────▼──────────────────────────────────────┐
│                   SOROBAN RUNTIME                               │
│  - Native authorization framework                               │
│  - __check_auth host function invocation                       │
│  - Signature verification (ed25519, secp256r1)                 │
└─────────────────────────────────────────────────────────────────┘
```

### Data Flow: Borrowing with Smart Account

```
1. User submits borrow transaction
   ↓
2. Transaction includes authorization entry for Smart Account
   ↓
3. Soroban runtime calls vault.borrow(smart_account_address, amount)
   ↓
4. Vault contract executes: smart_account_address.require_auth()
   ↓
5. Soroban host invokes SmartAccount::__check_auth()
   ↓
6. Smart Account verifies signatures
   ↓
7. Smart Account checks leverage limits, health factor, policies
   ↓
8. If authorized: returns Ok(()) → vault.borrow() continues
   If denied: returns Err() → transaction reverts
   ↓
9. Vault contract completes borrow logic
```

---

## CORE COMPONENTS SPECIFICATION

### 1. SMART ACCOUNT FACTORY

**Purpose**: Deploy and manage Smart Account instances for users

**Contract Address**: `SMART_ACCOUNT_FACTORY_CONTRACT_ID`

#### Interface

```rust
#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, BytesN, Vec};

#[contract]
pub struct SmartAccountFactory;

#[contracttype]
pub enum AccountType {
    Basic,      // 3x leverage, auto-deleverage
    Pro,        // 10x leverage, session keys
    Institutional, // Custom multisig
}

#[contracttype]
pub struct AccountConfig {
    pub account_type: AccountType,
    pub owner: Address,
    pub signers: Vec<BytesN<32>>,
    pub max_leverage: u32,
    pub auto_deleverage_threshold: u32, // Health factor in basis points (e.g., 12000 = 1.2)
}

#[contractimpl]
impl SmartAccountFactory {
    /// Deploy a new Smart Account for a user
    pub fn create_account(
        env: Env,
        config: AccountConfig,
        salt: BytesN<32>,
    ) -> Address {
        // Verify caller is the owner
        config.owner.require_auth();
        
        // Deploy appropriate contract based on type
        let wasm_hash = match config.account_type {
            AccountType::Basic => get_basic_account_wasm_hash(&env),
            AccountType::Pro => get_pro_account_wasm_hash(&env),
            AccountType::Institutional => get_institutional_account_wasm_hash(&env),
        };
        
        // Deploy contract with constructor arguments
        let deployed_address = env
            .deployer()
            .with_current_contract(salt)
            .deploy(wasm_hash);
        
        // Initialize the account
        let account_client = get_account_client(&env, &deployed_address);
        account_client.initialize(&config);
        
        // Store mapping for lookup
        env.storage().instance().set(
            &DataKey::UserAccount(config.owner.clone()),
            &deployed_address
        );
        
        // Emit event
        env.events().publish(
            (symbol_short!("create"), config.owner),
            deployed_address.clone()
        );
        
        deployed_address
    }
    
    /// Upgrade a user's account to a higher tier
    pub fn upgrade_account(
        env: Env,
        user: Address,
        new_type: AccountType,
    ) -> Result<(), Error> {
        user.require_auth();
        
        let account_addr: Address = env.storage().instance()
            .get(&DataKey::UserAccount(user.clone()))
            .ok_or(Error::AccountNotFound)?;
        
        // Deploy new account and migrate state
        // Implementation depends on upgrade strategy
        Ok(())
    }
    
    /// Get Smart Account address for a user
    pub fn get_account(env: Env, user: Address) -> Option<Address> {
        env.storage().instance().get(&DataKey::UserAccount(user))
    }
}
```

#### Storage Schema

```rust
#[contracttype]
pub enum DataKey {
    UserAccount(Address),           // user_address -> smart_account_address
    AccountCount,                    // Total accounts created
    WasmHash(AccountType),          // Account type -> WASM hash
}
```

---

### 2. BASIC MARGIN ACCOUNT

**Purpose**: Entry-level Smart Account with conservative leverage limits and automatic risk management

**Features**:
- Maximum 3x leverage
- Automatic deleveraging at 1.2 health factor
- Single-signature authorization
- Per-market concentration limits

#### Full Implementation

```rust
#![no_std]
use soroban_sdk::{
    auth::{Context, ContractContext, CustomAccountInterface},
    contract, contractimpl, contracttype, symbol_short,
    Address, BytesN, Env, Hash, Map, Vec, Symbol,
};

#[contract]
pub struct BasicMarginAccount;

// ============================================================================
// DATA STRUCTURES
// ============================================================================

#[contracttype]
pub enum DataKey {
    Owner,                          // Primary owner address
    Signer(BytesN<32>),            // Authorized signers
    MaxLeverage,                    // Max leverage in basis points (300 = 3x)
    AutoDeleverageThreshold,        // Health factor threshold (1200 = 1.2)
    MarketExposureLimit,            // Max exposure per market (in USD)
    TotalBorrowLimit,              // Max total borrows across all assets
    VaultContract,                  // Peridot vault contract address
    OracleContract,                 // Price oracle contract address
}

#[contracttype]
pub struct Signature {
    pub public_key: BytesN<32>,
    pub signature: BytesN<64>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    Unauthorized = 1,
    InvalidSignature = 2,
    ExcessiveLeverage = 3,
    InsufficientHealth = 4,
    MarketConcentration = 5,
    TotalBorrowExceeded = 6,
    NotInitialized = 7,
}

// ============================================================================
// INITIALIZATION
// ============================================================================

#[contractimpl]
impl BasicMarginAccount {
    /// Initialize the Smart Account (called once by factory)
    pub fn __constructor(
        env: Env,
        owner: Address,
        signer: BytesN<32>,
        vault_contract: Address,
        oracle_contract: Address,
    ) {
        // Set owner
        env.storage().instance().set(&DataKey::Owner, &owner);
        
        // Set initial signer
        env.storage().instance().set(&DataKey::Signer(signer), &true);
        
        // Set default parameters for Basic account
        env.storage().instance().set(&DataKey::MaxLeverage, &300_u32); // 3x
        env.storage().instance().set(&DataKey::AutoDeleverageThreshold, &1200_u32); // 1.2
        env.storage().instance().set(&DataKey::MarketExposureLimit, &50_000_i128); // $50k per market
        env.storage().instance().set(&DataKey::TotalBorrowLimit, &100_000_i128); // $100k total
        
        // Store contract references
        env.storage().instance().set(&DataKey::VaultContract, &vault_contract);
        env.storage().instance().set(&DataKey::OracleContract, &oracle_contract);
    }
    
    /// Update account parameters (requires owner auth)
    pub fn update_params(
        env: Env,
        max_leverage: Option<u32>,
        auto_deleverage_threshold: Option<u32>,
        market_exposure_limit: Option<i128>,
    ) -> Result<(), Error> {
        let owner: Address = env.storage().instance()
            .get(&DataKey::Owner)
            .ok_or(Error::NotInitialized)?;
        owner.require_auth();
        
        if let Some(leverage) = max_leverage {
            env.storage().instance().set(&DataKey::MaxLeverage, &leverage);
        }
        if let Some(threshold) = auto_deleverage_threshold {
            env.storage().instance().set(&DataKey::AutoDeleverageThreshold, &threshold);
        }
        if let Some(limit) = market_exposure_limit {
            env.storage().instance().set(&DataKey::MarketExposureLimit, &limit);
        }
        
        Ok(())
    }
}

// ============================================================================
// CUSTOM ACCOUNT INTERFACE IMPLEMENTATION
// ============================================================================

#[contractimpl]
impl CustomAccountInterface for BasicMarginAccount {
    type Signature = Vec<Signature>;
    type Error = Error;
    
    fn __check_auth(
        env: Env,
        signature_payload: Hash<32>,
        signatures: Self::Signature,
        auth_contexts: Vec<Context>,
    ) -> Result<(), Self::Error> {
        // STEP 1: AUTHENTICATE - Verify signatures
        Self::verify_signatures(&env, &signature_payload, &signatures)?;
        
        // STEP 2: AUTHORIZE - Enforce policies based on what's being authorized
        Self::enforce_policies(&env, &auth_contexts)?;
        
        Ok(())
    }
}

// ============================================================================
// AUTHENTICATION LOGIC
// ============================================================================

impl BasicMarginAccount {
    /// Verify all signatures are from authorized signers
    fn verify_signatures(
        env: &Env,
        signature_payload: &Hash<32>,
        signatures: &Vec<Signature>,
    ) -> Result<(), Error> {
        if signatures.is_empty() {
            return Err(Error::Unauthorized);
        }
        
        for sig in signatures.iter() {
            // Check if this public key is an authorized signer
            let is_authorized: bool = env.storage().instance()
                .get(&DataKey::Signer(sig.public_key.clone()))
                .unwrap_or(false);
            
            if !is_authorized {
                return Err(Error::Unauthorized);
            }
            
            // Verify ed25519 signature
            env.crypto().ed25519_verify(
                &sig.public_key,
                &signature_payload.clone().into(),
                &sig.signature,
            );
        }
        
        Ok(())
    }
}

// ============================================================================
// AUTHORIZATION POLICY ENFORCEMENT
// ============================================================================

impl BasicMarginAccount {
    /// Enforce risk management policies on authorized operations
    fn enforce_policies(
        env: &Env,
        auth_contexts: &Vec<Context>,
    ) -> Result<(), Error> {
        for ctx in auth_contexts.iter() {
            if let Context::Contract(contract_ctx) = ctx {
                Self::enforce_contract_policy(env, contract_ctx)?;
            }
        }
        Ok(())
    }
    
    fn enforce_contract_policy(
        env: &Env,
        ctx: &ContractContext,
    ) -> Result<(), Error> {
        let fn_name = ctx.fn_name.to_string();
        
        match fn_name.as_str() {
            // BORROW OPERATIONS
            "borrow" | "borrow_asset" => {
                Self::check_borrow_policy(env, ctx)?;
            }
            
            // MARGIN TRADING OPERATIONS
            "open_position" | "increase_position" => {
                Self::check_position_policy(env, ctx)?;
            }
            
            // ADMINISTRATIVE OPERATIONS (always allowed for owner)
            "repay" | "close_position" | "withdraw" => {
                // These reduce risk, always allowed
            }
            
            _ => {
                // Unknown operation - allow but log
            }
        }
        
        Ok(())
    }
    
    /// Check leverage and health factor for borrow operations
    fn check_borrow_policy(
        env: &Env,
        ctx: &ContractContext,
    ) -> Result<(), Error> {
        // Extract borrow amount from contract arguments
        // Assuming: borrow(borrower: Address, asset: Address, amount: i128)
        let amount: i128 = ctx.args.get(2)
            .ok_or(Error::Unauthorized)?
            .try_into_val(env)
            .map_err(|_| Error::Unauthorized)?;
        
        let asset: Address = ctx.args.get(1)
            .ok_or(Error::Unauthorized)?
            .try_into_val(env)
            .map_err(|_| Error::Unauthorized)?;
        
        // Get current account state
        let current_collateral = Self::get_total_collateral_usd(env)?;
        let current_borrows = Self::get_total_borrows_usd(env)?;
        let asset_price = Self::get_asset_price(env, &asset)?;
        let new_borrow_usd = (amount * asset_price) / 1_000_000; // Assume 6 decimals
        
        // POLICY 1: Check total borrow limit
        let total_borrow_limit: i128 = env.storage().instance()
            .get(&DataKey::TotalBorrowLimit)
            .unwrap_or(100_000);
        
        if current_borrows + new_borrow_usd > total_borrow_limit {
            return Err(Error::TotalBorrowExceeded);
        }
        
        // POLICY 2: Check leverage ratio
        let max_leverage: u32 = env.storage().instance()
            .get(&DataKey::MaxLeverage)
            .unwrap_or(300);
        
        let new_total_borrows = current_borrows + new_borrow_usd;
        let leverage_bp = (new_total_borrows * 100) / current_collateral; // in basis points
        
        if leverage_bp > max_leverage as i128 {
            return Err(Error::ExcessiveLeverage);
        }
        
        // POLICY 3: Check health factor after borrow
        let health_factor = Self::calculate_health_factor_after(
            env,
            current_collateral,
            new_total_borrows,
        )?;
        
        let min_health: u32 = env.storage().instance()
            .get(&DataKey::AutoDeleverageThreshold)
            .unwrap_or(1200);
        
        if health_factor < min_health {
            return Err(Error::InsufficientHealth);
        }
        
        // POLICY 4: Check per-market concentration
        let market_exposure = Self::get_market_exposure_usd(env, &ctx.contract)?;
        let market_limit: i128 = env.storage().instance()
            .get(&DataKey::MarketExposureLimit)
            .unwrap_or(50_000);
        
        if market_exposure + new_borrow_usd > market_limit {
            return Err(Error::MarketConcentration);
        }
        
        Ok(())
    }
    
    /// Check leverage limits for opening/increasing positions
    fn check_position_policy(
        env: &Env,
        ctx: &ContractContext,
    ) -> Result<(), Error> {
        // Similar to borrow policy but for perpetual positions
        // Extract position size and market
        let position_size: i128 = ctx.args.get(1)
            .ok_or(Error::Unauthorized)?
            .try_into_val(env)
            .map_err(|_| Error::Unauthorized)?;
        
        let market: Address = ctx.args.get(0)
            .ok_or(Error::Unauthorized)?
            .try_into_val(env)
            .map_err(|_| Error::Unauthorized)?;
        
        // Calculate total exposure including this position
        let current_collateral = Self::get_total_collateral_usd(env)?;
        let current_exposure = Self::get_total_position_exposure(env)?;
        let new_exposure = current_exposure + position_size;
        
        // Check leverage
        let leverage = (new_exposure * 100) / current_collateral;
        let max_leverage: u32 = env.storage().instance()
            .get(&DataKey::MaxLeverage)
            .unwrap_or(300);
        
        if leverage > max_leverage as i128 {
            return Err(Error::ExcessiveLeverage);
        }
        
        Ok(())
    }
}

// ============================================================================
// HELPER FUNCTIONS - INTEGRATION WITH PERIDOT PROTOCOL
// ============================================================================

impl BasicMarginAccount {
    /// Get total collateral value in USD from Peridot vault
    fn get_total_collateral_usd(env: &Env) -> Result<i128, Error> {
        let vault_contract: Address = env.storage().instance()
            .get(&DataKey::VaultContract)
            .ok_or(Error::NotInitialized)?;
        
        let account_address = env.current_contract_address();
        
        // Call Peridot vault to get collateral
        // Assuming: vault.get_collateral_value(user: Address) -> i128
        let vault_client = create_vault_client(env, &vault_contract);
        let collateral_usd = vault_client.get_collateral_value(&account_address);
        
        Ok(collateral_usd)
    }
    
    /// Get total borrows in USD from Peridot vault
    fn get_total_borrows_usd(env: &Env) -> Result<i128, Error> {
        let vault_contract: Address = env.storage().instance()
            .get(&DataKey::VaultContract)
            .ok_or(Error::NotInitialized)?;
        
        let account_address = env.current_contract_address();
        
        // Call Peridot vault to get borrow balance
        let vault_client = create_vault_client(env, &vault_contract);
        let borrows_usd = vault_client.get_borrow_value(&account_address);
        
        Ok(borrows_usd)
    }
    
    /// Get asset price from oracle
    fn get_asset_price(env: &Env, asset: &Address) -> Result<i128, Error> {
        let oracle_contract: Address = env.storage().instance()
            .get(&DataKey::OracleContract)
            .ok_or(Error::NotInitialized)?;
        
        let oracle_client = create_oracle_client(env, &oracle_contract);
        let price = oracle_client.get_price(asset);
        
        Ok(price)
    }
    
    /// Calculate health factor: (collateral * LTV) / borrows
    fn calculate_health_factor_after(
        env: &Env,
        collateral_usd: i128,
        total_borrows_usd: i128,
    ) -> Result<u32, Error> {
        if total_borrows_usd == 0 {
            return Ok(u32::MAX); // Infinite health
        }
        
        // Assume 80% LTV for simplicity
        let ltv_bp = 8000_u32; // 80%
        let adjusted_collateral = (collateral_usd * ltv_bp as i128) / 10000;
        
        // Health factor in basis points
        let health_bp = ((adjusted_collateral * 10000) / total_borrows_usd) as u32;
        
        Ok(health_bp)
    }
    
    /// Get exposure to a specific market
    fn get_market_exposure_usd(env: &Env, market: &Address) -> Result<i128, Error> {
        let vault_contract: Address = env.storage().instance()
            .get(&DataKey::VaultContract)
            .ok_or(Error::NotInitialized)?;
        
        let account_address = env.current_contract_address();
        
        let vault_client = create_vault_client(env, &vault_contract);
        let exposure = vault_client.get_market_borrow(&account_address, market);
        
        Ok(exposure)
    }
    
    /// Get total position exposure for perpetuals
    fn get_total_position_exposure(env: &Env) -> Result<i128, Error> {
        // This would call your perpetuals contract
        // For now, return 0
        Ok(0)
    }
}

// ============================================================================
// AUTO-DELEVERAGING (KEEPER CALLABLE)
// ============================================================================

#[contractimpl]
impl BasicMarginAccount {
    /// Automatically reduce position when approaching liquidation
    /// Can be called by anyone (keepers/bots)
    pub fn auto_deleverage(env: Env) -> Result<(), Error> {
        let current_collateral = Self::get_total_collateral_usd(&env)?;
        let current_borrows = Self::get_total_borrows_usd(&env)?;
        
        let health_factor = Self::calculate_health_factor_after(
            &env,
            current_collateral,
            current_borrows,
        )?;
        
        let threshold: u32 = env.storage().instance()
            .get(&DataKey::AutoDeleverageThreshold)
            .unwrap_or(1200);
        
        if health_factor < threshold {
            // Smart account authorizes itself to repay
            env.current_contract_address().require_auth();
            
            // Calculate safe repayment amount (reduce to 1.5 health)
            let target_health = 1500_u32; // 1.5x
            let target_borrows = (current_collateral * 8000) / target_health as i128;
            let repay_amount = current_borrows - target_borrows;
            
            // Call Peridot vault to repay
            let vault_contract: Address = env.storage().instance()
                .get(&DataKey::VaultContract)
                .ok_or(Error::NotInitialized)?;
            
            let vault_client = create_vault_client(&env, &vault_contract);
            vault_client.repay(&env.current_contract_address(), &repay_amount);
            
            // Emit event
            env.events().publish(
                (symbol_short!("auto_dlv"),),
                (health_factor, repay_amount)
            );
        }
        
        Ok(())
    }
}

// ============================================================================
// CLIENT HELPER FUNCTIONS (PSEUDO-CODE)
// ============================================================================

// These would be properly implemented with contract clients
fn create_vault_client(env: &Env, address: &Address) -> VaultClient {
    // peridot_vault::Client::new(env, address)
    unimplemented!()
}

fn create_oracle_client(env: &Env, address: &Address) -> OracleClient {
    // oracle::Client::new(env, address)
    unimplemented!()
}

// Placeholder client structs
struct VaultClient;
impl VaultClient {
    fn get_collateral_value(&self, _user: &Address) -> i128 { 0 }
    fn get_borrow_value(&self, _user: &Address) -> i128 { 0 }
    fn get_market_borrow(&self, _user: &Address, _market: &Address) -> i128 { 0 }
    fn repay(&self, _user: &Address, _amount: &i128) {}
}

struct OracleClient;
impl OracleClient {
    fn get_price(&self, _asset: &Address) -> i128 { 1_000_000 }
}
```

---

### 3. PRO MARGIN ACCOUNT

**Purpose**: Advanced Smart Account with session keys and higher leverage limits

**Additional Features**:
- Maximum 10x leverage
- Session key management with time and amount limits
- Custom per-token policies
- Advanced position strategies

#### Session Key Implementation

```rust
#[contracttype]
pub struct SessionKey {
    pub key: BytesN<32>,
    pub expires_at: u64,              // Ledger timestamp
    pub max_amount_per_tx: i128,      // Max amount per transaction
    pub daily_limit: i128,            // Max total amount per day
    pub allowed_operations: Vec<Symbol>, // Whitelisted functions
}

#[contracttype]
pub enum DataKey {
    // ... Basic account keys ...
    SessionKey(BytesN<32>),           // session_key -> SessionKey
    SessionDailyUsage(BytesN<32>, u64), // (session_key, day) -> amount_used
}

#[contractimpl]
impl ProMarginAccount {
    /// Add a session key (owner only)
    pub fn add_session_key(
        env: Env,
        key: BytesN<32>,
        expires_at: u64,
        max_per_tx: i128,
        daily_limit: i128,
        allowed_ops: Vec<Symbol>,
    ) -> Result<(), Error> {
        let owner: Address = env.storage().instance()
            .get(&DataKey::Owner)
            .ok_or(Error::NotInitialized)?;
        owner.require_auth();
        
        let session = SessionKey {
            key: key.clone(),
            expires_at,
            max_amount_per_tx: max_per_tx,
            daily_limit,
            allowed_operations: allowed_ops,
        };
        
        env.storage().instance().set(&DataKey::SessionKey(key), &session);
        Ok(())
    }
    
    /// Revoke a session key
    pub fn revoke_session_key(env: Env, key: BytesN<32>) -> Result<(), Error> {
        let owner: Address = env.storage().instance()
            .get(&DataKey::Owner)
            .ok_or(Error::NotInitialized)?;
        owner.require_auth();
        
        env.storage().instance().remove(&DataKey::SessionKey(key));
        Ok(())
    }
}

// Modified __check_auth to support session keys
#[contractimpl]
impl CustomAccountInterface for ProMarginAccount {
    type Signature = Vec<Signature>;
    type Error = Error;
    
    fn __check_auth(
        env: Env,
        signature_payload: Hash<32>,
        signatures: Self::Signature,
        auth_contexts: Vec<Context>,
    ) -> Result<(), Self::Error> {
        // Try session key authentication first
        if signatures.len() == 1 {
            if let Ok(()) = Self::verify_session_key(&env, &signatures, &auth_contexts) {
                return Ok(());
            }
        }
        
        // Fall back to standard signature verification
        Self::verify_signatures(&env, &signature_payload, &signatures)?;
        Self::enforce_policies(&env, &auth_contexts)?;
        
        Ok(())
    }
}

impl ProMarginAccount {
    fn verify_session_key(
        env: &Env,
        signatures: &Vec<Signature>,
        auth_contexts: &Vec<Context>,
    ) -> Result<(), Error> {
        let sig = signatures.get(0).ok_or(Error::Unauthorized)?;
        
        // Check if this is a valid session key
        let session: SessionKey = env.storage().instance()
            .get(&DataKey::SessionKey(sig.public_key.clone()))
            .ok_or(Error::Unauthorized)?;
        
        // Check expiration
        if env.ledger().timestamp() > session.expires_at {
            return Err(Error::SessionExpired);
        }
        
        // Verify session key limits
        for ctx in auth_contexts.iter() {
            if let Context::Contract(c) = ctx {
                Self::check_session_limits(env, &session, c)?;
            }
        }
        
        Ok(())
    }
    
    fn check_session_limits(
        env: &Env,
        session: &SessionKey,
        ctx: &ContractContext,
    ) -> Result<(), Error> {
        // Check if operation is allowed
        if !session.allowed_operations.contains(&ctx.fn_name) {
            return Err(Error::OperationNotAllowed);
        }
        
        // Extract amount from context (varies by function)
        let amount = Self::extract_amount_from_context(env, ctx)?;
        
        // Check per-transaction limit
        if amount > session.max_amount_per_tx {
            return Err(Error::ExceedsSessionLimit);
        }
        
        // Check daily limit
        let current_day = env.ledger().timestamp() / 86400;
        let daily_key = DataKey::SessionDailyUsage(session.key.clone(), current_day);
        
        let used_today: i128 = env.storage().temporary()
            .get(&daily_key)
            .unwrap_or(0);
        
        if used_today + amount > session.daily_limit {
            return Err(Error::DailyLimitExceeded);
        }
        
        // Update daily usage
        env.storage().temporary().set(&daily_key, &(used_today + amount));
        
        Ok(())
    }
}
```

---

### 4. INSTITUTIONAL MARGIN ACCOUNT

**Purpose**: Enterprise-grade Smart Account with multisig and compliance features

**Additional Features**:
- M-of-N multisignature requirements
- Tiered signature requirements based on amount
- Market blacklisting for compliance
- Time-based trading restrictions
- Audit trail and event logging

#### Multisig Implementation

```rust
#[contracttype]
pub struct MultisigConfig {
    pub signers: Vec<BytesN<32>>,
    pub threshold: u32,               // Standard threshold
    pub large_amount_threshold: i128,  // Threshold for "large" operations
    pub large_amount_signatures: u32,  // Required sigs for large amounts
}

#[contracttype]
pub enum DataKey {
    // ... other keys ...
    MultisigConfig,
    BlacklistedMarket(Address),       // Markets not allowed
    TradingHoursStart,                // Daily trading window start (seconds)
    TradingHoursEnd,                  // Daily trading window end (seconds)
}

#[contractimpl]
impl CustomAccountInterface for InstitutionalAccount {
    type Signature = Vec<Signature>;
    type Error = Error;
    
    fn __check_auth(
        env: Env,
        signature_payload: Hash<32>,
        signatures: Self::Signature,
        auth_contexts: Vec<Context>,
    ) -> Result<(), Self::Error> {
        // Get multisig config
        let config: MultisigConfig = env.storage().instance()
            .get(&DataKey::MultisigConfig)
            .ok_or(Error::NotInitialized)?;
        
        // Verify all signatures are from authorized signers
        Self::verify_multisig_signatures(&env, &signature_payload, &signatures, &config)?;
        
        // Determine required signature count based on operation
        let required_sigs = Self::get_required_signatures(&env, &config, &auth_contexts)?;
        
        if signatures.len() < required_sigs as usize {
            return Err(Error::InsufficientSignatures);
        }
        
        // Enforce compliance policies
        Self::enforce_compliance_policies(&env, &auth_contexts)?;
        
        // Enforce standard risk policies
        Self::enforce_policies(&env, &auth_contexts)?;
        
        Ok(())
    }
}

impl InstitutionalAccount {
    fn verify_multisig_signatures(
        env: &Env,
        signature_payload: &Hash<32>,
        signatures: &Vec<Signature>,
        config: &MultisigConfig,
    ) -> Result<(), Error> {
        for sig in signatures.iter() {
            // Verify signer is authorized
            if !config.signers.contains(&sig.public_key) {
                return Err(Error::UnauthorizedSigner);
            }
            
            // Verify signature
            env.crypto().ed25519_verify(
                &sig.public_key,
                &signature_payload.clone().into(),
                &sig.signature,
            );
        }
        
        Ok(())
    }
    
    fn get_required_signatures(
        env: &Env,
        config: &MultisigConfig,
        auth_contexts: &Vec<Context>,
    ) -> Result<u32, Error> {
        // Check if any operation involves large amounts
        let mut is_large_operation = false;
        
        for ctx in auth_contexts.iter() {
            if let Context::Contract(c) = ctx {
                if let Ok(amount) = Self::extract_amount_from_context(env, c) {
                    if amount > config.large_amount_threshold {
                        is_large_operation = true;
                        break;
                    }
                }
            }
        }
        
        Ok(if is_large_operation {
            config.large_amount_signatures
        } else {
            config.threshold
        })
    }
    
    fn enforce_compliance_policies(
        env: &Env,
        auth_contexts: &Vec<Context>,
    ) -> Result<(), Error> {
        for ctx in auth_contexts.iter() {
            if let Context::Contract(c) = ctx {
                // Check market blacklist
                let is_blacklisted: bool = env.storage().instance()
                    .get(&DataKey::BlacklistedMarket(c.contract.clone()))
                    .unwrap_or(false);
                
                if is_blacklisted {
                    return Err(Error::BlacklistedMarket);
                }
                
                // Check trading hours
                if !Self::is_within_trading_hours(env)? {
                    return Err(Error::OutsideTradingHours);
                }
            }
        }
        
        Ok(())
    }
    
    fn is_within_trading_hours(env: &Env) -> Result<bool, Error> {
        let start: u64 = env.storage().instance()
            .get(&DataKey::TradingHoursStart)
            .unwrap_or(0);
        let end: u64 = env.storage().instance()
            .get(&DataKey::TradingHoursEnd)
            .unwrap_or(86400);
        
        let current_time = env.ledger().timestamp();
        let time_of_day = current_time % 86400;
        
        Ok(time_of_day >= start && time_of_day <= end)
    }
}
```

---

## INTEGRATION WITH PERIDOT VAULT CONTRACTS

### Vault Contract Modification (Minimal)

Your existing Peridot vault contracts require **zero changes** if they already use `require_auth()`:

```rust
// EXISTING PERIDOT VAULT CODE - NO CHANGES NEEDED
pub fn borrow(
    env: Env,
    borrower: Address,  // This can be a Smart Account address
    asset: Address,
    amount: i128,
) -> Result<(), VaultError> {
    // This automatically triggers SmartAccount::__check_auth() if borrower is a smart account
    borrower.require_auth();
    
    // Your existing logic
    let collateral_value = get_collateral_value(&env, &borrower)?;
    let borrow_value = get_borrow_value(&env, &borrower)?;
    
    // Check protocol-level limits
    if !is_sufficient_collateral(collateral_value, borrow_value + amount) {
        return Err(VaultError::InsufficientCollateral);
    }
    
    // Update state
    update_borrow_balance(&env, &borrower, &asset, amount)?;
    accrue_interest(&env, &asset)?;
    
    // Transfer tokens
    transfer_token(&env, &asset, &borrower, amount)?;
    
    Ok(())
}
```

### Optional: Add Smart Account Awareness

If you want to provide better UX or analytics:

```rust
#[contractimpl]
impl PeridotVault {
    /// Helper to check if an address is a Smart Account
    pub fn is_smart_account(env: Env, user: Address) -> bool {
        // Try to call a Smart Account function
        // If it succeeds, it's a smart account
        match env.try_invoke_contract::<()>(
            &user,
            &symbol_short!("get_owner"),
            Vec::new(&env),
        ) {
            Ok(_) => true,
            Err(_) => false,
        }
    }
    
    /// Get effective leverage for a user (works for both regular and smart accounts)
    pub fn get_user_leverage(env: Env, user: Address) -> i128 {
        let collateral = get_collateral_value(&env, &user);
        let borrows = get_borrow_value(&env, &user);
        
        if collateral == 0 {
            return 0;
        }
        
        (borrows * 100) / collateral // Leverage in percentage
    }
}
```

---

## DEPLOYMENT STRATEGY

### Phase 1: Testnet Deployment (Week 1-2)

```rust
// deployment_config.rs

pub struct DeploymentConfig {
    pub network: Network,
    pub admin: Address,
    pub vault_contract: Address,
    pub oracle_contract: Address,
}

pub async fn deploy_smart_account_system(config: DeploymentConfig) -> Result<DeploymentResult, Error> {
    // 1. Deploy Smart Account WASM binaries
    let basic_wasm_hash = deploy_wasm("basic_margin_account.wasm").await?;
    let pro_wasm_hash = deploy_wasm("pro_margin_account.wasm").await?;
    let institutional_wasm_hash = deploy_wasm("institutional_account.wasm").await?;
    
    // 2. Deploy Factory Contract
    let factory_address = deploy_contract(
        "smart_account_factory.wasm",
        vec![
            config.admin.to_scval(),
            basic_wasm_hash.to_scval(),
            pro_wasm_hash.to_scval(),
            institutional_wasm_hash.to_scval(),
        ]
    ).await?;
    
    // 3. Create test accounts for each tier
    let test_basic = create_test_account(&factory_address, AccountType::Basic).await?;
    let test_pro = create_test_account(&factory_address, AccountType::Pro).await?;
    let test_institutional = create_test_account(&factory_address, AccountType::Institutional).await?;
    
    // 4. Run integration tests
    run_integration_tests(&test_basic, &test_pro, &test_institutional).await?;
    
    Ok(DeploymentResult {
        factory_address,
        basic_wasm_hash,
        pro_wasm_hash,
        institutional_wasm_hash,
    })
}
```

### Phase 2: Mainnet Migration (Week 3-4)

1. **Audit smart account contracts** (engage 2+ audit firms)
2. **Deploy to mainnet** with identical configuration
3. **Beta program** - 50 users test with real funds (limited amounts)
4. **Gradual rollout** - Increase limits based on performance
5. **Full launch** - Make available to all users

---

## TESTING STRATEGY

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, AuthorizedFunction, AuthorizedInvocation};
    use soroban_sdk::{symbol_short, vec, Env, IntoVal};
    
    #[test]
    fn test_basic_account_leverage_limit() {
        let env = Env::default();
        env.mock_all_auths();
        
        // Deploy smart account
        let account_id = env.register_contract(None, BasicMarginAccount);
        let owner = Address::generate(&env);
        let signer = BytesN::from_array(&env, &[0; 32]);
        let vault = Address::generate(&env);
        let oracle = Address::generate(&env);
        
        // Initialize
        let client = BasicMarginAccountClient::new(&env, &account_id);
        client.initialize(&owner, &signer, &vault, &oracle);
        
        // Create mock borrow context
        let borrow_amount = 100_000_i128; // $100k
        let mock_collateral = 30_000_i128; // $30k collateral
        
        // Mock oracle and vault responses
        mock_vault_collateral(&env, &vault, &account_id, mock_collateral);
        mock_vault_borrows(&env, &vault, &account_id, 0);
        
        // Try to borrow (should fail - would create 3.3x leverage, max is 3x)
        let payload = BytesN::random(&env);
        let signature = create_signature(&env, &signer, &payload);
        let borrow_context = create_borrow_context(&env, &vault, borrow_amount);
        
        let result = env.try_invoke_contract_check_auth::<Error>(
            &account_id,
            &payload,
            vec![&env, signature],
            &vec![&env, borrow_context],
        );
        
        assert_eq!(result.err().unwrap().unwrap(), Error::ExcessiveLeverage);
    }
    
    #[test]
    fn test_session_key_daily_limit() {
        let env = Env::default();
        env.mock_all_auths();
        
        // Setup
        let account_id = env.register_contract(None, ProMarginAccount);
        let client = ProMarginAccountClient::new(&env, &account_id);
        
        // Add session key with $1000 daily limit
        let session_key = BytesN::from_array(&env, &[1; 32]);
        client.add_session_key(
            &session_key,
            &1000000, // expires far future
            &500,     // $500 per tx
            &1000,    // $1000 daily
            &vec![&env, symbol_short!("borrow")],
        );
        
        // First trade: $500 (should succeed)
        let result1 = execute_with_session_key(&env, &account_id, &session_key, 500);
        assert!(result1.is_ok());
        
        // Second trade: $500 (should succeed, hitting limit)
        let result2 = execute_with_session_key(&env, &account_id, &session_key, 500);
        assert!(result2.is_ok());
        
        // Third trade: $100 (should fail - exceeds daily limit)
        let result3 = execute_with_session_key(&env, &account_id, &session_key, 100);
        assert_eq!(result3.err().unwrap().unwrap(), Error::DailyLimitExceeded);
    }
    
    #[test]
    fn test_auto_deleverage() {
        let env = Env::default();
        env.mock_all_auths();
        
        // Setup account with low health factor
        let account_id = env.register_contract(None, BasicMarginAccount);
        let client = BasicMarginAccountClient::new(&env, &account_id);
        
        // Mock: $100k collateral, $90k borrows -> 1.11 health (below 1.2 threshold)
        mock_vault_collateral(&env, &vault, &account_id, 100_000);
        mock_vault_borrows(&env, &vault, &account_id, 90_000);
        
        // Call auto_deleverage (anyone can call)
        let result = client.auto_deleverage();
        assert!(result.is_ok());
        
        // Verify repayment was made
        verify_vault_repay_called(&env, &vault);
    }
}
```

### Integration Tests

```rust
#[test]
fn test_end_to_end_borrow_with_smart_account() {
    let env = Env::default();
    env.mock_all_auths();
    
    // 1. Deploy full system
    let vault_id = env.register_contract_wasm(None, PERIDOT_VAULT_WASM);
    let factory_id = env.register_contract_wasm(None, FACTORY_WASM);
    let basic_account_wasm = env.deployer().upload_contract_wasm(BASIC_ACCOUNT_WASM);
    
    // 2. Create Smart Account
    let user = Address::generate(&env);
    let factory_client = FactoryClient::new(&env, &factory_id);
    let smart_account = factory_client.create_account(
        &AccountConfig {
            account_type: AccountType::Basic,
            owner: user.clone(),
            signers: vec![&env, generate_keypair(&env).public_key],
            max_leverage: 300,
            auto_deleverage_threshold: 1200,
        },
        &BytesN::from_array(&env, &[0; 32]),
    );
    
    // 3. Fund Smart Account with collateral
    let usdc = deploy_token(&env, "USDC");
    let vault_client = VaultClient::new(&env, &vault_id);
    
    usdc.mint(&smart_account, &10_000_000_000); // $10k USDC
    vault_client.supply(&smart_account, &usdc.address, &10_000_000_000);
    
    // 4. Borrow using Smart Account
    let xlm = deploy_token(&env, "XLM");
    let borrow_amount = 3_000_000_000; // $3k worth
    
    // Smart Account automatically enforces 3x leverage limit
    let result = vault_client.borrow(&smart_account, &xlm.address, &borrow_amount);
    assert!(result.is_ok());
    
    // 5. Try to over-leverage (should fail at Smart Account level)
    let excessive_borrow = 30_000_000_000; // $30k - would create 4x leverage
    let result = vault_client.try_borrow(&smart_account, &xlm.address, &excessive_borrow);
    assert_eq!(result.err().unwrap(), Error::ExcessiveLeverage);
}
```

---

## SECURITY CONSIDERATIONS

### 1. Reentrancy Protection

Smart Accounts are inherently protected from reentrancy because `__check_auth` can only be invoked by the Soroban host during authorization checks, not by external contracts.

### 2. Signature Verification

Always use Soroban's native crypto verification:

```rust
// CORRECT
env.crypto().ed25519_verify(&public_key, &payload, &signature);

// INCORRECT - Don't implement your own crypto
fn custom_verify_signature(...) { /* manual implementation */ }
```

### 3. Storage Access Control

```rust
// Ensure only owner can modify critical parameters
pub fn update_params(env: Env, new_leverage: u32) -> Result<(), Error> {
    let owner: Address = env.storage().instance()
        .get(&DataKey::Owner)
        .ok_or(Error::NotInitialized)?;
    
    // CRITICAL: Require owner authorization
    owner.require_auth();
    
    env.storage().instance().set(&DataKey::MaxLeverage, &new_leverage);
    Ok(())
}
```

### 4. Integer Overflow Protection

Rust's default panic on overflow in debug mode helps, but be explicit:

```rust
// Use checked arithmetic for financial calculations
let new_borrows = current_borrows.checked_add(amount)
    .ok_or(Error::ArithmeticOverflow)?;

let leverage = (new_borrows.checked_mul(100)
    .ok_or(Error::ArithmeticOverflow)?)
    .checked_div(collateral)
    .ok_or(Error::DivisionByZero)?;
```

### 5. Oracle Manipulation

```rust
// Use time-weighted average prices or multiple oracle sources
fn get_safe_price(env: &Env, asset: &Address) -> Result<i128, Error> {
    let oracle1_price = get_oracle_price(env, &ORACLE_1, asset)?;
    let oracle2_price = get_oracle_price(env, &ORACLE_2, asset)?;
    
    // Reject if prices diverge too much (>2%)
    let diff = (oracle1_price - oracle2_price).abs();
    if diff * 100 / oracle1_price > 2 {
        return Err(Error::OraclePriceDivergence);
    }
    
    Ok((oracle1_price + oracle2_price) / 2)
}
```

### 6. Upgrade Safety

```rust
// Implement upgrade pattern with timelock
pub fn upgrade_contract(
    env: Env,
    new_wasm_hash: BytesN<32>,
) -> Result<(), Error> {
    let owner: Address = env.storage().instance()
        .get(&DataKey::Owner)
        .ok_or(Error::NotInitialized)?;
    owner.require_auth();
    
    // Check if upgrade is in timelock period
    let proposed_at: u64 = env.storage().instance()
        .get(&DataKey::UpgradeProposedAt)
        .unwrap_or(0);
    
    let current_time = env.ledger().timestamp();
    let timelock_period = 86400 * 7; // 7 days
    
    if current_time < proposed_at + timelock_period {
        return Err(Error::TimelockNotExpired);
    }
    
    // Perform upgrade
    env.deployer().update_current_contract_wasm(new_wasm_hash);
    
    Ok(())
}
```

---

## MONITORING AND ANALYTICS

### Events to Emit

```rust
// In Smart Account contracts
env.events().publish(
    (symbol_short!("auth"), function_name),
    (user_address, amount, leverage)
);

env.events().publish(
    (symbol_short!("auto_dlv"),),
    (health_factor, repaid_amount)
);

env.events().publish(
    (symbol_short!("sess_add"), session_key),
    (expires_at, daily_limit)
);
```

### Dashboard Metrics

Track these metrics for Smart Accounts:

1. **Total accounts created** by type
2. **Average leverage** across all accounts
3. **Auto-deleveraging frequency** (lower is better)
4. **Session key usage** vs standard auth
5. **Health factor distribution**
6. **Policy violation attempts** (security monitoring)

---

## FRONTEND INTEGRATION GUIDE

### Smart Account Creation Flow

```typescript
// frontend/src/utils/smartAccount.ts

import { 
  Contract, 
  SorobanRpc, 
  TransactionBuilder,
  Networks,
  Keypair,
  Operation
} from '@stellar/stellar-sdk';

export async function createSmartAccount(
  userAddress: string,
  accountType: 'basic' | 'pro' | 'institutional',
  signerKeypair: Keypair,
  rpcUrl: string
): Promise<string> {
  const server = new SorobanRpc.Server(rpcUrl);
  
  // 1. Get factory contract
  const factoryAddress = 'FACTORY_CONTRACT_ADDRESS';
  const factory = new Contract(factoryAddress);
  
  // 2. Prepare creation parameters
  const config = {
    account_type: accountType === 'basic' ? 0 : accountType === 'pro' ? 1 : 2,
    owner: userAddress,
    signers: [signerKeypair.publicKey()],
    max_leverage: accountType === 'basic' ? 300 : 1000,
    auto_deleverage_threshold: 1200,
  };
  
  const salt = Buffer.from(Keypair.random().publicKey(), 'hex');
  
  // 3. Build transaction
  const account = await server.getAccount(userAddress);
  
  const transaction = new TransactionBuilder(account, {
    fee: '100000',
    networkPassphrase: Networks.TESTNET,
  })
    .addOperation(
      factory.call('create_account', config, salt)
    )
    .setTimeout(300)
    .build();
  
  // 4. Simulate to get auth entries
  const simulated = await server.simulateTransaction(transaction);
  
  // 5. Sign and submit
  const prepared = SorobanRpc.assembleTransaction(transaction, simulated);
  prepared.sign(signerKeypair);
  
  const result = await server.sendTransaction(prepared);
  
  // 6. Poll for result
  const smartAccountAddress = await pollForAccountCreation(server, result.hash);
  
  return smartAccountAddress;
}

export async function borrowWithSmartAccount(
  smartAccountAddress: string,
  vaultAddress: string,
  asset: string,
  amount: number,
  userKeypair: Keypair,
  rpcUrl: string
): Promise<string> {
  const server = new SorobanRpc.Server(rpcUrl);
  const vault = new Contract(vaultAddress);
  
  // Build borrow transaction
  const account = await server.getAccount(smartAccountAddress);
  
  const transaction = new TransactionBuilder(account, {
    fee: '100000',
    networkPassphrase: Networks.TESTNET,
  })
    .addOperation(
      vault.call('borrow', smartAccountAddress, asset, amount)
    )
    .setTimeout(300)
    .build();
  
  // Simulate to get authorization requirements
  const simulated = await server.simulateTransaction(transaction);
  
  // Smart Account's __check_auth will be called during this
  const prepared = SorobanRpc.assembleTransaction(transaction, simulated);
  
  // Sign with user's key
  prepared.sign(userKeypair);
  
  const result = await server.sendTransaction(prepared);
  return result.hash;
}
```

### Session Key Management UI

```typescript
export interface SessionKey {
  publicKey: string;
  expiresAt: number;
  maxPerTx: number;
  dailyLimit: number;
  allowedOps: string[];
}

export async function addSessionKey(
  smartAccountAddress: string,
  sessionKey: SessionKey,
  ownerKeypair: Keypair,
  rpcUrl: string
): Promise<void> {
  const server = new SorobanRpc.Server(rpcUrl);
  const smartAccount = new Contract(smartAccountAddress);
  
  const transaction = new TransactionBuilder(/* ... */)
    .addOperation(
      smartAccount.call(
        'add_session_key',
        sessionKey.publicKey,
        sessionKey.expiresAt,
        sessionKey.maxPerTx,
        sessionKey.dailyLimit,
        sessionKey.allowedOps
      )
    )
    .build();
  
  // Sign and submit
  // ...
}
```

---

## PERFORMANCE OPTIMIZATION

### 1. Storage Efficiency

Use instance storage for frequently accessed data:

```rust
// GOOD - Instance storage for hot data
env.storage().instance().set(&DataKey::Owner, &owner);
env.storage().instance().set(&DataKey::MaxLeverage, &300_u32);

// AVOID - Persistent storage unless necessary
// env.storage().persistent().set(&DataKey::Owner, &owner);
```

### 2. Lazy Loading

Only load data when needed:

```rust
fn __check_auth(/*...*/) -> Result<(), Error> {
    // Don't load vault/oracle data unless operation requires it
    if requires_risk_check(&auth_contexts) {
        let collateral = Self::get_total_collateral_usd(&env)?;
        // ... check policies
    }
    Ok(())
}
```

### 3. Batch Operations

```rust
// Allow batching multiple borrows in one transaction
pub fn borrow_multiple(
    env: Env,
    borrower: Address,
    assets: Vec<Address>,
    amounts: Vec<i128>,
) -> Result<(), Error> {
    borrower.require_auth(); // Single auth check for all borrows
    
    for i in 0..assets.len() {
        internal_borrow(&env, &borrower, &assets.get(i)?, &amounts.get(i)?)?;
    }
    Ok(())
}
```

---

## MIGRATION PATH FOR EXISTING USERS

### Option 1: One-Click Migration

```rust
pub fn migrate_to_smart_account(
    env: Env,
    user: Address,
    account_type: AccountType,
) -> Result<Address, Error> {
    user.require_auth();
    
    // 1. Create Smart Account
    let smart_account = create_account(&env, user.clone(), account_type)?;
    
    // 2. Transfer all positions to Smart Account
    let vault = get_vault_contract(&env);
    vault.transfer_positions(&user, &smart_account)?;
    
    // 3. Update frontend to use Smart Account address
    Ok(smart_account)
}
```

### Option 2: Gradual Transition

1. Users keep existing regular accounts
2. New borrows/positions go through Smart Account
3. Old positions stay on regular account
4. Eventually consolidate when user desires

---

## SUCCESS METRICS

### Week 1-2 (Testnet)
- [ ] 100+ test accounts created
- [ ] 1000+ test borrows executed
- [ ] 0 security incidents
- [ ] Auto-deleverage tested 50+ times

### Week 3-4 (Mainnet Beta)
- [ ] 50 beta users onboarded
- [ ] $500k+ TVL in Smart Accounts
- [ ] <0.1% failed transactions
- [ ] Average health factor >1.5

### Month 2-3 (Full Launch)
- [ ] 30% of active users using Smart Accounts
- [ ] 0 liquidations for Smart Account users (due to auto-deleverage)
- [ ] 50%+ of trades using session keys (pro accounts)
- [ ] Institution partners onboarded (3+)

---

## APPENDIX A: COMPLETE CONTRACT INTERFACES

```rust
// Smart Account Factory
pub trait SmartAccountFactory {
    fn create_account(env: Env, config: AccountConfig, salt: BytesN<32>) -> Address;
    fn upgrade_account(env: Env, user: Address, new_type: AccountType) -> Result<(), Error>;
    fn get_account(env: Env, user: Address) -> Option<Address>;
}

// Basic Margin Account
pub trait BasicMarginAccount {
    fn __constructor(env: Env, owner: Address, signer: BytesN<32>, vault: Address, oracle: Address);
    fn update_params(env: Env, max_leverage: Option<u32>, threshold: Option<u32>, limit: Option<i128>) -> Result<(), Error>;
    fn auto_deleverage(env: Env) -> Result<(), Error>;
}

// Pro Margin Account (extends Basic)
pub trait ProMarginAccount {
    fn add_session_key(env: Env, key: BytesN<32>, expires: u64, max_tx: i128, daily: i128, ops: Vec<Symbol>) -> Result<(), Error>;
    fn revoke_session_key(env: Env, key: BytesN<32>) -> Result<(), Error>;
    fn get_session_keys(env: Env) -> Vec<SessionKey>;
}

// Institutional Account (extends Pro)
pub trait InstitutionalAccount {
    fn update_multisig(env: Env, config: MultisigConfig) -> Result<(), Error>;
    fn blacklist_market(env: Env, market: Address) -> Result<(), Error>;
    fn set_trading_hours(env: Env, start: u64, end: u64) -> Result<(), Error>;
}
```

---

## APPENDIX B: ERROR CODES

```rust
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    // Authentication errors (1-99)
    Unauthorized = 1,
    InvalidSignature = 2,
    UnauthorizedSigner = 3,
    InsufficientSignatures = 4,
    SessionExpired = 5,
    
    // Policy errors (100-199)
    ExcessiveLeverage = 100,
    InsufficientHealth = 101,
    MarketConcentration = 102,
    TotalBorrowExceeded = 103,
    ExceedsSessionLimit = 104,
    DailyLimitExceeded = 105,
    OperationNotAllowed = 106,
    BlacklistedMarket = 107,
    OutsideTradingHours = 108,
    
    // State errors (200-299)
    NotInitialized = 200,
    AlreadyInitialized = 201,
    AccountNotFound = 202,
    
    // Arithmetic errors (300-399)
    ArithmeticOverflow = 300,
    DivisionByZero = 301,
    
    // Oracle errors (400-499)
    OraclePriceDivergence = 400,
    StalePrice = 401,
}
```

---

## CONCLUSION

This technical architecture provides a complete specification for implementing Smart Accounts in Peridot Finance's Soroban lending protocol. The design prioritizes:

1. **Minimal protocol changes** - Existing vault contracts work unchanged
2. **Security by default** - Multiple layers of policy enforcement
3. **User experience** - Auto-deleveraging prevents liquidations
4. **Flexibility** - Three tiers support different user needs
5. **Native integration** - Leverages Soroban's built-in authorization framework

The implementation can be completed in 4-6 weeks with a small team, providing Peridot with a significant competitive advantage in the Stellar DeFi ecosystem.

**Next Steps**:
1. Review and approve architecture
2. Begin smart account contract development
3. Integrate with existing Peridot contracts
4. Deploy to testnet for testing
5. Security audits
6. Mainnet launch

---

**Document Version**: 1.0  
**Last Updated**: 2026-02-02  
**Status**: READY FOR IMPLEMENTATION
