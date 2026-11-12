"""Offline checker regressions using synthetic measurements, never execution proof.

Run: PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts/tests -p test_cl_resource_profiles.py
"""
import copy
import io
import json
import hashlib
import gzip
import lzma
from contextlib import redirect_stdout
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from cl_resource_profiles import create, FIELDS, POLICY
from check_local_budget import load_profile, validate_row, main, NUMBERS, REQUIRED
from cl_fee_estimate import estimate
from cl_proof_evidence import validate_manifest, cargo_outcomes
from cl_transaction_evidence import fee_configuration, compute_fees, validate_transaction, validate_transactions


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
        settings['configSettingContractHistoricalDataV0'] = {'values': {'feeHistorical1Kb': 100}}
        settings['configSettingContractBandwidthV0']['values']['feeTxSize1Kb'] = 50
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
        self.assertIn('RELEASE APPROVAL: NOT GRANTED', output.getvalue())

    def test_independent_invocation_gate_cannot_grant_checkpoint_or_release(self):
        report = self.directory / 'one-invocation.jsonl'
        report.write_text(json.dumps(self.row) + '\n')
        for gate, expected in [('invocation', 0), ('lifecycle', 1), ('transaction', 1), ('checkpoint', 1)]:
            output = io.StringIO()
            with redirect_stdout(output):
                status = main([str(report), '--profile', str(self.manifest), '--gate', gate])
            self.assertEqual(status, expected)
            self.assertIn('CHECKPOINT GATE: FAIL', output.getvalue())
            self.assertIn('RELEASE APPROVAL: NOT GRANTED', output.getvalue())

    def test_proof_mapping_requires_completed_assertions_and_bound_source_log_report(self):
        test = 'test::recovery::compiled_unit_fixture'
        source = self.directory / 'unit.rs'
        source.write_text('#[test]\nfn compiled_unit_fixture() { assert_eq!(1, 1); }\n')
        log = self.directory / 'unit.log'
        log.write_text('test ' + test + ' ... resource output\nok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored;\n')
        report = self.directory / 'unit.jsonl'
        report.write_text(json.dumps(self.row) + '\n')
        def bound(path):
            return {'path': path.name, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}
        manifest = {
            'schema': 'cl-lifecycle-proof-v1', 'profile': self.network['name'],
            'config_sha256': self.network['network_sha256'],
            'resource_report_sha256': bound(report)['sha256'],
            'test_runs': [dict(bound(log), exit_code=0)],
            'sources': {test: bound(source)},
            'proofs': [{'label': 'UNIT_TEST_ONLY', 'tests': [test],
                        'assertions': ['Synthetic parser test; not contract execution proof']}],
        }
        reviewed_map = self.directory / 'reviewed-map.json'
        reviewed_map.write_text(json.dumps({'schema': 'cl-proof-map-v1', 'sources': {test: source.name}, 'proofs': manifest['proofs']}))
        manifest['source_root'] = str(self.directory)
        manifest['reviewed_map'] = bound(reviewed_map)
        path = self.directory / 'proof.json'
        def check(m):
            path.write_text(json.dumps(m))
            return validate_manifest(path, report, self.network, {'UNIT_TEST_ONLY'}, {'UNIT_TEST_ONLY'})
        self.assertEqual(check(manifest), [])
        for mutation in ['source', 'log', 'report', 'exit', 'missing_mapping', 'unmeasured', 'unfinished']:
            with self.subTest(mutation=mutation):
                m = copy.deepcopy(manifest)
                if mutation == 'source': m['sources'][test]['sha256'] = '0'*64
                elif mutation == 'log': m['test_runs'][0]['sha256'] = '0'*64
                elif mutation == 'report': m['resource_report_sha256'] = '0'*64
                elif mutation == 'exit': m['test_runs'][0]['exit_code'] = 101
                elif mutation == 'missing_mapping': m['proofs'] = []
                elif mutation == 'unmeasured': m['proofs'][0]['label'] = 'UNKNOWN'
                else:
                    log.write_text('test ' + test + ' ... emitted successful row\nFAILED\n\ntest result: FAILED. 0 passed; 1 failed;\n')
                    m['test_runs'][0] = dict(bound(log), exit_code=0)
                self.assertTrue(check(m))
        self.assertEqual(cargo_outcomes('test x::compiled_y ... ok\n'), {'x::compiled_y': 'ok'})

    def test_complete_fee_formula_includes_return_history_bandwidth_and_per_entry_rent(self):
        fee, rent = fee_configuration(self.config)
        resources = dict(instructions=10_001, disk_read_entries=2, write_entries=1,
                         disk_read_bytes=1025, write_bytes=1025,
                         contract_events_size_bytes=1025, transaction_size_bytes=1025)
        changes = [dict(is_persistent=True, is_code_entry=False, old_size_bytes=0,
                        new_size_bytes=1024, old_live_until_ledger=0, new_live_until_ledger=200)]
        result = compute_fees(resources, fee, changes, rent, 100, 100)
        self.assertEqual(result['non_refundable'], 4+1562+1250+224+438+130+51)
        self.assertEqual(result['refundable'], 301)
        self.assertEqual(result['rent'], 1+1250+21)
        self.assertEqual(result['total'], sum(result[k] for k in ['non_refundable', 'refundable', 'rent', 'inclusion']))
        # Code rent discount is per-entry, before the shared TTL write fees.
        big = dict(changes[0], new_size_bytes=1_000_000, new_live_until_ledger=1000)
        ordinary = compute_fees(resources, fee, [big], rent, 100, 100)['rent'] - 1250 - 21
        code = compute_fees(resources, fee, [dict(big, is_code_entry=True)], rent, 100, 100)['rent'] - 1250 - 21
        self.assertEqual(code, (ordinary+2)//3)

    def transaction_fixture(self):
        """Synthetic validator-unit fixture; decoder is separately tested on XDR."""
        fee, rent = fee_configuration(self.config)
        resources = dict(instructions=1000, disk_read_entries=0, write_entries=0,
                         disk_read_bytes=0, write_bytes=0,
                         contract_events_size_bytes=4, transaction_size_bytes=256)
        fees = compute_fees(resources, fee, [], rent, 100, 100)
        decoded = dict(envelope_sha256='a'*64, host_function_sha256='b'*64,
                       authorization_size_bytes=4, return_value_size_bytes=4,
                       contract_events_size_bytes=0, authorizers=[], instructions=1000,
                       disk_read_bytes=0, write_bytes=0, envelope_size_bytes=256,
                       read_only_xdr=[], read_write_xdr=[],
                       resource_fee=str(fees['total']-100), post_state_data_sha256=hashlib.sha256(b'{}').hexdigest(), transaction_fee=fees['total'], function_name='UNIT_TEST_ONLY',contract_address='UNIT_CONTRACT',max_contract_data_key_size_bytes=0,max_contract_data_entry_size_bytes=0,max_contract_code_entry_size_bytes=0,restored_rw_entry_indices=[])
        record = dict(schema='cl-transaction-evidence-v1', flow='UNIT_TEST_ONLY', test_name='test::compiled_unit_fixture',
                      profile=self.network['name'], run_mode='acceptance', runtime_protocol=29,
                      target_protocol=29, config_sha256=self.network['network_sha256'],
                      scope='controlled-state-unsigned-simulation', engine='soroban-env-host-e2e',
                      success=True, outcome='succeeded', account_authorization_mode='recording-only',
                      contract_authorization_enforced=True, base_fee_source='controlled-state-ledger-setting',
                      limits_pass=True, diagnostic_shadow_ok=True, exact_captured_cost_model=True,
                      signature_verification_measured=False, live_state_validation=False, release_approval=False,
                      contract_role='UNIT_ROLE', contract_address='UNIT_CONTRACT', archive={'enabled':False},
                      footprint={'read_only_xdr':[], 'read_write_xdr':[]}, restored_rw_entry_indices=[],
                      diagnostic_shadow_cpu_used=1, diagnostic_shadow_memory_used=1,
                      diagnostic_shadow_cpu_limit=100,diagnostic_shadow_memory_limit=100,
                      expected_external_authorizers=[], resources=dict(resources, memory_bytes=100),
                      fee_inputs=fee, rent_fee_inputs=rent, rent_changes=[], current_ledger=100, fees=fees)
        for key in ['envelope_sha256', 'host_function_sha256', 'authorization_size_bytes',
                    'return_value_size_bytes', 'contract_events_size_bytes', 'post_state_data_sha256',
                    'max_contract_data_key_size_bytes', 'max_contract_data_entry_size_bytes', 'max_contract_code_entry_size_bytes']:
            record[key] = decoded[key]
        return record, decoded

    def test_complete_transaction_evidence_is_accepted_and_tampering_rejected(self):
        record, decoded = self.transaction_fixture()
        self.assertEqual(validate_transaction(record, self.network, lambda _: decoded), [])
        for mutation in ['runtime', 'envelope_size', 'model', 'fees', 'envelope_fee', 'contract_auth', 'return', 'memory', 'signature_mode']:
            with self.subTest(mutation=mutation):
                r, d = copy.deepcopy(record), copy.deepcopy(decoded)
                if mutation == 'runtime': r['runtime_protocol'] = 25
                elif mutation == 'envelope_size': r['resources']['transaction_size_bytes'] += 1
                elif mutation == 'model': r['fee_inputs']['fee_per_instruction_increment'] += 1
                elif mutation == 'fees': r['fees']['rent'] = 1
                elif mutation == 'envelope_fee': d['transaction_fee'] += 1
                elif mutation == 'contract_auth': d['authorizers'] = ['UNAUTHORIZED_CONTROLLER']
                elif mutation == 'return': r['resources']['contract_events_size_bytes'] = 0
                elif mutation == 'memory': r['resources']['memory_bytes'] = 41_943_041
                else: r['account_authorization_mode'] = 'mock_all_auths'
                self.assertTrue(validate_transaction(r, self.network, lambda _: d))
        self.assertTrue(validate_transactions(None, self.network, REQUIRED))

    def rejection_fixture(self):
        record, decoded = self.transaction_fixture()
        record.update(success=False, outcome='rejected', expected_rejection=True,
                      proof_case='CASE_A', return_value_xdr=None, error_xdr='AAAAAQAAAAY=',
                      contract_events_xdr=[], ledger_changes_empty=True,
                      pre_state_full_data_sha256='c'*64, post_state_full_data_sha256='c'*64,
                      post_state_unchanged=True, return_value_size_bytes=0)
        record['resources']['contract_events_size_bytes'] = 0
        record['fees'] = compute_fees(record['resources'], record['fee_inputs'], [], record['rent_fee_inputs'], 100, 100)
        decoded.update(return_value_size_bytes=0, error_type='sceWasmVm', error_code='scecInvalidAction',
                       transaction_fee=record['fees']['total'], resource_fee=str(record['fees']['total']-100))
        return record, decoded

    def test_rejected_transaction_requires_measured_error_costs_and_complete_rollback(self):
        record, decoded = self.rejection_fixture()
        self.assertEqual(validate_transaction(record, self.network, lambda _: decoded), [])
        record['account_authorization_mode'] = 'enforcing-empty'
        self.assertEqual(validate_transaction(record, self.network, lambda _: decoded), [])
        for mutation in ['success', 'expected', 'case', 'state', 'changes', 'unchanged', 'return',
                         'events', 'rent', 'budget', 'runtime_error', 'auth', 'output_cost']:
            with self.subTest(mutation=mutation):
                r, d = copy.deepcopy(record), copy.deepcopy(decoded)
                if mutation == 'success': r['success'] = True
                elif mutation == 'expected': r['expected_rejection'] = False
                elif mutation == 'case': del r['proof_case']
                elif mutation == 'state': r['post_state_full_data_sha256'] = 'd'*64
                elif mutation == 'changes': r['ledger_changes_empty'] = False
                elif mutation == 'unchanged': r['post_state_unchanged'] = False
                elif mutation == 'return': r['return_value_xdr'] = 'AAAAAQ=='
                elif mutation == 'events': r['contract_events_xdr'] = ['unexpected']
                elif mutation == 'rent': r['fees']['rent'] = 1
                elif mutation == 'budget': d['error_type'] = 'sceBudget'
                elif mutation == 'runtime_error': d['error_code'] = 'scecExceededLimit'
                elif mutation == 'auth': d['authorizers'] = ['external']; r['expected_external_authorizers'] = ['external']
                else: r['resources']['write_bytes'] = 1
                self.assertTrue(validate_transaction(r, self.network, lambda _: d))

    def test_rejected_mapping_cannot_reuse_another_case_success_or_another_test(self):
        record, decoded = self.rejection_fixture()
        calls = self.directory / 'calls.jsonl'
        calls.write_text(json.dumps({'calls': [{'role': 'UNIT_ROLE', 'address': 'UNIT_CONTRACT'}]}) + '\n')
        manifest = {'profile': self.network['name'], 'config_sha256': self.network['network_sha256'],
                    'call_profile': {'path': str(calls), 'sha256': hashlib.sha256(calls.read_bytes()).hexdigest()},
                    'contracts': {'UNIT_ROLE': ['UNIT_CONTRACT']},
                    'proofs': [{'label': 'CASE_A', 'tests': [record['test_name']], 'transactions': [
                        {'function': record['flow'], 'contract_role': record['contract_role'], 'outcome': 'rejected', 'proof_case': 'CASE_A'}]}]}
        transaction = self.directory / 'rejected.jsonl'
        transaction.write_text(json.dumps(record) + '\n')
        manifest_path = self.directory / 'mapping.json'
        def check(m):
            manifest_path.write_text(json.dumps(m))
            with patch('cl_transaction_evidence.decode_xdr', return_value=decoded):
                return validate_transactions(transaction, self.network, {'CASE_A'}, manifest_path)
        self.assertEqual(check(manifest), [])
        for mutation in ['other_case', 'missing_case', 'success', 'test']:
            with self.subTest(mutation=mutation):
                m = copy.deepcopy(manifest)
                call = m['proofs'][0]['transactions'][0]
                if mutation == 'other_case': call['proof_case'] = 'CASE_B'
                elif mutation == 'missing_case': del call['proof_case']
                elif mutation == 'success': call['outcome'] = 'success'
                else: m['proofs'][0]['tests'] = ['another::compiled_test']
                self.assertTrue(check(m))

        # A mismatched recipient tree cannot be replaced by missing authorization,
        # a different mismatched argument, or a non-Auth contract failure.
        record['account_authorization_mode'] = 'enforcing-mismatched-root'
        record['expected_external_authorizers'] = ['EXTERNAL']
        decoded.update(authorizers=['EXTERNAL'], error_type='sceAuth', mismatched_argument_indices=[2])
        transaction.write_text(json.dumps(record) + '\n')
        call = manifest['proofs'][0]['transactions'][0]
        call.update(account_authorization_mode='enforcing-mismatched-root', mismatched_argument_indices=[2])
        self.assertEqual(check(manifest), [])
        for mutation in ['missing_indices', 'wrong_index', 'empty_auth', 'non_auth_error']:
            with self.subTest(mutation=mutation):
                m = copy.deepcopy(manifest)
                if mutation == 'missing_indices': del m['proofs'][0]['transactions'][0]['mismatched_argument_indices']
                elif mutation == 'wrong_index': m['proofs'][0]['transactions'][0]['mismatched_argument_indices'] = [3]
                elif mutation == 'empty_auth': m['proofs'][0]['transactions'][0]['account_authorization_mode'] = 'enforcing-empty'
                else: decoded['error_type'] = 'sceWasmVm'
                self.assertTrue(check(m))

    def test_checkpoint_gate_can_pass_complete_evidence_without_release_approval(self):
        # A validator-unit fixture, not a successful contract execution claim.
        # Real XDR parsing is independently exercised on emitted host29 evidence.
        record, decoded = self.transaction_fixture()
        test = record['test_name']
        source = self.directory / 'unit.rs'
        source.write_text('fn compiled_unit_fixture() { assert!(true); }')
        report = self.directory / 'resources.jsonl'
        report.write_text(json.dumps(self.row) + '\n')
        log = self.directory / 'tests.log'
        log.write_text('test ' + test + ' ... ok\ntest result: ok. 1 passed; 0 failed;\n')
        calls = self.directory / 'calls.jsonl'
        calls.write_text(json.dumps({'calls': [{'role': 'UNIT_ROLE', 'address': 'UNIT_CONTRACT'}]}) + '\n')
        def bound(path):
            return {'path': str(path), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}
        proof = {'label': 'UNIT_TEST_ONLY', 'tests': [test], 'assertions': ['validator-unit fixture'],
                 'resource_flows': ['UNIT_TEST_ONLY'], 'transactions': [{'function': 'UNIT_TEST_ONLY', 'contract_role': 'UNIT_ROLE'}]}
        mapping = self.directory / 'map.json'
        mapping.write_text(json.dumps({'schema': 'cl-proof-map-v1', 'sources': {test: str(source)}, 'proofs': [proof]}))
        manifest = self.directory / 'proof.json'
        manifest.write_text(json.dumps({
            'schema': 'cl-lifecycle-proof-v1', 'profile': self.network['name'],
            'config_sha256': self.network['network_sha256'], 'resource_report_sha256': bound(report)['sha256'],
            'reviewed_map': bound(mapping), 'source_root': str(self.directory),
            'test_runs': [dict(bound(log), exit_code=0)], 'sources': {test: bound(source)},
            'proofs': [proof], 'call_profile': bound(calls), 'contracts': {'UNIT_ROLE': ['UNIT_CONTRACT']},
        }))
        transaction = self.directory / 'transaction.jsonl'
        transaction.write_text(json.dumps(record) + '\n')
        args = [str(report), '--profile', str(self.manifest), '--proof-manifest', str(manifest),
                '--transaction-evidence', str(transaction)]
        output = io.StringIO()
        with patch('check_local_budget.REQUIRED', {'UNIT_TEST_ONLY'}), patch('cl_transaction_evidence.decode_xdr', return_value=decoded), redirect_stdout(output):
            self.assertEqual(main(args), 0)
        self.assertIn('CHECKPOINT GATE: PASS', output.getvalue())
        self.assertIn('RELEASE APPROVAL: NOT GRANTED', output.getvalue())
        for suffix, compress in [('.gz', gzip.compress), ('.xz', lzma.compress)]:
            compressed = self.directory / ('transaction.jsonl' + suffix)
            compressed.write_bytes(compress(transaction.read_bytes()))
            with patch('check_local_budget.REQUIRED', {'UNIT_TEST_ONLY'}), patch('cl_transaction_evidence.decode_xdr', return_value=decoded), redirect_stdout(io.StringIO()):
                self.assertEqual(main(args[:-1] + [str(compressed)]), 0)
            compressed.write_bytes(b'not a compressed artifact')
            self.assertTrue(validate_transactions(compressed, self.network, {'UNIT_TEST_ONLY'}, manifest))
        # Omitting transaction evidence cannot preserve the checkpoint pass.
        with patch('check_local_budget.REQUIRED', {'UNIT_TEST_ONLY'}), redirect_stdout(io.StringIO()):
            self.assertEqual(main(args[:-2]), 1)

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
