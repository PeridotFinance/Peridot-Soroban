"""Offline synthetic XDR regressions; no host execution or signing claim."""
import copy,hashlib,importlib.util,json,subprocess,tempfile,unittest
from pathlib import Path
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[1]
spec=importlib.util.spec_from_file_location('restore_checker',HERE/'restore-checker.py');checker=importlib.util.module_from_spec(spec);spec.loader.exec_module(checker)
CONFIG=ROOT.parent/'docs/deployment-handoffs/evidence/cl-resource-decision-refresh-20261102/network-config.json'

JS=r'''
const {xdr,Address}=require('./bots/margin-liquidator/node_modules/@stellar/stellar-sdk');
const crypto=require('node:crypto');const fs=require('node:fs'),arg=JSON.parse(fs.readFileSync(0,'utf8'));
let row=arg.row;
if(!row){
const d=new xdr.ContractDataEntry({ext:new xdr.ExtensionPoint(0),contract:Address.fromString('CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM').toScAddress(),key:xdr.ScVal.scvSymbol('state'),durability:xdr.ContractDataDurability.persistent(),val:xdr.ScVal.scvU32(7)});
const e=new xdr.LedgerEntry({lastModifiedLedgerSeq:1,data:xdr.LedgerEntryData.contractData(d),ext:new xdr.LedgerEntryExt(0)});
let k=xdr.LedgerKey.contractData(new xdr.LedgerKeyContractData({contract:d.contract(),key:d.key(),durability:d.durability()}));
if(arg.code){
const code=Buffer.from('0061736d01000000','hex'),h=crypto.createHash('sha256').update(code).digest();
let ext=new xdr.ContractCodeEntryExt(0);
if(arg.code===1){const fields={ext:new xdr.ExtensionPoint(0)};for(const [name] of xdr.ContractCodeCostInputs._fields)if(name!=='ext')fields[name]=3;ext=new xdr.ContractCodeEntryExt(1,new xdr.ContractCodeEntryV1({ext:new xdr.ExtensionPoint(0),costInputs:new xdr.ContractCodeCostInputs(fields)}));}
e.data(xdr.LedgerEntryData.contractCode(new xdr.ContractCodeEntry({ext,hash:h,code})));k=xdr.LedgerKey.contractCode(new xdr.LedgerKeyContractCode({hash:h}));
}
const r=new xdr.SorobanResources({footprint:new xdr.LedgerFootprint({readOnly:[],readWrite:[k]}),instructions:0,diskReadBytes:e.toXDR().length,writeBytes:e.toXDR().length});
const data=new xdr.SorobanTransactionData({ext:new xdr.SorobanTransactionDataExt(0),resources:r,resourceFee:xdr.Int64.fromString('0')});
const tx=new xdr.Transaction({sourceAccount:xdr.MuxedAccount.keyTypeEd25519(Buffer.alloc(32)),fee:100,seqNum:xdr.SequenceNumber.fromString('1'),cond:xdr.Preconditions.precondNone(),memo:xdr.Memo.memoNone(),operations:[new xdr.Operation({sourceAccount:null,body:xdr.OperationBody.restoreFootprint(new xdr.RestoreFootprintOp({ext:new xdr.ExtensionPoint(0)}))})],ext:new xdr.TransactionExt(1,data)});
const env=xdr.TransactionEnvelope.envelopeTypeTx(new xdr.TransactionV1Envelope({tx,signatures:[]}));
row={unsigned_envelope_xdr:env.toXDR('base64'),helper_transaction_data_xdr:data.toXDR('base64'),keys:[{key_xdr:k.toXDR('base64'),entry_xdr:e.toXDR('base64'),predicted_entry_xdr:e.toXDR('base64'),old_ttl:1}],resources:{instructions:0,disk_read_entries:1,write_entries:1,disk_read_bytes:e.toXDR().length,write_bytes:e.toXDR().length,transaction_size_bytes:env.toXDR().length}};
}else{
 const env=xdr.TransactionEnvelope.fromXDR(row.unsigned_envelope_xdr,'base64'),tx=env.v1().tx();
 if(arg.mode==='fee'){tx.fee(row.fees.total);tx.ext().sorobanData().resourceFee(xdr.Int64.fromString(String(row.fees.total-row.fees.inclusion)));}
 if(arg.mode==='signature')env.v1().signatures([new xdr.DecoratedSignature({hint:Buffer.alloc(4),signature:Buffer.alloc(64)})]);
 if(arg.mode==='wrongop')tx.operations()[0].body(xdr.OperationBody.extendFootprintTtl(new xdr.ExtendFootprintTtlOp({ext:new xdr.ExtensionPoint(0),extendTo:1})));
 if(arg.mode==='mutation'){const e=xdr.LedgerEntry.fromXDR(row.keys[0].entry_xdr,'base64');e.data().contractData().val(xdr.ScVal.scvU32(8));row.keys[0].predicted_entry_xdr=e.toXDR('base64');}
 row.unsigned_envelope_xdr=env.toXDR('base64');
}
process.stdout.write(JSON.stringify(row));
'''

def node(arg):
    p=subprocess.run(['node','-e',JS],cwd=ROOT,input=json.dumps(arg),capture_output=True,text=True,check=True)
    return json.loads(p.stdout)

class RestoreCheckerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        # The canonical committed capture is read; no network or generated target file.
        if not CONFIG.is_file():raise AssertionError('committed network capture required')
        cls.config_path=CONFIG;cls.profile=checker.create(cls.config_path,'captured-network-diagnostic');cls.config=json.loads(cls.config_path.read_text())
        row=node({});row['source_sha256']=hashlib.sha256(cls.config_path.read_bytes()).hexdigest();row.update(schema='cl-restore-preparation-v1',profile=cls.profile['name'],limits=cls.profile['limits'],config_sha256=cls.profile['network_sha256'],current_ledger=cls.profile['captured_ledger'],predicted_only=True,sdk_invocation_applicable=False,core_application_verified=False,release_approval=False)
        ttl=row['current_ledger']+cls.config['settings']['configSettingStateArchival']['values']['minPersistentTtl']-1
        row['keys'][0]['new_ttl']=ttl
        row['rent_changes']=[dict(is_persistent=True,is_code_entry=False,old_size_bytes=0,new_size_bytes=row['resources']['write_bytes'],old_live_until_ledger=0,new_live_until_ledger=ttl)]
        row['fee_inputs'],row['rent_fee_inputs']=checker.fee_configuration(cls.config)
        row['fees']=checker.compute_fees({**row['resources'],'contract_events_size_bytes':0},row['fee_inputs'],row['rent_changes'],row['rent_fee_inputs'],row['current_ledger'],100)
        row['helper_resource_fee']=0 # synthetic serialized helper fixture, not real simulator output
        cls.row=node({'row':row,'mode':'fee'})
    def errors(self,row):return checker.validate(row,self.profile,self.config)
    def test_valid_unsigned_preparation_never_approves_core_or_release(self):
        self.assertEqual(self.errors(self.row),[])
        r=checker.check([self.row],self.profile,self.config)
        self.assertTrue(r['gates']['restore_batches']);self.assertFalse(r['gates']['restore_preparation']);self.assertFalse(r['gates']['core_transaction_application']);self.assertFalse(r['gates']['release']);self.assertIsNone(r['native_memory_measurement'])
    def test_unsigned_decoder_rejects_signature(self):
        self.assertIn('signed envelope forbidden',' '.join(self.errors(node({'row':self.row,'mode':'signature'}))))
    def test_unsigned_decoder_rejects_other_operation(self):
        self.assertIn('RestoreFootprintOp',' '.join(self.errors(node({'row':self.row,'mode':'wrongop'}))))
    def test_data_change_rejects(self):
        self.assertIn('changed contract entry data',' '.join(self.errors(node({'row':self.row,'mode':'mutation'}))))
    def test_missing_entry_and_duplicate_reject(self):
        for keys in [[],self.row['keys']*2]:
            row=copy.deepcopy(self.row);row['keys']=keys;self.assertTrue(self.errors(row))
    def test_stale_ttl_and_wrong_target_reject(self):
        for key,value in [('old_ttl',self.row['current_ledger']),('new_ttl',self.row['keys'][0]['new_ttl']+1)]:
            row=copy.deepcopy(self.row);row['keys'][0][key]=value;self.assertTrue(self.errors(row))
    def test_resource_counts_and_fee_components_are_bound(self):
        for section,key in [('resources','disk_read_entries'),('resources','write_bytes'),('fees','rent'),('fee_inputs','fee_per_disk_read_entry')]:
            row=copy.deepcopy(self.row);row[section][key]+=1;self.assertTrue(self.errors(row))
    def test_profile_tamper_and_memory_fabrication_reject(self):
        row=copy.deepcopy(self.row);row['limits']['write_entries']+=1;self.assertTrue(self.errors(row))
        row=copy.deepcopy(self.row);row['resources']['memory_bytes']=0;self.assertTrue(self.errors(row))
    def test_empty_required_records_fail(self):self.assertFalse(checker.check([],self.profile,self.config)['gates']['restore_preparation'])
    def test_bounded_key_count_and_bytes_predicates(self):
        # Predicate-only synthetic decoder results, separate from real-XDR tests.
        d=checker.decode(self.row);d['keys']*=51;d['entries']*=51
        errors=checker.validate(self.row,self.profile,self.config,decoder=lambda _:d)
        self.assertIn('restore batch key count exceeds bounded/profile limits',errors)
        row=copy.deepcopy(self.row);row['resources']['write_bytes']=132097
        self.assertIn('restore bounded batch write bytes exceed132096',self.errors(row))
    def test_internal_profile_is_separate_and_unchanged(self):
        profile=checker.create(self.config_path,'internal-policy');row=copy.deepcopy(self.row)
        row.update(profile=profile['name'],limits=profile['limits'])
        self.assertEqual(checker.validate(row,profile,self.config),[])
        self.assertEqual(profile['limits']['instructions'],100000000)
        self.assertEqual(profile['limits']['write_entries'],50)
    def test_cli_defaults_to_failed_qualification_and_labels_preparation(self):
        with tempfile.TemporaryDirectory() as directory:
            d=Path(directory);(d/'input.jsonl').write_text(json.dumps(self.row)+'\n');(d/'profile.json').write_text(json.dumps(self.profile))
            command=['python3',str(HERE/'restore-checker.py'),'--input',str(d/'input.jsonl'),'--profile',str(d/'profile.json'),'--network-config',str(self.config_path),'--output',str(d/'report.json'),'--producer-file',str(self.config_path)]
            self.assertEqual(subprocess.run(command,capture_output=True).returncode,1)
            self.assertTrue(json.loads((d/'report.json').read_text())['gates']['restore_batches'])
            self.assertEqual(subprocess.run(command+['--gate','preparation'],capture_output=True).returncode,1)
            self.assertFalse(json.loads((d/'report.json').read_text())['gates']['release'])
    def test_producer_source_binding_rejects_mismatch(self):
        self.assertIn('producer source hash mismatch',checker.validate(self.row,self.profile,self.config,producer_sha256='0'*64))
    def test_code_rent_memory_is_decoded_and_independently_checked(self):
        for version in [2,1]: # 2 requests V0; 1 requests V1 fixture extension.
            row=node({'code':version})
            for key,value in self.row.items():
                if key not in row:row[key]=copy.deepcopy(value)
            row['keys'][0]['new_ttl']=self.row['keys'][0]['new_ttl']
            d=checker.decode(row)['entries'][0]
            models=self.config['settings']['configSettingContractCostParamsMemoryBytes']['values']
            memory=sum(models[x['cost_type']]['constTerm']+(models[x['cost_type']]['linearTerm']*x['input']>>7) for x in d['code_memory_inputs'])
            row['rent_changes'][0].update(is_code_entry=True,new_size_bytes=d['entry_bytes']+memory)
            row['fees']=checker.compute_fees({**row['resources'],'contract_events_size_bytes':0},row['fee_inputs'],row['rent_changes'],row['rent_fee_inputs'],row['current_ledger'],100)
            row=node({'row':row,'mode':'fee'})
            self.assertEqual(self.errors(row),[])
            row['rent_changes'][0]['new_size_bytes']-=1
            self.assertIn('rent size differs from decoded entry and captured module-memory cost model',self.errors(row))
    def test_unsupported_core_approval_rejects(self):
        row=copy.deepcopy(self.row);row['core_application_verified']=True;self.assertTrue(self.errors(row))

class JourneyTests(RestoreCheckerTests):
    """Journey metadata predicates use synthetic financial-validator results.

    Real XDR validation is covered above; these fixtures are never execution proof.
    """
    def journey(self):
        batch=copy.deepcopy(self.row)
        batch.update(batch=0,flow='liquidate_funded_v3',snapshot_before_sha256='a'*64,snapshot_after_sha256='b'*64,financial_data_unchanged=True)
        final=dict(flow=batch['flow'],profile=self.profile['name'],config_sha256=self.profile['network_sha256'],snapshot_sha256='b'*64,expected_external_authorizers=['SYNTHETIC'],success=True,limits_pass=True,return_value_xdr='SYNTHETIC_RETURN',restored_rw_entry_indices=[],fees={'total':100},pre_state_full_data_sha256='d'*64)
        sdk=dict(flow=batch['flow'],profile=self.profile['name'],config_sha256=self.profile['network_sha256'],ok=True,return_value_xdr='SYNTHETIC_RETURN',return_matches_final=True,real_signature_proof=False,disk_read_entries=0)
        sdk['postconditions']=dict(schema='cl-restoration-postconditions-v1',flow=batch['flow'],source_sha256='1'*64,contract_state_matches_after_exact_mock_nonce_substitution=True,contract_events_match_host=True,actual_reward_backing_matches_received_minus_paid=True,raw_full_contract_state_matches_host=True,nonce_substitutions=[])
        return dict(schema='cl-restore-journey-v1',source_sha256=batch['source_sha256'],config_sha256=batch['config_sha256'],profile=batch['profile'],operation=batch['flow'],core_restoration_application_verified=False,live_state_validation=False,signed_authorization_measured=False,release_approval=False,network_latency_measured=False,preparation=[batch],required_archived_key_xdrs=[batch['keys'][0]['key_xdr']],required_archived_keys=1,preparation_transactions=1,cold_snapshot_sha256='a'*64,prepared_snapshot_sha256='b'*64,final_input_snapshot_sha256='b'*64,financial_data_unchanged_by_preparation=True,preparation_fee_total=batch['fees']['total'],preparation_rent_total=batch['fees']['rent'],final_transaction=final,sdk_invocation=sdk,intervening_pool_trade=None,borrower_authorization_for_financial_root=['SYNTHETIC'],final_fee=100,journey_fee_total=batch['fees']['total']+100,transaction_count=2,serial_confirmation_rounds_if_waiting_each=2,limits_fit=True)
    def audit(self,j,known=None,**kw):
        known=known if known is not None else {checker.canonical(b) for b in j['preparation']}
        return checker.validate_journey(j,self.profile,known,j['source_sha256'],financial_validator=kw.get('financial_validator',lambda tx,p:[]),sdk_validator=kw.get('sdk_validator',lambda row,p:[]))
    def test_complete_metadata_chain(self):
        r=self.audit(self.journey());self.assertEqual(r['integrity_errors'],[]);self.assertEqual(r['execution_errors'],[])
    def test_missing_external_batch_rejects(self):
        self.assertTrue(self.audit(self.journey(),known=set())['integrity_errors'])
    def test_batch_union_and_order_reject(self):
        for edit in ['missing','duplicate','order']:
            j=self.journey()
            if edit=='missing':j['required_archived_key_xdrs']=[]
            if edit=='duplicate':j['required_archived_key_xdrs']*=2
            if edit=='order':j['preparation'][0]['batch']=1
            self.assertTrue(self.audit(j)['integrity_errors'])
    def test_snapshot_chain_and_pretrade_boundary_reject(self):
        for key in ['cold_snapshot_sha256','prepared_snapshot_sha256','final_input_snapshot_sha256']:
            j=self.journey();j[key]='c'*64;self.assertTrue(self.audit(j)['integrity_errors'])
    def test_totals_and_counts_reject(self):
        for key in ['preparation_fee_total','preparation_rent_total','final_fee','journey_fee_total','preparation_transactions','transaction_count','serial_confirmation_rounds_if_waiting_each']:
            j=self.journey();j[key]+=1;self.assertTrue(self.audit(j)['integrity_errors'])
    def test_final_unpaid_restore_and_wrong_roots_reject(self):
        for key,value in [('restored_rw_entry_indices',[0]),('profile','internal-policy'),('flow','unexpected')]:
            j=self.journey();j['final_transaction'][key]=value;self.assertTrue(self.audit(j)['integrity_errors'])
    def test_sdk_return_mismatch_rejects(self):
        j=self.journey();j['sdk_invocation']['return_value_xdr']='WRONG';self.assertTrue(self.audit(j)['integrity_errors'])
    def test_failed_sdk_remains_execution_failure_separate_from_integrity(self):
        j=self.journey();j['sdk_invocation'].update(ok=False,return_value_xdr=None,return_matches_final=False);j['limits_fit']=False
        r=self.audit(j,sdk_validator=lambda row,p:['invocation/proof failed'])
        self.assertEqual(r['integrity_errors'],[]);self.assertTrue(r['execution_errors'])
    def test_pretrade_and_posttrade_snapshot_links(self):
        j=self.journey();tx=copy.deepcopy(j['final_transaction']);tx.update(flow='swap',post_state_full_data_sha256='d'*64)
        quote=copy.deepcopy(tx);quote['flow']='estimate_swap'
        sdk=copy.deepcopy(j['sdk_invocation']);sdk['flow']='swap'
        sdk['postconditions']['flow']='swap'
        j['intervening_pool_trade']=dict(quote=quote,transaction=tx,sdk_invocation=sdk,operator_fee_includes_trade=False,third_party_fee=100,snapshot_after_trade_sha256='c'*64)
        j['final_input_snapshot_sha256']='c'*64;j['final_transaction']['snapshot_sha256']='c'*64
        self.assertEqual(self.audit(j)['integrity_errors'],[])
        j['prepared_snapshot_sha256']='c'*64 # historical v4 ambiguity must reject
        self.assertTrue(self.audit(j)['integrity_errors'])
    def test_final_fee_evidence_failure_is_integrity_failure(self):
        self.assertTrue(self.audit(self.journey(),financial_validator=lambda tx,p:['complete fee components do not reconcile'])['integrity_errors'])
    def test_missing_postconditions_fail_execution_only(self):
        j=self.journey();del j['sdk_invocation']['postconditions']
        result=self.audit(j)
        self.assertEqual(result['integrity_errors'],[])
        self.assertIn('final SDK postconditions missing',result['execution_errors'])
    def test_false_postconditions_fail_execution_only(self):
        for key in ['contract_state_matches_after_exact_mock_nonce_substitution','contract_events_match_host','actual_reward_backing_matches_received_minus_paid','source_sha256','raw_full_contract_state_matches_host','nonce_substitutions']:
            j=self.journey();j['sdk_invocation']['postconditions'][key]=False if key!='source_sha256' else 'invalid'
            result=self.audit(j)
            self.assertEqual(result['integrity_errors'],[],key);self.assertTrue(result['execution_errors'],key)
    def test_sdk_disk_reads_fail_and_explicit_mock_nonce_is_not_signature_proof(self):
        j=self.journey();post=j['sdk_invocation']['postconditions']
        post.update(raw_full_contract_state_matches_host=False,nonce_substitutions=[dict(actor='SYNTHETIC',host_key_xdr='HOST_KEY',sdk_key_xdr='SDK_KEY',host_entry_data_xdr='HOST_DATA',sdk_entry_data_xdr='SDK_DATA',host_credential_nonce='1',fresh_in_both_runtimes=True,same_declared_external_actor=True)])
        result=self.audit(j);self.assertEqual(result['integrity_errors'],[]);self.assertEqual(result['execution_errors'],[])
        j['sdk_invocation']['disk_read_entries']=1
        result=self.audit(j);self.assertEqual(result['integrity_errors'],[]);self.assertTrue(result['execution_errors'])
    def test_failed_limits_do_not_hide_fee_validation_or_change_raw_evidence(self):
        j=self.journey();j['final_transaction']['limits_pass']=False;j['limits_fit']=False
        before=copy.deepcopy(j);seen=[]
        def verify(tx,profile):
            seen.append(tx['limits_pass'])
            return ['complete fee components do not reconcile'] if tx['limits_pass'] else ['transaction evidence mismatch: limits_pass']
        result=self.audit(j,financial_validator=verify)
        self.assertEqual(j,before);self.assertEqual(seen,[True])
        self.assertIn('final: complete fee components do not reconcile',result['integrity_errors'])
        self.assertIn('final financial limits failed',result['execution_errors'])

class RunnerExitTests(unittest.TestCase):
    def test_failed_or_missing_gate_never_returns_success(self):
        spec=importlib.util.spec_from_file_location('check_restore_runs',HERE/'check_runs.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
        self.assertFalse(module.completed_status({}))
        data={p:{name:{'exit_code':0,'selected_gate_passed':True} for name in ['financial','preparation']} for p in module.PROFILES}
        data['internal-policy']['preparation']['exit_code']=1
        self.assertFalse(module.completed_status(data))
        data['internal-policy']['preparation'].update(exit_code=0,selected_gate_passed=False)
        self.assertFalse(module.completed_status(data))

if __name__=='__main__':unittest.main()
