#!/usr/bin/env python3
"""Verify every durable member, exact gates, outcomes and separate WASM identities."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import tarfile

def main():
    p=argparse.ArgumentParser();p.add_argument('directory',nargs='?',type=Path,default=Path(__file__).parent);a=p.parse_args()
    index=json.loads((a.directory/'index.json').read_text());payload=a.directory/index['archive']['file']
    h=hashlib.sha256()
    with payload.open('rb') as f:
        for b in iter(lambda:f.read(1024*1024),b''):h.update(b)
    assert payload.stat().st_size==index['archive']['bytes'] and h.hexdigest()==index['archive']['sha256']
    seen=set();gates={};tests={};wasm={}
    with tarfile.open(payload,'r|xz') as tar:
        for m in tar:
            assert m.isfile() and m.name in index['members'] and m.name not in seen
            seen.add(m.name);h=hashlib.sha256();keep=m.name.endswith(('gates.json','tests.log'))
            retained=[]
            f=tar.extractfile(m)
            for b in iter(lambda:f.read(1024*1024),b''):
                h.update(b)
                if keep:retained.append(b)
            wanted=index['members'][m.name]
            assert m.size==wanted['bytes'] and h.hexdigest()==wanted['sha256'],m.name
            if m.name.endswith('.wasm'):wasm[m.name]=wanted
            if m.name.endswith('gates.json'):gates[m.name]=json.loads(b''.join(retained))
            if m.name.endswith('tests.log'):
                s=b''.join(retained).decode(errors='replace')
                tests[m.name]={'cargo_results':re.findall(r'test result: .*',s),'complete_result_present':bool(re.search(r'test result: ',s))}
    assert seen==set(index['members'])
    # A failed or interrupted run is kept as evidence, never turned into a pass.
    print(json.dumps({'archive_verified':True,'members':len(seen),'exact_gate_reports':gates,'test_outcomes':tests,'separate_wasm_identities':wasm,'full_checker_replay':False,'release_approval':False},indent=2))

if __name__=='__main__':main()
