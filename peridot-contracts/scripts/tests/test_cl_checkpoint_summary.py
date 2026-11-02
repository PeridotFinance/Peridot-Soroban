"""Fail-closed summary regression fixtures, never contract execution evidence.

The fake WASM headers, profiles, logs and numeric records are deliberately only
metadata. These tests need no runner artifacts, keys, network or SDK installation.
"""
import copy
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import summarize_cl_checkpoint as s

SDK = {key: 1 for key in (
    'instructions', 'memory_bytes', 'read_entries', 'write_entries',
    'disk_read_entries', 'read_bytes', 'write_bytes', 'contract_events_size_bytes',
    'persistent_rent_ledger_bytes', 'persistent_entry_rent_bumps',
    'temporary_rent_ledger_bytes', 'temporary_entry_rent_bumps')}
SDK.update(ok=True, runtime_protocol=29, flow='synthetic_metadata_only')
TX = {
    'success': True, 'expected_rejection': False,
    'flow': 'synthetic_metadata_only', 'test_name': 'synthetic_metadata_only',
    'resources': {key: 1 for key in (
        'instructions', 'memory_bytes', 'transaction_size_bytes', 'write_entries',
        'disk_read_entries', 'disk_read_bytes', 'write_bytes', 'contract_events_size_bytes')},
    'fees': {'inclusion': 1, 'non_refundable': 1, 'refundable': 1, 'rent': 1, 'total': 4},
    'fee_inputs': {'synthetic_test_only': 1}, 'rent_fee_inputs': {'synthetic_test_only': 1},
    'footprint': {'read_only_xdr': [], 'read_write_xdr': []},
    'restored_rw_entry_indices': [], 'authorization_size_bytes': 4,
    'return_value_size_bytes': 4,
}

class SummaryTests(unittest.TestCase):
 def setUp(self):
  self.tmp = tempfile.TemporaryDirectory(prefix='cl-summary-test-')
  self.run = Path(self.tmp.name).resolve()
  (self.run/'logs').mkdir(); (self.run/'artifacts').mkdir()
  self.runner = self.run/'runner.log'
  self.runner.write_text(f'Checkpoint: {self.run}\nStatus: 1 (missing proofs remain failures)\n')
  (self.run/'network-config.json').write_text('{"synthetic_test_only":true}\n')
  digest = s.digest(self.run/'network-config.json')
  for a in s.ARTIFACTS:
   (self.run/'artifacts'/f'{a}.wasm').write_bytes(b'\0asm\1\0\0\0')
   (self.run/'logs'/f'build-{a.replace("_","-")}.log').write_text('Build Complete\n')
  log='test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1s\n'
  (self.run/'logs/reward-tests.log').write_text(log)
  for p in s.PROFILES:
   for suite in s.SUITES: (self.run/'logs'/f'{p}-{suite}.log').write_text(log)
   gate={'profile':p,'config_sha256':digest,'gates':{g:False for g in s.GATES},'release_approval':False,'missing_required_flows':['synthetic-missing-flow'],'lifecycle_errors':['synthetic failed test'],'transaction_errors':['synthetic unavailable live state']}
   (self.run/f'{p}-checkpoint-summary.json').write_text(json.dumps(gate))
   (self.run/'logs'/f'{p}-checkpoint.log').write_text('\n'.join(f'{g.upper()} GATE: FAIL' for g in s.GATES))
   (self.run/'logs'/f'{p}-native-funded-budget.log').write_text('CHECKPOINT GATE: FAIL\n')
   (self.run/f'{p}.json').write_text(json.dumps({'name':p,'network_sha256':digest,'limits':{'synthetic_test_only':1}}))
   for suffix,data in [('resources',SDK),('native-funded',SDK),('calls',{'synthetic':True}),('transactions',TX)]:
    row=copy.deepcopy(data);row['profile']=p
    (self.run/f'{p}-{suffix}.jsonl').write_text(json.dumps(row)+'\n')
   manifest={'profile':p,'resource_report_sha256':s.digest(self.run/f'{p}-resources.jsonl'),'test_runs':[{'path':str(self.run/'logs'/f'{p}-{suf}.log'),'sha256':s.digest(self.run/'logs'/f'{p}-{suf}.log'),'exit_code':101} for suf in ['harness','archive','history-scale-archive']]}
   (self.run/f'{p}-proof-manifest.json').write_text(json.dumps(manifest))
 def tearDown(self): self.tmp.cleanup()
 def test_completed_failed_run_remains_failed(self):
  result=s.summarize(self.run,self.runner)
  self.assertFalse(result['release_approval']);self.assertEqual(result['runner_exit_status'],1)
  self.assertFalse(result['profiles']['internal-policy']['gate_summary']['gates']['checkpoint'])
 def test_no_terminal_status(self):
  self.runner.write_text('still running\n')
  with self.assertRaises(ValueError): s.summarize(self.run,self.runner)
 def test_wrong_run_path(self):
  self.runner.write_text('Checkpoint: /wrong\nStatus: 1 (missing proofs remain failures)\n')
  with self.assertRaises(ValueError): s.summarize(self.run,self.runner)
 def test_missing_required_files(self):
  for relative in ['logs/captured-network-diagnostic-harness.log','logs/captured-network-diagnostic-archive.log','logs/captured-network-diagnostic-history-scale-archive.log','captured-network-diagnostic-checkpoint-summary.json','artifacts/margin_controller.wasm']:
   p=self.run/relative;data=p.read_bytes();p.unlink()
   with self.subTest(relative=relative), self.assertRaises((ValueError,KeyError)): s.summarize(self.run,self.runner)
   p.write_bytes(data)
 def test_zero_status_cannot_hide_failures(self):
  self.runner.write_text(f'Checkpoint: {self.run}\nStatus: 0 (missing proofs remain failures)\n')
  with self.assertRaises(ValueError): s.summarize(self.run,self.runner)
 def test_changed_bound_log(self):
  p=self.run/'logs/internal-policy-harness.log';p.write_text(p.read_text()+'changed\n')
  with self.assertRaises(ValueError): s.summarize(self.run,self.runner)
 def test_success_missing_measurement_rejected(self):
  row=copy.deepcopy(TX);row.pop('resources')
  with self.assertRaises(ValueError): s.transaction_summary([row])
 def test_limit_failure_has_no_fabricated_zero(self):
  row={'success':False,'expected_rejection':False,'error':'Budget ExceededLimit','flow':'swap_open'}
  result=s.transaction_summary([row]);self.assertEqual(result['unexpected_failures'],1)
  self.assertIsNone(result['max_resources']['instructions']);self.assertEqual(len(result['unmeasured_failures']),1)
if __name__ == '__main__':
 unittest.main()
