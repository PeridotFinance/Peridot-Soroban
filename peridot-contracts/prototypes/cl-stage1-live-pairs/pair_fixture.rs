// Controlled setup only. Same asset codes and issuer public keys as captured.
// Mainnet SAC identities imported into controlled private genesis with exact asset metadata.
// No issuer signature is claimed; setup assigns the SAC admin in controlled genesis.
#[contracttype]
#[derive(Clone)]
struct PairSlot { sqrt_price_x96:soroban_sdk::U256,tick:i32 }
fn pair_sac(e:&Env,admin:&Address,code:&str,issuer:&str,public_sac:&str)->Address {
    use soroban_sdk::{xdr::*,TryFromVal};
    use std::rc::Rc;
    let issuer_addr=Address::from_str(e,issuer);
    let ScVal::Address(ScAddress::Account(issuer_id))=ScVal::from(&issuer_addr) else {panic!("issuer must be account")};
    let k=Rc::new(LedgerKey::Account(LedgerKeyAccount{account_id:issuer_id.clone()}));
    let v=Rc::new(LedgerEntry{data:LedgerEntryData::Account(AccountEntry{account_id:issuer_id.clone(),balance:1_000_000_000_000,flags:0,home_domain:Default::default(),inflation_dest:None,num_sub_entries:0,seq_num:SequenceNumber(0),thresholds:Thresholds([1;4]),signers:VecM::default(),ext:AccountEntryExt::V0}),last_modified_ledger_seq:0,ext:LedgerEntryExt::V0});
    e.host().add_ledger_entry(&k,&v,None).unwrap();
    let asset=if code.len()<=4 {let mut b=[0u8;4];b[..code.len()].copy_from_slice(code.as_bytes());Asset::CreditAlphanum4(AlphaNum4{asset_code:AssetCode4(b),issuer:issuer_id})}else{let mut b=[0u8;12];b[..code.len()].copy_from_slice(code.as_bytes());Asset::CreditAlphanum12(AlphaNum12{asset_code:AssetCode12(b),issuer:issuer_id})};
    let val=e.host().invoke_function(HostFunction::CreateContract(CreateContractArgs{contract_id_preimage:ContractIdPreimage::Asset(asset),executable:ContractExecutable::StellarAsset})).unwrap();
    let created=Address::try_from_val(e,&val).unwrap();
    let token=Address::from_str(e,public_sac);
    // Preserve canonical public token order; private-derived IDs sort differently.
    // Import the initialized built-in SAC instance into controlled genesis, with
    // its existing TTL and exact issuer/code metadata. No public deployment occurs.
    let old_key=Rc::new(LedgerKey::ContractData(LedgerKeyContractData{contract:ScAddress::from(&created),key:ScVal::LedgerKeyContractInstance,durability:ContractDataDurability::Persistent}));
    let (old_entry,ttl)=e.host().get_ledger_entry(&old_key).unwrap().unwrap();
    let mut entry=(*old_entry).clone();
    let LedgerEntryData::ContractData(ref mut data)=entry.data else {panic!("SAC instance expected")};
    data.contract=ScAddress::from(&token);
    let key=Rc::new(LedgerKey::ContractData(LedgerKeyContractData{contract:data.contract.clone(),key:data.key.clone(),durability:data.durability}));
    e.host().add_ledger_entry(&key,&Rc::new(entry),ttl).unwrap();
    // Explicit controlled genesis assignment, before any financial operation.
    // No issuer authorization/signature proof; never register a mock at a G address.
    // SAC InstanceDataKey::Admin encodes as the single-symbol vector below.
    e.as_contract(&token,||e.storage().instance().set(&soroban_sdk::vec![e,Symbol::new(e,"Admin")],admin));
    token
}
