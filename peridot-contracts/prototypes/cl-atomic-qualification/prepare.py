#!/usr/bin/env python3
"""Isolate atomic qualification; preserve the executed baseline bytes and policy."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tarfile

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
BASELINE = '9cf43082bfb57334d57dbc58cfa90ccad4d98078'
ARCHIVE_SHA = 'b1b1017bf87ec095061846c81acc913a2b4bc57bbc918034dacc4f3056152fb0'


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--optimization', type=Path)
    args = parser.parse_args()
    out = args.output.resolve()
    if out.exists():
        raise SystemExit('Refusing existing output')
    pins = json.loads((ROOT / 'prototypes/cl-split-transactions/baseline-source-hashes.json').read_text())
    for name, expected in pins.items():
        path = (ROOT / name).resolve()
        assert path.is_relative_to(ROOT) and digest(path) == expected, name
    evidence = ROOT.parent / 'docs/deployment-handoffs/evidence/cl-decisions-reserve-20261102'
    index = json.loads((evidence / 'index.json').read_text())
    archive = evidence / 'checkpoint-payload.tar.xz'
    assert digest(archive) == index['archive_sha256'] == ARCHIVE_SHA
    artifacts = out / 'artifacts'
    artifacts.mkdir(parents=True)
    # Reuse the exact measured atomic binaries. Do not trust similarly named
    # mutable Cargo output directories or build another broad baseline.
    with tarfile.open(archive, 'r:xz') as tar:
        for name, expected in index['wasm'].items():
            data = tar.extractfile('checkpoint/artifacts/' + name).read()
            assert hashlib.sha256(data).hexdigest() == expected['sha256']
            assert len(data) == expected['bytes']
            (artifacts / name).write_bytes(data)
    overlay_sources = {}
    if args.optimization:
        workspace = out / 'workspace'
        for name in pins:
            dest = workspace / name
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / name, dest)
        subprocess.run(['patch', '--batch', '-p1', '-i', str(args.optimization.resolve())], cwd=workspace, check=True)
        changed = [name for name, expected in pins.items() if digest(workspace / name) != expected]
        packages = {name.split('/')[1] for name in changed if name.startswith('contracts/')}
        assert changed and packages <= {'cl-share-vault', 'receipt-vault'}, changed
        assert all(name.startswith('contracts/') for name in changed), changed
        overlay_sources = {name: digest(workspace / name) for name in changed}
        manifest = workspace / 'Cargo.toml'
        manifest.write_text(manifest.read_text().replace('../openzeppelin-stellar-contracts', str((ROOT / '../openzeppelin-stellar-contracts').resolve())))
        env = dict(os.environ, CARGO_INCREMENTAL='0', CARGO_NET_OFFLINE='true', CARGO_TARGET_DIR=str(out / 'cargo-optimization'))
        for key in ['RECEIPT_VAULT_INIT_ADMIN', 'SIMPLE_PERIDOTTROLLER_INIT_ADMIN', 'MARGIN_CONTROLLER_INIT_ADMIN', 'SWAP_ADAPTER_INIT_ADMIN', 'JUMP_RATE_MODEL_INIT_ADMIN', 'AQUARIUS_LP_VAULT_INIT_ADMIN']:
            env[key] = 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM'
        with (out / 'optimization-build.log').open('w') as log:
            for package in sorted(packages):
                subprocess.run(['stellar', 'contract', 'build', '--manifest-path', str(manifest), '--package', package, '--out-dir', str(artifacts)], cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
    harness = out / 'harness'
    (harness / 'src').mkdir(parents=True)
    (harness / 'Cargo.toml').write_text((ROOT / 'harness/cl-runtime29/Cargo.toml').read_text().replace('name = "cl-runtime29-harness"', 'name = "cl-atomic-qualification-harness"'))
    lock = (ROOT / 'harness/cl-runtime29/Cargo.lock').read_text()
    assert lock.count('name = "cl-runtime29-harness"') == 1
    (harness / 'Cargo.lock').write_text(lock.replace('name = "cl-runtime29-harness"', 'name = "cl-atomic-qualification-harness"'))
    lib = (ROOT / 'harness/cl-runtime29/src/lib.rs').read_text().replace('../../../contracts/cl-share-pool-facade/src/test.rs', 'test.rs')
    lib = lib.replace('../../../test-support/cl_resource_limits.rs', str(ROOT / 'test-support/cl_resource_limits.rs'))
    lib = lib.replace('../../../contracts/cl-share-pool-facade/src/resource_profile.rs', str(ROOT / 'contracts/cl-share-pool-facade/src/resource_profile.rs'))
    (harness / 'src/lib.rs').write_text(lib)
    tests = (ROOT / 'contracts/cl-share-pool-facade/src/test.rs').read_text()
    tests = re.sub(r'#\[path\s*=\s*"([^"/]+\.rs)"\]', lambda m: '#[path = "' + str(ROOT / 'contracts/cl-share-pool-facade/src' / m[1]) + '"]', tests)
    tests += '\n#[path = "' + str(HERE / 'tests/combined.rs') + '"]\nmod atomic_qualification;\n'
    (harness / 'src/test.rs').write_text(tests)
    transaction = (ROOT / 'harness/cl-runtime29/src/transaction_evidence.rs').read_text()
    old = '''        let key = crate::resource_profile::contract_address(role)
            .ok_or("unknown archive fixture role")?;
        let selected = addr(e, &Address::from_str(e, &key));'''
    new = '''        // Qualification-only combined restoration: each selected contract's
        // persistent entries are expired in the disposable transaction snapshot.
        // No entry is warmed or restored outside the measured host transaction.
        let selected: std::vec::Vec<_> = role.split(',').map(|role| {
            let key = crate::resource_profile::contract_address(role)
                .expect("unknown archive fixture role");
            addr(e, &Address::from_str(e, &key))
        }).collect();'''
    assert old in transaction
    transaction = transaction.replace(old, new).replace('k.contract==selected &&', 'selected.contains(&k.contract) &&')
    # Evidence only, after official host execution and metering. Normalize only
    # executable identity for full logical-state A/B comparisons; all original
    # hashes, serialized entries and bytecode identities remain in the record.
    addition = '''    let receipt_addresses: std::vec::Vec<_> = ["cash_receipt", "share_receipt"].iter().map(|role| {
        addr(e, &Address::from_str(e, &crate::resource_profile::contract_address(role).unwrap()))
    }).collect();
    let logical = |source: &std::collections::BTreeMap<String, String>| {
        let mut normalized = std::collections::BTreeMap::<String, String>::new();
        for (key, value) in source {
            let mut data = LedgerEntryData::from_xdr_base64(value, Limits::none()).unwrap();
            if let LedgerEntryData::ContractData(ref mut entry) = data {
                if receipt_addresses.contains(&entry.contract) {
                    if let ScVal::ContractInstance(ref mut instance) = entry.val {
                        instance.executable = ContractExecutable::Wasm(Hash([0; 32]));
                    }
                }
                normalized.insert(key.clone(), b64(&data));
            }
        }
        hash(&serde_json::to_vec(&normalized).unwrap())
    };
    row["qualification_full_logical_prestate_sha256"] = json!(logical(&full_before));
    row["qualification_full_logical_poststate_sha256"] = json!(logical(&full_after));
    row["qualification_ttl_changes"] = json!(result.ledger_changes.iter().filter_map(|change| {
        change.ttl_change.as_ref().map(|ttl| json!({
            "key_xdr":b64(&LedgerKey::from_xdr(&change.encoded_key,Limits::none()).unwrap()),
            "old_live_until":ttl.old_live_until_ledger,"new_live_until":ttl.new_live_until_ledger
        }))
    }).collect::<std::vec::Vec<_>>());
'''
    assert '    Ok(row)\n}' in transaction
    transaction = transaction.replace('    Ok(row)\n}', addition + '    Ok(row)\n}')
    (harness / 'src/transaction_evidence.rs').write_text(transaction)
    manifest = {
        'schema': 'cl-atomic-qualification-build-v1', 'baseline_commit': BASELINE,
        'baseline_archive_sha256': ARCHIVE_SHA, 'baseline_source_hashes': pins,
        'optimization_sha256': digest(args.optimization) if args.optimization else None,
        'optimized_source_hashes': overlay_sources,
        'production_source_changed': False, 'policy_change_implemented': False,
        'release_approval': False,
        'wasm': {f.name: {'sha256': digest(f), 'bytes': f.stat().st_size} for f in sorted(artifacts.glob('*.wasm'))},
        'harness_generated_sources': {str(f.relative_to(out)): digest(f) for f in sorted((harness / 'src').glob('*.rs'))},
    }
    (out / 'build-manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(out)


if __name__ == '__main__':
    main()
