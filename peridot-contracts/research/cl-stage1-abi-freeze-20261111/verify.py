#!/usr/bin/env python3
"""Offline file-identity replay for package1; not a financial gate."""
import hashlib
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
manifest = json.loads((HERE / 'manifest.json').read_text())
for row in manifest['files']:
    data = (ROOT / row['path']).read_bytes()
    assert len(data) == row['bytes'], row['path']
    assert hashlib.sha256(data).hexdigest() == row['sha256'], row['path']
assert manifest['financial_tests_run'] is False
assert manifest['release_approval'] is False
print(f"PASS: {len(manifest['files'])} package1 file identities; financial gates remain unrun.")
