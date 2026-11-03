#!/usr/bin/env python3
"""Derive restore-prefix feasibility separately from unchanged financial gates."""
import argparse
from collections import Counter
import hashlib
import importlib.util
import json
from pathlib import Path
import re

HERE=Path(__file__).resolve().parent
SPEC=importlib.util.spec_from_file_location('restore_checker_prefix',HERE/'restore-checker.py')
CHECKER=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(CHECKER)
PROFILES=('internal-policy','captured-network-diagnostic')

def digest(raw):return hashlib.sha256(raw).hexdigest()

def prefix_errors(journey,profile,known_batches,producer_hash):
    """No final host/SDK result is used to judge this completed prefix."""
    errors=[]
    def require(value,message):
        if not value:errors.append(message)
    def sha(value):return isinstance(value,str) and re.fullmatch('[0-9a-f]{64}',value) is not None
    try:
        require(journey.get('schema')=='cl-restore-journey-v1','prefix journey schema mismatch')
        require(journey.get('profile')==profile['name'],'prefix profile mismatch')
        require(journey.get('config_sha256')==profile['network_sha256'],'prefix configuration mismatch')
        require(journey.get('source_sha256')==producer_hash,'prefix producer source mismatch')
        operation=journey['operation'];batches=journey['preparation'];required=journey['required_archived_key_xdrs']
        require(isinstance(operation,str) and bool(operation),'prefix operation missing')
        require(isinstance(batches,list) and bool(batches),'prefix batches missing')
        require(isinstance(required,list) and bool(required) and all(isinstance(k,str) for k in required),'prefix required keys missing')
        require(len(required)==len(set(required)),'duplicate required archived key')
        require(type(journey['required_archived_keys']) is int and journey['required_archived_keys']==len(required),'prefix required key count mismatch')
        require(type(journey['preparation_transactions']) is int and journey['preparation_transactions']==len(batches),'prefix batch count mismatch')
        previous=journey['cold_snapshot_sha256'];prepared=journey['prepared_snapshot_sha256'];keys=[]
        require(sha(previous) and sha(prepared),'prefix snapshot hash invalid')
        for index,batch in enumerate(batches):
            require(CHECKER.canonical(batch) in known_batches,'prefix batch absent from preparation report')
            require(type(batch['batch']) is int and batch['batch']==index,'prefix batch order mismatch')
            require(batch['flow']==operation and batch['profile']==profile['name'],'prefix batch root/profile mismatch')
            require(batch['source_sha256']==producer_hash,'prefix batch source mismatch')
            require(batch['snapshot_before_sha256']==previous,'prefix snapshot chain broken')
            previous=batch['snapshot_after_sha256'];require(sha(previous),'prefix batch snapshot hash invalid')
            require(batch.get('financial_data_unchanged') is True,'prefix batch TTL-only financial preservation missing')
            keys.extend(k['key_xdr'] for k in batch['keys'])
        require(keys==required,'prefix ordered key union differs from required archived keys')
        require(len(keys)==len(set(keys)),'prefix duplicate key across batches')
        require(previous==prepared,'prefix does not end at pretrade prepared snapshot')
        require(journey.get('financial_data_unchanged_by_preparation') is True,'prefix TTL-only financial preservation missing')
        for field,fee in [('preparation_fee_total','total'),('preparation_rent_total','rent')]:
            require(type(journey[field]) is int and journey[field]==sum(b['fees'][fee] for b in batches),'prefix fee sum mismatch: '+field)
    except (ValueError,KeyError,TypeError,IndexError) as error:errors.append('incomplete prefix evidence: '+str(error))
    return errors

def derive_profile(rows,journeys,profile,config,producer_hash,validator=CHECKER.validate):
    batch_results=[{'row':i,'errors':validator(r,profile,config,producer_sha256=producer_hash)} for i,r in enumerate(rows)]
    errors=[]
    if not rows:errors.append('missing restore batch records')
    if not journeys:errors.append('missing restore prefix journeys')
    known=Counter(CHECKER.canonical(r) for r in rows)
    try:
        used=Counter(CHECKER.canonical(b) for j in journeys for b in j['preparation'])
        if used!=known:errors.append('prefix journeys do not cover external preparation records exactly once')
    except (KeyError,TypeError):errors.append('prefix batch coverage cannot be established')
    results=[{'operation':j.get('operation'),'errors':prefix_errors(j,profile,known,producer_hash)} for j in journeys]
    integrity=bool(journeys) and not errors and all(not r['errors'] for r in results)
    batches=bool(rows) and all(not r['errors'] for r in batch_results)
    return {'profile':profile['name'],'rows':batch_results,'prefixes':results,'errors':errors,
            'gates':{'prefix_integrity':integrity,'restoration_prefix_feasible':integrity and batches},
            'gate_scope':'Ordered unsigned restore prefix only. Final financial execution, full journey evidence, Core application, signatures and live state are separate gates.',
            'state_scope':'Decoded per-key LedgerEntryData preservation and predicted minimum TTL, plus hash-linked producer snapshot attestations; actual Core metadata/application is unverified.',
            'release_approval':False}

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--run',type=Path,required=True);parser.add_argument('--output',type=Path,required=True);args=parser.parse_args()
    run=args.run.resolve();out=args.output.resolve()
    if out.exists():raise SystemExit('refusing to overwrite derived report directory')
    out.mkdir(parents=True)
    source_files={}
    def read(path):
        raw=path.read_bytes();source_files[str(path)]={'sha256':digest(raw),'bytes':len(raw)};return raw
    try:
        verification=json.loads(read(run/'verification.json'))
        configuration=json.loads(read(run/'checkpoint/run-configuration.json'))
        producer=Path(configuration['build'])/'harness/src/restoration.rs';producer_hash=digest(read(producer))
        config_path=run/'network-config.json';config=json.loads(read(config_path))
        rows=[json.loads(line) for line in read(run/'preparation.jsonl').splitlines() if line.strip()]
        journeys=[json.loads(line) for line in read(run/'journeys.jsonl').splitlines() if line.strip()]
        if any(r.get('profile') not in PROFILES for r in rows+journeys):raise ValueError('unknown/missing profile in raw evidence')
        profiles={}
        for name in PROFILES:
            profile=json.loads(read(run/'profiles'/f'{name}.json'));expected=CHECKER.create(config_path,name)
            for field in ['schema','name','mode','network_sha256','captured_ledger','target_protocol','limits','release_approval']:
                if profile.get(field)!=expected[field]:raise ValueError('profile validation failed: '+name+'/'+field)
            profile['network_config_path']=str(config_path)
            report=derive_profile([r for r in rows if r['profile']==name],[j for j in journeys if j['profile']==name],profile,config,producer_hash)
            for label,filename in [('original_preparation','preparation-gates.json'),('original_financial','gates.json')]:
                prior=json.loads(read(run/name/filename));reported=verification['profiles'][name]['preparation' if label=='original_preparation' else 'financial']['gate_summary']
                if prior!=reported:raise ValueError('completed verification does not bind exact original report: '+name+'/'+label)
                report[label]=prior
            profiles[name]=report
        for name,pin in source_files.items():
            if digest(Path(name).read_bytes())!=pin['sha256']:raise ValueError('source changed during verification: '+name)
        result={'schema':'cl-restoration-prefix-summary-v1','run':str(run),'profiles':profiles,'source_files':source_files,'producer_sha256':producer_hash,'checker_sha256':digest((HERE/'restore-checker.py').read_bytes()),'prefix_checker_sha256':digest(Path(__file__).read_bytes()),'release_approval':False}
        passed=all(p['gates']['restoration_prefix_feasible'] for p in profiles.values())
    except (OSError,ValueError,KeyError,TypeError) as error:
        result={'schema':'cl-restoration-prefix-summary-v1','errors':[str(error)],'source_files':source_files,'release_approval':False};passed=False
    result['selected_gate']='restoration_prefix_feasible';result['selected_gate_passed']=passed
    (out/'summary.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({'selected_gate':result['selected_gate'],'passed':passed,'release_approval':False}))
    return 0 if passed else 1

if __name__=='__main__':raise SystemExit(main())
