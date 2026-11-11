"""Prepare copies of the accepted harness/artifacts. No build, tests or signatures.

Current Aquarius bytecode + actual pair/fee/range/price inputs; controlled state,
not a live-state fork. The accepted prototype and production sources stay intact.
"""
import argparse, hashlib, json, re, shutil
from decimal import Decimal, localcontext, ROUND_CEILING, ROUND_FLOOR
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
HERE=Path(__file__).resolve().parent
BASE=ROOT/'target/cl-stage1-cash-first-build-v4'
SIGNED=ROOT/'target/cl-stage1-signed-local/harness'
PUBLIC=ROOT/'target/cl-stage1-live-pairs/public'
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def one(s,a,b):
    assert s.count(a)==1,(a,s.count(a))
    return s.replace(a,b)
p=argparse.ArgumentParser();p.add_argument('--pair',required=True,choices=['pyusd-usdc','usdc-eurc']);p.add_argument('--output',required=True,type=Path);a=p.parse_args()
out=a.output.resolve();out.mkdir(exist_ok=False)
data=json.loads((ROOT/'research/cl-pool-selection-20261111/pool-data/getters.json').read_text())
pool=next(r for r in data['pools'] if r['address'].startswith('CAPIO' if a.pair=='pyusd-usdc' else 'CDTSE'))
slot=next(r['value'] for r in pool['views'] if r['method']=='get_slot0')
lo,hi=(-2020,2000) if a.pair=='pyusd-usdc' else (-3060,1020)
spacing,fee=(20,10) if a.pair=='pyusd-usdc' else (60,30)
tokens=[s.split(':') for s in pool['api_tokens']]
public_tokens=next(r['value'] for r in pool['views'] if r['method']=='get_tokens')
price0,price1=(99983050930781,100000000000000) if a.pair=='pyusd-usdc' else (100090113634635,111517507041370)
assert lo%spacing==0 and hi%spacing==0 and lo<int(slot['tick'])<hi
with localcontext() as c:
    c.prec=100
    sqrtlo=int((Decimal('1.0001')**(Decimal(lo)/2)*10**18).to_integral_value(rounding=ROUND_CEILING))
    sqrthi=int((Decimal('1.0001')**(Decimal(hi)/2)*10**18).to_integral_value(rounding=ROUND_FLOOR))
shutil.copytree(BASE/'workspace',out/'workspace')
shutil.copytree(BASE/'artifacts',out/'artifacts')
shutil.copytree(SIGNED,out/'harness')
(out/'logs').mkdir()
ws=out/'workspace';h=out/'harness'
for file in ['Cargo.toml','Cargo.lock']:
    f=h/file;f.write_text(f.read_text().replace('cl-stage1-cash-runtime29-harness','cl-stage1-live-'+a.pair+'-runtime29-harness'))
vault=ws/'contracts/cl-share-vault/src/contract.rs';s=vault.read_text()
s=s.replace('(-2000,2000)',f'({lo},{hi})')
s=one(s,'let lo=904_841_941_932_768_879u128;let hi=1_105_165_392_603_232_697u128;',f'let lo={sqrtlo}u128;let hi={sqrthi}u128;')
# Fixture band is deliberately explicit. Economics wide bounds are asymmetric;
# do not let centered_range silently select a different position.
needle='''        let (tick_lower, tick_upper) = centered_range(
            slot.tick,
            tick_spacing,
            DEFAULT_HALF_WIDTH_TICKS,
            MAX_TICK_ABS,
        );'''
s=one(s,needle,f'''        // ISOLATED MEASUREMENT FIXTURE ONLY: accepted economics band.
        assert_eq!(tick_spacing,{spacing});
        assert!(slot.tick>={lo} && slot.tick<{hi});
        let (tick_lower,tick_upper)=({lo},{hi});''')
vault.write_text(s)
lib=h/'src/lib.rs';lib.write_text(lib.read_text().replace(str(SIGNED/'src/transaction_evidence.rs'),str(h/'src/transaction_evidence.rs')))
src=h/'src/test.rs';s=src.read_text()
s=s.replace('12fca5a7a96577273b6d4184cf9c984036cda0e8f0594747e7b2933dced37ee6','155a17b9929ffb1f9e84bd6ef5c00a4d613c1ab5f4ad4c502d84515250cc2907')
start=s.index('        let a = e\n            .register_stellar_asset_contract_v2')
end=s.index('        let reward = e',start)
s=s[:start]+f'''        let under=pair_sac(&e,&admin,"{tokens[0][0]}","{tokens[0][1]}","{public_tokens[0]}");
        let paired=pair_sac(&e,&admin,"{tokens[1][0]}","{tokens[1][1]}","{public_tokens[1]}");
'''+s[end:]
s=one(s,'let pool = e.register(bytes.as_slice(), ());',f'''let pool = e.register_at(&Address::from_str(&e,"{pool['address']}"),bytes.as_slice(), ());
        let public=std::path::PathBuf::from("{PUBLIC}");
        let cfg_bytes=std::fs::read(public.join("aquarius-config.wasm")).unwrap();
        let config_addr=e.register_at(&Address::from_str(&e,"CAUJNVN5DWOBCKLV5KPR3KRQBKRI7L2X7VOUAWF5FDISVSQNXCEYITPJ"),cfg_bytes.as_slice(),(admin.clone(),admin.clone()));
        for (key,file) in [("GaugeWASM","aquarius-gaugewasm.wasm"),("VaultWASM","aquarius-vaultwasm.wasm")] {{
            let code=std::fs::read(public.join(file)).unwrap();
            let hash=e.deployer().upload_contract_wasm(code.as_slice());
            let k=soroban_sdk::vec![&e,Symbol::new(&e,key)];
            auth::<()>(&e,&admin,&config_addr,"set_value",(admin.clone(),k,hash).into_val(&e),&[]);
        }}''')
s=one(s,'                10u32,\n                20i32,\n                0u32,',f'                {fee}u32,\n                {spacing}i32,\n                5000u32,')
s=one(s,'                (reward.clone(), boost, plane.clone()),\n                plane.clone(),','                (reward.clone(), boost, plane.clone()),\n                plane.clone(),\n                config_addr.clone(),')
seed1=100_000_000_000*int(slot['sqrt_price_x96'])**2//(1<<192)
s=one(s,'let desired = soroban_sdk::vec![&e, 100_000_000_000u128, 100_000_000_000u128];',f'let desired = soroban_sdk::vec![&e, 100_000_000_000u128, {seed1}u128];')
s=s.replace('(-2000i32, 2000i32, desired.clone())',f'({lo}i32, {hi}i32, desired.clone())')
s=s.replace('amount(actual.get(0).unwrap())','amount(desired.get(0).unwrap())').replace('amount(actual.get(1).unwrap())','amount(desired.get(1).unwrap())')
s=s.replace('(user.clone(), -2000i32, 2000i32, actual, 0u128)',f'(user.clone(), {lo}i32, {hi}i32, desired.clone(), 0u128)')
s=one(s,'        let oracle = wasm(&e, "cl_integration_fixtures");',f'''        // Actual first-deposit execution establishes price; never patch Slot0.
        let seeded_slot:PairSlot=call(&e,&pool,"get_slot0",().into_val(&e));
        let captured_sqrt={slot['sqrt_price_x96']}u128;
        assert!(seeded_slot.sqrt_price_x96.to_u128().unwrap().abs_diff(captured_sqrt)<=captured_sqrt/1_000_000_000,"initial venue price differs from captured price beyond seed rounding");
        assert!((seeded_slot.tick-({slot['tick']})).abs()<=1,"seed tick differs from captured tick");
        std::println!("PAIR_SEED_PRICE pool={{:?}} tick={{}} sqrt={{}} captured_sqrt={{}}",pool,seeded_slot.tick,seeded_slot.sqrt_price_x96.to_u128().unwrap(),captured_sqrt);
        let oracle = wasm(&e, "cl_integration_fixtures");
        // Captured numeric primary observations, served by the existing compiled
        // test oracle. This does NOT measure real Reflector code or the new guard.
        for(t,p)in [(&under,{price0}i128),(&paired,{price1}i128)]{{auth::<()>(&e,&admin,&oracle,"set_test_price",(admin.clone(),Asset::Stellar(t.clone()),p).into_val(&e),&[]);}}''')
stages=out/'stages.rs';stage=(ROOT/'prototypes/cl-stage1-cash-first/tests/stages.rs').read_text()
stage=stage.replace('include!("../command_types.rs");','include!("'+str(ROOT/'prototypes/cl-stage1-cash-first/command_types.rs')+'");')
stage=one(stage,'    auth::<()>(e,&f.user,&f.reward,"transfer",(f.user.clone(),f.pool.clone(),100_000_000i128).into_val(e),&[]);','''    // Current Aquarius reward custody is its actual deployed child vault.
    let venue_reward_vault:Address=call(e,&f.pool,"get_vault_addr",().into_val(e));
    auth::<()>(e,&f.user,&f.reward,"transfer",(f.user.clone(),venue_reward_vault,100_000_000i128).into_val(e),&[]);''')
stage=one(stage,'    next(&f.e);let e=&f.e;', '''    next(&f.e);let e=&f.e;
    let quoted:u128=quiet(||call(e,&f.pool,"estimate_swap",(0u32,1u32,100_000_000u128).into_val(e)));
    let min_swap=quoted*99/100;
    assert!(min_swap>0);''')
stage=stage.replace('99_000_000u128,190_000_000u128','min_swap,190_000_000u128')
stages.write_text(stage)
s=s.replace(str(ROOT/'prototypes/cl-stage1-cash-first/tests/stages.rs'),str(stages))
s+='\n'+(HERE/'pair_fixture.rs').read_text();src.write_text(s)
accounts=(ROOT/'prototypes/cl-stage1-signed-local/accounts.cjs').read_text()
accounts=accounts.replace("require('../../bots/margin-liquidator/node_modules/@stellar/stellar-sdk')","require('"+str(ROOT/'bots/margin-liquidator/node_modules/@stellar/stellar-sdk')+"')")
accounts=one(accounts,"const COWNER='CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABDWC6';",'''const COWNER=require.main===module?JSON.parse(fs.readFileSync(path.join(process.argv[2],'transactions.jsonl'),'utf8').trim().split('\\n').find(s=>JSON.parse(s).flow==='stage_prepare')).expected_external_authorizers[0]:'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABDWC6';''')
accounts=one(accounts,"if(info[0]!=='AlphaNum4')throw Error('only fixture alpha4');\n    const a=new xdr.AlphaNum4({assetCode:Buffer.from(info[1].asset_code),issuer:xdr.PublicKey.publicKeyTypeEd25519(info[1].issuer)});\n    assets.set(Address.fromScAddress(c.contract()).toString(),xdr.TrustLineAsset.assetTypeCreditAlphanum4(a));",'''if(!['AlphaNum4','AlphaNum12'].includes(info[0]))throw Error('unsupported fixture asset');
    const C=info[0]==='AlphaNum4'?xdr.AlphaNum4:xdr.AlphaNum12;
    const a=new C({assetCode:Buffer.from(info[1].asset_code),issuer:xdr.PublicKey.publicKeyTypeEd25519(info[1].issuer)});
    assets.set(Address.fromScAddress(c.contract()).toString(),info[0]==='AlphaNum4'?xdr.TrustLineAsset.assetTypeCreditAlphanum4(a):xdr.TrustLineAsset.assetTypeCreditAlphanum12(a));''')
accounts=one(accounts," for(const [addr,asset] of assets){const tl=",''' // Controlled genesis may contain unused private-derived aliases of a public SAC.
 const uniqueAssets=new Map();for(const [addr,asset] of assets){const key=asset.toXDR('base64');const row=uniqueAssets.get(key)||{asset,balance:0n};row.balance+=BigInt(balances.get(addr)||0n);uniqueAssets.set(key,row);}
 for(const {asset,balance} of uniqueAssets.values()){const tl=''')
accounts=accounts.replace("String(balances.get(addr)||0n)","String(balance)").replace('account(owner,assets.size)','account(owner,uniqueAssets.size)').replace('trustlines:assets.size','trustlines:uniqueAssets.size')
(out/'accounts.cjs').write_text(accounts)
sign=(ROOT/'prototypes/cl-stage1-signed-local/sign-local.cjs').read_text().replace("path.resolve(__dirname,'../../../docs/deployment-handoffs/evidence/cl-resource-decision-refresh-20261102/network-config.json')",repr(str(ROOT.parent/'docs/deployment-handoffs/evidence/cl-resource-decision-refresh-20261102/network-config.json')))
(out/'sign-local.cjs').write_text(sign)
manifest={'scope':'isolated current Aquarius bytecode with controlled pair state, NOT a live-state fork','pair':a.pair,'pool':pool['address'],'fee_bps':fee,'tick_spacing':spacing,'protocol_fee_fraction':5000,'tokens':tokens,'slot':slot,'range':[lo,hi],'sqrt_1e18_rounded_inward':[sqrtlo,sqrthi],'oracle_prices_1e14':[price0,price1],'oracle_scope':'captured numeric prices in existing compiled oracle fixture; no real oracle or depeg guard cost proof','baseline':str(BASE),'source_files':{str(f):sha(f)for f in [vault,src,stages,lib,out/'accounts.cjs',out/'sign-local.cjs']},'dependencies':{f.name:sha(f)for f in PUBLIC.glob('*.wasm')},'build_performed':False,'financial_execution_performed':False,'production_adoption':False}
(out/'preparation.json').write_text(json.dumps(manifest,indent=2)+'\n')
print(out)
