import copy
import importlib.util
from pathlib import Path
import unittest

HERE=Path(__file__).resolve().parent
SPEC=importlib.util.spec_from_file_location('prefix_summary',HERE/'prefix_summary.py')
PREFIX=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(PREFIX)

class PrefixTests(unittest.TestCase):
    def fixture(self):
        profile={'name':'internal-policy','network_sha256':'c'*64};source='d'*64
        batch={'batch':0,'flow':'swap_open_position_v3','profile':profile['name'],'source_sha256':source,'snapshot_before_sha256':'a'*64,'snapshot_after_sha256':'b'*64,'financial_data_unchanged':True,'keys':[{'key_xdr':'SYNTHETIC_KEY'}],'fees':{'total':100,'rent':90}}
        journey={'schema':'cl-restore-journey-v1','profile':profile['name'],'config_sha256':profile['network_sha256'],'source_sha256':source,'operation':batch['flow'],'preparation':[batch],'required_archived_key_xdrs':['SYNTHETIC_KEY'],'required_archived_keys':1,'preparation_transactions':1,'cold_snapshot_sha256':'a'*64,'prepared_snapshot_sha256':'b'*64,'financial_data_unchanged_by_preparation':True,'preparation_fee_total':100,'preparation_rent_total':90,'final_transaction':{'success':False,'error':'BudgetExceededLimit'},'sdk_invocation':{'ok':False}}
        return profile,source,batch,journey
    def test_valid_prefix_failed_cpu_does_not_approve_full_journey(self):
        profile,source,batch,j=self.fixture();before=copy.deepcopy(j)
        # Synthetic metadata fixture: the real XDR/resource validator is tested
        # separately and is never bypassed by the executable prefix CLI.
        result=PREFIX.derive_profile([batch],[j],profile,{},source,validator=lambda *a,**k:[])
        self.assertTrue(result['gates']['prefix_integrity']);self.assertTrue(result['gates']['restoration_prefix_feasible'])
        full=PREFIX.CHECKER.validate_journey(j,profile,{PREFIX.CHECKER.canonical(batch)},source,financial_validator=lambda *a:[],sdk_validator=lambda *a:[])
        self.assertTrue(full['integrity_errors']);self.assertEqual(j,before);self.assertFalse(result['release_approval'])
    def test_prefix_key_order_snapshot_fee_or_preservation_failure_rejects(self):
        for key,value in [('required_archived_key_xdrs',[]),('prepared_snapshot_sha256','e'*64),('preparation_fee_total',101),('financial_data_unchanged_by_preparation',False)]:
            profile,source,batch,j=self.fixture();j[key]=value
            result=PREFIX.derive_profile([batch],[j],profile,{},source,validator=lambda *a,**k:[])
            self.assertFalse(result['gates']['prefix_integrity'],key)
    def test_batch_ttl_resource_error_fails_feasibility(self):
        profile,source,batch,j=self.fixture()
        result=PREFIX.derive_profile([batch],[j],profile,{},source,validator=lambda *a,**k:['incorrect predicted restoration TTL'])
        self.assertTrue(result['gates']['prefix_integrity']);self.assertFalse(result['gates']['restoration_prefix_feasible'])
    def test_duplicate_or_missing_external_batch_rejects(self):
        for rows in [[],[1,2]]:
            profile,source,batch,j=self.fixture();data=[batch]*len(rows)
            result=PREFIX.derive_profile(data,[j],profile,{},source,validator=lambda *a,**k:[])
            self.assertFalse(result['gates']['prefix_integrity'])

if __name__=='__main__':unittest.main()
