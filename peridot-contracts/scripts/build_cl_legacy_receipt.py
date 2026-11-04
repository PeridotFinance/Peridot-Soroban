#!/usr/bin/env python3
"""Build exact pre-reserve receipt source for offline ABI-negative tests only.

No identities, signing, network or deployment. All WASM builds use Stellar CLI.
The modified standalone build manifest and original source hashes are recorded.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import re

BASE = "47858c1a13c2588970395faa2212e7b68bd33060"
ADMIN = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM"

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output-dir", required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    out = Path(args.output_dir).resolve()
    if out.exists():
        raise SystemExit(f"Refusing to overwrite legacy build: {out}")
    source = out / "source"
    source.mkdir(parents=True)
    def git(*args):
        return subprocess.check_output(["git", *args], cwd=root)
    prefix = git("rev-parse", "--show-prefix").decode().strip()
    paths = git("ls-tree", "-r", "--name-only", BASE, "--", "contracts/receipt-vault/src").decode().splitlines()
    assert paths, "historical sources missing"
    hashes = {}
    for name in paths:
        data = git("show", f"{BASE}:{prefix}{name}")
        dest = source / Path(name).relative_to("contracts/receipt-vault")
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_bytes(data)
        hashes[name] = hashlib.sha256(data).hexdigest()
    manifest_path = "contracts/receipt-vault/Cargo.toml"
    original = git("show", f"{BASE}:{prefix}{manifest_path}")
    hashes[manifest_path] = hashlib.sha256(original).hexdigest()
    sdk = re.findall(r'name = "soroban-sdk"\nversion = "([^"]+)"', (root / "Cargo.lock").read_text())
    assert sdk == ["25.3.1"], f"review historical-build SDK change: {sdk}"
    tokens_path = re.search(r'stellar-tokens = \{ path = "([^"]+)" \}', (root / "Cargo.toml").read_text())
    assert tokens_path, "workspace stellar-tokens path missing"
    tokens = (root / tokens_path.group(1)).resolve()
    manifest = original.decode().split("[dev-dependencies]")[0]
    manifest = manifest.replace('name = "receipt-vault"', 'name = "receipt-vault-legacy-abi"')
    manifest = manifest.replace('soroban-sdk = { workspace = true }', 'soroban-sdk = { version = "=25.3.1", features = ["experimental_spec_shaking_v2"] }')
    manifest = manifest.replace('stellar-tokens = { workspace = true }', f'stellar-tokens = {{ path = "{tokens}" }}')
    manifest += '\n[workspace]\n\n[profile.release]\nopt-level="z"\noverflow-checks=true\ndebug=0\nstrip="symbols"\ndebug-assertions=false\npanic="abort"\ncodegen-units=1\nlto=true\n'
    (source / "Cargo.toml").write_text(manifest)
    (source / "Cargo.lock").write_bytes((root / "Cargo.lock").read_bytes())
    env = dict(os.environ, RECEIPT_VAULT_INIT_ADMIN=ADMIN, CARGO_NET_OFFLINE="true", CARGO_TARGET_DIR=str(root / "target"))
    cmd = ["stellar", "contract", "build", "--manifest-path", str(source / "Cargo.toml"), "--out-dir", str(out / "artifacts")]
    metadata = {"schema":"cl-historical-receipt-build-v1", "source_commit":BASE, "purpose":"missing-reserve-ABI negative fixture, not deployment", "source_sha256":hashes, "modified_manifest_sha256":hashlib.sha256(manifest.encode()).hexdigest(), "command":cmd, "sdk":"25.3.1", "synthetic_init_admin":ADMIN}
    (out / "manifest.json").write_text(json.dumps(metadata, indent=2)+"\n")
    with (out / "build.log").open("w") as log:
        result = subprocess.run(cmd, cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT)
    if result.returncode:
        raise SystemExit(result.returncode)
    wasm = out / "artifacts" / "receipt_vault_legacy_abi.wasm"
    metadata["wasm_sha256"] = hashlib.sha256(wasm.read_bytes()).hexdigest()
    metadata["wasm_bytes"] = wasm.stat().st_size
    (out / "manifest.json").write_text(json.dumps(metadata, indent=2)+"\n")
    print(wasm)

if __name__ == "__main__":
    main()
