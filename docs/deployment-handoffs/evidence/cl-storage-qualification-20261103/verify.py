#!/usr/bin/env python3
"""Verify every member of the adjacent controlled-state evidence archive."""
import hashlib, json, tarfile
from pathlib import Path
base=Path(__file__).resolve().parent
index=json.loads((base/'index.json').read_text())
archive=base/index['archive']
def digest(path):
 h=hashlib.sha256()
 with path.open('rb') as f:
  for block in iter(lambda:f.read(1024*1024),b''):h.update(block)
 return h.hexdigest()
assert archive.stat().st_size==index['archive_bytes']
assert digest(archive)==index['archive_sha256']
seen=set();total=0
with tarfile.open(archive,'r:xz') as tar:
 for member in tar:
  assert member.isfile() and member.name not in seen and member.name in index['members']
  assert not member.name.startswith('/') and '..' not in Path(member.name).parts
  expected=index['members'][member.name];seen.add(member.name);h=hashlib.sha256();size=0
  with tar.extractfile(member) as f:
   for block in iter(lambda:f.read(1024*1024),b''):h.update(block);size+=len(block)
  assert size==member.size==expected['bytes']
  assert h.hexdigest()==expected['sha256'],member.name
  total+=size
assert seen==set(index['members']) and len(seen)==index['member_count']
assert total==index['total_uncompressed_bytes']
print(json.dumps({'archive_sha256':index['archive_sha256'],'archive_bytes':index['archive_bytes'],'verified_members':len(seen),'verified_uncompressed_bytes':total,'verification':'PASS','release_approval':False},indent=2))
