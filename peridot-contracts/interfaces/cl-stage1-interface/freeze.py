#!/usr/bin/env python3
"""Source-derived key/API audit. No RPC, keys, signing, financial execution or builds."""
import argparse,hashlib,json,re
from pathlib import Path
root=Path(__file__).resolve().parents[2];here=Path(__file__).resolve().parent
sources=[('receipt','contracts/receipt-vault/src/storage.rs','DataKey'),('receipt','contracts/receipt-vault/src/reward_backing.rs','BackingKey'),('receipt','contracts/receipt-vault/src/reward_claims.rs','CoordinatorKey'),('receipt','contracts/receipt-vault/src/reward_ledger.rs','LedgerKey')]
sources += [('vault','contracts/cl-share-vault/src/'+n+'.rs','DataKey'if n=='storage'else'Key')for n in ['storage','accounting','allowances','custody','rewards','reward_history']]
sources += [('facade','contracts/cl-share-pool-facade/src/lib.rs','Key')]
source=(here/'src/lib.rs').read_text()
# New key variants are checked against every old variant regardless of durability,
# stronger than a cross-durability check for new namespaces. Legacy collisions are reported.
rows=[];parts=['// Generated from exact source enum declarations by freeze.py; not runtime storage.','use soroban_sdk::{contracttype, Address, Env, xdr::ToXdr};','use cl_stage1_interface::{ReceiptKeyV1,VaultKeyV1};']
for i,(contract,name,enum)in enumerate(sources):
 text=(root/name).read_text();m=re.search(r'\benum '+enum+r'\s*\{([^}]+)\}',text);assert m,name
 body=re.sub(r'//[^\n]*','',m[1]);alias=f'LegacyKey{i}'
 parts.append(f'#[contracttype]\n#[derive(Clone)]\nenum {alias} {{ {body} }}')
 variants=re.findall(r'(\w+)\s*(?:\(([^)]+)\))?\s*,',body)
 for variant,types in variants:
  args=[]
  for t in types.split(',')if types else[]:
   t=t.strip();args.append({'Address':'address.clone()','u64':'42u64','u32':'42u32','i32':'42i32'}[t])
  expr=alias+'::'+variant+('('+','.join(args)+')'if args else'')
  rows.append(dict(contract=contract,source=name,enum=enum,variant=variant,types=types,expression=expr))
parts+=['#[test]\nfn new_key_namespaces_do_not_alias_existing_contract_keys(){','let e=Env::default();','let address=Address::from_string(&soroban_sdk::String::from_str(&e,"CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM"));','let mut existing:std::collections::BTreeMap<&str,std::collections::BTreeSet<std::vec::Vec<u8>>>=Default::default();']
for r in rows:parts.append(f'existing.entry("{r["contract"]}").or_default().insert({r["expression"]}.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>());')
for contract,enum in [('receipt','ReceiptKeyV1'),('vault','VaultKeyV1')]:
 m=re.search(r'pub enum '+enum+r'\s*\{([^}]+)\}',source);assert m
 variants=re.findall(r'(\w+)\s*(?:\(([^)]+)\))?\s*(?:,|$)',m[1].strip())
 parts.append('let mut new=std::collections::BTreeSet::new();')
 for variant,types in variants:
  expr=enum+'::'+variant+('('+('address.clone()'if types=='Address'else'42u64')+')'if types else'')
  parts.append(f'let bytes={expr}.to_xdr(&e).iter().collect::<std::vec::Vec<u8>>();assert!(!existing["{contract}"].contains(&bytes),"new namespace collision {variant}");assert!(new.insert(bytes),"duplicate new key");')
parts.append('}')
generated='\n'.join(line.rstrip() for line in '\n'.join(parts).splitlines())+'\n'
api={}
for contract,trait in [('receipt','Stage1ReceiptV1'),('vault','Stage1VaultV1')]:
 body=re.search('pub trait '+trait+r'\s*\{([^}]+)\}',source)[1]
 api[contract]=[' '.join(x.split())for x in re.findall(r'fn\s+[^;]+;',body)]
 # No function may shadow an already-exported entrypoint in its target contract.
 paths=[root/'contracts'/('receipt-vault'if contract=='receipt'else'cl-share-vault')/'src/contract.rs']
 if contract=='receipt':paths+=list((root/'contracts/receipt-vault/src').glob('*.rs'))
 old='\n'.join(p.read_text()for p in paths)
 for sig in api[contract]:
  name=re.match(r'fn\s+(\w+)',sig)[1]
  assert not re.search(r'pub fn\s+'+name+r'\s*\(',old),f'existing ABI collision {contract}.{name}'
# Reserved error range must not be reused by existing contract errors.
for contract in ['receipt-vault','cl-share-vault','cl-share-pool-facade']:
 for path in (root/'contracts'/contract/'src').rglob('*.rs'):
  t=path.read_text();assert not re.search(r'=\s*17(?:0\d|1\d)\b',t),str(path)
manifest=dict(schema='stage1-abi-freeze-v1',sdk='25.3.1',source_sha256=hashlib.sha256(source.encode()).hexdigest(),apis=api,legacy_key_variants=rows,source_files={name:hashlib.sha256((root/name).read_bytes()).hexdigest()for _,name,_ in sources},limits=dict(cpu=100000000,memory=41943040,sdk_total=100,unique_keys=100,writes=50),scope='wire/key freeze only; no financial gate passed',new_key_durability={'receipt':{'Stage1ConfigV1':'instance','Stage1Next':'instance','Stage1Busy':'instance','Stage1Job':'persistent'},'vault':{'Stage1Receipt':'instance','Stage1RiskV1':'instance','Stage1Reserved':'instance','Stage1Liability':'persistent','Stage1Locked':'persistent','Stage1Allocation':'persistent'}})
p=argparse.ArgumentParser();p.add_argument('--initialize',action='store_true');args=p.parse_args()
outputs={'tests/key_catalog.rs':generated,'abi-manifest.json':json.dumps(manifest,indent=2)+'\n'}
for name,text in outputs.items():
 target=here/name
 if args.initialize:
  assert not target.exists(),f'never overwrite frozen output: {target}'
  target.write_text(text)
 else:assert target.read_text()==text,f'freeze drift: {target}; explicit version/review required'
print(f'{len(rows)} existing key variants; {sum(map(len,api.values()))} signatures; source/API freeze PASS')
