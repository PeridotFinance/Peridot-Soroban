"""Fail-closed CI metadata tests; these are not contract execution proofs."""
import copy
import hashlib
import json
from pathlib import Path
import tempfile
import subprocess
import sys
import unittest

import ci


class QualificationMetadataTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='cl-ci-metadata-')
        self.root = Path(self.temporary.name)
        self.summary = {
            'schema': 'cl-verification-gates-v1', 'profile': 'internal-policy',
            'selected_gate': 'checkpoint', 'release_approval': False,
            'gates': {g: False for g in ci.GATES}, 'missing_required_flows': ['deposit'],
            'lifecycle_errors': ['fixture failed'], 'transaction_errors': ['cold restoration exceeds limit'],
        }

    def tearDown(self):
        self.temporary.cleanup()

    def gate(self, summary=None, status=1):
        (self.root / 'gates.json').write_text(json.dumps(summary or self.summary))
        (self.root / 'budget-exit-code.txt').write_text(str(status))
        return ci.gate_result(self.root, 'internal-policy')

    def test_expected_failure_remains_unqualified_with_exact_errors(self):
        result = self.gate()
        self.assertFalse(result['qualified'])
        self.assertEqual(result['summary'], self.summary)

    def test_swallowed_failure_exit_zero_is_rejected(self):
        with self.assertRaises(AssertionError):
            self.gate(status=0)

    def test_checkpoint_true_cannot_hide_individual_gate_failure(self):
        wrong = copy.deepcopy(self.summary)
        wrong['gates']['checkpoint'] = True
        with self.assertRaises(AssertionError):
            self.gate(wrong, status=0)

    def test_green_gates_cannot_hide_required_errors(self):
        wrong = copy.deepcopy(self.summary)
        wrong['gates'] = {g: True for g in ci.GATES}
        with self.assertRaises(AssertionError):
            self.gate(wrong, status=0)

    def test_missing_summary_remains_failure(self):
        with self.assertRaises(FileNotFoundError):
            ci.gate_result(self.root, 'internal-policy')

    def test_stale_zero_test_binary_is_not_a_completed_fixture(self):
        (self.root / 'test-exit-code.txt').write_text('0\n')
        (self.root / 'tests.log').write_text('test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 42 filtered out;\n')
        with self.assertRaisesRegex(AssertionError, 'exactly four'):
            ci.completed_fixture(self.root)

    def test_package_identity_changes_with_generated_source_and_updates_lock(self):
        names = []
        for i in range(2):
            build = self.root / str(i)
            (build / 'harness/src').mkdir(parents=True)
            code = build / 'harness/src/lib.rs'
            code.write_text('// synthetic metadata fixture ' + str(i))
            for filename in ('Cargo.toml', 'Cargo.lock'):
                (build / 'harness' / filename).write_text('name = "cl-atomic-qualification-harness"\n')
            (build / 'build-manifest.json').write_text(json.dumps({
                'harness_generated_sources': {'harness/src/lib.rs': hashlib.sha256(code.read_bytes()).hexdigest()},
                'wasm': {'synthetic': {'sha256': '0' * 64}},
            }))
            process = subprocess.run([sys.executable, str(ci.HERE / 'identify_build.py'), '--build', str(build)],
                                     check=True, capture_output=True, text=True)
            name = process.stdout.strip()
            names.append(name)
            for filename in ('Cargo.toml', 'Cargo.lock'):
                self.assertIn(name, (build / 'harness' / filename).read_text())
            self.assertEqual(json.loads((build / 'build-manifest.json').read_text())['cargo_harness_identity']['name'], name)
        self.assertNotEqual(*names)


if __name__ == '__main__':
    unittest.main()
