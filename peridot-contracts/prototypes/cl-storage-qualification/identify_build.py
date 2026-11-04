#!/usr/bin/env python3
"""Give a prepared, not-yet-executed harness a content-specific Cargo identity.

Shared dependency caches are safe, but same-name isolated harnesses with copied
source mtimes can otherwise reuse stale relative Cargo dependency records.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re

p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--build',type=Path,required=True)
a=p.parse_args()
m=json.loads((a.build/'build-manifest.json').read_text())
hashes={str(f.relative_to(a.build)):hashlib.sha256(f.read_bytes()).hexdigest() for f in sorted((a.build/'harness/src').glob('*.rs'))}
assert hashes==m['harness_generated_sources']
name='cl-storage-'+hashlib.sha256(json.dumps(hashes,sort_keys=True).encode()).hexdigest()[:16]+'-harness'
manifest=a.build/'harness/Cargo.toml'
old=re.search(r'^name = "([^"]+)"',manifest.read_text(),re.M)[1]
for f in [manifest,a.build/'harness/Cargo.lock']:
    text=f.read_text()
    assert text.count('name = "'+old+'"')==1
    f.write_text(text.replace('name = "'+old+'"','name = "'+name+'"'))
m['cargo_harness_identity']={'name':name,'source_hashes':hashes,'script_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
(a.build/'build-manifest.json').write_text(json.dumps(m,indent=2)+'\n')
print(name)
