#!/usr/bin/env python3
"""Offline diagnostic analysis of the preserved Linux CI checkpoint; not a gate.

Reads only the hash-pinned unsigned artifact. Does not build, mutate profiles,
simulate, sign, broadcast, or turn diagnostic passes into release approval.
"""
import argparse
import base64
import collections
import hashlib
import json
from pathlib import Path, PurePosixPath
import tarfile
import zipfile

ARTIFACT_SHA = "9b01db8227a07f0b8a4bc13dfbf9b043002df79f0c6de068d126e3dd74d87d03"
SELECTED = ["deposit_underlying", "redeem", "receipt_custody_ownership_change",
            "swap_open_position_v3", "activate_open_position_v3",
            "prepare_close_position_v3", "swap_close", "finish_close",
            "funded_liquidation", "both_reward_calls_failed_funded_liquidation",
            "historical_unresolved_intervals", "repay_release", "cancel_after_swap"]


def sha(data):
    return hashlib.sha256(data).hexdigest()


class EvidenceReader:
    """Expose identical artifact-member bytes from either preserved container."""
    def __init__(self, artifact, durable_evidence):
        self.z = None
        self.members = {}
        if (artifact is None) == (durable_evidence is None):
            raise ValueError("Select exactly one artifact ZIP or durable evidence directory")
        self.provenance = {"artifact": str(artifact) if artifact else None,
                           "artifact_sha256": ARTIFACT_SHA}
        if artifact is not None:
            if sha(artifact.read_bytes()) != ARTIFACT_SHA:
                raise ValueError("Linux artifact SHA mismatch; do not silently substitute builds")
            self.z = zipfile.ZipFile(artifact)
            return
        index_path = durable_evidence / "index.json"
        index_raw = index_path.read_bytes()
        index = json.loads(index_raw)
        if (index["schema"] != "cl-linux-ci-archive-v1"
                or index["original_zip_sha256"] != ARTIFACT_SHA
                or index["commit"] != "da4e1a598b7c652c79745836417d9943e561c6f2"
                or index["release_approval"] is not False):
            raise ValueError("Durable index identity differs from preserved Linux checkpoint")
        archive = durable_evidence / "linux-ci-payload.tar.xz"
        raw = archive.read_bytes()
        if sha(raw) != index["archive_sha256"] or len(raw) != index["archive_bytes"]:
            raise ValueError("Durable archive hash/size mismatch")
        expected = index["payload_members"]
        if not expected:
            raise ValueError("Empty durable payload index")
        with tarfile.open(archive, "r:xz") as tar:
            entries = tar.getmembers()
            if len({e.name for e in entries}) != len(entries) or set(e.name for e in entries) != set(expected):
                raise ValueError("Durable archive member coverage mismatch")
            for entry in entries:
                path = PurePosixPath(entry.name)
                if not entry.isfile() or path.is_absolute() or ".." in path.parts:
                    raise ValueError("Unsafe durable archive member")
                data = tar.extractfile(entry).read()
                if len(data) != expected[entry.name]["bytes"] or sha(data) != expected[entry.name]["sha256"]:
                    raise ValueError("Durable payload hash/size mismatch: " + entry.name)
                if entry.name.startswith("artifact/"):
                    self.members[entry.name.removeprefix("artifact/")] = data
        for name in ("internal-policy", "captured-network-diagnostic"):
            summary = json.loads(self.members["cl-ci-checkpoint/"+name+"-checkpoint-summary.json"])
            if summary != index["profiles"][name]:
                raise ValueError("Durable index gate summary differs from original member")
        self.provenance.update(durable_evidence=str(durable_evidence),
            durable_index_sha256=sha(index_raw), durable_archive_sha256=sha(raw))

    def read(self, name):
        return self.z.read(name) if self.z else self.members[name]

    def namelist(self):
        return self.z.namelist() if self.z else list(self.members)

    def __enter__(self):
        return self

    def __exit__(self, *_args):
        if self.z:
            self.z.close()


def analyze(artifact=None, durable_evidence=None):
    with EvidenceReader(artifact, durable_evidence) as z:
        prefix = "cl-ci-checkpoint/"

        def read(name):
            return z.read(prefix + name)

        def rows(name):
            return [json.loads(line) for line in read(name).splitlines() if line.strip()]

        config = read("network-config.json")
        profiles = {}
        for name in ("internal-policy", "captured-network-diagnostic"):
            profile = json.loads(read(name + ".json"))
            if profile["network_sha256"] != sha(config) or profile["target_protocol"] != 29:
                raise ValueError("Profile/config/runtime mismatch")
            summary = json.loads(read(name + "-checkpoint-summary.json"))
            profiles[name] = {"limits": profile["limits"], "gates": summary["gates"],
                              "missing_required_flows": summary["missing_required_flows"],
                              "lifecycle_errors": summary["lifecycle_errors"],
                              "transaction_errors": summary["transaction_errors"]}
        policy = profiles["internal-policy"]["limits"]
        if [policy[k] for k in ("instructions", "ledger_entries", "write_entries", "mem_bytes")] != [100000000, 100, 50, 41943040]:
            raise ValueError("Expected preserved internal policy")
        resources = rows("captured-network-diagnostic-resources.jsonl")
        calls = rows("captured-network-diagnostic-calls.jsonl")
        transactions = rows("captured-network-diagnostic-transactions.jsonl")
        if not resources or not calls or not transactions:
            raise ValueError("Missing required evidence")
        for row in calls:
            if not row["exclusive_reconciles"] or sum(c["exclusive"]["instructions"] for c in row["calls"]) != row["root_instructions"]:
                raise ValueError("Non-reconciling CPU attribution")
        flow_results = {}
        for flow in SELECTED:
            rr = [r for r in resources if r["flow"] == flow and r["ok"]]
            cc = [r for r in calls if r["flow"] == flow and r["invocation_ok"]]
            if not rr or not cc:
                raise ValueError("Missing successful required diagnostic flow: " + flow)
            peak = max(cc, key=lambda r: r["root_instructions"])
            maxima = dict(instructions=max(r["instructions"] for r in rr),
                          mem_bytes=max(r["memory_bytes"] for r in rr),
                          ledger_entries=max(r["read_entries"] + r["write_entries"] + r["disk_read_entries"] for r in rr),
                          write_entries=max(r["write_entries"] for r in rr))
            by_role = collections.Counter()
            memory_by_role = collections.Counter()
            for c in peak["calls"]:
                by_role[c["role"]] += c["exclusive"]["instructions"]
                memory_by_role[c["role"]] += c["exclusive"]["memory_counter_contribution_bytes"]
            venue = [c for c in peak["calls"] if c["role"] == "aquarius"]
            # Select only outermost venue subtrees, so children are never counted twice.
            disjoint = [c for c in venue if not any(c["path"].startswith(p["path"] + ".") for p in venue if p != c)]
            mandatory = [c for c in disjoint if c["function"] in ("claim", "claim_all_position_fees", "deposit_position", "withdraw_position", "swap")]
            mandatory_cpu = sum(c["inclusive"]["instructions"] for c in mandatory)
            # An intentionally unrealistic bound: all repeat calls of a getter are
            # deleted, including reads across mutations. NOT a safe optimization.
            seen = set()
            duplicate_getters = []
            for c in peak["calls"]:
                if c["function"] != "balance" and not c["function"].startswith("get_"):
                    continue
                key = (c["address"], c["function"])
                if key in seen:
                    duplicate_getters.append(c)
                seen.add(key)
            disjoint_getters = [c for c in duplicate_getters if not any(c["path"].startswith(p["path"] + ".") for p in duplicate_getters if p != c)]
            deletion_ceiling = sum(c["inclusive"]["instructions"] for c in disjoint_getters)
            violations = {k: max(0, v-policy[k]) for k, v in maxima.items()}
            if mandatory_cpu > policy["instructions"]:
                finding = "infeasible for wrapper-only optimization with these venue subcalls preserved"
            elif any(violations.values()):
                finding = "currently fails internal limits; feasibility after optimization unknown"
            else:
                finding = "observed invocation fits these four limits; stress/release sufficiency unknown"
            flow_results[flow] = dict(sample_count=len(rr), maxima=maxima,
                required_reduction=violations, peak_cpu_roles=dict(by_role),
                peak_memory_counter_growth_roles=dict(memory_by_role),
                mandatory_unchanged_venue_subtree_cpu=mandatory_cpu,
                mandatory_venue_subtrees=[{k:c[k] for k in ("path", "function", "inclusive")} for c in mandatory],
                repeated_getter_unrealistic_deletion_ceiling=deletion_ceiling,
                repeated_getter_calls=[{k:c[k] for k in ("path", "role", "function", "inclusive")} for c in disjoint_getters],
                cpu_after_unrealistic_getter_deletion=peak["root_instructions"]-deletion_ceiling,
                finding=finding)
        wasm = {n.removeprefix(prefix): sha(z.read(n)) for n in z.namelist() if n.startswith(prefix+"artifacts/") and n.endswith(".wasm")}
        address_roles = {base64.b32decode(c["address"]+"="*((-len(c["address"]))%8))[1:33]: c["role"] for r in calls for c in r["calls"] if c["address"]}
        tx_results = {}
        for flow in ("swap_open_position_v3", "activate_open_position_v3", "liquidate_funded_v3", "swap_close_position_v3"):
            tt = [t for t in transactions if t["flow"] == flow and t["success"]]
            if not tt:
                raise ValueError("Missing complete transaction flow: "+flow)
            peak = max(tt, key=lambda t:t["resources"]["instructions"])
            roles = {}
            for kind, keys in peak["footprint"].items():
                counts = collections.Counter()
                for key in keys:
                    b = base64.b64decode(key)
                    typ = int.from_bytes(b[:4], "big")
                    label = address_roles.get(b[8:40], "unlabelled_contract") if typ == 6 else {0:"account",7:"contract_code"}.get(typ,"ledger_key_type_"+str(typ))
                    counts[label] += 1
                roles[kind] = dict(counts)
            tx_results[flow] = dict(samples=len(tt), max_instructions=max(t["resources"]["instructions"] for t in tt),
                max_unique_footprint=max(len(set(t["footprint"]["read_only_xdr"]+t["footprint"]["read_write_xdr"])) for t in tt),
                max_write_entries=max(t["resources"]["write_entries"] for t in tt),
                peak_cpu_row_footprint_roles=roles)
        archive = [t for t in transactions if t["archive"]["restoration_observed"]]
        archive_summary = dict(roles=dict(collections.Counter(t["archive"]["role"] for t in archive)),
            test_names=sorted(set(t["test_name"] for t in archive)),
            max_instructions=max(t["resources"]["instructions"] for t in archive),
            max_disk_read_bytes=max(t["resources"]["disk_read_bytes"] for t in archive),
            max_fees_stroops={k:max(t["fees"][k] for t in archive) for k in ("inclusion", "non_refundable", "refundable", "rent", "total")})
        return dict(schema="cl-resource-feasibility-v1", **z.provenance,
                    config_sha256=sha(config), protocol=29, profiles=profiles, wasm_sha256=wasm,
                    sdk_rows=len(resources), unsigned_transaction_rows=len(transactions),
                    restoration_rows=len(archive), archive_summary=archive_summary,
                    flows=flow_results, transactions=tx_results,
                    release_approval=False, new_execution_performed=False)


def markdown(result):
    lines = ["# Measured resource feasibility — preserved Linux checkpoint", "",
        "Stage 1 remains incomplete. This is offline analysis of existing protocol 29 execution, not a new acceptance run or a policy change. No transaction stages or claim-first boundaries changed.", "",
        f"Artifact SHA256: `{result['artifact_sha256']}`. Captured settings SHA256: `{result['config_sha256']}`. Source CI run 41434868065 / commit da4e1a598b7c652c79745836417d9943e561c6f2. Linux WASM hashes are separately included in the machine report; they must not be replaced by local hashes.", "",
        "## Per-flow requirements", "",
        "These are maxima across successful captured-network invocations; columns can come from different samples. Internal limits remain 100M CPU / 41,943,040 memory bytes / 100 SDK entries / 50 writes. SDK entries is read+write+disk-read, not serialized unique footprint. The original internal-profile executions fail; this table quantifies the deficit using complete captured-network executions, not truncated 100M runs.", "",
        "| Flow | CPU | Memory bytes | SDK entries | Writes | CPU reduction needed | Entry reduction needed | Write reduction needed |",
        "|---|---:|---:|---:|---:|---:|---:|---:|"]
    for flow, r in result["flows"].items():
        m,d = r["maxima"],r["required_reduction"]
        lines.append(f"| {flow} | {m['instructions']:,} | {m['mem_bytes']:,} | {m['ledger_entries']} | {m['write_entries']} | {d['instructions']:,} | {d['ledger_entries']} | {d['write_entries']} |")
    lines += ["", "## Feasibility conclusions", ""]
    for flow,r in result["flows"].items():
        lines.append(f"- `{flow}`: {r['finding']}. Preserved venue mutation/claim subtree floor at its CPU-worst sample: {r['mandatory_unchanged_venue_subtree_cpu']:,} instructions.")
    swap=result["flows"]["swap_open_position_v3"]
    lines += ["", "## Swap-open attribution and bounded optimization assessment", "",
        "Exclusive CPU by role sums exactly to the root. Inclusive subtrees include children and must not be summed unless disjoint. Memory is modeled global high-water growth with refunds, not process RSS; attributed growth is not each contract's independent peak or removable allocation. Signed ledger deltas are not additive footprint counts.", "",
        "| Role | Exclusive CPU | Attributed memory counter growth bytes |", "|---|---:|---:|"]
    for role,cpu in sorted(swap['peak_cpu_roles'].items(),key=lambda x:-x[1]):
        lines.append(f"| {role} | {cpu:,} | {swap['peak_memory_counter_growth_roles'][role]:,} |")
    lines += ["", "Required current venue mutation/claim calls (disjoint inclusive subtrees):", "",
        "| Path | Aquarius function | CPU |", "|---|---|---:|"]
    for c in swap['mandatory_venue_subtrees']:
        lines.append(f"| {c['path']} | {c['function']} | {c['inclusive']['instructions']:,} |")
    lines += ["",
        "At this measured state, fee collection plus position deposit alone exceed 100M. Keeping the present pinned venue path means making every non-Aquarius component free still cannot meet 100M. This is a conditional lower bound for this path/state, not a theorem about all conceivable equivalent venue algorithms. Fees must remain in NAV before ownership changes; simply omitting collection/valuation is not an acceptable saving.", "",
        "The three Aquarius `claim` calls are deposit, transfer and custody assignment. All three stay required, including zero-reward claims. Their CPU is not an optimization allowance.", "",
        f"An intentionally unrealistic deletion of every repeated `balance`/`get_*` call after its first occurrence would save at most {swap['repeated_getter_unrealistic_deletion_ceiling']:,} observed instructions and still leave {swap['cpu_after_unrealistic_getter_deletion']:,}. This is NOT a safe or achievable saving: arguments can differ, state changes between reads, balance checks protect donated-dust and custody invariants, and getters include guards. The detailed candidates are in JSON. A correctness-preserving cache may only reuse an identical read while all dependencies are unchanged; its achievable saving remains unmeasured.", "",
        "Other bounded investigation targets: pass an immutable config snapshot inside one contract invocation instead of deserializing it repeatedly; consolidate legacy receipt/controller configuration into bounded records; avoid redundant local NAV serialization while retaining fresh quote, slippage and donation checks. Current call instrumentation cannot isolate these inline costs. No positive savings claim is made until a before/after target-runtime replay proves all boundaries, rollback, valuation and reward conservation unchanged. Packing unrelated mutable records can increase write bytes, restoration rent, contention and data-entry size, so fewer keys alone is insufficient.", "",
        f"Measured worst swap memory headroom is {41943040-swap['maxima']['mem_bytes']:,} bytes ({100*(41943040-swap['maxima']['mem_bytes'])/41943040:.2f}%). CPU and entry cardinality bind before memory in these samples. Four/16-holder stressed pool traces do not establish arbitrary historical-state headroom. New larger-history and archive measurements must be reported separately, never inferred from this spare memory.", "",
        "## Serialized transaction footprint", "",
        "| Actual entrypoint | Max CPU | Max unique footprint | Max writes |", "|---|---:|---:|---:|"]
    for flow,t in result['transactions'].items():
        lines.append(f"| {flow} | {t['max_instructions']:,} | {t['max_unique_footprint']} | {t['max_write_entries']} |")
    lines += ["",
        "These actual ledger keys are already lower bounds on what the unchanged state layout touches; deleting repeated reads cannot delete the first access to each key. They are not immutable lower bounds after a schema/architecture redesign. Machine JSON includes read-only/read-write key counts grouped by observed contract role for each CPU-worst transaction. CPU-worst and footprint-worst transactions may differ. No subtraction of contract footprints assumes that their guards or balances are dispensable.", "",
        "## Architectural alternatives (unimplemented; limits and stages unchanged)", "",
        "1. Compact receipt/controller configuration and narrow request-local caching. Most promising for activation and custody flows whose CPU already fits but entry counts do not. Measure changed key counts and restore rent; keep all reserve/auth/valuation checks. This cannot solve the measured swap venue CPU floor by itself.",
        "2. A CL accounting/venue path that establishes exact pre-mint fee-inclusive NAV more cheaply, or a revised venue implementation that computes equivalent fee/position updates with less work. Requires a new reviewed algorithm and pinned-bytecode evidence, partial-exit/donation/tick/rounding equivalence and all three reward claims. It is an architectural dependency, not a safe deletion of today's `claim_all_position_fees` call. No such replacement is measured here.",
        "3. Redesign storage ownership between V3, receipt and vault to reduce duplicate bookkeeping while retaining transferable shares and beneficiary history. Requires explicit migration, historical reward, auth and reserve proofs; writes may move rather than disappear. Combining wrappers alone does not remove the venue cost floor.",
        "", "No additional transaction stages are proposed or implemented by this assessment. Funded repayment and permitted seizure remain atomic. Any future staged proposal would need a complete intermediate-state recovery design and separate approval.", "",
        "## Policy alternatives (not implemented or approved)", "",
        "Retaining the current architecture has measured demands above internal CPU/entries/writes but below the captured-network ceilings. Raising internal budgets is a distinct risk-policy decision; captured-network diagnostic success does not authorize it. A proposal would need justified stress bounds, safety headroom, network-config freshness, complete transaction fees/restoration/auth evidence and review. No numeric policy change is recommended as an automatic fix.", "",
        "## Existing archival baseline", "",
        f"All {result['restoration_rows']} restoration observations target vault entries in cloned controlled state across three historical test reruns. Max archived invocation CPU is {result['archive_summary']['max_instructions']:,}; max disk reads are {result['archive_summary']['max_disk_read_bytes']:,} bytes. Fee component maxima (stroops, potentially different rows): {json.dumps(result['archive_summary']['max_fees_stroops'], sort_keys=True)}. Total-fee maximum is inclusive of rent; do not add rent again. These unsigned estimates exclude real signature verification and do not prove an arbitrarily long archived history can fit. The machine report lists the exact test names.", "",
        "## Evidence limits and reproduction", "",
        f"Analyzed {result['sdk_rows']} SDK rows, {result['unsigned_transaction_rows']} unsigned transaction rows and {result['restoration_rows']} restoration observations from the preserved Linux artifact. The protocol 29 host uses captured cost coefficients; these are actual controlled-state target-runtime measurements, not protocol 25 quantities repriced with 29 fees. Exact live-state, real account-signature and production release validation remain unavailable. Internal gate summary remains all four FAIL; captured summary remains all four PASS; release approval remains false. Detailed gate errors are preserved in JSON.", "",
        "```sh", "python3 scripts/analyze_cl_feasibility.py " + ("--durable-evidence " + result["durable_evidence"] if result.get("durable_evidence") else "--artifact " + result["artifact"]) + " --output-dir target/cl-next-feasibility", "```", "",
        "The script verifies the artifact/config/profile pins, requires each reported flow and reconciles every exclusive CPU tree. It fails on missing evidence. It is an analysis tool, not a replacement for `check_budget.sh`, lifecycle proofs, transaction gates or release approval."]
    return "\n".join(lines)+"\n"


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    source=parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--artifact",type=Path)
    source.add_argument("--durable-evidence",type=Path)
    parser.add_argument("--output-dir",type=Path,default=Path("target/cl-next-feasibility"))
    args=parser.parse_args()
    result=analyze(args.artifact, args.durable_evidence)
    args.output_dir.mkdir(parents=True,exist_ok=True)
    (args.output_dir/"resource-feasibility.json").write_text(json.dumps(result,indent=2)+"\n")
    (args.output_dir/"resource-feasibility.md").write_text(markdown(result))
    print("Offline feasibility analysis written; internal policy unchanged; Stage 1 incomplete; release approval false")


if __name__ == "__main__":
    main()
