#!/usr/bin/env python3
"""Build an isolated variant; reuse compiler cache, never measured WASM outputs."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
spec = importlib.util.spec_from_file_location('storage_prepare', HERE / 'prepare.py')
prepare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prepare)
repayment = '--repayment-tests' in sys.argv
if repayment:
    sys.argv.remove('--repayment-tests')
run = subprocess.run


def build(command, **kwargs):
    if command[:3] == ['stellar', 'contract', 'build']:
        env = dict(kwargs['env'])
        env['CARGO_TARGET_DIR'] = str(ROOT / 'target/cl-storage-candidate-build/cargo-optimization')
        kwargs['env'] = env
        if command[command.index('--package') + 1] == 'margin-controller':
            command = command + ['--features', 'funded-liquidation-validation']
    return run(command, **kwargs)


prepare.subprocess.run = build
prepare.main()
out = Path(sys.argv[sys.argv.index('--output') + 1]).resolve()
manifest_path = out / 'build-manifest.json'
manifest = json.loads(manifest_path.read_text())
if repayment:
    tests = (ROOT / 'prototypes/cl-atomic-qualification/tests/combined.rs').read_text()
    tests += '\n#[path = "' + str(HERE / 'repayment-tests-v2.rs') + '"]\nmod repayment_cases;\n'
    (out / 'harness/src/repayment_fixture.rs').write_text(tests)
    test = out / 'harness/src/test.rs'
    test.write_text(test.read_text() + '\n#[path = "repayment_fixture.rs"]\nmod repayment_qualification;\n')
manifest['harness_generated_sources'] = {
    str(p.relative_to(out)): hashlib.sha256(p.read_bytes()).hexdigest()
    for p in sorted((out / 'harness/src').glob('*.rs'))}
manifest['variant_prepare_sha256'] = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
manifest['margin_build_feature'] = 'funded-liquidation-validation' if repayment else 'unchanged pinned baseline'
manifest['repayment_tests'] = repayment
manifest_path.write_text(json.dumps(manifest, indent=2) + '\n')
