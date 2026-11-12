#!/usr/bin/env python3
"""Copy a verified atomic v3 build and repair only recording-mode state projection.

No contracts are built and no tests are run. The source build is never modified.
Host29 footprint-only placeholders remain in all resource/fee/restoration data.
Effective restored TTL for a handled-failure placeholder remains a separate
full-transaction runtime gate; preserving logical data does not establish TTL.
"""
import argparse
import hashlib
import json
from pathlib import Path


PARTIAL_SHA = '9ea12fc319ca1da1f0a39a198312ac3637698d1bbb8545101240e9837ed860d1'
FULL_SHA = '719fc16c22df5587c5cc2fa166f13175a49a06b57cf3c7ab50398cb3cae2a213'
TRANSACTION = 'harness/src/transaction_evidence.rs'
CLASSIFIER = '''    // Evidence projection only, after official host execution/metering.
    // Host29.0.1 e2e_invoke::add_footprint_only_ledger_changes creates these
    // default records for handled failing subcalls. They are not deletions.
    let footprint_only_contract_placeholder = |change: &soroban_env_host::e2e_invoke::LedgerEntryChange, key: &LedgerKey| {
        matches!(key, LedgerKey::ContractData(_) | LedgerKey::ContractCode(_))
            && change.encoded_new_value.is_none()
            && change.old_entry_size_bytes_for_rent == 0
            && change.new_entry_size_bytes_for_rent == 0
            && change.ttl_change.is_none()
    };
    let mut footprint_only_projection_proofs = std::vec::Vec::<Value>::new();
'''
PROOF = '''        if footprint_only_contract_placeholder(change, &key) {
            let initial = snapshot.ledger_entries.iter().find(|(k, _)| k.as_ref() == &key);
            let rw_index = result.resources.footprint.read_write.iter().position(|k| k == &key);
            let restored = rw_index.map(|i| result.restored_rw_entry_indices.contains(&(i as u32))).unwrap_or(false);
            footprint_only_projection_proofs.push(json!({
                "key_xdr": b64(&key),
                "classification": "host29-recording-footprint-only-contract-placeholder",
                "read_only": change.read_only,
                "encoded_new_value": null,
                "old_entry_size_bytes_for_rent": change.old_entry_size_bytes_for_rent,
                "new_entry_size_bytes_for_rent": change.new_entry_size_bytes_for_rent,
                "ttl_change": null,
                "initial_entry_exists": initial.is_some(),
                "initial_entry_xdr": initial.map(|(_, value)| b64(value.0.as_ref())),
                "logical_action": "retain-initial-data-or-absence",
                "read_write_footprint_index": rw_index,
                "listed_as_restored": restored,
                "effective_restored_ttl_established": false,
                "ttl_scope": "No effective TTL inferred from placeholder; restored TTL requires independent full-transaction runtime evidence",
                "source_proof": "soroban-env-host-29.0.1/src/e2e_invoke.rs:add_footprint_only_ledger_changes; ordinary ContractData/Code changes initialize ttl_change"
            }));
        }
'''
EXPORT = '''    row["recording_projection_repair"] = json!({
        "schema": "host29-contract-footprint-placeholder-projection-v1",
        "scope": "logical-state projection only; no resource, fee, footprint, TTL or restoration-index changes",
        "placeholders": footprint_only_projection_proofs,
        "effective_restored_ttl_gate": "unresolved for handled-failure placeholders without independent full-transaction runtime evidence",
        "signature_verification_proof": false
    });
'''


def sha(data):
    return hashlib.sha256(data).hexdigest()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def pinned_projection(source, start_marker, end_marker, expected):
    require(source.count(start_marker) == 1, f'Ambiguous start: {start_marker}')
    start = source.index('    for change in &result.ledger_changes {', source.index(start_marker))
    end = source.index(end_marker, start)
    snippet = source[start:end]
    require(sha(snippet.encode()) == expected, f'Unexpected old projection: {start_marker}')
    return snippet


def repair(source):
    partial = pinned_projection(source, 'let mut post_state =',
                                '    let post_state_data_sha256', PARTIAL_SHA)
    full = pinned_projection(source, 'let mut full_after =',
                             '    let full_before_hash', FULL_SHA)
    key_line = '        let key = LedgerKey::from_xdr(&change.encoded_key, Limits::none()).unwrap();\n'
    changed_partial = partial.replace(key_line, key_line + PROOF, 1).replace(
        '} else if change.read_only {',
        '} else if change.read_only || footprint_only_contract_placeholder(change, &key) {', 1)
    changed_full = full.replace('            || change.read_only\n',
                               '            || change.read_only\n            || footprint_only_contract_placeholder(change, &key)\n', 1)
    source = source.replace(partial, CLASSIFIER + changed_partial, 1)
    source = source.replace(full, changed_full, 1)
    anchor = '    row["outcome"] = '
    require(source.count(anchor) == 1, 'Unexpected outcome export anchor')
    return source.replace(anchor, EXPORT + anchor, 1)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--build', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    build, output = args.build.resolve(), args.output.resolve()
    require(build.is_dir(), 'Missing source build')
    require(not output.exists(), 'Refusing existing output')
    require(not output.is_relative_to(build), 'Output must be separate from source build')
    require(not build.is_relative_to(output), 'Output must not contain source build')
    manifest_bytes = (build / 'build-manifest.json').read_bytes()
    manifest = json.loads(manifest_bytes)
    require(manifest.get('schema') == 'cl-atomic-qualification-build-v1', 'Unexpected build schema')
    require('recording_projection_repair' not in manifest, 'Already repaired build')
    require(manifest.get('release_approval') is False, 'Unexpected release approval')
    require(manifest.get('policy_change_implemented') is False, 'Unexpected policy mutation')
    sources = manifest['harness_generated_sources']
    require(TRANSACTION in sources, 'Unpinned transaction exporter')
    require(set(sources) == {str(p.relative_to(build)) for p in (build / 'harness/src').rglob('*') if p.is_file()},
            'Generated source set differs from manifest')
    files = {}
    for name, expected in sources.items():
        path = build / name
        require(path.resolve().is_relative_to(build / 'harness/src') and not path.is_symlink(), 'Unsafe source path')
        data = path.read_bytes()
        require(sha(data) == expected, f'Generated source hash mismatch: {name}')
        files[name] = data
    wasm = manifest['wasm']
    require(set(wasm) == {p.name for p in (build / 'artifacts').iterdir() if p.is_file()}, 'Artifact set mismatch')
    for name, pin in wasm.items():
        path = build / 'artifacts' / name
        require(path.name == name and not path.is_symlink(), 'Unsafe artifact path')
        data = path.read_bytes()
        require(sha(data) == pin['sha256'] and len(data) == pin['bytes'], f'WASM mismatch: {name}')
        files['artifacts/' + name] = data
    for name in ['harness/Cargo.toml', 'harness/Cargo.lock']:
        require(not (build / name).is_symlink(), 'Unsafe Cargo input path')
        files[name] = (build / name).read_bytes()
    require('soroban-env-host = { version = "=29.0.1"' in files['harness/Cargo.toml'].decode(), 'Unsupported host version')
    original_hashes = {name: sha(data) for name, data in files.items()}
    repaired = repair(files[TRANSACTION].decode()).encode()
    # Recheck frozen inputs before creating the fresh output. Read bytes above
    # are then copied, so a concurrent source rewrite cannot change copied data.
    for name, expected in original_hashes.items():
        require(sha((build / name).read_bytes()) == expected, f'Input changed: {name}')
    require((build / 'build-manifest.json').read_bytes() == manifest_bytes, 'Manifest changed')
    files[TRANSACTION] = repaired
    manifest['harness_generated_sources'] = {name: sha(files[name]) for name in sources}
    manifest['recording_projection_repair'] = {
        'schema': 'host29-contract-footprint-placeholder-projection-v1',
        'parent_build': str(build),
        'parent_build_manifest_sha256': sha(manifest_bytes),
        'repair_script_sha256': sha(Path(__file__).read_bytes()),
        'parent_copied_file_sha256': original_hashes,
        'copied_file_sha256': {name: sha(data) for name, data in files.items()},
        'expected_old_partial_projection_sha256': PARTIAL_SHA,
        'expected_old_full_projection_sha256': FULL_SHA,
        'only_modified_generated_source': TRANSACTION,
        'wasm_bytes_changed': False,
        'resource_fee_ttl_restoration_calculation_changed': False,
        'tests_run': False,
        'effective_restored_ttl_gate': 'Outstanding for handled-failure placeholders; logical-state retention does not establish network-installed TTL',
    }
    output.mkdir(parents=True, exist_ok=False)
    for name, data in files.items():
        path = output / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
    # No Cargo output, optimization workspace, logs or prior run evidence copied.
    (output / 'build-manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(json.dumps({'output': str(output), 'parent_manifest_sha256': sha(manifest_bytes),
                      'transaction_exporter_sha256': sha(repaired), 'tests_run': False}))


if __name__ == '__main__':
    main()
