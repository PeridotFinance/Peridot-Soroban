#!/usr/bin/env python3
"""Offline evidence identities and research status; never a deployment gate."""
import hashlib
import json
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
manifest = json.loads((HERE / 'manifest.json').read_text())
for row in manifest['files']:
    data = (ROOT / row['path']).read_bytes()
    assert len(data) == row['bytes'], row['path']
    assert hashlib.sha256(data).hexdigest() == row['sha256'], row['path']
for child in ['pool-data', 'price-history']:
    subprocess.run([sys.executable, str(HERE / child / 'verify.py')], check=True)
net = json.loads((HERE / 'lead/net-sensitivity.json').read_text())
assert len(net['rows']) == 60
assert all(r['realized_net_yield_usd'] is None and r['realized_arbitrage_loss_usd'] is None for r in net['rows'])
for filename in ['net-sensitivity.json', 'current-guard-sensitivity.json']:
    doc = json.loads((HERE / 'lead' / filename).read_text())
    for name, expected in doc['inputs'].items():
        source = HERE / ('pool-data' if filename == 'net-sensitivity.json' else '') / name
        assert hashlib.sha256(source.read_bytes()).hexdigest() == expected, name
assert manifest['new_financial_execution'] is False
print(f"PASS {len(manifest['files'])} checkpoint file identities; realized nets and new resource gates remain unproven.")
