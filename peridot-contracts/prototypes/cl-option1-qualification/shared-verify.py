#!/usr/bin/env python3
"""Independent report/batch/link checks, not Core application or release approval."""
import argparse,collections,hashlib,importlib.util,json,sys
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--run',type=Path,required=True);p.add_argument('--build',type=Path,required=True);a=p.parse_args()
a.run=a.run.resolve();a.build=a.build.resolve();root=Path.cwd()
sys.path.insert(0,str(root/'scripts'))
spec=importlib.util.spec_from_file_location('restore_checker',root/'prototypes/cl-restoration-preparation/restore-checker.py');mod=importlib.util.module_from_spec(spec);spec.loader.exec_module(mod)
read=lambda p:json.loads(p.read_text());digest=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
config=read(a.run/'network-config.json');producer=digest(a.build/'harness/src/restoration.rs')
all_batches=[json.loads(x) for x in (a.run/'preparation.jsonl').read_text().splitlines()]
manifest=read(a.build/'build-manifest.json')
assert all(digest(a.build/k)==v for k,v in manifest['harness_generated_sources'].items())
report={'schema':'cl-shared-eight-independent-report-check-v1','checks':{},'source_sha256':digest(Path(__file__)),'core_application_verified':False,'real_signature_costs_verified':False,'full_envelope_sequence_application_verified':False,'release_approval':False}
for profile,short in [('internal-policy','internal'),('captured-network-diagnostic','captured')]:
 r=read(a.run/'state'/f'{profile}.json');pr=read(a.run/f'{short}-profile.json')
 assert r['source_sha256']==digest(a.build/'harness/src/shared-recovery.rs')
 assert r['original_joint_snapshot_sha256']==digest(a.run/'state/joint-original.json')
 assert r['cold_joint_snapshot_sha256']==digest(a.run/'state/joint-cold.json')
 batches=[x for x in all_batches if x['profile']==profile]
 assert batches==r['preparation']
 errors=[{'batch':i,'errors':mod.validate(x,pr,config,producer_sha256=producer)} for i,x in enumerate(batches)]
 assert all(not x['errors'] for x in errors),errors
 keys=[k['key_xdr'] for b in batches for k in b['keys']]
 assert len(keys)==len(set(keys))==r['required_union_keys']
 previous=r['cold_joint_snapshot_sha256']
 for batch in batches:
  assert batch['snapshot_before_sha256']==previous
  previous=batch['snapshot_after_sha256']
 assert previous==r['prepared_snapshot_sha256']
 assert r['preparation_fee']==sum(b['fees']['total'] for b in batches)
 accepted=[]
 for index,item in enumerate(r['chain']):
  assert item['input_snapshot_sha256']==previous
  host,sdk=item['host'],item['sdk']
  if not item['accepted']:
   assert index==len(r['chain'])-1 and (not host['limits_pass'] or not sdk['ok'])
   break
  assert host['success'] and host['limits_pass'] and sdk['ok'] and sdk['return_matches_final']
  assert host['restored_rw_entry_indices']==[] and sdk['disk_read_entries']==0
  assert item['sdk_observed_dependency_union_covered'] and item['sdk_preexisting_keys_not_prepared']==[]
  assert len(host['expected_external_authorizers'])==1 and host['expected_external_authorizers'][0]==item['payer']
  assert item['payer']!=item['borrower']
  post=sdk['postconditions'];assert post['contract_state_matches_after_exact_mock_nonce_substitution'] and post['contract_events_match_host']
  assert post['actual_reward_backing_matches_received_minus_paid']
  b=post['beneficiaries'];assert b['supported'] and b['borrower']==item['borrower'] and b['entitlement_not_reduced'] and b['exact_unchanged'] and b['no_new_borrower_waiver_index']
  assert b['liquidator']['exact_unchanged']
  c=sdk['reward_conservation'];n={k:int(c[k]) for k in ['received','paid','pending','quarantine','fractions','rounding_scaled']}
  assert n['received']*10**18==(n['paid']+n['pending']+n['quarantine'])*10**18+n['fractions']+n['rounding_scaled']
  states=item['postconditions']['positions'];assert len(states)==8
  assert all((int(v['debt'])==0 and not v['pending']) if j<=index else (int(v['debt'])>0 and v['pending']) for j,v in enumerate(states))
  previous=item['output_snapshot_sha256']
  assert previous==digest(a.run/'state'/f'{profile}-after-{index+1}.json')
  accepted.append(item)
 assert previous==r['final_snapshot_sha256']
 assert len(accepted)==r['completed_borrowers']
 assert r['accepted_financial_fee']==sum(i['host']['fees']['total'] for i in accepted)
 if r['complete']:
  assert len(accepted)==8 and len({i['payer'] for i in accepted})==8 and len({i['borrower'] for i in accepted})==8
  assert r['complete_journey_fee']==r['preparation_fee']+r['accepted_financial_fee']
  assert r['complete_journey_transaction_count']==len(batches)+8
 else:assert r['complete_journey_fee'] is None and r['complete_journey_transaction_count'] is None
 report['checks'][profile]={'native_batch_xdr_fees_rent_checks':errors,'native_batches_pass':True,'chain_links_and_saved_snapshot_hashes_pass':True,'exact_sdk_host_state_events_and_booked_rewards_pass_for_accepted_roots':True,'accepted_roots':len(accepted),'full_chain_complete':r['complete'],'union_keys':len(keys),'preparation_batches':len(batches),'preparation_fee':r['preparation_fee'],'complete_journey_fee':r['complete_journey_fee'],'sdk_extra_prepared_keys':len(r['sdk_observed_additional_key_xdrs']),'source_data_scope':'hash-linked producer snapshots plus exact SDK/host comparison; not independent Core transaction application','sdk_maxima':{k:max(i['sdk'][k] for i in r['chain']) for k in ['instructions','memory_bytes','read_entries','write_entries']}}
assert report['checks']['captured-network-diagnostic']['full_chain_complete']
assert not report['checks']['internal-policy']['full_chain_complete']
(a.run/'independent-shared-checks.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report,indent=2))
