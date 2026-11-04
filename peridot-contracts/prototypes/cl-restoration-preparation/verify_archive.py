#!/usr/bin/env python3
"""Verify a restoration evidence archive without extracting or executing it."""
import argparse
import hashlib
import json
import re
import tarfile
from pathlib import Path, PurePosixPath


def digest(data):
    return hashlib.sha256(data).hexdigest()


def test_outcomes(text):
    """Handle --nocapture output between a serial test's name and result."""
    completed, pending, unmatched = [], [], []
    for line in text.splitlines():
        match = re.match(r"^test ([^\r\n]+?) \.\.\. (.*)$", line)
        if match:
            name, tail = match.groups()
            if tail.strip() in ("ok", "FAILED", "ignored"):
                completed.append((name, tail.strip()))
            else:
                pending.append(name)
        elif line.strip() in ("ok", "FAILED", "ignored"):
            if len(pending) == 1:
                completed.append((pending.pop(), line.strip()))
            else:
                unmatched.append(line.strip())
    return {"named_outcomes": completed, "unresolved_test_names": pending,
            "unattributed_terminal_outcomes": unmatched}


def describe(members):
    """Recompute identities and exact results from raw archived bytes."""
    result = {"wasm": {}, "generated_sources": {}, "clean_source": {}, "runs": {}, "research": {}}
    manifest_path = "build/build-manifest.json"
    manifest = json.loads(members[manifest_path])
    assert manifest.get("release_approval") is False
    for leaf, expected in manifest["wasm"].items():
        assert PurePosixPath(leaf).name == leaf and leaf.endswith(".wasm")
        data = members["build/artifacts/" + leaf]
        actual = {"sha256": digest(data), "bytes": len(data)}
        assert actual == expected, f"WASM manifest mismatch: {leaf}"
        result["wasm"][leaf] = actual
    actual_wasm = {p.rsplit("/", 1)[-1] for p in members if p.startswith("build/artifacts/")}
    assert actual_wasm == set(manifest["wasm"])
    clean = json.loads(members["clean-build/build-manifest.json"])
    assert clean["wasm"] == manifest["wasm"], "final artifacts differ from clean contract build"
    for rel, expected in clean["optimized_source_hashes"].items():
        actual = digest(members["clean-build/overlaid-source/" + rel])
        assert actual == expected, f"clean overlaid source mismatch: {rel}"
        result["clean_source"][rel] = actual
    for rel, expected in manifest.get("harness_generated_sources", {}).items():
        actual = digest(members["build/" + rel])
        assert actual == expected, f"generated source mismatch: {rel}"
        result["generated_sources"][rel] = actual
    assert "build/harness/Cargo.lock" in members and "build/harness/Cargo.toml" in members
    for key, data in members.items():
        parts = key.split("/")
        if parts[0] != "runs":
            continue
        name, rel = parts[1], "/".join(parts[2:])
        run = result["runs"].setdefault(name, {"gate_reports": {}, "test_logs": {}, "record_counts": {}, "exit_codes": {}})
        if rel.endswith("gates.json"):
            report = json.loads(data)
            assert isinstance(report.get("gates"), dict) and report["gates"]
            assert all(type(v) is bool for v in report["gates"].values())
            assert report.get("release_approval") is False
            for k in ("errors", "lifecycle_errors", "transaction_errors", "missing_required_flows"):
                if k in report:
                    assert isinstance(report[k], list), (key, k)
            run["gate_reports"][rel] = report  # Exact lists, including every error.
        elif rel.endswith(".log"):
            text = data.decode("utf-8", errors="replace")
            outcomes = test_outcomes(text)
            totals = re.findall(r"test result: (\w+)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out", text)
            if outcomes["named_outcomes"] or outcomes["unresolved_test_names"] or totals:
                run["test_logs"][rel] = {**outcomes, "cargo_totals": totals,
                                         "sha256": digest(data), "shared_view_log_not_additional_executions": True}
        elif rel.endswith(".jsonl"):
            rows = [json.loads(line) for line in data.splitlines() if line.strip()]
            run["record_counts"][rel] = {"rows": len(rows), "sha256": digest(data)}
        elif rel.endswith("-exit.txt") or rel.endswith("-exit-code.txt"):
            run["exit_codes"][rel] = int(data.strip())
    for run in result["runs"].values():
        required = [f"{profile}/{leaf}" for profile in ("internal-policy", "captured-network-diagnostic")
                    for leaf in ("gates.json", "preparation-gates.json")]
        run["missing_profile_reports"] = [p for p in required if p not in run["gate_reports"]]
        # Missing evidence remains explicitly missing, never inferred from exit1.
    index = json.loads(members["research/source-index.json"])
    for leaf, expected in index.items():
        data = members["research/" + leaf]
        actual = {"sha256": digest(data), "bytes": len(data)}
        assert actual == expected, f"research source mismatch: {leaf}"
        result["research"][leaf] = actual
    return result


def verify(directory):
    index = json.loads((directory / "index.json").read_text())
    assert index["schema"] == "cl-restoration-durable-v1" and index["release_approval"] is False
    assert index["archive"]["file"] == "restoration-evidence.tar.xz"
    archive = directory / index["archive"]["file"]
    raw = archive.read_bytes()
    assert len(raw) == index["archive"]["bytes"] and digest(raw) == index["archive"]["sha256"]
    members = {}
    with tarfile.open(archive, "r:xz") as tar:
        for member in tar:
            p = PurePosixPath(member.name)
            assert member.isfile() and not p.is_absolute() and ".." not in p.parts
            assert member.name not in members, f"duplicate member {member.name}"
            assert member.name in index["members"]
            data = tar.extractfile(member).read()
            assert {"bytes": len(data), "sha256": digest(data)} == index["members"][member.name]
            members[member.name] = data
    assert set(members) == set(index["members"]), "missing or extra archived member"
    summary = describe(members)
    # JSON normalization makes regex tuple/list differences irrelevant.
    assert json.loads(json.dumps(summary)) == index["verified_results"], "gate/error/test/identity summary mismatch"
    assert (directory / "verify_archive.py").read_bytes() == members["source/verify_archive.py"]
    return {"archive_sha256": digest(raw), "members": len(members), "wasm_identities": len(summary["wasm"]),
            "runs": len(summary["runs"]), "exact_gate_errors_and_test_outcomes_verified": True,
            "full_checker_replay_performed": False, "release_approval": False}


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("directory", nargs="?", type=Path, default=Path(__file__).resolve().parent)
    args = parser.parse_args()
    print(json.dumps(verify(args.directory), indent=2))
