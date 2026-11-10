"""Isolated 1x overlay; only stellar contract build emits WASM. No network/keys."""
import argparse,hashlib,json,os,re,shutil,subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2];HERE=Path(__file__).resolve().parent
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def replace_once(text,old,new):
    assert text.count(old)==1,('overlay anchor changed',old,text.count(old))
    return text.replace(old,new,1)
p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);a=p.parse_args()
out=a.output.resolve();out.mkdir(exist_ok=False);ws=out/'workspace';ws.mkdir()
files=subprocess.check_output(['git','ls-files','Cargo.toml','Cargo.lock','contracts'],cwd=ROOT,text=True).splitlines()
pins={}
for name in files:
    if Path(name).suffix not in ['.rs','.toml','.lock']:continue
    src=ROOT/name;dest=ws/name;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(src,dest);pins[name]=sha(src)
manifest=ws/'Cargo.toml';manifest.write_text(manifest.read_text().replace('../openzeppelin-stellar-contracts',str((ROOT/'../openzeppelin-stellar-contracts').resolve())))
types=(HERE/'command_types.rs').read_text()
for package,overlay in [('receipt-vault','receipt.rs'),('cl-share-vault','vault.rs')]:
    path=ws/'contracts'/package/'src/contract.rs';path.write_text(path.read_text()+'\n'+types+'\n'+(HERE/overlay).read_text())
v=ws/'contracts/cl-share-vault/src'
shutil.copy2(HERE/'stage_storage.rs',v/'stage_storage.rs')
(v/'lib.rs').write_text((v/'lib.rs').read_text()+'\nmod stage_storage;\n')
path=v/'accounting.rs';s=replace_once(path.read_text(),'token::Client::new(e, t)\n        .balance(&e.current_contract_address())\n        .try_into()\n        .unwrap()', 'u128::try_from(token::Client::new(e, t).balance(&e.current_contract_address())).unwrap()\n        .checked_sub(crate::stage_storage::reserved(e,t)).expect("pending command liabilities unfunded")');path.write_text(s)
path=v/'contract.rs';s=path.read_text()
s=replace_once(s,'fn burn_shares(env: &Env, from: &Address, amount: u128) {','fn burn_shares(env: &Env, from: &Address, amount: u128) {\n        assert!(crate::stage_storage::available(env,from)>=amount,"shares locked by command");')
s=replace_once(s,'let amount = to_u128(amount);\n        assert!(shares_of(env, from)', 'let amount = to_u128(amount);\n        assert!(crate::stage_storage::available(env,from)>=amount,"shares locked by command");\n        assert!(shares_of(env, from)')
path.write_text(s)
path=v/'constants.rs';path.write_text(replace_once(path.read_text(),'DEFAULT_HALF_WIDTH_TICKS: u32 = 40','DEFAULT_HALF_WIDTH_TICKS: u32 = 2000'))
artifacts=out/'artifacts';artifacts.mkdir();logs=out/'logs';logs.mkdir()
# Unchanged fixture dependencies preserved with exact byte identities.
base=ROOT/'target/cl-restoration-combined-build/artifacts'
for src in base.glob('*.wasm'):shutil.copy2(src,artifacts/src.name)
env=dict(os.environ,CARGO_INCREMENTAL='0',CARGO_NET_OFFLINE='true',CARGO_TARGET_DIR=str(out/'cargo-wasm'))
admin='CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM'
for k in ['RECEIPT_VAULT_INIT_ADMIN','SIMPLE_PERIDOTTROLLER_INIT_ADMIN','MARGIN_CONTROLLER_INIT_ADMIN','SWAP_ADAPTER_INIT_ADMIN','JUMP_RATE_MODEL_INIT_ADMIN','AQUARIUS_LP_VAULT_INIT_ADMIN']:env[k]=admin
for package in ['cl-share-vault','receipt-vault']:
    cmd=['stellar','contract','build','--manifest-path',str(manifest),'--package',package,'--out-dir',str(artifacts)]
    with (logs/(package+'.log')).open('w') as log:subprocess.run(cmd,cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
h=out/'harness';(h/'src').mkdir(parents=True)
(h/'Cargo.toml').write_text((ROOT/'harness/cl-runtime29/Cargo.toml').read_text().replace('name = "cl-runtime29-harness"','name = "cl-stage1-cash-runtime29-harness"'))
(h/'Cargo.lock').write_text((ROOT/'harness/cl-runtime29/Cargo.lock').read_text().replace('name = "cl-runtime29-harness"','name = "cl-stage1-cash-runtime29-harness"'))
s=(ROOT/'harness/cl-runtime29/src/lib.rs').read_text().replace('../../../contracts/cl-share-pool-facade/src/test.rs','test.rs').replace('../../../test-support/cl_resource_limits.rs',str(ROOT/'test-support/cl_resource_limits.rs')).replace('../../../contracts/cl-share-pool-facade/src/resource_profile.rs',str(ROOT/'contracts/cl-share-pool-facade/src/resource_profile.rs')).replace('path = "transaction_evidence.rs"','path = "'+str(ROOT/'harness/cl-runtime29/src/transaction_evidence.rs')+'"')
(h/'src/lib.rs').write_text(s)
s=(ROOT/'contracts/cl-share-pool-facade/src/test.rs').read_text()
s=re.sub(r'#\[path\s*=\s*"([^"]+)"\]',lambda m:'#[path = "'+str((ROOT/'contracts/cl-share-pool-facade/src'/m[1]).resolve())+'"]',s)
s+='\n#[path = "'+str(HERE/'tests/stages.rs')+'"]\nmod stage1_cash_tests;\n';(h/'src/test.rs').write_text(s)
(out/'build-manifest.json').write_text(json.dumps(dict(schema='cl-stage1-cash-first-build-v1',base=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),baseline=pins,overlay={p.name:sha(p)for p in HERE.glob('*.rs')},wasm={p.name:dict(sha256=sha(p),bytes=p.stat().st_size)for p in artifacts.glob('*.wasm')},production_adoption=False),indent=2)+'\n')
print(out)
