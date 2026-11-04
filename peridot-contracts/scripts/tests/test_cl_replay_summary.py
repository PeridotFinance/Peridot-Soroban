import copy
import sys
import unittest
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from cl_replay_summary import compare, compare_invocations

class ReplaySummaryTests(unittest.TestCase):
    def fixture(self):
        return dict(schema="cl-verification-gates-v1", profile="internal-policy",config_sha256="abc",gates=dict(invocation=False,lifecycle=False,transaction=False,checkpoint=False),selected_gate="checkpoint",release_approval=False,missing_required_flows=[],lifecycle_errors=["failure /old/test.log"],transaction_errors=["CPU exceeds"])
    def test_invocation_errors_are_bound(self):
        x="PROFILE internal-policy\nswap: FAIL cpu=100000001\n  CPU exceeded\nINTERNAL-POLICY: FAIL\n"
        self.assertTrue(compare_invocations(x,x))
        with self.assertRaises(ValueError):compare_invocations(x,x.replace("CPU exceeded","different reason"))
    def test_expected_failure_is_verified(self):
        x=self.fixture();self.assertTrue(compare(x,x,1)["verified"])
    def test_changed_gate_is_not_hidden_by_exit_one(self):
        x=self.fixture();y=copy.deepcopy(x);y["gates"]["invocation"]=True
        with self.assertRaises(ValueError):compare(x,y,1)
    def test_changed_error_is_rejected(self):
        x=self.fixture();y=copy.deepcopy(x);y["transaction_errors"]=["different"]
        with self.assertRaises(ValueError):compare(x,y,1)
    def test_missing_summary_field_is_rejected(self):
        x=self.fixture();y=copy.deepcopy(x);del y["lifecycle_errors"]
        with self.assertRaises(ValueError):compare(x,y,1)
    def test_declared_log_relocation_only(self):
        x=self.fixture();y=copy.deepcopy(x);y["lifecycle_errors"]=["failure /new/test.log"]
        self.assertTrue(compare(x,y,1,[("/old/test.log","<log>"),("/new/test.log","<log>")])["verified"])
        with self.assertRaises(ValueError):compare(x,y,1)
    def test_approval_and_wrong_status_rejected(self):
        x=self.fixture()
        with self.assertRaises(ValueError):compare(x,x,0)
        y=copy.deepcopy(x);y["release_approval"]=True
        with self.assertRaises(ValueError):compare(x,y,1)
if __name__=="__main__":unittest.main()
