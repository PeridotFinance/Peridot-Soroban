#!/usr/bin/env python3
"""Reject false feasibility conclusions and profile/candidate relabelling."""
import copy,hashlib,importlib.util,json,tempfile,unittest
from pathlib import Path
HERE=Path(__file__).resolve().parent
def module(name):
    spec=importlib.util.spec_from_file_location(name,HERE/(name+'.py'))
    m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
D=module('decision-joined');S=module('summarize-joined')
def fixture():
    patches=[D.ROOT/'prototypes/cl-atomic-qualification/optimization.patch',D.ROOT/'prototypes/cl-storage-qualification/storage.patch',D.ROOT/'prototypes/cl-storage-qualification/repayment.patch']+[HERE/n for n in ['pending-recovery.patch','pending-unhealthy.patch','closing-recovery.patch']]
    manifest={'optimization_sha256':hashlib.sha256(b''.join(p.read_bytes() for p in patches)).hexdigest(),'margin_build_feature':'funded-liquidation-validation','release_approval':False,'wasm':{'v3':{'sha256':'synthetic-test-identity'}}}
    captured={'driver':{'profile':'captured-network-diagnostic','test_counts':[{'passed':7,'failed':0}]},'gates':{'profile':'captured-network-diagnostic'},'build_manifest':manifest,
        'roots':{name:{'positive_sdk_rows_accepted':True,'host_completed':1,'observed_maxima':{'instructions':227685319,'memory_bytes':22793790,'sdk_entries':176,'unique_keys':126,'write_entries':52}} for name in D.REQUIRED_ROOTS}}
    internal={'driver':{'profile':'internal-policy'},'gates':{'profile':'internal-policy'},'build_manifest':copy.deepcopy(manifest)}
    return {'proposed_workload_boundaries':dict(D.BOUNDARIES),'profiles':{'captured-network-diagnostic':captured,'internal-policy':internal}}
class ReportingTests(unittest.TestCase):
    def test_observed_fit_is_analysis_only(self):
        measured,peak,manifest=D.validate(fixture())
        self.assertEqual(peak['instructions'],227685319)
        self.assertFalse(manifest['release_approval'])
    def test_captured_success_above_workload_cannot_recommend_fit(self):
        r=fixture();r['profiles']['captured-network-diagnostic']['roots']['swap-open']['observed_maxima']['instructions']=240000001
        with self.assertRaisesRegex(AssertionError,'exceeds proposal'):D.validate(r)
    def test_missing_root_cannot_recommend_fit(self):
        r=fixture();del r['profiles']['captured-network-diagnostic']['roots']['swap-close']
        with self.assertRaisesRegex(AssertionError,'Required measured root'):D.validate(r)
    def test_cross_candidate_comparison_rejects(self):
        r=fixture();r['profiles']['internal-policy']['build_manifest']['wasm']['v3']['sha256']='different-test-identity'
        with self.assertRaisesRegex(AssertionError,'identical artifacts'):D.validate(r)
    def test_swapped_recorded_profile_rejects_before_analysis(self):
        with tempfile.TemporaryDirectory() as tmp:
            run=Path(tmp)/'run';run.mkdir()
            run.with_name('run.driver.json').write_text(json.dumps({'seven_cases_completed':True,'profile':'internal-policy'}))
            (run/'profile.json').write_text(json.dumps({'name':'internal-policy'}))
            with self.assertRaisesRegex(AssertionError,'Recorded profile'):S.summarize(run,'captured-network-diagnostic')
    def test_proposed_boundary_cannot_be_widened_by_input(self):
        r=fixture();r['proposed_workload_boundaries']['instructions']=300000000
        with self.assertRaisesRegex(AssertionError,'Unchanged proposed'):D.validate(r)
if __name__=='__main__':unittest.main()
