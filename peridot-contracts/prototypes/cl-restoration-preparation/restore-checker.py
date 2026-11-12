#!/usr/bin/env python3
"""Validate unsigned restore preparation; never approve Core application/release."""
import argparse
from collections import Counter
import hashlib
import json
import re
from pathlib import Path
import subprocess
import sys
ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'scripts'))
from cl_resource_profiles import create
from cl_transaction_evidence import fee_configuration, compute_fees, positive_int, validate_transaction
from check_local_budget import validate_row


def decode(row):
    result=subprocess.run(['node',str(Path(__file__).with_name('decode-restore.cjs'))],input=json.dumps(row),capture_output=True,text=True,timeout=30)
    if result.returncode:raise ValueError('restore XDR validation: '+result.stderr.strip())
    return json.loads(result.stdout)


def validate(row,profile,config,decoder=decode,producer_sha256=None):
    errors=[]
    try:
        required={'schema':'cl-restore-preparation-v1','profile':profile['name'],
                  'predicted_only':True,'sdk_invocation_applicable':False,'core_application_verified':False}
        for name,value in required.items():
            if row.get(name)!=value or type(row.get(name)) is not type(value):errors.append('evidence mismatch: '+name)
        for name in ['release_approval','signature_verification_measured','live_state_validation']:
            if row.get(name,False) is not False:errors.append('unsupported approval claim: '+name)
        if profile['target_protocol']!=29 or profile['mode']!='acceptance':errors.append('only validated protocol29 acceptance profiles supported')
        if row['limits']!=profile['limits'] or any(type(v) is not int for v in row['limits'].values()):errors.append('limits differ from validated selected profile')
        if row['config_sha256']!=profile['network_sha256']:errors.append('network capture hash mismatch')
        source=row.get('source_sha256')
        if not isinstance(source,str) or not re.fullmatch(r'[0-9a-f]{64}',source):errors.append('missing or invalid producer source hash')
        elif producer_sha256 is not None and source!=producer_sha256:errors.append('producer source hash mismatch')
        ledger=positive_int(row['current_ledger'],'current_ledger')
        if row.get('memory_bytes') is not None:errors.append('native restore memory is not measured')
        d=decoder(row);n=len(d['keys']);r=row['resources'];limits=profile['limits']
        if not 1<=n<=min(50,limits['write_entries'],limits['disk_read_entries'],limits['ledger_entries']):errors.append('restore batch key count exceeds bounded/profile limits')
        total_bytes=sum(e['entry_bytes'] for e in d['entries'])
        expected={'instructions':0,'disk_read_entries':n,'write_entries':n,'disk_read_bytes':total_bytes,'write_bytes':total_bytes,'transaction_size_bytes':d['envelope_size_bytes']}
        for name,value in expected.items():
            positive_int(r[name],name,True)
            if r[name]!=value:errors.append('resource count/serialized size mismatch: '+name)
            if r[name]>limits[name]:errors.append('resource exceeds profile: '+name)
        if r['write_bytes']>132096:errors.append('restore bounded batch write bytes exceed132096')
        if r.get('contract_events_size_bytes',0)!=0:errors.append('restore has no contract return/events')
        if any(name in r for name in ['memory_bytes','sdk_entry_total','sdk_instructions']):errors.append('restore cannot fabricate SDK/native-memory measurements')
        for name in ['instructions','disk_read_bytes','write_bytes']:
            if d[name]!=r[name]:errors.append('envelope resource mismatch: '+name)
        ttl_settings=config['settings']['configSettingStateArchival']['values']
        new_ttl=ledger+positive_int(ttl_settings['minPersistentTtl'],'minimum persistent TTL')-1
        if new_ttl>2**32-1:errors.append('restored TTL overflow')
        if len(row['rent_changes'])!=n:errors.append('rent entries do not cover exact restore footprint')
        for item,entry,rent in zip(row['keys'],d['entries'],row['rent_changes']):
            old=positive_int(item['old_ttl'],'old_ttl',True)
            if old>=ledger:errors.append('batch contains already-live entry')
            if item['new_ttl']!=new_ttl:errors.append('incorrect predicted restoration TTL')
            kind=entry['kind'];cap='max_contract_code_entry_size_bytes' if kind=='contractCode' else 'max_contract_data_entry_size_bytes'
            if entry['entry_bytes']>limits[cap]:errors.append('serialized entry exceeds profile: '+cap)
            if kind=='contractData' and entry['key_bytes']>limits['max_contract_data_key_size_bytes']:errors.append('serialized data key exceeds profile')
            expected_rent={'is_persistent':True,'is_code_entry':kind=='contractCode','old_size_bytes':0,'old_live_until_ledger':0,'new_live_until_ledger':new_ttl}
            for key,value in expected_rent.items():
                if rent.get(key)!=value or type(rent.get(key)) is not type(value):errors.append('restore rent field mismatch: '+key)
            size=positive_int(rent['new_size_bytes'],'rent_size')
            memory=0
            models=config['settings']['configSettingContractCostParamsMemoryBytes']['values']
            for cost in entry.get('code_memory_inputs',[]):
                model=models[cost['cost_type']]
                linear=min((1<<64)-1,model['linearTerm']*cost['input'])>>7
                memory=min((1<<64)-1,memory+min((1<<64)-1,model['constTerm']+linear))
            expected_size=min((1<<32)-1,entry['entry_bytes']+min((1<<32)-1,memory))
            if size!=expected_size:errors.append('rent size differs from decoded entry and captured module-memory cost model')
            if 'rent_size_bytes' in item and item['rent_size_bytes']!=size:errors.append('per-key rent size differs from rent change')
        for section in ['fees','fee_inputs','rent_fee_inputs']:
            for name,value in row[section].items():positive_int(value,section+'.'+name,True)
        fee,rent=fee_configuration(config)
        if row['fee_inputs']!=fee or row['rent_fee_inputs']!=rent:errors.append('fee model differs from captured configuration')
        inclusion=positive_int(row['fees']['inclusion'],'inclusion fee')
        actual=compute_fees({**r,'contract_events_size_bytes':0},fee,row['rent_changes'],rent,ledger,inclusion)
        if row['fees']!=actual:errors.append('complete fee components do not reconcile')
        if int(d['resource_fee'])!=actual['total']-inclusion:errors.append('envelope resource fee mismatch')
        if int(d['tx_fee'])!=actual['total']:errors.append('unsigned envelope total fee mismatch')
        if positive_int(row['helper_resource_fee'],'helper_resource_fee',True)!=int(d['helper_resource_fee']):errors.append('helper fee output mismatch')
    except (ValueError,KeyError,TypeError,IndexError,OverflowError,subprocess.SubprocessError) as error:
        errors.append(str(error))
    return errors


def canonical(value):
    return json.dumps(value,sort_keys=True,separators=(',',':'))


def validate_journey(journey,profile,known_batches,producer_sha256=None,
                     financial_validator=validate_transaction,sdk_validator=validate_row):
    errors=[];execution=[]
    def require(condition,message):
        if not condition:errors.append(message)
    def sha(value,label):
        require(isinstance(value,str) and re.fullmatch(r'[0-9a-f]{64}',value) is not None,'invalid snapshot/source hash: '+label)
    def financial(tx,label,flow):
        require(tx.get('flow')==flow and tx.get('profile')==profile['name'],label+' financial root/profile mismatch')
        require(tx.get('config_sha256')==profile['network_sha256'],label+' financial configuration mismatch')
        if tx.get('success') is not True:execution.append(label+' financial execution failed')
        require(tx.get('restored_rw_entry_indices')==[],label+' still restores unpaid/omitted keys')
        require(type(tx.get('limits_pass')) is bool,label+' financial limit result missing')
        if tx.get('limits_pass') is not True:execution.append(label+' financial limits failed')
        # The existing verifier stops before XDR/fee checks when limits_pass is
        # false. This ephemeral validation view only passes that early assertion;
        # its numeric limits still run, and the original failure remains above.
        # Never rewrite, export, or count this view as execution evidence.
        validation_view={**tx,'limits_pass':True}
        for error in financial_validator(validation_view,profile):
            limit_error=(error=='transaction evidence mismatch: limits_pass' or error.startswith('transaction resource exceeds profile:') or error=='transaction memory exceeds profile' or error=='invalid or excessive transaction footprint')
            (execution if limit_error else errors).append(label+': '+error)
    def sdk_check(sdk,tx,label,flow):
        require(sdk.get('flow')==flow and sdk.get('profile')==profile['name'],label+' SDK root/profile mismatch')
        require(sdk.get('config_sha256')==profile['network_sha256'],label+' SDK configuration mismatch')
        require(sdk.get('real_signature_proof') is False,label+' unsupported signature claim')
        actual_match=(sdk.get('ok') is True and isinstance(sdk.get('return_value_xdr'),str) and sdk['return_value_xdr']==tx.get('return_value_xdr'))
        require(type(sdk.get('return_matches_final')) is bool and sdk['return_matches_final']==actual_match,label+' SDK return-match attestation inconsistent')
        if sdk.get('ok') is True:require(actual_match,label+' SDK return differs from final host result')
        execution.extend(label+' SDK: '+error for error in sdk_validator(sdk,profile))
        if not actual_match:execution.append(label+' SDK successful return equality missing')
        if sdk.get('ok') is True:
            if type(sdk.get('disk_read_entries')) is not int or sdk['disk_read_entries']!=0:
                execution.append(label+' SDK prepared execution has nonzero/missing measured disk-read entries')
            post=sdk.get('postconditions')
            if not isinstance(post,dict):
                execution.append(label+' SDK postconditions missing')
            else:
                for key,value in {
                    'schema':'cl-restoration-postconditions-v1','flow':flow,
                    'contract_state_matches_after_exact_mock_nonce_substitution':True,
                    'contract_events_match_host':True,
                    'actual_reward_backing_matches_received_minus_paid':True,
                }.items():
                    if post.get(key)!=value or type(post.get(key)) is not type(value):
                        execution.append(label+' SDK postcondition missing/false: '+key)
                if not isinstance(post.get('source_sha256'),str) or re.fullmatch(r'[0-9a-f]{64}',post['source_sha256']) is None:
                    execution.append(label+' SDK postcondition source hash missing/invalid')
                raw_match=post.get('raw_full_contract_state_matches_host')
                substitutions=post.get('nonce_substitutions')
                if type(raw_match) is not bool or not isinstance(substitutions,list):
                    execution.append(label+' SDK explicit raw state/nonce comparison missing')
                elif (raw_match and substitutions) or (not raw_match and not substitutions):
                    execution.append(label+' SDK raw state/nonce substitution attestation inconsistent')
                if isinstance(substitutions,list):
                    actors=[]
                    for item in substitutions:
                        valid=isinstance(item,dict)
                        if valid:
                            valid=(all(isinstance(item.get(key),str) and bool(item[key]) for key in ['actor','host_key_xdr','sdk_key_xdr','host_entry_data_xdr','sdk_entry_data_xdr','host_credential_nonce'])
                                   and item.get('fresh_in_both_runtimes') is True and item.get('same_declared_external_actor') is True
                                   and item.get('actor') in tx.get('expected_external_authorizers',[])
                                   and item.get('host_key_xdr')!=item.get('sdk_key_xdr'))
                            if isinstance(item.get('actor'),str):actors.append(item['actor'])
                        if not valid:execution.append(label+' SDK nonce substitution missing exact actor/key/entry attestation')
                    if len(actors)!=len(set(actors)):execution.append(label+' SDK duplicate nonce substitution actor')
        return actual_match
    try:
        require(journey.get('schema')=='cl-restore-journey-v1','journey schema missing/mismatched')
        require(journey.get('profile')==profile['name'],'journey profile mismatch')
        require(journey.get('config_sha256')==profile['network_sha256'],'journey configuration mismatch')
        sha(journey.get('source_sha256'),'producer')
        if producer_sha256 is not None:require(journey.get('source_sha256')==producer_sha256,'journey producer source hash mismatch')
        for key in ['core_restoration_application_verified','live_state_validation','signed_authorization_measured','release_approval','network_latency_measured']:
            require(journey.get(key) is False,'unsupported/missing journey claim: '+key)
        batches=journey['preparation'];required=journey['required_archived_key_xdrs'];op=journey['operation']
        require(isinstance(op,str) and bool(op),'missing financial operation')
        require(isinstance(batches,list) and bool(batches),'missing ordered restore batches')
        require(isinstance(required,list) and bool(required) and all(isinstance(k,str) for k in required),'required archived keys missing')
        require(len(required)==len(set(required)),'duplicate required archived key')
        require(type(journey['required_archived_keys']) is int and journey['required_archived_keys']==len(required),'required archived count mismatch')
        require(type(journey['preparation_transactions']) is int and journey['preparation_transactions']==len(batches),'preparation transaction count mismatch')
        for label in ['cold_snapshot_sha256','prepared_snapshot_sha256','final_input_snapshot_sha256']:sha(journey[label],label)
        previous=journey['cold_snapshot_sha256'];keys=[]
        for i,batch in enumerate(batches):
            require(canonical(batch) in known_batches,'journey batch missing from external preparation report')
            require(type(batch['batch']) is int and batch['batch']==i,'restore batch order/index mismatch')
            require(batch['flow']==op and batch['profile']==profile['name'],'restore batch root/profile mismatch')
            require(batch['source_sha256']==journey['source_sha256'],'restore batch source mismatch')
            require(batch['snapshot_before_sha256']==previous,'restore snapshot chain broken')
            sha(batch['snapshot_after_sha256'],'batch after')
            previous=batch['snapshot_after_sha256']
            require(batch.get('financial_data_unchanged') is True,'batch financial-state preservation missing')
            keys.extend(k['key_xdr'] for k in batch['keys'])
        require(keys==required,'ordered restore batch union differs from required archived keys')
        require(len(keys)==len(set(keys)),'duplicate key across restore batches')
        require(previous==journey['prepared_snapshot_sha256'],'prepared pretrade snapshot does not end restore chain')
        require(journey.get('financial_data_unchanged_by_preparation') is True,'preparation financial-state preservation missing')
        prep_total=sum(b['fees']['total'] for b in batches);rent_total=sum(b['fees']['rent'] for b in batches)
        require(journey['preparation_fee_total']==prep_total,'preparation fee sum mismatch')
        require(journey['preparation_rent_total']==rent_total,'preparation rent sum mismatch')
        final=journey['final_transaction'];sdk=journey['sdk_invocation'];trade=journey['intervening_pool_trade']
        require(final.get('snapshot_sha256')==journey['final_input_snapshot_sha256'],'final transaction input snapshot mismatch')
        require(final.get('expected_external_authorizers')==journey['borrower_authorization_for_financial_root'],'financial root authorizers differ from journey')
        if trade is None:require(journey['final_input_snapshot_sha256']==journey['prepared_snapshot_sha256'],'unexplained state change between preparation and financial root')
        else:
            quote=trade['quote'];tx=trade['transaction']
            require(quote.get('snapshot_sha256')==previous and tx.get('snapshot_sha256')==previous,'intervening quote/trade input is not prepared snapshot')
            require(trade.get('snapshot_after_trade_sha256')==journey['final_input_snapshot_sha256'],'trade output is not final financial input')
            require(tx.get('post_state_full_data_sha256')==final.get('pre_state_full_data_sha256'),'trade data output does not match final prestate')
            require(trade.get('operator_fee_includes_trade') is False,'third-party trade fee must remain separate')
            require(trade.get('third_party_fee')==tx.get('fees',{}).get('total'),'third-party trade fee mismatch')
            financial(quote,'intervening quote','estimate_swap');financial(tx,'intervening trade','swap')
            sdk_check(trade['sdk_invocation'],tx,'intervening trade','swap')
        financial(final,'final',op);match=sdk_check(sdk,final,'final',op)
        final_fee=final.get('fees',{}).get('total')
        require(type(final_fee) is int and final_fee>=0,'final fee is unavailable')
        require(journey['final_fee']==final_fee,'final fee mismatch')
        require(journey['journey_fee_total']==(prep_total+final_fee if type(final_fee) is int else None),'journey fee total mismatch')
        for key in ['transaction_count','serial_confirmation_rounds_if_waiting_each']:
            require(type(journey[key]) is int and journey[key]==len(batches)+1,'operator transaction/confirmation count mismatch: '+key)
        fit=final.get('success') is True and final.get('limits_pass') is True and sdk.get('ok') is True and match and final.get('restored_rw_entry_indices')==[]
        require(type(journey.get('limits_fit')) is bool and journey['limits_fit']==fit,'journey limits-fit attestation mismatch')
    except (ValueError,KeyError,TypeError,IndexError,OverflowError,subprocess.SubprocessError) as error:
        errors.append('incomplete/invalid journey: '+str(error))
    return {'integrity_errors':errors,'execution_errors':execution,'operation':journey.get('operation')}


def check(rows,profile,config,producer_sha256=None,journeys=None):
    results=[{'row':i,'errors':validate(row,profile,config,producer_sha256=producer_sha256)} for i,row in enumerate(rows)]
    errors=[] if rows else ['missing required restore-preparation records']
    batches_ok=bool(rows) and not errors and all(not x['errors'] for x in results)
    journey_errors=[];journey_results=[]
    if not journeys:journey_errors.append('missing required complete restore journeys')
    else:
        known=Counter(canonical(row) for row in rows)
        used=Counter(canonical(batch) for j in journeys for batch in j.get('preparation',[]))
        if used!=known:journey_errors.append('journeys do not cover external preparation records exactly once')
        journey_results=[validate_journey(j,profile,known,producer_sha256) for j in journeys]
    integrity=bool(journeys) and not journey_errors and all(not r['integrity_errors'] for r in journey_results)
    executed=integrity and all(not r['execution_errors'] for r in journey_results)
    return {'schema':'cl-restore-preparation-report-v1','profile':profile['name'],'rows':results,'errors':errors,
            'journeys':journey_results,'journey_errors':journey_errors,
            'gates':{'restore_batches':batches_ok,'journey_integrity':integrity,'restore_preparation':batches_ok and integrity,'post_preparation_execution':batches_ok and executed,'core_transaction_application':False,'signed_authentication_costs':False,'live_state_validation':False,'release':False},
            'unsupported_gates':['Core transaction application and actual restored TTL metadata','real account signatures, source sequence and fee charging','live-state execution'],
            'snapshot_chain_scope':'Hash-linked producer attestations and decoded touched-entry data; not independent full snapshot/Core application verification',
            'financial_structure_validation_scope':'For failed limits, an ephemeral limits_pass=true validation view reaches the existing XDR/fee verifier; all numeric ceilings still run and the original failed execution remains failed. Raw records are never rewritten.',
            'prepared_execution_restoration_scope':'Successful SDK roots require zero measured disk_read_entries; official host roots require empty restored_rw_entry_indices. These are measured counter checks, not a claim about unobserved native/Core restoration behavior.',
            'declared_restore_instructions':0,'declared_instruction_scope':'native operation resource declaration; not measured Core CPU',
            'native_memory_measurement':None,'sdk_invocation_applicable':False,'code_rent_size_validation':'independently recomputed from decoded code extension and captured memory cost parameters using host29 integer saturation/scale',
            'release_approval':False}


def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--input',type=Path,required=True);p.add_argument('--profile',type=Path,required=True);p.add_argument('--network-config',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--producer-file',type=Path,default=Path(__file__).with_name('restoration.rs'));p.add_argument('--journeys-input',type=Path);p.add_argument('--gate',choices=['preparation','qualification'],default='qualification');a=p.parse_args()
    try:
        profile=json.loads(a.profile.read_text());generated=create(a.network_config,profile['name'])
        for key in ['schema','name','mode','network_sha256','captured_ledger','target_protocol','limits','release_approval']:
            if profile.get(key)!=generated[key]:raise ValueError('invalid supplied profile: '+key)
        raw=a.input.read_bytes();config=json.loads(a.network_config.read_text())
        text=raw.decode();rows=json.loads(text) if text.lstrip().startswith('[') else [json.loads(line) for line in text.splitlines() if line.strip()]
        if not isinstance(rows,list) or any(not isinstance(row,dict) for row in rows):raise ValueError('expected JSON array or JSONL records')
        producer_hash=hashlib.sha256(a.producer_file.read_bytes()).hexdigest()
        journeys=None
        if a.journeys_input:
            journey_raw=a.journeys_input.read_bytes();journeys=[json.loads(line) for line in journey_raw.splitlines() if line.strip()]
            if any(not isinstance(j,dict) for j in journeys):raise ValueError('expected journey JSONL records')
        profile['network_config_path']=str(a.network_config.resolve())
        report=check(rows,profile,config,producer_hash,journeys);
        if a.journeys_input:report['journeys_input_sha256']=hashlib.sha256(journey_raw).hexdigest()
        report['producer_sha256']=producer_hash;report['input_sha256']=hashlib.sha256(raw).hexdigest();report['network_sha256']=profile['network_sha256'];report['selected_gate']=a.gate
    except (ValueError,KeyError,TypeError,OSError) as error:
        report={'schema':'cl-restore-preparation-report-v1','errors':[str(error)],'gates':{'restore_preparation':False,'core_transaction_application':False,'signed_authentication_costs':False,'live_state_validation':False,'release':False},'release_approval':False,'selected_gate':a.gate}
    a.output.parent.mkdir(parents=True,exist_ok=True);a.output.write_text(json.dumps(report,indent=2)+'\n')
    return 0 if a.gate=='preparation' and report['gates']['restore_preparation'] else 1
if __name__=='__main__':raise SystemExit(main())
