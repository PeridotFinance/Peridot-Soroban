"""Offline validation of unsigned target-runtime transaction simulation evidence.

The producer must run host e2e execution, not reprice SDK invocation quantities.
This validates serialized artifacts and independently recomputes protocol29 fees.
It does not prove real account signatures or give controlled state live-state status.
"""
import json
import gzip
import lzma
import re
from pathlib import Path
import subprocess


def ceil(n, d):
    return (n + d - 1) // d


def positive_int(value, label, zero=False):
    if type(value) is not int or value < (0 if zero else 1):
        raise ValueError('invalid numeric evidence: ' + label)
    return value


def fee_configuration(config):
    s = {k: v['values'] for k, v in config['settings'].items()}
    c = s['configSettingContractComputeV0']
    l = s['configSettingContractLedgerCostV0']
    ext = s['configSettingContractLedgerCostExtV0']
    archival = s['configSettingStateArchival']
    window = s['configSettingLiveSorobanStateSizeWindow']
    values = list(window.values()) if isinstance(window, dict) else window
    if not isinstance(values, list) or not values:
        raise ValueError('captured live-state window missing')
    size = sum(positive_int(v, 'live state size', True) for v in values) // len(values)
    target = positive_int(l['sorobanStateTargetSizeBytes'], 'state target')
    low, high = l['rentFee1KbSorobanStateSizeLow'], l['rentFee1KbSorobanStateSizeHigh']
    delta = high - low
    rent_rate = (low + ceil(delta * size, target) if size <= target else
                 high + ceil(delta * (size-target) * l['sorobanStateRentFeeGrowthFactor'], target))
    rent_rate = max(1000, rent_rate)
    fee = {
        'fee_per_instruction_increment': c['feeRatePerInstructionsIncrement'],
        'fee_per_disk_read_entry': l['feeDiskReadLedgerEntry'],
        'fee_per_write_entry': l['feeWriteLedgerEntry'],
        'fee_per_disk_read_1kb': l['feeDiskRead1Kb'],
        'fee_per_write_1kb': ext['feeWrite1Kb'],
        'fee_per_historical_1kb': s['configSettingContractHistoricalDataV0']['feeHistorical1Kb'],
        'fee_per_contract_event_1kb': s['configSettingContractEventsV0']['feeContractEvents1Kb'],
        'fee_per_transaction_size_1kb': s['configSettingContractBandwidthV0']['feeTxSize1Kb'],
    }
    rent = {
        'fee_per_write_1kb': ext['feeWrite1Kb'], 'fee_per_rent_1kb': rent_rate,
        'fee_per_write_entry': l['feeWriteLedgerEntry'],
        'persistent_rent_rate_denominator': archival['persistentRentRateDenominator'],
        'temporary_rent_rate_denominator': archival['tempRentRateDenominator'],
    }
    for key, value in {**fee, **rent}.items():
        positive_int(value, key, True)
    return fee, rent


def compute_fees(resources, fee, rent_changes, rent, ledger, inclusion):
    """Integer formulas from pinned soroban-env-host29.0.1/src/fees.rs."""
    non_refundable = (
        ceil(resources['instructions'] * fee['fee_per_instruction_increment'], 10_000)
        + resources['disk_read_entries'] * fee['fee_per_disk_read_entry']
        + resources['write_entries'] * fee['fee_per_write_entry']
        + ceil(resources['disk_read_bytes'] * fee['fee_per_disk_read_1kb'], 1024)
        + ceil(resources['write_bytes'] * fee['fee_per_write_1kb'], 1024)
        + ceil((resources['transaction_size_bytes'] + 300) * fee['fee_per_historical_1kb'], 1024)
        + ceil(resources['transaction_size_bytes'] * fee['fee_per_transaction_size_1kb'], 1024))
    refundable = ceil(resources['contract_events_size_bytes'] * fee['fee_per_contract_event_1kb'], 1024)
    rent_total, extensions = 0, 0
    for entry in rent_changes:
        if type(entry['is_persistent']) is not bool or type(entry['is_code_entry']) is not bool:
            raise ValueError('invalid rent entry type')
        for key in ['old_size_bytes', 'new_size_bytes', 'old_live_until_ledger', 'new_live_until_ledger']:
            positive_int(entry[key], key, True)
        old_size, new_size = entry['old_size_bytes'], entry['new_size_bytes']
        old_ttl, new_ttl = entry['old_live_until_ledger'], entry['new_live_until_ledger']
        new = old_size == 0 and old_ttl == 0
        denom = 1024 * rent['persistent_rent_rate_denominator' if entry['is_persistent'] else 'temporary_rent_rate_denominator']
        if denom <= 0:
            raise ValueError('invalid rent denominator')
        previous = max(ledger - 1, 0) if new else old_ttl
        entry_fee = 0
        if new_ttl >= previous:
            entry_fee += ceil(new_size * rent['fee_per_rent_1kb'] * (new_ttl - previous), denom)
        if not new and old_ttl >= ledger and new_size >= old_size:
            entry_fee += ceil((new_size - old_size) * rent['fee_per_rent_1kb'] * (old_ttl-ledger+1), denom)
        if entry['is_code_entry']:
            entry_fee = ceil(entry_fee, 3)
        rent_total += entry_fee
        extensions += old_ttl < new_ttl
    rent_total += extensions * rent['fee_per_write_entry'] + ceil(48 * extensions * rent['fee_per_write_1kb'], 1024)
    return {'non_refundable': non_refundable, 'refundable': refundable,
            'rent': rent_total, 'inclusion': inclusion,
            'total': non_refundable + refundable + rent_total + inclusion}


def decode_xdr(record):
    tool = Path(__file__).with_name('decode_cl_transaction_evidence.cjs')
    result = subprocess.run(['node', str(tool)], input=json.dumps(record), text=True,
                            capture_output=True, timeout=30, check=False)
    if result.returncode:
        raise ValueError('unsigned XDR validation failed: ' + result.stderr.strip())
    return json.loads(result.stdout)


def validate_transaction(record, profile, decoder=None):
    errors = []
    try:
        if profile['target_protocol'] != 29:
            return ['transaction fee verifier supports protocol29 only']
        expected = {
            'schema': 'cl-transaction-evidence-v1', 'profile': profile['name'],
            'run_mode': 'acceptance', 'runtime_protocol': profile['target_protocol'],
            'target_protocol': profile['target_protocol'], 'config_sha256': profile['network_sha256'],
            'scope': 'controlled-state-unsigned-simulation', 'engine': 'soroban-env-host-e2e',
            'contract_authorization_enforced': True,
            'base_fee_source': 'controlled-state-ledger-setting',
            'limits_pass': True, 'diagnostic_shadow_ok': True,
            'exact_captured_cost_model': True, 'signature_verification_measured': False,
            'live_state_validation': False, 'release_approval': False,
        }
        for key, value in expected.items():
            if record.get(key) != value or type(record.get(key)) is not type(value):
                errors.append('transaction evidence mismatch: ' + key)
        outcome = record.get('outcome')
        if outcome not in ('succeeded', 'rejected') or record.get('success') is not (outcome == 'succeeded'):
            errors.append('transaction outcome/success classification mismatch')
        rejected = outcome == 'rejected'
        auth_modes = ('recording-only', 'enforcing-empty', 'enforcing-mismatched-root') if rejected else ('recording-only',)
        if record.get('account_authorization_mode') not in auth_modes:
            errors.append('transaction evidence mismatch: account_authorization_mode')
        if rejected:
            if record.get('expected_rejection') is not True or not isinstance(record.get('proof_case'), str) or not record['proof_case']:
                errors.append('rejected transaction needs an explicit expected proof case')
            # Full snapshot hashes are producer attestations. The row independently
            # proves touched-entry XDR; paired test assertions bind the rollback
            # claim to the official host error result, which exports no changes.
            before, after = record.get('pre_state_full_data_sha256'), record.get('post_state_full_data_sha256')
            if not isinstance(before, str) or re.fullmatch('[0-9a-f]{64}', before) is None or before != after:
                errors.append('rejected transaction full-state rollback attestation mismatch')
            if record.get('ledger_changes_empty') is not True or record.get('post_state_unchanged') is not True:
                errors.append('rejected transaction does not attest complete rollback')
            if record.get('return_value_xdr') is not None or record.get('contract_events_xdr') != [] or record.get('rent_changes') != []:
                errors.append('rejected transaction cannot return a value, events or rent changes')
        elif record.get('expected_rejection') is True:
            errors.append('expected rejection unexpectedly succeeded')
        if not isinstance(record.get('flow'), str) or not record['flow']:
            errors.append('transaction flow missing')
        if not isinstance(record.get('test_name'), str) or '::compiled_' not in record['test_name']:
            errors.append('transaction test provenance missing')
        if errors:
            return errors
        decoded = (decoder or decode_xdr)(record)
        if rejected:
            if decoded.get('error_type') not in ('sceContract', 'sceWasmVm', 'sceAuth') or decoded.get('error_code') == 'scecExceededLimit':
                errors.append('resource/setup/runtime failure is not a semantic rejection proof')
            if decoded['return_value_size_bytes'] != 0 or decoded['contract_events_size_bytes'] != 0:
                errors.append('rejected transaction exported a return value or events')
            if record['account_authorization_mode'] == 'enforcing-empty' and decoded['authorizers']:
                errors.append('empty-auth rejection contains authorization entries')
            if record['account_authorization_mode'] == 'enforcing-mismatched-root' and (
                    decoded.get('error_type') != 'sceAuth' or len(decoded['authorizers']) != 1
                    or not decoded.get('mismatched_argument_indices')):
                errors.append('mismatched-root proof must reject actual external root argument mismatch with Auth error')
        if record['flow'] != decoded['function_name'] or record['contract_address'] != decoded['contract_address']:
            errors.append('serialized function/target does not match transaction evidence')
        if not isinstance(record['contract_role'], str) or not record['contract_role']:
            errors.append('transaction fixture role missing')
        for key in ['envelope_sha256', 'host_function_sha256', 'authorization_size_bytes',
                    'return_value_size_bytes', 'contract_events_size_bytes', 'post_state_data_sha256']:
            if record.get(key) != decoded[key]:
                errors.append('serialized transaction mismatch: ' + key)
        allowed = record['expected_external_authorizers']
        if not isinstance(allowed, list) or any(not isinstance(v, str) for v in allowed):
            raise ValueError('expected external authorizers missing')
        if set(decoded['authorizers']) - set(allowed):
            errors.append('unexpected recorded authorization (contract auth cannot be bypassed)')
        resources = record['resources']
        names = ['instructions', 'disk_read_entries', 'write_entries', 'disk_read_bytes',
                 'write_bytes', 'contract_events_size_bytes', 'transaction_size_bytes']
        for key in names:
            positive_int(resources[key], key, True)
        for key in ['instructions', 'disk_read_bytes', 'write_bytes']:
            if resources[key] != decoded[key]:
                errors.append('envelope resource declaration mismatch: ' + key)
        if resources['transaction_size_bytes'] != decoded['envelope_size_bytes']:
            errors.append('transaction fee input must use full serialized unsigned envelope')
        if resources['contract_events_size_bytes'] != decoded['contract_events_size_bytes'] + decoded['return_value_size_bytes']:
            errors.append('events plus return size mismatch')
        if resources['write_entries'] != len(decoded['read_write_xdr']):
            errors.append('write footprint mismatch')
        if record['footprint'] != {'read_only_xdr': decoded['read_only_xdr'], 'read_write_xdr': decoded['read_write_xdr']}:
            errors.append('serialized footprint differs from reported footprint')
        if record['restored_rw_entry_indices'] != decoded['restored_rw_entry_indices']:
            errors.append('serialized restoration indices differ from report')
        footprint = len(decoded['read_only_xdr']) + len(decoded['read_write_xdr'])
        limits = profile['limits']
        for key in ['max_contract_data_key_size_bytes', 'max_contract_data_entry_size_bytes', 'max_contract_code_entry_size_bytes']:
            positive_int(record[key], key, True)
            if record[key] != decoded[key] or decoded[key] > limits[key]:
                errors.append('entry/key/code resource check failed: ' + key)
        for used, cap in [('diagnostic_shadow_cpu_used', 'diagnostic_shadow_cpu_limit'),
                          ('diagnostic_shadow_memory_used', 'diagnostic_shadow_memory_limit')]:
            if positive_int(record[used], used, True) > positive_int(record[cap], cap):
                errors.append('transaction diagnostic budget exhausted: ' + used)
        memory = positive_int(resources['memory_bytes'], 'memory_bytes', True)
        if memory > limits['mem_bytes']:
            errors.append('transaction memory exceeds profile')
        for key in names:
            if resources[key] > limits[key]:
                errors.append('transaction resource exceeds profile: ' + key)
        if footprint > limits['ledger_entries'] or resources['disk_read_entries'] > footprint:
            errors.append('invalid or excessive transaction footprint')
        config = json.loads(Path(profile['network_config_path']).read_text())
        fee, rent = fee_configuration(config)
        for name in ['fee_inputs', 'rent_fee_inputs', 'fees']:
            for key, value in record[name].items():
                positive_int(value, name + '.' + key, True)
        if record['fee_inputs'] != fee or record['rent_fee_inputs'] != rent:
            errors.append('transaction fee model differs from captured configuration')
        ledger = positive_int(record['current_ledger'], 'current_ledger')
        inclusion = positive_int(record['fees']['inclusion'], 'controlled base fee')
        recomputed = compute_fees(resources, fee, record['rent_changes'], rent, ledger, inclusion)
        if record['fees'] != recomputed:
            errors.append('complete fee components do not reconcile')
        if rejected and (record['fees']['rent'] != 0 or record['fees']['refundable'] != 0 or record['resources']['write_bytes'] != 0):
            errors.append('rejected transaction has nonzero rollback output costs')
        resource_fee = recomputed['total'] - inclusion
        if int(decoded['resource_fee']) != resource_fee or decoded['transaction_fee'] != recomputed['total']:
            errors.append('unsigned envelope fee declarations do not reconcile')
        archive = record['archive']
        if type(archive['enabled']) is not bool:
            raise ValueError('archive simulation classification missing')
        if archive['enabled'] and archive['modified_keys']:
            indices = record['restored_rw_entry_indices']
            if type(archive['restoration_observed']) is not bool or archive['restoration_observed'] != bool(indices):
                errors.append('restoration observation does not match actual restored indices')
            selected = set()
            for item in archive['modified_keys']:
                before = positive_int(item['original_live_until_ledger'], 'original TTL', True)
                expired = positive_int(item['archived_live_until_ledger'], 'expired TTL', True)
                if before < ledger or expired >= ledger:
                    errors.append('archive snapshot does not cross the TTL boundary')
                selected.add(item['key_xdr'])
            for index in indices:
                if type(index) is not int or index < 0 or index >= len(decoded['read_write_xdr']) or decoded['read_write_xdr'][index] not in selected:
                    errors.append('restored index is not a deliberately expired snapshot entry')
            if archive['return_matches_baseline'] is not True or archive['baseline_return_value_xdr'] != record['return_value_xdr']:
                errors.append('archive simulation return differs from baseline')
            if archive['contract_changes_match_baseline'] is not True or archive['baseline_post_state_data_sha256'] != decoded['post_state_data_sha256']:
                errors.append('archive simulation changed contract state or historical entitlements')
    except (OSError, KeyError, TypeError, ValueError, subprocess.SubprocessError) as error:
        errors.append('invalid transaction evidence: ' + str(error))
    return errors


def validate_transactions(path, profile, required, proof_manifest=None):
    if path is None:
        return ['unsigned target-runtime transaction evidence missing']
    errors, seen = [], set()
    try:
        opener = (lzma.open if str(path).endswith('.xz') else
                  gzip.open if str(path).endswith('.gz') else open)
        with opener(path, 'rt') as source:
            rows = [json.loads(line) for line in source if line.strip()]
        for index, row in enumerate(rows):
            row_errors = validate_transaction(row, profile)
            errors.extend(f'transaction row {index}: {error}' for error in row_errors)
            if not row_errors:
                archived = row['archive']['enabled'] and row['archive'].get('restoration_observed') is True
                # Decode only accepted rows for archived key-name provenance.
                decoded = decode_xdr(row) if archived or row['account_authorization_mode'] == 'enforcing-mismatched-root' else {}
                names = frozenset(decoded.get('footprint_key_names', {}).get(row['footprint']['read_write_xdr'][i], '')
                                  for i in row['restored_rw_entry_indices'])
                outcome = 'success' if row['outcome'] == 'succeeded' else 'rejected'
                seen.add((row['flow'], row['contract_role'], row['contract_address'], outcome, archived, row['test_name'], names, row.get('proof_case'), row['account_authorization_mode'], tuple(decoded.get('mismatched_argument_indices', []))))
    except (OSError, EOFError, lzma.LZMAError, TypeError, ValueError) as error:
        errors.append('invalid transaction evidence file: ' + str(error))
    if proof_manifest is None:
        errors.append('transaction proof-to-call mapping missing')
    else:
        try:
            from cl_proof_evidence import verified_file
            manifest_path = Path(proof_manifest)
            manifest = json.loads(manifest_path.read_text())
            if manifest['profile'] != profile['name'] or manifest['config_sha256'] != profile['network_sha256']:
                raise ValueError('transaction mapping profile mismatch')
            # Bind fixture roles to the explicit call-profile artifact.
            calls_path = verified_file(manifest_path.parent, manifest['call_profile'])
            roles = {}
            for line in calls_path.read_text().splitlines():
                for call in json.loads(line)['calls']:
                    if call.get('address') and call.get('role') != 'unlabelled':
                        roles.setdefault(call['role'], set()).add(call['address'])
            if manifest.get('contracts') != {r: sorted(v) for r, v in roles.items()}:
                raise ValueError('transaction fixture role map mismatch')
            mappings = {proof['label']: proof for proof in manifest['proofs']}
            for label in sorted(required):
                calls = mappings.get(label, {}).get('transactions', [])
                if not isinstance(calls, list) or not calls:
                    errors.append('transaction call mapping missing: ' + label)
                    continue
                for call in calls:
                    function, role = call['function'], call['contract_role']
                    outcome = call.get('outcome', 'success')
                    if outcome not in ('success', 'rejected'):
                        raise ValueError('unsupported transaction mapping outcome')
                    if outcome == 'rejected' and (not isinstance(call.get('proof_case'), str) or not call['proof_case']):
                        raise ValueError('rejected transaction mapping needs an explicit proof_case: ' + label)
                    mode = call.get('account_authorization_mode')
                    indices = call.get('mismatched_argument_indices')
                    if mode == 'enforcing-mismatched-root' and (not isinstance(indices, list) or not indices or any(type(i) is not int or i < 0 for i in indices)):
                        raise ValueError('mismatched root mapping needs exact argument indices: ' + label)
                    matching = any(f == function and r == role and a in roles.get(role, set()) and o == outcome
                                   and test in mappings[label]['tests']
                                   and (outcome != 'rejected' or case == call['proof_case'])
                                   and (mode is None or auth_mode == mode)
                                   and (indices is None or tuple(indices) == mismatch_indices)
                                   and (not (call.get('archive', False) or call.get('requires_archival_restore', False)
                                             or mappings[label].get('requires_archival_restore', False)) or archived)
                                   and set(call.get('required_archived_key_names', [])) <= names
                                   for f, r, a, o, archived, test, names, case, auth_mode, mismatch_indices in seen)
                    if not matching:
                        details = [outcome]
                        if outcome == 'rejected': details.append('proof_case=' + call['proof_case'])
                        if mode is not None: details.append('auth_mode=' + str(mode))
                        if indices is not None: details.append('mismatched_argument_indices=' + str(indices))
                        errors.append(f'unsigned transaction proof missing: {label} -> {role}.{function} ({", ".join(details)})')
        except (OSError, KeyError, TypeError, ValueError) as error:
            errors.append('invalid transaction proof mapping: ' + str(error))
    return errors
