#!/usr/bin/env python3
"""Verify frozen baseline identities and honest proof status; no financial gate."""
import hashlib
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
pins = json.loads((HERE / 'fixture-pins.json').read_text())
for name, expected in pins['baseline_files'].items():
    assert hashlib.sha256((ROOT / name).read_bytes()).hexdigest() == expected, name
proof = json.loads((HERE / 'proof-map.json').read_text())
assert [r['row'] for r in proof['rows']] == ['O1','O2','O3','O4','O5','C1','C2a','C2b','C3']
assert [g['gate'] for g in proof['gates']] == ['G1','G2','G3','G4']
assert all(r['implementation_status'] == 'not_implemented' and r['execution_status'] == 'not_run' for r in proof['rows'])
assert all(g['status'] == 'not_run' and g['passing_evidence'] is None for g in proof['gates'])
assert proof['release_approval'] is False and proof['missing_required_flows_must_fail'] is True
print('PASS: baseline identities and package1 incomplete proof status. No lifecycle/release gate passed.')
