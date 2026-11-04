#!/usr/bin/env python3
"""Verify preserved unsigned input bytes and public captured settings before replay."""
from pathlib import Path
import argparse,hashlib,json
p=argparse.ArgumentParser();p.add_argument('directory',type=Path);a=p.parse_args();r=a.directory
pairs=[('receipt-code-input','receipt-code-input.json'),('receipt-state-input','receipt-state-input.json'),('maintenance-input','maintenance-input.json'),('partial-input','partial-input.json')]
expected_config=None;out=[]
for stem,name in pairs:
 d=json.loads((r/name).read_text());b=(r/(stem+'.txt')).read_bytes()
 if stem=='receipt-code-input':
  row=d['row'];settings=json.loads((r/'captured-network-config.json').read_text())['settings'];lines=[str(row['current_ledger']),str(row['fees']['total']),row['unsigned_envelope_xdr'],str(len(settings))]+[s['xdr'] for s in settings.values()]+[str(len(row['keys']))]
  for k in row['keys']:lines.extend([k['entry_xdr'],str(k['old_ttl'])])
  assert b==('\n'.join(lines)+'\n').encode(),stem
 else:assert hashlib.sha256(b).hexdigest()==d['input_sha256'],stem
 config=d['network_config_sha256'];assert expected_config is None or config==expected_config;expected_config=config
 out.append({'input':stem+'.txt','bytes':len(b),'sha256':hashlib.sha256(b).hexdigest()})
assert hashlib.sha256((r/'captured-network-config.json').read_bytes()).hexdigest()==expected_config
print(json.dumps({'inputs_verified':out,'network_config_sha256':expected_config,'execution_claim':False},indent=2))
