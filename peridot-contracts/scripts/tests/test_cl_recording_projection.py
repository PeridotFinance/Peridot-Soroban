#!/usr/bin/env python3
"""Offline decoder regressions using real unsigned host29 envelope/entry XDR.

Embedded fixtures retain one real ledger-key evidence slice from completed v4
archived and v3 live funded liquidations. They are decoder unit inputs, NOT full
transaction evidence or release approval. No contract execution/signing/network.
Provenance hashes cover JSON.stringify(original full row); original rows remain
in target/cl-atomic-v4-baseline-archived and cl-atomic-v3-baseline-captured.
"""
import base64
import copy
import json
from pathlib import Path
import subprocess
import unittest
import zlib

ROOT = Path(__file__).resolve().parents[2]
FIXTURE_ZLIB_BASE64 = (
    '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'
    'LLk='
)

FIXTURES = json.loads(zlib.decompress(base64.b64decode(FIXTURE_ZLIB_BASE64)))


class RecordingProjectionTests(unittest.TestCase):
    def decode(self, row):
        return subprocess.run(['node', str(ROOT / 'scripts/decode_cl_transaction_evidence.cjs')],
                              input=json.dumps(row), text=True, capture_output=True, cwd=ROOT)

    def fixture(self):
        row = copy.deepcopy(FIXTURES['archived'])
        return row, row['recording_projection_repair']['placeholders'][0]

    def rejected(self, row, diagnostic):
        result = self.decode(row)
        self.assertEqual(result.returncode, 1, result.stdout)
        self.assertIn(diagnostic, result.stderr)
        self.assertEqual(result.stdout, '')

    def test_v4_archived_real_xdr_retains_exact_prior_data(self):
        row, marker = self.fixture()
        self.assertTrue(marker['listed_as_restored'])
        self.assertFalse(marker['read_only'])
        result = self.decode(row)
        self.assertEqual(result.returncode, 0, result.stderr)
        decoded = json.loads(result.stdout)
        self.assertEqual(decoded['post_state_data_sha256'], FIXTURES['expected'])
        self.assertEqual(len(decoded['read_write_xdr']), 103)

    def test_v3_live_real_xdr_keeps_legacy_projection(self):
        result = self.decode(FIXTURES['legacy'])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)['post_state_data_sha256'], FIXTURES['expected'])

    def test_missing_marker_does_not_silently_repair_legacy_rw(self):
        row, _ = self.fixture()
        del row['recording_projection_repair']
        result = self.decode(row)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotEqual(json.loads(result.stdout)['post_state_data_sha256'], FIXTURES['expected'])

    def test_mutated_rent_sizes_ttl_and_new_value_reject(self):
        for field, value in [('old_entry_size_bytes_for_rent', 1),
                             ('new_entry_size_bytes_for_rent', 1),
                             ('ttl_change', {'old_live_until': 0, 'new_live_until': 1}),
                             ('encoded_new_value', ''),
                             ('effective_restored_ttl_established', True)]:
            with self.subTest(field=field):
                row, marker = self.fixture()
                marker[field] = value
                self.rejected(row, 'invalid recording projection placeholder')

    def test_mutated_key_rejects(self):
        row, marker = self.fixture()
        # A different real, canonical key from this transaction's footprint.
        decoded = json.loads(self.decode(row).stdout)
        marker['key_xdr'] = next(k for k in decoded['read_write_xdr'] if k != marker['key_xdr'])
        self.rejected(row, 'placeholder')

    def test_mutated_restoration_index_and_access_reject(self):
        for field, value in [('listed_as_restored', False),
                             ('read_write_footprint_index', None), ('read_only', True)]:
            with self.subTest(field=field):
                row, marker = self.fixture()
                marker[field] = value
                self.rejected(row, 'placeholder footprint/restoration mismatch')

    def test_mutated_initial_entry_rejects(self):
        for field, value in [('initial_entry_exists', False), ('initial_entry_xdr', None),
                             ('initial_entry_xdr', FIXTURES['legacy']['return_value_xdr'])]:
            with self.subTest(field=field):
                row, marker = self.fixture()
                marker[field] = value
                self.rejected(row, 'placeholder initial entry mismatch')

    def test_after_entry_cannot_be_overridden(self):
        row, _ = self.fixture()
        after = copy.deepcopy(row['entry_size_checks'][0])
        after['phase'] = 'after'
        row['entry_size_checks'].append(after)
        self.rejected(row, 'after entry')

    def test_duplicate_missing_unknown_marker_fields_reject(self):
        row, marker = self.fixture()
        row['recording_projection_repair']['placeholders'].append(copy.deepcopy(marker))
        self.rejected(row, 'duplicate')
        row, marker = self.fixture()
        del marker['ttl_change']
        self.rejected(row, 'invalid recording projection placeholder')
        row, marker = self.fixture()
        marker['unknown'] = True
        self.rejected(row, 'invalid recording projection placeholder')

    def test_unknown_null_or_malformed_repair_rejects(self):
        for value in [None, {}, {'schema': 'future-schema'}, []]:
            with self.subTest(value=value):
                row, _ = self.fixture()
                row['recording_projection_repair'] = value
                self.rejected(row, 'invalid recording projection repair schema')


if __name__ == '__main__':
    unittest.main()
