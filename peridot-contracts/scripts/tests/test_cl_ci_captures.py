"""Pinned archival identity tests; no contract or release approval evidence."""
import copy
from pathlib import Path
import sys
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from cl_ci_captures import CAPTURES, validate_identity

class CaptureTests(unittest.TestCase):
    def index(self, name):
        return dict(CAPTURES[name], schema='cl-linux-ci-archive-v1', release_approval=False,
                    profiles={'internal-policy': {}, 'captured-network-diagnostic': {}})
    def test_both_captures_remain_independently_pinned(self):
        for name in CAPTURES:
            self.assertEqual(validate_identity(self.index(name)), CAPTURES[name])
    def test_mixed_identity_is_rejected(self):
        for key in CAPTURES['ac63255']:
            value=self.index('ac63255');value[key]=CAPTURES['da4e1a5'][key]
            with self.subTest(key=key), self.assertRaises(ValueError):validate_identity(value)
    def test_missing_profile_and_release_claim_rejected(self):
        for key,value in [('profiles',{'internal-policy':{}}),('release_approval',True),('schema','other')]:
            index=self.index('ac63255');index[key]=value
            with self.subTest(key=key), self.assertRaises(ValueError):validate_identity(index)
    def test_unknown_capture_rejected(self):
        value=self.index('ac63255');value['commit']='0'*40
        with self.assertRaises(ValueError):validate_identity(value)

if __name__=='__main__':unittest.main()
