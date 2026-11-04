#!/usr/bin/env python3
"""Offline exact-member preservation verification; no contract execution."""
import hashlib,json,tarfile
from pathlib import Path
root=Path(__file__).resolve().parent
index=json.loads((root/'index.json').read_text());archive=root/index['archive_file']
def digest(path):
 h=hashlib.sha256()
 with path.open('rb') as f:
  for b in iter(lambda:f.read(1024*1024),b''):h.update(b)
 return h.hexdigest()
assert archive.stat().st_size==index['archive_bytes'] and digest(archive)==index['archive_sha256']
seen=set()
with tarfile.open(archive,'r:xz') as tar:
 for m in tar:
  assert m.isfile() and m.name not in seen and not m.name.startswith('/') and '..' not in Path(m.name).parts
  seen.add(m.name);expected=index['payload_members'][m.name];h=hashlib.sha256();size=0
  with tar.extractfile(m) as f:
   for b in iter(lambda:f.read(1024*1024),b''):h.update(b);size+=len(b)
  assert size==m.size==expected['bytes'] and h.hexdigest()==expected['sha256'],m.name
assert seen==set(index['payload_members'])
for name,v in json.loads((root/'supporting-checksums.json').read_text()).items():assert (root/name).stat().st_size==v['bytes'] and digest(root/name)==v['sha256'],name
print(json.dumps({'preservation':'PASS','members':len(seen),'archive_sha256':index['archive_sha256'],'archive_bytes':index['archive_bytes'],'full_gate_replay_performed':False,'release_approval':False},indent=2))
