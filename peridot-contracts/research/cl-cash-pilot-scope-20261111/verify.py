#!/usr/bin/env python3
"""Offline scope/evidence verification only: no build, signing or RPC."""
import hashlib
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
manifest = json.loads((HERE / 'inventory.json').read_text())
for item in manifest['files']:
    data = (ROOT / item['path']).read_bytes()
    assert len(data) == item['bytes'], item['path']
    assert hashlib.sha256(data).hexdigest() == item['sha256'], item['path']
resources = json.loads((HERE / 'resource-evidence.json').read_text())
native = resources['native_reserve_borrow']
r = native['original_row']
assert native['sdk_entries'] == r['read_entries'] + r['disk_read_entries'] + r['write_entries'] == 51
assert all(row['status'] == 'FAIL_MISSING_COMPLETE_EVIDENCE' for row in resources['required_cash_roots'])
assert resources['release_approval'] is False
assert resources['new_execution_performed'] is False
print(f"PASS: {len(manifest['files'])} pinned sources/evidence files; native SDK arithmetic; all missing cash-root gates remain failed. No runtime test performed.")
