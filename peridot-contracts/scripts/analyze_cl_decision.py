#!/usr/bin/env python3
"""Offline, pinned resource decision evidence. Never generates an active profile."""
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import tarfile
from cl_resource_profiles import FIELDS, POLICY

LINUX_SHA = "3f1496b938090f43ef248c719a938da8ec7a4705d762fd70380c6dd630f8fd5f"
LOCAL_SHA = "d7edc70705e23ccd14fd841bf6471c2d8be2f29acb4f41d5517a3e268a53e612"
CONFIG_SHA = "a28fb64bb85c74f8f0d1d4bd7a0daf413ada6f603647c64f556a7905af554dcd"
FLOWS = {"deposit_underlying": "deposit_underlying", "redeem": "redeem",
    "receipt_custody_ownership_change": "transfer",
    "swap_open_position_v3": "swap_open_position_v3",
    "activate_open_position_v3": "activate_open_position_v3",
    "prepare_close_position_v3": "prepare_close_position_v3",
    "swap_close": "swap_close_position_v3", "finish_close": "finish_close_position_v3",
    "funded_liquidation": "liquidate_funded_v3", "repay_release": "release_debt_free_position_v3",
    "history_scale_64_owner_exit": "emergency_redeem",
    "history_scale_64_owner_settle": "settle_reward_segment",
    "history_scale_8_funded_fault_exit": "liquidate_funded_v3"}

def sha(b):
    return hashlib.sha256(b).hexdigest()

def require(ok, message):
    if not ok:
        raise ValueError(message)

def read_archive(directory, linux):
    index_bytes = (directory / "index.json").read_bytes()
    index = json.loads(index_bytes)
    archive = directory / ("linux-ci-payload.tar.xz" if linux else "checkpoint-payload.tar.xz")
    raw = archive.read_bytes()
    require(sha(raw) == index["archive_sha256"] == (LINUX_SHA if linux else LOCAL_SHA), "Archive pin mismatch")
    require(len(raw) == index["archive_bytes"], "Archive length mismatch")
    expected = index["payload_members" if linux else "files"]
    prefix = "artifact/cl-ci-checkpoint/" if linux else "checkpoint/"
    members = {}
    with tarfile.open(archive, "r:xz") as tar:
        entries = tar.getmembers()
        require(len(entries) == len(set(e.name for e in entries)), "Duplicate archive member")
        require(set(e.name for e in entries) == set(expected), "Incomplete indexed archive")
        for entry in entries:
            path = PurePosixPath(entry.name)
            require(entry.isfile() and not path.is_absolute() and ".." not in path.parts, "Unsafe member")
            b = tar.extractfile(entry).read()
            require(sha(b) == expected[entry.name]["sha256"] and len(b) == expected[entry.name]["bytes"], "Member pin mismatch: " + entry.name)
            if entry.name.startswith(prefix):
                members[entry.name[len(prefix):]] = b
    return index, members, sha(index_bytes)

def maximum(rows, getters):
    require(bool(rows), "Missing required measurement rows")
    return {k: max(f(r) for r in rows) for k, f in getters.items()}

SDK = {"instructions": lambda r:r["instructions"], "memory_bytes":lambda r:r["memory_bytes"],
    "sdk_entry_total":lambda r:r["read_entries"]+r["disk_read_entries"]+r["write_entries"],
    "write_entries":lambda r:r["write_entries"]}
TX = {"instructions":lambda r:r["resources"]["instructions"],
    "memory_bytes":lambda r:r["resources"]["memory_bytes"],
    "unique_footprint_keys":lambda r:len(set(r["footprint"]["read_only_xdr"]+r["footprint"]["read_write_xdr"])),
    **{k:(lambda r,k=k:r["resources"][k]) for k in ("write_entries", "disk_read_entries", "disk_read_bytes", "write_bytes", "transaction_size_bytes")},
    "authorization_size_bytes":lambda r:r["authorization_size_bytes"],
    **{"fee_"+k:(lambda r,k=k:r["fees"][k]) for k in ("total", "rent", "refundable", "non_refundable", "inclusion")}}

def analyze(directory, linux):
    index, members, index_sha = read_archive(directory, linux)
    require(sha(members["network-config.json"]) == CONFIG_SHA, "Network config mismatch")
    profiles = {}
    for name in ("internal-policy", "captured-network-diagnostic"):
        p = json.loads(members[name+".json"])
        s = json.loads(members[name+"-checkpoint-summary.json"])
        require(p["network_sha256"] == CONFIG_SHA and p["target_protocol"] == 29, "Invalid profile config")
        # Compare all ceilings, not only the headline CPU/entry/write numbers.
        config = json.loads(members["network-config.json"])
        captured = {k: config["settings"][setting]["values"][field] if field else config["settings"][setting]["values"] for k,(setting,field) in FIELDS.items()}
        expected_limits = {k:min(v,captured[k]) for k,v in POLICY.items()} if name == "internal-policy" else captured
        require(p["limits"] == expected_limits and p["mode"] == "acceptance", "Changed profile limits or acceptance mode")
        require(s["release_approval"] is False, "Unexpected release approval")
        require(not s["missing_required_flows"], "Missing required flows")
        expected = name == "captured-network-diagnostic"
        require(s["gates"] == dict.fromkeys(("invocation", "lifecycle", "transaction", "checkpoint"), expected), "Unexpected per-profile gates")
        require((len(s["lifecycle_errors"]),len(s["transaction_errors"])) == ((0,0) if expected else (97,284)), "Unexpected profile errors")
        profiles[name] = {"limits":p["limits"], "gates":s["gates"], "lifecycle_errors":s["lifecycle_errors"], "transaction_errors":s["transaction_errors"]}
    def rows(suffix):
        return [json.loads(l) for l in members["captured-network-diagnostic-"+suffix+".jsonl"].splitlines() if l.strip()]
    rr, cc, tt = rows("resources"), rows("calls"), rows("transactions")
    require((len(rr),len(cc),len(tt)) == (1490,1490,2562), "Incomplete compiled evidence")
    for r in rr+tt:
        require(r["config_sha256"] == CONFIG_SHA and r["runtime_protocol"] == 29 and r["exact_captured_cost_model"] is True and r["run_mode"] == "acceptance" and r["sdk_limits_enforced"] is True, "Wrong measurement runtime/configuration")
    for r in cc:
        require(r["exclusive_reconciles"] and sum(c["exclusive"]["instructions"] for c in r["calls"]) == r["root_instructions"], "Non-reconciling CPU tree")
    successful = [r for r in tt if r["success"]]
    flows = {}
    for flow, txflow in FLOWS.items():
        sdk = [r for r in rr if r["flow"] == flow and r["ok"]]
        # Transaction labels are ABI entrypoints, not SDK proof labels. These are
        # explicitly separate populations, never an assertion of paired samples.
        tx = [r for r in successful if r["flow"] == txflow]
        calls = [r for r in cc if r["flow"] == flow and r["invocation_ok"]]
        require(bool(calls), "Missing call evidence: "+flow)
        peak = max(calls,key=lambda r:r["root_instructions"])
        venue = [c for c in peak["calls"] if c["role"] == "aquarius"]
        outer = [c for c in venue if not any(c["path"].startswith(p["path"]+".") for p in venue if p != c)]
        mandatory = [c for c in outer if c["function"] in ("claim","claim_all_position_fees","deposit_position","withdraw_position","swap")]
        flows[flow] = {"sdk_samples":len(sdk), "sdk_maxima":maximum(sdk,SDK),
            "transaction_entrypoint":txflow,"transaction_samples":len(tx), "unsigned_transaction_maxima":maximum(tx,TX),
            "venue_preserved_subtrees_cpu":sum(c["inclusive"]["instructions"] for c in mandatory),
            "venue_subtrees":[{"path":c["path"],"function":c["function"],"instructions":c["inclusive"]["instructions"]} for c in mandatory]}
    restored = [r for r in successful if r["archive"]["restoration_observed"]]
    return {"source":str(directory),"index_sha256":index_sha,"archive_sha256":index["archive_sha256"],
        "profiles":profiles,"wasm_sha256":{n:sha(b) for n,b in members.items() if n.endswith(".wasm")},
        "sdk_rows":len(rr),"transaction_rows":len(tt),"successful_transactions":len(successful),
        "restored_transactions":len(restored),"sdk_maxima":maximum([r for r in rr if r["ok"]],SDK),
        "unsigned_transaction_maxima":maximum(successful,TX),"restored_transaction_maxima":maximum(restored,TX),"flows":flows}

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("--linux-evidence",type=Path,required=True)
    p.add_argument("--local-evidence",type=Path,required=True)
    p.add_argument("--output",type=Path,required=True)
    a=p.parse_args()
    builds={"linux_ac63255":analyze(a.linux_evidence,True),"macos_ac63255":analyze(a.local_evidence,False)}
    maxima={}
    for build in builds.values():
        for kind in ("sdk_maxima","unsigned_transaction_maxima"):
            for k,v in build[kind].items(): maxima[k]=max(maxima.get(k,0),v)
    proposed={"instructions":300000000,"memory_bytes":32*1024*1024,"sdk_entry_total":300,"unique_footprint_keys":300,"write_entries":100}
    network={"instructions":400000000,"memory_bytes":41943040,"unique_footprint_keys":400,"write_entries":200}
    result={"schema":"cl-resource-decision-analysis-v1","release_approval":False,"policy_change_implemented":False,
        "scope":"Unsigned controlled-state evidence; SDK populations and unsigned ABI entrypoint populations are separate; independent metric maxima need not co-occur.",
        "builds":builds,"combined_observed_maxima":maxima,
        "proposal_analysis_only":{k:{"candidate_ceiling":v,"observed":maxima[k],"workload_ceiling_with_20_percent_reserve":v*4//5,
            "remaining_before_workload_ceiling":v*4//5-maxima[k],"captured_network_ceiling":network.get(k),
            "candidate_network_reserve_percent":100*(network[k]-v)/network[k] if k in network else None} for k,v in proposed.items()},
        "unmeasured_gates":["Real account and contract-wallet signature/authentication execution and signed envelope costs", "Live state", "Crossproduct of maximum ownership history, repeated outages, archival restoration, stressed ranges/ticks and complete lifecycle", "Explicit admitted pool and workload bounds"]}
    a.output.parent.mkdir(parents=True,exist_ok=True)
    a.output.write_text(json.dumps(result,indent=2)+"\n")
    print(json.dumps({"output":str(a.output),"sha256":sha(a.output.read_bytes()),"analysis_verified":True,"release_approval":False}))

if __name__ == "__main__": main()
