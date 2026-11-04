#!/usr/bin/env python3
"""Extract raw independently metered companion rows and re-run all budget checks."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--run',type=Path,required=True)
p.add_argument('--output',type=Path,required=True)
p.add_argument('--view',choices=['live','data'],required=True)
p.add_argument('--test-filter',help='Select transaction rows whose exact recorded test_name contains this substring; source test logs remain unchanged')
a=p.parse_args()
assert a.test_filter is None or (a.test_filter and a.test_filter.strip()==a.test_filter), 'Nonempty exact test-name substring required'
assert not a.output.exists(), 'Refusing overwrite'
a.output.mkdir(parents=True)
for name in ['tests.log','test-exit-code.txt','profile.json','setup-profile.json','network-config.json','resources.jsonl','calls.jsonl','proof-manifest.json','source-before.json']:
    shutil.copy2(a.run/name,a.output/name)
configuration=json.loads((a.run/'run-configuration.json').read_text())
configuration.update({'view':a.view,'companion_source':str(a.run.resolve()),'archive_code_instance':False})
configuration['transaction_test_filter']=a.test_filter
configuration['transaction_selection_scope']='Selected recorded test_name substring only; full source Cargo log and SDK report remain unchanged, omitted tests have no transaction coverage claim' if a.test_filter is not None else 'All raw transaction rows'
if a.view=='live': configuration['archive_roles']=None
(a.output/'run-configuration.json').write_text(json.dumps(configuration,indent=2)+'\n')
raw=(a.run/'transactions.jsonl').read_bytes()
source_sha256=hashlib.sha256(raw).hexdigest()
rows=[]
source_rows=0
skipped_rows=0
selected_tests=set()
for line in raw.splitlines():
    parent=json.loads(line)
    source_rows+=1
    if a.test_filter is not None and a.test_filter not in parent.get('test_name',''):
        skipped_rows+=1
        continue
    selected_tests.add(parent.get('test_name'))
    child=parent.get('storage_'+a.view+'_transaction')
    if child is None:
        raise ValueError('Missing complete companion; never synthesize a passing row')
    child=copy.deepcopy(child)
    repair=None
    if a.view=='data' and child.get('archive',{}).get('enabled') is True:
        # Older companion exporters retained the complete live sibling but
        # omitted three baseline-comparison metadata fields from data.archive.
        # Recover ONLY those fields from that authentic raw sibling; never
        # derive resources, fees, authorization or contract state from data.
        live=parent.get('storage_live_transaction')
        if not isinstance(live,dict) or not isinstance(live.get('resources'),dict):
            raise ValueError('Complete authentic live companion required for archive baseline metadata')
        archive=child['archive']
        assert archive['baseline_return_value_xdr']==live['return_value_xdr'], 'Baseline return differs from raw live sibling'
        assert type(archive['return_matches_baseline']) is bool
        assert archive['return_matches_baseline']==(child['return_value_xdr']==live['return_value_xdr'])
        assert type(archive['contract_changes_match_baseline']) is bool
        assert archive['contract_changes_match_baseline']==(child['post_state_data_sha256']==live['post_state_data_sha256'])
        values={
            'baseline_post_state_data_sha256':live['post_state_data_sha256'],
            'baseline_snapshot_sha256':live['snapshot_sha256'],
            'baseline_fees':live['fees'],
        }
        added=[]
        for key,value in values.items():
            if key in archive:
                assert archive[key]==value, 'Archive baseline metadata conflicts with raw live sibling: '+key
            else:
                archive[key]=copy.deepcopy(value)
                added.append(key)
        repair={
            'schema':'cl-companion-authentic-baseline-metadata-v1',
            'source':'parent.storage_live_transaction',
            'raw_parent_row_sha256':hashlib.sha256(line).hexdigest(),
            'raw_live_canonical_sha256':hashlib.sha256(json.dumps(live,sort_keys=True,separators=(',',':')).encode()).hexdigest(),
            'raw_child_canonical_sha256':hashlib.sha256(json.dumps(parent['storage_data_transaction'],sort_keys=True,separators=(',',':')).encode()).hexdigest(),
            'added_fields':added,
            'execution_fields_changed':False,
        }
    # Capture metadata is common to the three independent host executions.
    # All execution fields come from the raw child. In particular a failed cold
    # parent's error must never leak into a successful live/data child.
    metadata={k:v for k,v in parent.items() if k in {
        'schema','flow','test_name','scope','engine','account_authorization_mode',
        'release_approval','expected_rejection','proof_case', 'profile','run_mode',
        'runtime_protocol','target_protocol','captured_ledger','config_sha256',
        'sdk_limits_enforced','exact_captured_cost_model',
        'diagnostic_shadow_cpu_limit','diagnostic_shadow_memory_limit'}}
    metadata.update(child)
    metadata['companion_provenance']={'source_sha256':source_sha256,'view':a.view,'separate_complete_host_invocation':True}
    if repair is not None:
        metadata['companion_provenance']['archive_baseline_metadata']=repair
    rows.append(metadata)
if a.test_filter is not None and not rows:
    raise ValueError('Test filter selected no transaction rows; missing coverage remains failure')
(a.output/'transactions.jsonl').write_text(''.join(json.dumps(r,separators=(',',':'))+'\n' for r in rows))
command=['bash','scripts/check_budget.sh','--local-report',str(a.output/'resources.jsonl'),'--profile',str(a.output/'profile.json'),'--transaction-evidence',str(a.output/'transactions.jsonl'),'--proof-manifest',str(a.output/'proof-manifest.json'),'--gate','checkpoint','--summary-json',str(a.output/'gates.json')]
with (a.output/'budget.log').open('w') as log:
    result=subprocess.run(command,stdout=log,stderr=subprocess.STDOUT)
(a.output/'budget-exit-code.txt').write_text(str(result.returncode)+'\n')
(a.output/'extraction.json').write_text(json.dumps({'source':str(a.run.resolve()),'source_sha256':source_sha256,'source_rows':source_rows,'rows':len(rows),'skipped_rows':skipped_rows,'test_filter':a.test_filter,'selected_test_names':sorted(selected_tests),'source_test_log_unchanged':True,'nonselected_transaction_coverage_claimed':False,'command':command,'gate_exit':result.returncode,'gate_failure_preserved':result.returncode!=0},indent=2)+'\n')
print(json.dumps({'rows':len(rows),'view':a.view,'checkpoint_exit':result.returncode}))
# Evidence extraction success is separate from qualification; consumers must
# inspect gates.json. CI explicitly requires every gate and propagates failure.
