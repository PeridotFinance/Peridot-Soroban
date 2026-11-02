"""Offline checker regressions using synthetic measurements, never execution proof.

Run: PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts/tests -p test_cl_resource_profiles.py
"""
import copy
import io
import json
from contextlib import redirect_stdout
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from cl_resource_profiles import create, FIELDS, POLICY
from check_local_budget import load_profile, validate_row, main, NUMBERS, REQUIRED
from cl_fee_estimate import estimate


class ResourceProfileTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name)
        settings = {}
        for field, (name, key) in FIELDS.items():
            if key:
                settings.setdefault(name, {'values': {}})['values'][key] = POLICY[field]
            else:
                settings[name] = {'values': POLICY[field]}
        settings['configSettingContractComputeV0']['values'].update(
            txMaxInstructions=400_000_000, feeRatePerInstructionsIncrement=3)
        settings['configSettingContractLedgerCostExtV0']['values'].update(
            txMaxFootprintEntries=400, feeWrite1Kb=437)
        settings['configSettingContractLedgerCostV0']['values'].update(
            txMaxWriteLedgerEntries=200, txMaxDiskReadEntries=200,
            sorobanStateTargetSizeBytes=3_000_000_000,
            rentFee1KbSorobanStateSizeLow=-17000,
            rentFee1KbSorobanStateSizeHigh=10000,
            sorobanStateRentFeeGrowthFactor=5000,
            feeDiskReadLedgerEntry=781, feeWriteLedgerEntry=1250, feeDiskRead1Kb=223)
        settings['configSettingContractEventsV0']['values']['feeContractEvents1Kb'] = 300
        settings['configSettingStateArchival'] = {'values': {
            'persistentRentRateDenominator': 252480, 'tempRentRateDenominator': 2524800}}
        settings['configSettingLiveSorobanStateSizeWindow'] = {'values': [100, 200]}
        for suffix in ['CpuInstructions', 'MemoryBytes']:
            settings['configSettingContractCostParams' + suffix] = {'values': [
                {'ext': {'variant': 0}, 'constTerm': 1, 'linearTerm': 0} for _ in range(86)]}
        self.config = {'network': {'passphrase': 'Public Global Stellar Network ; September 2015',
                                   'protocolVersion': 29}, 'ledger': 123, 'settings': settings}
        self.config_path = self.directory / 'network-config.json'
        self.config_path.write_text(json.dumps(self.config))
        self.network = create(self.config_path, 'captured-network-diagnostic')
        self.policy = create(self.config_path, 'internal-policy')
        self.manifest = self.directory / 'profile.json'
        self.manifest.write_text(json.dumps(self.network))
        self.row = {key: 0 for key in NUMBERS}
        self.row.update(flow='UNIT_TEST_ONLY', ok=True, scope='synthetic-checker-unit-test',
                        profile=self.network['name'], run_mode='acceptance',
                        runtime_protocol=29, target_protocol=29,
                        config_sha256=self.network['network_sha256'],
                        sdk_limits_enforced=True, diagnostic_shadow_ok=True,
                        exact_captured_cost_model=True,
                        diagnostic_shadow_cpu_limit=20_000_000_000,
                        diagnostic_shadow_memory_limit=4_294_967_296)

    def test_profiles_keep_distinct_limits(self):
        self.assertEqual(load_profile(self.manifest), self.network)
        self.assertEqual(self.network['limits']['instructions'], 400_000_000)
        self.assertEqual(self.policy['limits']['instructions'], 100_000_000)
        self.assertEqual(self.network['limits']['ledger_entries'], 400)
        self.assertEqual(self.policy['limits']['ledger_entries'], 100)
        self.assertEqual(self.network['limits']['write_entries'], 200)
        self.assertEqual(self.policy['limits']['write_entries'], 50)
        self.assertFalse(self.network['release_approval'])
        self.assertEqual(validate_row(self.row, self.network), [])

    def test_all_limits_and_evidence_fail_closed(self):
        mutations = [('runtime_protocol', 25), ('run_mode', 'profiling-only'),
                     ('diagnostic_shadow_ok', False), ('exact_captured_cost_model', False),
                     ('sdk_limits_enforced', False), ('instructions', 400_000_001),
                     ('memory_bytes', 41_943_041), ('read_entries', 401),
                     ('write_entries', 201), ('disk_read_entries', 201),
                     ('read_bytes', 200_001), ('write_bytes', 132_097),
                     ('contract_events_size_bytes', 16_385),
                     ('diagnostic_shadow_cpu_used', 20_000_000_001),
                     ('diagnostic_shadow_memory_used', 4_294_967_297)]
        for key, value in mutations:
            with self.subTest(field=key):
                row = dict(self.row, **{key: value})
                self.assertTrue(validate_row(row, self.network))
        for key in NUMBERS:
            with self.subTest(missing=key):
                row = self.row.copy()
                del row[key]
                self.assertTrue(validate_row(row, self.network))

    def test_internal_policy_caps_are_not_network_caps(self):
        row = dict(self.row, profile='internal-policy')
        for key, value in [('instructions', 100_000_001), ('read_entries', 101),
                           ('write_entries', 51)]:
            with self.subTest(field=key):
                self.assertTrue(validate_row(dict(row, **{key: value}), self.policy))
                self.assertFalse(validate_row(dict(self.row, **{key: value}), self.network))

    def test_tampering_config_and_profile_is_rejected(self):
        for change in ['limit', 'hash', 'release']:
            with self.subTest(change=change):
                profile = copy.deepcopy(self.network)
                if change == 'limit':
                    profile['limits']['instructions'] += 1
                elif change == 'hash':
                    profile['network_sha256'] = '0' * 64
                else:
                    profile['release_approval'] = True
                self.manifest.write_text(json.dumps(profile))
                with self.assertRaises(ValueError):
                    load_profile(self.manifest)
        for change in ['missing_limit', 'negative_limit', 'negative_cost', 'invalid_cost_ext']:
            with self.subTest(change=change):
                config = copy.deepcopy(self.config)
                compute = config['settings']['configSettingContractComputeV0']['values']
                costs = config['settings']['configSettingContractCostParamsCpuInstructions']['values']
                if change == 'missing_limit':
                    del compute['txMaxInstructions']
                elif change == 'negative_limit':
                    compute['txMaxInstructions'] = -1
                elif change == 'negative_cost':
                    costs[0]['linearTerm'] = -1
                else:
                    costs[0]['ext'] = {'variant': 1}
                self.config_path.write_text(json.dumps(config))
                with self.assertRaises((KeyError, ValueError)):
                    create(self.config_path, 'captured-network-diagnostic')

    def test_missing_flows_never_pass(self):
        report = self.directory / 'empty.jsonl'
        report.write_text('')
        output = io.StringIO()
        with redirect_stdout(output):
            status = main([str(report), '--profile', str(self.manifest)])
        self.assertEqual(status, 1)
        self.assertIn('UNMEASURED:', output.getvalue())
        for required in REQUIRED:
            self.assertIn(required, output.getvalue())
        self.assertIn('CAPTURED-NETWORK-DIAGNOSTIC: FAIL', output.getvalue())
        self.assertIn('RELEASE GATE: FAIL', output.getvalue())

    def test_partial_fees_support_both_window_shapes_and_actual_runtime(self):
        array_fee = estimate(self.row, self.config_path)
        config = copy.deepcopy(self.config)
        config['settings']['configSettingLiveSorobanStateSizeWindow']['values'] = {'0': 100, '1': 200}
        self.config_path.write_text(json.dumps(config))
        self.assertEqual(estimate(self.row, self.config_path), array_fee)
        self.assertEqual(array_fee['local_host_protocol'], 29)
        self.assertFalse(array_fee['full_transaction_fee'])
        self.assertFalse(estimate(dict(self.row, runtime_protocol=25), self.config_path)['runtime_matches_network'])
        config['settings']['configSettingLiveSorobanStateSizeWindow']['values'] = []
        self.config_path.write_text(json.dumps(config))
        with self.assertRaises(ValueError):
            estimate(self.row, self.config_path)


if __name__ == '__main__':
    unittest.main()
