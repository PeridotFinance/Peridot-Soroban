"""Lead-invoked isolated WASM build only. No signing or financial invocation."""
from pathlib import Path
import hashlib,json,os,subprocess,sys
root=Path(__file__).resolve().parents[2];out=Path(sys.argv[1]).resolve()
assert out.is_relative_to(root/'target/cl-stage1-live-pairs')
pin=json.loads((out/'preparation.json').read_text())
for f,h in pin['source_files'].items():assert hashlib.sha256(Path(f).read_bytes()).hexdigest()==h,f
env=dict(os.environ,CARGO_INCREMENTAL='0',CARGO_NET_OFFLINE='true',CARGO_TARGET_DIR=str(out/'cargo-wasm-correct-admin'))
admin='CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM'
for k in ['RECEIPT_VAULT_INIT_ADMIN','SIMPLE_PERIDOTTROLLER_INIT_ADMIN','MARGIN_CONTROLLER_INIT_ADMIN','SWAP_ADAPTER_INIT_ADMIN','JUMP_RATE_MODEL_INIT_ADMIN','AQUARIUS_LP_VAULT_INIT_ADMIN']:env[k]=admin
log=out/'logs/build-vault-isolated.log'
assert not log.exists(),'fresh attempt directory required'
cmd=['stellar','contract','build','--manifest-path',str(out/'workspace/Cargo.toml'),'--package','cl-share-vault','--out-dir',str(out/'artifacts')]
(out/'logs/build-isolated-command.json').write_text(json.dumps({'cmd':cmd,'target':env['CARGO_TARGET_DIR'],'fixture_admin':admin})+'\n')
with log.open('w')as f:r=subprocess.run(cmd,cwd=root,env=env,stdout=f,stderr=subprocess.STDOUT)
(out/'logs/build-isolated-exit.txt').write_text(str(r.returncode)+'\n')
if r.returncode:sys.exit(r.returncode)
(out/'build-manifest.json').write_text(json.dumps({'scope':pin['scope'],'preparation_sha256':hashlib.sha256((out/'preparation.json').read_bytes()).hexdigest(),'wasm':{p.name:{'sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'bytes':p.stat().st_size}for p in (out/'artifacts').glob('*.wasm')}},indent=2)+'\n')
print(out/'build-manifest.json')
