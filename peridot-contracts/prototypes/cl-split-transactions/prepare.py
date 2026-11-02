#!/usr/bin/env python3
"""Rebuild isolated hash-bound split overlay; no signing/network/deployment."""
import argparse, hashlib, json, os, re, shutil, subprocess
from pathlib import Path

ROOT=Path(__file__).resolve().parents[2]
HERE=Path(__file__).resolve().parent
ADMIN='CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM'
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def main():
 p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True);a=p.parse_args()
 out=a.output.resolve()
 if out.exists(): raise SystemExit('Refusing existing output')
 pins=json.loads((HERE/'baseline-source-hashes.json').read_text())
 for name,h in pins.items():
  path=(ROOT/name).resolve()
  if not path.is_relative_to(ROOT) or sha(path)!=h:raise SystemExit('Baseline source differs: '+name)
 ws=out/'workspace';ws.mkdir(parents=True)
 for name in pins:
  dest=ws/name;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(ROOT/name,dest)
 for name in json.loads((HERE/'overlay-files.json').read_text()): (ws/name).parent.mkdir(parents=True,exist_ok=True)
 subprocess.run(['patch','--batch','-p1','-i',str(HERE/'prototype.patch')],cwd=ws,check=True)
 manifest=ws/'Cargo.toml';s=manifest.read_text().replace('../openzeppelin-stellar-contracts',str((ROOT/'../openzeppelin-stellar-contracts').resolve()));manifest.write_text(s)
 logs=out/'logs';logs.mkdir();baseline=out/'baseline-artifacts';artifacts=out/'artifacts';baseline.mkdir();artifacts.mkdir()
 env=dict(os.environ,CARGO_INCREMENTAL='0',CARGO_NET_OFFLINE='true')
 for key in ['RECEIPT_VAULT_INIT_ADMIN','SIMPLE_PERIDOTTROLLER_INIT_ADMIN','MARGIN_CONTROLLER_INIT_ADMIN','SWAP_ADAPTER_INIT_ADMIN','JUMP_RATE_MODEL_INIT_ADMIN','AQUARIUS_LP_VAULT_INIT_ADMIN']:env[key]=ADMIN
 def build(manifest,package,dest,label,features=None):
  # Same-named crates in two workspaces must never share the final WASM path:
  # Cargo can consider one crate fresh after the other overwrites that path.
  build_env=dict(env,CARGO_TARGET_DIR=str(out/('cargo-baseline' if dest==baseline else 'cargo-prototype')))
  cmd=['stellar','contract','build','--manifest-path',str(manifest),'--package',package,'--out-dir',str(dest)]
  if features:cmd+=['--features',features]
  with (logs/(label+'.log')).open('w') as log: subprocess.run(cmd,cwd=ROOT,env=build_env,stdout=log,stderr=subprocess.STDOUT,check=True)
 for package in ['cl-integration-fixtures','cl-share-vault','cl-share-pool-facade','margin-controller','swap-adapter','receipt-vault','simple-peridottroller','jump-rate-model']:
  build(ROOT/'Cargo.toml',package,baseline,'baseline-'+package,'funded-liquidation-validation' if package=='margin-controller' else None)
 for file in baseline.glob('*.wasm'):shutil.copy2(file,artifacts/file.name)
 for package in ['cl-share-vault','margin-controller','cl-split-escrow']:
  build(ws/'Cargo.toml',package,artifacts,'prototype-'+package,'funded-liquidation-validation' if package=='margin-controller' else None)
 harness=out/'harness';(harness/'src').mkdir(parents=True)
 s=(ROOT/'harness/cl-runtime29/Cargo.toml').read_text().replace('name = "cl-runtime29-harness"','name = "cl-split-runtime29-harness"');(harness/'Cargo.toml').write_text(s)
 shutil.copy2(ROOT/'harness/cl-runtime29/Cargo.lock',harness/'Cargo.lock')
 s=(ROOT/'harness/cl-runtime29/src/lib.rs').read_text().replace('../../../contracts/cl-share-pool-facade/src/test.rs','test.rs').replace('../../../test-support/cl_resource_limits.rs',str(ROOT/'test-support/cl_resource_limits.rs')).replace('../../../contracts/cl-share-pool-facade/src/resource_profile.rs',str(ROOT/'contracts/cl-share-pool-facade/src/resource_profile.rs')).replace('path = "transaction_evidence.rs"','path = "'+str(ROOT/'harness/cl-runtime29/src/transaction_evidence.rs')+'"');(harness/'src/lib.rs').write_text(s)
 s=(ROOT/'contracts/cl-share-pool-facade/src/test.rs').read_text();s=re.sub(r'#\[path\s*=\s*"([^"/]+\.rs)"\]',lambda m:'#[path = "'+str(ROOT/'contracts/cl-share-pool-facade/src'/m[1])+'"]',s)
 s+='\n#[path = "'+str(HERE/'tests/split.rs')+'"]\nmod split_tests;\n';(harness/'src/test.rs').write_text(s)
 (out/'build-manifest.json').write_text(json.dumps({'schema':'cl-split-prototype-build-v1','baseline_commit':'9cf43082bfb57334d57dbc58cfa90ccad4d98078','baseline_files':pins,'overlay_sha256':sha(HERE/'prototype.patch'),'synthetic_admin':ADMIN,'release_approval':False,'wasm':{str(f.relative_to(out)):{'sha256':sha(f),'bytes':f.stat().st_size} for folder in [baseline,artifacts] for f in folder.glob('*.wasm')}},indent=2)+'\n')
 print(out)
if __name__=='__main__':main()
