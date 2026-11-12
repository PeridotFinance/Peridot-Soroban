// Generated from exact source enum declarations by freeze.py; not runtime storage.
use soroban_sdk::{contracttype, Address, Env, xdr::ToXdr};
use cl_stage1_interface::{ReceiptKeyV1,VaultKeyV1};
#[contracttype]
#[derive(Clone)]
enum LegacyKey0 {
    UnderlyingToken,
    ManagedCash,
    TotalDeposited,
    InterestRatePerSecond,
    LastUpdateTime,
    AccumulatedInterest,
    YearlyRateScaled,
    InitialExchangeRate,

    BorrowSnapshots(Address),
    HasBorrowed(Address),
    MarginBorrowSnapshots(u64),
    MarginHasBorrowed(u64),
    TotalBorrowed,
    BorrowIndex,
    BorrowYearlyRateScaled,
    CollateralFactorScaled,
    Admin,
    PendingAdmin,
    Peridottroller,
    InterestModel,
    ReserveFactorScaled,
    AdminFeeScaled,
    FlashLoanFeeScaled,
    TotalAdminFees,
    TotalReserves,
    SupplyCap,
    BorrowCap,
    Initialized,
    BoostedVault,
    BoostedUnderlyingCached,
    BoostedUnderlyingUpdatedAt,
    TotalBorrowPrincipal,
    RatesReady,
    IdleCashBufferBps,
    FlashLoanActive,
    MarginController,
    MarginWithdrawBypass(Address),
    PendingUpgradeHash,
    PendingUpgradeEta,


    DebtStateVersion,
    DebtStateMigratedAt,
    BorrowPrincipal(Address),
    MarginBorrowPrincipal(u64),
    MarginWithdrawBypassV2(Address),
    TotalBadDebt,
    BoostedAssetCount,
    BoostedHealthCache,
    MinCashReserveBps,
    ClCustody,
    ClCustodyContext,
    CashBorrowingEnabled,
 }
#[contracttype]
#[derive(Clone)]
enum LegacyKey1 {
    HybridBackingState,
    HybridBackingOwner(Address),
 }
#[contracttype]
#[derive(Clone)]
enum LegacyKey2 {
    RecycledRaw(Address),
    PoolOwed(Address),
    PinnedPrimary,
 }
#[contracttype]
#[derive(Clone)]
enum LegacyKey3 {
    HybridStreams,
    HybridStream(Address),
    HybridAccount(Address, Address),
    HybridEpoch(Address, u64),
    HybridRegistryInitialized,
 }
#[contracttype]
#[derive(Clone)]
enum LegacyKey4 {

    Config,
    Params,
    State,
    Admin,
    Initialized,

    PendingAdmin,
    PendingUpgradeHash,
    PendingUpgradeEta,
    Shares(Address),
    RewardRoute(Address),
    OracleSymbol(Address),


    ReceiptVault,
    PrimaryRewardToken,
    RewardMinRate(Address),


    PositionAmounts,
    RangeParams,
    RangeState,
 }
#[contracttype]
#[derive(Clone)]
enum LegacyKey5 {
    Busy,
    Book(Address),
    Excluded(Address),
 }
#[contracttype]
#[derive(Clone)]
enum LegacyKey6 {
    ShareAllowance(Address, Address),
 }
#[contracttype]
#[derive(Clone)]
enum LegacyKey7 {
    Registered(Address),
    Allocation(Address, Address),
    Total(Address),
    Beneficial(Address),
    Unassigned,
    Transfer(Address),
 }
#[contracttype]
#[derive(Clone)]
enum LegacyKey8 {
    Index,
    Holder(Address),
    Count,
    Boundary(u64),
    Interval,
    Open,
    Bucket(u64),
    Totals,
    Evidence(u64, Address),
    Proposal(u64),
 }
#[contracttype]
#[derive(Clone)]
enum LegacyKey9 {
    HistoryEnabled,
    HistoryRevision,
    HistoryTracked,
    HistoryCount(Address),
    HistoryWeight(Address, u32),
    HistorySegments,
    HistorySegment(u64),
    HistoryInterval(u64),
    HistoryWaiver(u64, u32),
    HistoryWaiverIndex(u64, Address),
    HistorySettled(u64, Address),
    HistoryProposal(u64),
    HistoryTotals,
 }
#[contracttype]
#[derive(Clone)]
enum LegacyKey10 {
    Config,
    Busy,
    NextController,
    ReadyAt,
 }
#[test]
fn new_key_namespaces_do_not_alias_existing_contract_keys(){
let e=Env::default();
let address=Address::from_string(&soroban_sdk::String::from_str(&e,"CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM"));
let mut existing:std::collections::BTreeMap<&str,std::collections::BTreeSet<std::vec::Vec<u8>>>=Default::default();
existing.entry("receipt").or_default().insert(LegacyKey0::UnderlyingToken.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::ManagedCash.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::TotalDeposited.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::InterestRatePerSecond.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::LastUpdateTime.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::AccumulatedInterest.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::YearlyRateScaled.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::InitialExchangeRate.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::BorrowSnapshots(address.clone()).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::HasBorrowed(address.clone()).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::MarginBorrowSnapshots(42u64).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::MarginHasBorrowed(42u64).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::TotalBorrowed.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::BorrowIndex.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::BorrowYearlyRateScaled.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::CollateralFactorScaled.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::Admin.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::PendingAdmin.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::Peridottroller.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::InterestModel.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::ReserveFactorScaled.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::AdminFeeScaled.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::FlashLoanFeeScaled.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::TotalAdminFees.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::TotalReserves.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::SupplyCap.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::BorrowCap.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::Initialized.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::BoostedVault.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::BoostedUnderlyingCached.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::BoostedUnderlyingUpdatedAt.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::TotalBorrowPrincipal.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::RatesReady.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::IdleCashBufferBps.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::FlashLoanActive.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::MarginController.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::MarginWithdrawBypass(address.clone()).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::PendingUpgradeHash.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::PendingUpgradeEta.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::DebtStateVersion.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::DebtStateMigratedAt.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::BorrowPrincipal(address.clone()).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::MarginBorrowPrincipal(42u64).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::MarginWithdrawBypassV2(address.clone()).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::TotalBadDebt.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::BoostedAssetCount.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::BoostedHealthCache.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::MinCashReserveBps.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::ClCustody.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::ClCustodyContext.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey0::CashBorrowingEnabled.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey1::HybridBackingState.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey1::HybridBackingOwner(address.clone()).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey2::RecycledRaw(address.clone()).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey2::PoolOwed(address.clone()).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey2::PinnedPrimary.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey3::HybridStreams.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey3::HybridStream(address.clone()).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey3::HybridAccount(address.clone(),address.clone()).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey3::HybridEpoch(address.clone(),42u64).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("receipt").or_default().insert(LegacyKey3::HybridRegistryInitialized.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey4::Config.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey4::Params.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey4::State.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey4::Admin.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey4::Initialized.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey4::PendingAdmin.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey4::PendingUpgradeHash.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey4::PendingUpgradeEta.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey4::Shares(address.clone()).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey4::RewardRoute(address.clone()).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey4::OracleSymbol(address.clone()).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey4::ReceiptVault.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey4::PrimaryRewardToken.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey4::RewardMinRate(address.clone()).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey4::PositionAmounts.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey4::RangeParams.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey4::RangeState.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey5::Busy.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey5::Book(address.clone()).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey5::Excluded(address.clone()).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey6::ShareAllowance(address.clone(),address.clone()).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey7::Registered(address.clone()).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey7::Allocation(address.clone(),address.clone()).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey7::Total(address.clone()).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey7::Beneficial(address.clone()).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey7::Unassigned.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey7::Transfer(address.clone()).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey8::Index.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey8::Holder(address.clone()).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey8::Count.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey8::Boundary(42u64).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey8::Interval.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey8::Open.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey8::Bucket(42u64).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey8::Totals.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey8::Evidence(42u64,address.clone()).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey8::Proposal(42u64).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey9::HistoryEnabled.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey9::HistoryRevision.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey9::HistoryTracked.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey9::HistoryCount(address.clone()).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey9::HistoryWeight(address.clone(),42u32).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey9::HistorySegments.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey9::HistorySegment(42u64).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey9::HistoryInterval(42u64).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey9::HistoryWaiver(42u64,42u32).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey9::HistoryWaiverIndex(42u64,address.clone()).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey9::HistorySettled(42u64,address.clone()).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey9::HistoryProposal(42u64).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("vault").or_default().insert(LegacyKey9::HistoryTotals.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("facade").or_default().insert(LegacyKey10::Config.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("facade").or_default().insert(LegacyKey10::Busy.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("facade").or_default().insert(LegacyKey10::NextController.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
existing.entry("facade").or_default().insert(LegacyKey10::ReadyAt.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());
let mut new=std::collections::BTreeSet::new();
let bytes=ReceiptKeyV1::Stage1ConfigV1.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>();assert!(!existing["receipt"].contains(&bytes),"new namespace collision Stage1ConfigV1");assert!(new.insert(bytes),"duplicate new key");
let bytes=ReceiptKeyV1::Stage1Next.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>();assert!(!existing["receipt"].contains(&bytes),"new namespace collision Stage1Next");assert!(new.insert(bytes),"duplicate new key");
let bytes=ReceiptKeyV1::Stage1Busy.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>();assert!(!existing["receipt"].contains(&bytes),"new namespace collision Stage1Busy");assert!(new.insert(bytes),"duplicate new key");
let bytes=ReceiptKeyV1::Stage1Job(42u64).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>();assert!(!existing["receipt"].contains(&bytes),"new namespace collision Stage1Job");assert!(new.insert(bytes),"duplicate new key");
let mut new=std::collections::BTreeSet::new();
let bytes=VaultKeyV1::Stage1Receipt.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>();assert!(!existing["vault"].contains(&bytes),"new namespace collision Stage1Receipt");assert!(new.insert(bytes),"duplicate new key");
let bytes=VaultKeyV1::Stage1RiskV1.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>();assert!(!existing["vault"].contains(&bytes),"new namespace collision Stage1RiskV1");assert!(new.insert(bytes),"duplicate new key");
let bytes=VaultKeyV1::Stage1Reserved(address.clone()).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>();assert!(!existing["vault"].contains(&bytes),"new namespace collision Stage1Reserved");assert!(new.insert(bytes),"duplicate new key");
let bytes=VaultKeyV1::Stage1Liability(42u64).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>();assert!(!existing["vault"].contains(&bytes),"new namespace collision Stage1Liability");assert!(new.insert(bytes),"duplicate new key");
let bytes=VaultKeyV1::Stage1Locked(address.clone()).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>();assert!(!existing["vault"].contains(&bytes),"new namespace collision Stage1Locked");assert!(new.insert(bytes),"duplicate new key");
let bytes=VaultKeyV1::Stage1Allocation(42u64).to_xdr(&e).iter().collect::<std::vec::Vec<u8>>();assert!(!existing["vault"].contains(&bytes),"new namespace collision Stage1Allocation");assert!(new.insert(bytes),"duplicate new key");
}
