#!/usr/bin/env python3
"""Package explicitly selected local restoration evidence; never run contracts."""
import argparse
import io
import json
import sys
import tarfile
from pathlib import Path
from verify_archive import describe, digest, verify

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
PROTOTYPE_LEAVES = {
    "README.md", "build.py", "prepare.py", "run.sh", "assess.py", "check_runs.py",
    "restoration.rs", "restore-checker.py", "test_restore_checker.py", "decode-restore.cjs",
    "runtime-Cargo.lock", "combined-repayment-tests.rs", "combined-repayment-notes.md",
    "restore-notes.md", "cpu-attribution.py", "cpu-attribution.json", "cpu-attribution.md",
    "decision-notes.md", "package.py", "verify_archive.py", "postconditions.rs",
    "proof-map.md", "ci.py", "test_prepare.py", "prefix_summary.py", "test_prefix_summary.py",
}
ROOT_RUN_LEAVES = {
    "network-config.json", "preparation.jsonl", "runner.log", "runner-exit.txt",
    "journeys.jsonl", "post-resources.jsonl", "post-transactions.jsonl", "verification.json",
    "assessment.json", "commands.json", "source-index.json", "tests.log", "syntax.log", "verification-exit.txt",
}
PROFILE_RUN_LEAVES = {
    "preparation.log", "preparation.jsonl", "transactions.jsonl", "gates.json",
    "resources.jsonl", "financial.log", "preparation-gates.json", "journeys.jsonl",
}
CHECKPOINT_LEAVES = {
    "network-config.json", "run-configuration.json", "budget.log", "test-exit-code.txt",
    "source-before.json", "transactions.jsonl", "budget-exit-code.txt", "setup-profile.json",
    "proof-manifest.json", "gates.json", "tests.log", "resources.jsonl", "calls.jsonl", "profile.json",
}
PROFILES = {"internal-policy", "captured-network-diagnostic"}
HARNESS_LEAVES = {
    "Cargo.toml", "Cargo.lock", "src/test.rs", "src/repayment_fixture.rs", "src/lib.rs",
    "src/transaction_evidence.rs", "src/restoration.rs", "src/postconditions.rs",
    "src/combined-repayment-tests.rs", "src/storage.patch", "src/repayment.patch",
}
WASM_LEAVES = {
    "cl_integration_fixtures.wasm", "cl_share_pool_facade.wasm", "cl_share_vault.wasm",
    "jump_rate_model.wasm", "margin_controller.wasm", "receipt_vault.wasm",
    "simple_peridottroller.wasm", "swap_adapter.wasm",
}
RESEARCH_LEAVES = {
    "source-index.json", "soroban-simulation-29.0.1.crate", "resources-ff6ae01.rs",
    "TransactionFrame-v29.0.0.cpp", "TransactionUtils-v29.0.0.cpp", "RestoreFootprintOpFrame-v29.0.0.cpp",
    "soroban-simulation-29.0.1/Cargo.toml", "soroban-simulation-29.0.1/Cargo.lock",
    "soroban-simulation-29.0.1/.cargo_vcs_info.json", "soroban-simulation-29.0.1/src/simulation.rs",
    "soroban-simulation-29.0.1/src/resources.rs", "soroban-simulation-29.0.1/src/snapshot_source.rs",
    "soroban-simulation-29.0.1/src/network_config.rs",
}


def scoped(path):
    path = path.absolute()
    rel = path.relative_to(ROOT / "target")
    assert rel.parts and rel.parts[0].startswith("cl-restoration-"), f"out-of-scope input: {path}"
    assert not any(part in ("..", ".aws", ".env") for part in rel.parts)
    # Refuse symlink components, including links that point back inside scope.
    cursor = ROOT / "target"
    for part in rel.parts:
        cursor /= part
        assert not cursor.is_symlink(), f"symlink input: {cursor}"
    assert path.exists()
    return path


def run_allowed(rel):
    parts = rel.parts
    return (len(parts) == 1 and parts[0] in ROOT_RUN_LEAVES or
            len(parts) == 2 and parts[0] in PROFILES and parts[1] in PROFILE_RUN_LEAVES or
            len(parts) == 2 and parts[0] == "profiles" and parts[1] in {p + ".json" for p in PROFILES} or
            len(parts) == 2 and parts[0] == "checkpoint" and parts[1] in CHECKPOINT_LEAVES)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run", action="append", type=Path, required=True)
    parser.add_argument("--build", type=Path, required=True)
    parser.add_argument("--clean-build", type=Path, required=True)
    parser.add_argument("--extra", action="append", type=Path, default=[])
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    assert not args.output.exists(), "output must be fresh; existing evidence is never overwritten"
    members, sources = {}, {}

    def add(name, path):
        assert name not in members and path.is_file() and not path.is_symlink(), (name, path)
        data = path.read_bytes()
        members[name] = data
        sources[name] = path

    actual_source_leaves = {p.name for p in HERE.iterdir() if p.is_file()}
    assert actual_source_leaves <= PROTOTYPE_LEAVES, f"unreviewed prototype leaf: {actual_source_leaves - PROTOTYPE_LEAVES}"
    for leaf in sorted(actual_source_leaves):
        add("source/" + leaf, HERE / leaf)
    runs = [scoped(p) for p in args.run]
    assert len({p.name for p in runs}) == len(runs), "duplicate run name"
    for run in runs:
        for path in sorted(run.rglob("*")):
            assert not path.is_symlink(), path
            if path.is_file():
                rel = path.relative_to(run)
                assert run_allowed(rel), f"unreviewed run leaf: {rel}"
                add(f"runs/{run.name}/{rel.as_posix()}", path)
    build = scoped(args.build)
    add("build/build-manifest.json", build / "build-manifest.json")
    manifest = json.loads(members["build/build-manifest.json"])
    assert set(manifest["wasm"]) == WASM_LEAVES
    for leaf in sorted(WASM_LEAVES):
        add("build/artifacts/" + leaf, scoped(build / "artifacts" / leaf))
    for rel in sorted(HARNESS_LEAVES):
        add("build/harness/" + rel, scoped(build / "harness" / rel))
    clean = scoped(args.clean_build)
    for leaf in ("build-manifest.json", "optimization-build.log"):
        add("clean-build/" + leaf, scoped(clean / leaf))
    # Preserve exact overlaid source files and workspace lock/manifest only;
    # never include the workspace tree, target outputs or compiler caches.
    clean_manifest = json.loads(members["clean-build/build-manifest.json"])
    source_leaves = {
        "contracts/margin-controller/src/contract.rs", "contracts/margin-controller/src/perps.rs",
        "contracts/receipt-vault/src/contract.rs", "contracts/receipt-vault/src/storage.rs",
        "contracts/simple-peridottroller/src/contract.rs", "contracts/simple-peridottroller/src/storage.rs",
    }
    assert set(clean_manifest["optimized_source_hashes"]) <= source_leaves
    for rel, expected in clean_manifest["optimized_source_hashes"].items():
        path = scoped(clean / "workspace" / rel)
        assert digest(path.read_bytes()) == expected, rel
        add("clean-build/overlaid-source/" + rel, path)
    for leaf in ("Cargo.toml", "Cargo.lock"):
        add("clean-build/overlaid-source/" + leaf, scoped(clean / "workspace" / leaf))
    research = scoped(ROOT / "target/cl-restoration-research")
    for leaf in sorted(RESEARCH_LEAVES):
        add("research/" + leaf, scoped(research / leaf))
    for arg in args.extra:
        path = scoped(arg)
        assert path.suffix in {".log", ".rs", ".py", ".json", ".jsonl", ".txt", ".md", ".toml", ".lock", ".patch", ".wasm"}
        assert not any(s in path.name.lower() for s in ("signed", "secret", "credential", "private-key"))
        rel = path.relative_to(ROOT / "target").as_posix()
        add("extras/" + rel, path)
    summary = describe(members)
    index = {"schema": "cl-restoration-durable-v1", "release_approval": False,
             "packaging_command": sys.argv,
             "full_checker_replay_performed": False, "archive": {},
             "members": {k: {"bytes": len(v), "sha256": digest(v)} for k, v in sorted(members.items())},
             "source_paths": {k: str(v.relative_to(ROOT)) for k, v in sorted(sources.items())},
             "verified_results": summary,
             "separate_ci_references": ["cl-linux-ci-627e2e2-20261103", "cl-storage-ci-627e2e2-20261103"]}
    args.output.mkdir(parents=True, exist_ok=False)
    archive = args.output / "restoration-evidence.tar.xz"
    with tarfile.open(archive, "w:xz", preset=6) as tar:
        for name, data in sorted(members.items()):
            info = tarfile.TarInfo(name)
            info.size, info.mtime, info.mode = len(data), 0, 0o644
            tar.addfile(info, io.BytesIO(data))
    # Refuse a capture whose producer changed any input while packaging.
    for name, path in sources.items():
        assert digest(path.read_bytes()) == index["members"][name]["sha256"], f"input changed during packaging: {path}"
    raw = archive.read_bytes()
    index["archive"] = {"file": archive.name, "bytes": len(raw), "sha256": digest(raw)}
    (args.output / "index.json").write_text(json.dumps(index, indent=2) + "\n")
    (args.output / "verify_archive.py").write_bytes(members["source/verify_archive.py"])
    (args.output / "README.md").write_text(
        "# Preserved conditional restoration evidence\n\nStage1 incomplete; no release approval. "
        "Each member hash, WASM identity, exact per-profile gate/error report and recorded test outcome "
        "is verified by `python3 verify_archive.py` from this directory. No archive extraction, signing, "
        "contract execution or full checker replay occurs. Missing reports remain explicit failures of "
        "coverage. Preparation simulation projects TTL; it does not prove Core application, signatures "
        "or live state. Completed Linux CI remains in its two separate archives.\n")
    print(json.dumps(verify(args.output), indent=2))


if __name__ == "__main__":
    main()
