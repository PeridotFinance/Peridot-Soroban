#!/usr/bin/env python3
"""Offline, hash-pinned operating-cost study; no RPC or contract execution."""
import argparse
import base64
from collections import Counter, defaultdict
from decimal import Decimal
import hashlib
import json
from pathlib import Path
import struct
import sys
import unittest

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
PINS = {
    "target/cl-restoration-final-v7-liquidations/journeys.jsonl": "e66ec0e1f486d84a5914cb0097c5b5767d66edacf788b13e53a5cb709df72a45",
    "target/cl-restoration-final-v7-liquidations/checkpoint/transactions.jsonl": "391977acecaa7676246464219ef9882129d02216552bcaa24da5f4997a8a10e6",
    "target/cl-restoration-final-v7-pending/journeys.jsonl": "99d246ceb6a8165cb5ba399247888e81784bdc0a43b36259877cf823836901ed",
    "target/cl-restoration-final-v7-pending/checkpoint/transactions.jsonl": "229b788832516a120d0ace1ba7507921814f09ada05395e89ea44dcb1cc5f966",
    "target/cl-restoration-final-v7-open-close/journeys.jsonl": "82c58f26c81dd7caf3cf14953ba8d81b202b0ab490c80e1c1c474f0217a79fdd",
    "target/cl-restoration-final-v7-open-close/checkpoint/transactions.jsonl": "09760981f85a52d499ed3b56dabe87b15455bc6f1135b1de4126dd87968a6bc5",
    "target/cl-restoration-final-v7-combined/journeys.jsonl": "dee584994f5f700346012ff6666895da9a228cd3039864e35a507e454847af96",
    "target/cl-restoration-final-v7-combined/checkpoint/transactions.jsonl": "c3fc3ad8bf5ab883ffbd19d7179fad5fb61c55702c4cab1c107f7823d2751f9e",
    "scripts/cl_transaction_evidence.py": "2e2de8ba8f862cfbd16ebbe4b846ead8da79f9e7f454c4f5e020cdbd4ea83c68",
}
CONFIG_SHA = "4102b600e1e910b88b80efb772be4e2d99ad24bf624de1e57250e24daff4f74a"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def read(path):
    data = (ROOT / path).read_bytes()
    if sha(data) != PINS[path]:
        raise ValueError("changed preserved input: " + path)
    return data


def rows(path):
    return [json.loads(line) for line in read(path).splitlines() if line.strip()]


def ceil_div(n, d):
    assert n >= 0 and d > 0
    return (n + d - 1) // d


def kind(key):
    """Only classify the validated LedgerKey prefix, not a generic XDR parser."""
    raw = base64.b64decode(key, validate=True)
    tag = struct.unpack(">I", raw[:4])[0]
    if tag == 7:
        assert len(raw) == 36
        return "code"
    assert tag == 6 and len(raw) >= 48 and struct.unpack(">I", raw[4:8])[0] == 1
    return "instance" if struct.unpack(">I", raw[40:44])[0] == 20 else "data"


def payload_rent(size, ledgers, code, fee):
    charge = ceil_div(size * ledgers * fee["fee_per_rent_1kb"], 1024 * fee["persistent_rent_rate_denominator"])
    return ceil_div(charge, 3) if code else charge


def native_authorizer_marker(raw, key, authorizers):
    """Recognize the exact SDK native marker, never treat it as deployed WASM."""
    if kind(key) != "code" or raw[8:12] != bytes(4):
        return None
    size = int.from_bytes(raw[44:48], "big")
    code = raw[48:48 + size]
    prefix = b"test_contract_for_contract_id_"
    if not code.startswith(prefix):
        return None
    assert len(code) == size and sha(code) == base64.b64decode(key)[4:].hex()
    address = code[len(prefix):].decode("ascii")
    assert address in authorizers, "native marker is not a declared external authorizer"
    return address


def extension_estimate(entries, ledgers, fee):
    """Unexecuted fixed-size persistent TTL extension rent only, incl TTL writes."""
    assert ledgers > 0
    payload = sum(payload_rent(e["rent_size_bytes"], ledgers, e["kind"] == "code", fee) for e in entries)
    ttl = len(entries) * fee["fee_per_write_entry"] + ceil_div(48 * len(entries) * fee["fee_per_write_1kb"], 1024)
    return {"additional_ledgers": ledgers, "entries": len(entries), "payload_rent_stroops": payload,
            "ttl_write_overhead_stroops": ttl, "rent_component_stroops": payload + ttl,
            "complete_transaction_fee_stroops": None, "executed": False,
            "excluded": ["non-refundable operation fees", "inclusion/surge", "real signature costs", "entry growth", "future fee changes"]}


def xlm(stroops):
    return f"{Decimal(stroops) / 10_000_000:.7f}"


def span(values):
    values = list(values)
    return [min(values), max(values)]


def main():
    read("scripts/cl_transaction_evidence.py")
    sys.path.insert(0, str(ROOT / "scripts"))
    from cl_transaction_evidence import compute_fees
    table, shared, code_scenarios, wallet = [], {}, {}, {}
    evaluated_batches = 0
    for group in ("liquidations", "pending", "open-close", "combined"):
        prefix = f"target/cl-restoration-final-v7-{group}/"
        journeys = [r for r in rows(prefix + "journeys.jsonl") if r["profile"] == "captured-network-diagnostic"]
        flowset = {j["operation"] for j in journeys}
        warm = [r for r in rows(prefix + "checkpoint/transactions.jsonl")
                if r.get("flow") in flowset and r.get("success") is True and r.get("archive", {}).get("enabled") is False]
        assert len(journeys) == len(warm)
        keymap, observations = defaultdict(list), defaultdict(list)
        for ordinal, (j, w) in enumerate(zip(journeys, warm)):
            assert j["config_sha256"] == w["config_sha256"] == CONFIG_SHA
            assert j["operation"] == w["flow"]
            assert j["final_transaction"]["host_function_sha256"] == w["host_function_sha256"]
            assert j["final_transaction"]["success"] is True
            assert j["final_transaction"]["restored_rw_entry_indices"] == []
            assert j["journey_fee_total"] == j["preparation_fee_total"] + j["final_fee"]
            assert j["final_fee"] == j["final_transaction"]["fees"]["total"]
            assert j["preparation_fee_total"] == sum(b["fees"]["total"] for b in j["preparation"])
            keys = []
            for b in j["preparation"]:
                fee = compute_fees({**b["resources"], "contract_events_size_bytes": 0}, b["fee_inputs"],
                                   b["rent_changes"], b["rent_fee_inputs"], b["current_ledger"], b["fees"]["inclusion"])
                assert fee == b["fees"], "recorded restoration fees differ from pinned host formula"
                evaluated_batches += 1
                assert len(b["keys"]) == len(b["rent_changes"])
                for k, change in zip(b["keys"], b["rent_changes"]):
                    assert k["new_ttl"] == b["current_ledger"] + 2_073_600 - 1
                    assert change["old_size_bytes"] == change["old_live_until_ledger"] == 0
                    assert change["new_size_bytes"] == k["rent_size_bytes"]
                    category = kind(k["key_xdr"])
                    assert change["is_code_entry"] == (category == "code")
                    raw_entry = base64.b64decode(k["entry_xdr"], validate=True)
                    observation = {"borrower_probe_ordinal": ordinal, "kind": category,
                                   "entry_sha256": sha(raw_entry),
                                   "synthetic_native_authorizer": native_authorizer_marker(raw_entry, k["key_xdr"], j["final_transaction"]["expected_external_authorizers"]),
                                   "rent_size_bytes": k["rent_size_bytes"], "raw_entry_bytes": len(base64.b64decode(k["entry_xdr"])),
                                   "payload_rent_stroops": payload_rent(k["rent_size_bytes"], 2_073_600, category == "code", b["rent_fee_inputs"])}
                    keymap[k["key_xdr"]].append(observation)
                    keys.append(k["key_xdr"])
            assert len(keys) == len(set(keys)) == j["required_archived_keys"]
            observations[j["operation"]].append({"ordinal": ordinal, "warm_fees": w["fees"],
                "preparation_fee_stroops": j["preparation_fee_total"], "preparation_rent_stroops": j["preparation_rent_total"],
                "post_preparation_fees": j["final_transaction"]["fees"], "cold_journey_fee_stroops": j["journey_fee_total"],
                "batches": j["preparation_transactions"], "keys": len(keys), "host_function_sha256": w["host_function_sha256"],
                "cold_snapshot_sha256": j["cold_snapshot_sha256"], "warm_snapshot_sha256": w["snapshot_sha256"],
                "limits_fit": j["limits_fit"]})
        for flow, values in observations.items():
            table.append({"fixture_family": group, "operation": flow, "samples": len(values),
                "warm_total_stroops": span(v["warm_fees"]["total"] for v in values),
                "one_cold_preparation_stroops": span(v["preparation_fee_stroops"] for v in values),
                "one_cold_post_preparation_stroops": span(v["post_preparation_fees"]["total"] for v in values),
                "one_cold_total_stroops": span(v["cold_journey_fee_stroops"] for v in values), "observations": values})
        if group not in ("liquidations", "pending"):
            continue
        assert len(journeys) == 8
        classified = []
        for key, versions in sorted(keymap.items()):
            classes = {v["kind"] for v in versions}
            assert len(classes) == 1
            category = next(iter(classes))
            distinct = sorted({v["entry_sha256"] for v in versions})
            shared_key = len(versions) > 1
            if category == "code":
                assert len(distinct) == 1, "code entry differs; do not grant an immutable reuse credit"
            classified.append({"key_xdr": key, "kind": category, "occurrences": len(versions),
                "entry_hashes": distinct, "shared": shared_key, "observed_mutation": len(distinct) > 1,
                "immutable_reuse_candidate": category == "code" and shared_key,
                "mutable_reuse_savings_credited": 0, "observations": versions})
        count = lambda pred: dict(Counter(k["kind"] for k in classified if pred(k)))
        shared[group] = {"eight_independent_cold_probes": True, "joint_snapshot_or_journey_executed": False,
            "total_key_occurrences": sum(k["occurrences"] for k in classified), "unique_keys": len(classified),
            "unique_by_kind": count(lambda k: True), "shared_by_kind": count(lambda k: k["shared"]),
            "shared_with_different_bytes": count(lambda k: k["shared"] and k["observed_mutation"]),
            "keys": classified}
        code = [k for k in classified if k["kind"] == "code"]
        first = [k["observations"][0] for k in code]
        native = [v for v in first if v["synthetic_native_authorizer"]]
        non_native = [v for v in first if not v["synthetic_native_authorizer"]]
        avoided_payload = sum(sum(v["payload_rent_stroops"] for v in k["observations"][1:]) for k in code
                              if not k["observations"][0]["synthetic_native_authorizer"])
        fee = journeys[0]["preparation"][0]["rent_fee_inputs"]
        code_scenarios[group] = {"scope": "unchanged byte-identical code remains live between probes; NOT an executed shared-restoration journey",
            "unique_code_entries": len(code), "unique_code_serialized_bytes": sum(v["raw_entry_bytes"] for v in first),
            "unique_code_rent_size_bytes": sum(v["rent_size_bytes"] for v in first),
            "code_payload_rent_once_stroops": sum(v["payload_rent_stroops"] for v in first),
            "synthetic_native_authorizer_code_entries": len(native),
            "synthetic_native_authorizer_unique_payload_rent_stroops": sum(v["payload_rent_stroops"] for v in native),
            "non_native_code_entries_including_controlled_fixtures": len(non_native),
            "non_native_unique_payload_rent_stroops": sum(v["payload_rent_stroops"] for v in non_native),
            "repeated_code_payload_rent_avoidable_if_reused_stroops": avoided_payload,
            "mutable_state_savings_stroops": 0,
            "maintenance_estimate_scope": "non-native-marker code only; still includes compiled controlled-state fixtures, not a production registry",
            "unexecuted_code_maintenance_rent_estimates": [extension_estimate(non_native, n, fee) for n in (17_280, 518_400, 2_073_600)],
            "fee_inputs": fee}
        values = next(v for v in table if v["fixture_family"] == group)["observations"]
        warm_max = max(v["warm_fees"]["total"] for v in values)
        cold_max = max(v["cold_journey_fee_stroops"] for v in values)
        wallet[group] = {
            "scope": "planning arithmetic from observed maxima, no guarantee, signatures/account reserve/surge/retries excluded",
            "one_warm_fee_max_stroops": warm_max, "observed_eight_warm_fee_sum_stroops": sum(v["warm_fees"]["total"] for v in values),
            "eight_warm_maxima_allowance_stroops": 8 * warm_max,
            "one_cold_plus_seven_warm_maxima_allowance_stroops": cold_max + 7 * warm_max,
            "one_cold_plus_seven_warm_with_25pct_fee_contingency_stroops": ceil_div((cold_max + 7 * warm_max) * 125, 100),
            "eight_independent_cold_probe_sum_NOT_lifecycle_stroops": sum(v["cold_journey_fee_stroops"] for v in values),
            "joint_eight_borrower_cold_lifecycle_cost_stroops": None}
    result = {"schema": "cl-option1-offline-cost-study-v1", "inputs_sha256": PINS,
        "script_sha256": sha(Path(__file__).read_bytes()), "network_config_sha256": CONFIG_SHA,
        "raw_units": "stroops; 10,000,000 stroops/XLM", "release_approval": False,
        "new_execution": False, "live_state": False, "signed_costs": False, "core_application": False,
        "restoration_batches_fee_reconciled": evaluated_batches, "flow_costs": table,
        "shared_dependencies": shared, "immutable_code_reuse_and_maintenance": code_scenarios,
        "operating_balance_scenarios": wallet}
    HERE.mkdir(parents=True, exist_ok=True)
    (HERE / "cost-study.json").write_text(json.dumps(result, indent=2) + "\n")
    md = ["# Conditional XLM operating-cost study", "",
        "Stage1 remains incomplete. This offline study changes no resource policy, contract, cap or production configuration. It reuses finalv7 controlled-state evidence, preserves failed qualification gates and adds no execution or signature proof. All figures are XLM converted from exact raw stroops; no fiat price is assumed.", "",
        "## Warm roots versus one cold preparation", "",
        "Warm is the original checkpoint invocation with live state. Post-preparation is a different invocation after projecting minimum restored TTL. It may pay additional rent to extend that TTL again. Pairing is checked by exact host-function hash and operation order. State and any controlled intervening trade still differ: the fee delta is not claimed to isolate restoration alone. These columns must not be conflated.", "",
        "| Fixture / operation | Samples | Warm fee range XLM | One-cold preparation range XLM | Immediate post-preparation range XLM | One-cold total range XLM |",
        "|---|---:|---:|---:|---:|---:|"]
    for row in table:
        ranges = ["–".join(xlm(n) for n in row[key]) for key in ("warm_total_stroops", "one_cold_preparation_stroops", "one_cold_post_preparation_stroops", "one_cold_total_stroops")]
        md.append(f'| {row["fixture_family"]} / {row["operation"]} | {row["samples"]} | ' + " | ".join(ranges) + " |")
    md += ["", "One-cold rows are independent root probes, not successive operations in a single restored lifecycle. Summing every cold open/activation/close or every borrower cold probe would repeatedly charge shared code and mutable dependencies. The actual joined lifecycle cost is not measured. Preparation simulation does not apply Core transactions; its projected TTL and unsigned fees are conditional evidence only.", "",
        "## Eight borrowers: overlap is not permission to reuse stale mutable state", "",
        "| Fixture | Key occurrences | Unique code / instance / data | Shared code / instance / data | Shared differing instance / data |",
        "|---|---:|---|---|---|"]
    for group, v in shared.items():
        fmt = lambda d, kinds: " / ".join(str(d.get(k, 0)) for k in kinds)
        md.append(f'| {group} | {v["total_key_occurrences"]} | {fmt(v["unique_by_kind"], ("code","instance","data"))} | {fmt(v["shared_by_kind"], ("code","instance","data"))} | {fmt(v["shared_with_different_bytes"], ("instance","data"))} |')
    md += ["", "The per-key JSON retains exact key XDR, entry hashes, observed versions, modeled rent sizes and borrower-probe ordinals. All repeated code entries have identical bytes and a content-addressed code key. Their restoration can be reused only while that same code remains live and required. Instance and data keys are mutable: even identical observed bytes do not establish a safe joint snapshot, and differing bytes explicitly disprove it. This study grants no mutable-state savings. Do not construct a merged snapshot by picking an arbitrary version of a shared key.", "",
        "Synthetic-account caveat: native authorizer markers may occur in every old financial fixture prefix, including open/close; this is not limited to the two eight-borrower groups analyzed below. The SDK snapshot contains native external-authorizer markers encoded as86-byte ASCII `test_contract_for_contract_id_<address>`, not deployable WASM. Their hashes and declared external-authorizer addresses are independently matched here. Each such code entry contributes7.5670556XLM of modeled minimum-TTL payload rent. Liquidation reuses one marker; pending recovery has eight different borrower markers. Measured controlled-state totals retain these costs, but a production G-account would not use these markers. Real account/sequence/signature costs are unmeasured, so this study does not silently subtract the markers and advertise a production price. Maintenance estimates below exclude native markers and still include other compiled controlled-state fixtures.", ""]
    for group, v in code_scenarios.items():
        md.append(f'{group}: the {v["unique_code_entries"]} unique code entries have {v["unique_code_serialized_bytes"]:,} serialized bytes and {v["unique_code_rent_size_bytes"]:,} modeled rent-size bytes. One minimum-TTL code payload rent is {xlm(v["code_payload_rent_once_stroops"])}XLM, comprising {xlm(v["non_native_unique_payload_rent_stroops"])}XLM for {v["non_native_code_entries_including_controlled_fixtures"]} non-native code entries and {xlm(v["synthetic_native_authorizer_unique_payload_rent_stroops"])}XLM for {v["synthetic_native_authorizer_code_entries"]} native authorizer markers. Avoiding repeated non-native identical-code restoration across these eight probes could avoid {xlm(v["repeated_code_payload_rent_avoidable_if_reused_stroops"])}XLM of code payload rent under the stated live-code assumption. This is unexecuted arithmetic, not a measured eight-borrower recovery saving; TTL writes, envelope fees and mutable costs are not deducted.')
    md += ["", "## Proactive maintenance estimate, not a measured operation", "",
        "The pinned host29 fee formula uses observed rent-size bytes, captured rent rate1000 stroops/KB, persistent denominator1215 and the code discount factor3. TTL metadata writes are1250 stroops/entry plus437 stroops/KB for48 bytes/entry. Code rent size includes the host's modeled module size; raw WASM bytes are not substituted. Estimate only the incremental ledger extension of unchanged code already live, with no size growth or re-upload.", "",
        "| Dependency set | Added ledgers | Rent + TTL-write component XLM | Complete operation fee |", "|---|---:|---:|---|"]
    for group, value in code_scenarios.items():
        for row in value["unexecuted_code_maintenance_rent_estimates"]:
            md.append(f'| {group} code | {row["additional_ledgers"]:,} | {xlm(row["rent_component_stroops"])} | UNMEASURED |')
    md += ["", "At an assumed5-second ledger cadence only,17,280/518,400/2,073,600 ledgers correspond to1/30/120 days; no actual monthly schedule or latency is measured. Extending too frequently repeatedly pays transaction/TTL overhead. Maintenance does not make long-duration rent free: it may avoid emergency restoration, code parsing and latency, but still pays for additional byte-ledgers. A real unsigned ExtendFootprintTTL simulation, exact envelope/batches, source-account reserve and signatures are needed before choosing a cadence or monthly allowance.", "",
        "## Conditional operating-wallet scenarios", "",
        "These are fee-planning arithmetic, not safe operating caps. One-cold-plus-seven-warm assumes the prepared dependencies stay live and subsequent calls have costs no worse than the recorded warm maximum. That combined sequence is unexecuted, mutable dependencies and new owners may require extra restoration, and account minimum balance, fee escalation, retries and real signatures are excluded.", "",
        "| Eight-borrower scenario | Liquidations XLM | Pending recoveries XLM |", "|---|---:|---:|"]
    for label, key in [("Observed eight warm root fees", "observed_eight_warm_fee_sum_stroops"),
                       ("Eight times observed warm maximum", "eight_warm_maxima_allowance_stroops"),
                       ("One cold + seven warm maxima (unexecuted)", "one_cold_plus_seven_warm_maxima_allowance_stroops"),
                       ("Same +25% fee contingency (unexecuted)", "one_cold_plus_seven_warm_with_25pct_fee_contingency_stroops")]:
        md.append("| " + label + " | " + " | ".join(xlm(wallet[g][key]) for g in ("liquidations", "pending")) + " |")
    md += ["", "Economic recommendation: investigate shared-code TTL maintenance and a measured sequential eight-borrower recovery before funding every position as independently cold. Keep a separately earmarked emergency XLM allowance for one actual cold incident; the above scenario is a starting estimate with explicit exclusions, not an approved required balance. Refresh fees and snapshot immediately before any eventual submission. Do not substitute speculative shared-state savings for complete recovery capability.", "",
        "XLM operating/restore funds are distinct from supplier withdrawal cash, dedicated loss reserves and the liquidator's underlying debt-repayment capital. No dollar conversion, loss-seed credit, reward/quarantine appropriation or subscription is included. A cheaper operation does not cure price/authorization/resource failures or establish that a full-range/wide pool is profitable. Failed included transactions also spend fees; their rates/frequency and a sustainable keeper cadence remain unqualified.", "",
        "Reproduce: `python3 prototypes/cl-option1-qualification/cost-study.py`; focused checks: `python3 prototypes/cl-option1-qualification/cost-study.py --self-test`. The script fails on changed source hashes, unmatched calls, duplicated keys or unreconciled recorded fees. It rewrites only its sibling JSON/Markdown. Fee formulas trace to pinned `soroban-env-host29.0.1/src/fees.rs`, already used by `scripts/cl_transaction_evidence.py`; no current-network fee claim is made.", ""]
    (HERE / "cost-study.md").write_text("\n".join(md))
    print(json.dumps({"paired_financial_roots": sum(t["samples"] for t in table), "reconciled_restore_batches": evaluated_batches,
                      "new_executions": 0, "release_approval": False}))


class FocusedChecks(unittest.TestCase):
    def test_span_generator(self):
        self.assertEqual(span(v for v in [3, 1, 8]), [1, 8])

    def test_code_key(self):
        self.assertEqual(kind(base64.b64encode(struct.pack(">I", 7) + b"x" * 32)), "code")

    def test_instance_key(self):
        self.assertEqual(kind(base64.b64encode(struct.pack(">II", 6, 1) + b"x" * 32 + struct.pack(">II", 20, 1))), "instance")

    def test_unknown_key_fails(self):
        with self.assertRaises(AssertionError):
            kind(base64.b64encode(struct.pack(">I", 0) + b"x" * 40))

    def test_rounding_discount(self):
        self.assertEqual(payload_rent(1024, 1215, True, {"fee_per_rent_1kb": 1000, "persistent_rent_rate_denominator": 1215}), 334)

    def test_fee_is_not_complete(self):
        r = extension_estimate([{"kind": "code", "rent_size_bytes": 1024}], 1215,
            {"fee_per_rent_1kb": 1000, "persistent_rent_rate_denominator": 1215, "fee_per_write_entry": 1250, "fee_per_write_1kb": 437})
        self.assertEqual(r["rent_component_stroops"], 1605)
        self.assertIsNone(r["complete_transaction_fee_stroops"])
        self.assertFalse(r["executed"])

    def test_native_marker_requires_declared_actor(self):
        address = "C" + "A" * 55
        code = b"test_contract_for_contract_id_" + address.encode()
        key = base64.b64encode(struct.pack(">I", 7) + bytes.fromhex(sha(code)))
        raw = bytes(4) + struct.pack(">II", 7, 0) + bytes.fromhex(sha(code)) + struct.pack(">I", len(code)) + code
        self.assertEqual(native_authorizer_marker(raw, key, [address]), address)
        with self.assertRaises(AssertionError):
            native_authorizer_marker(raw, key, [])


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        unittest.main(argv=[sys.argv[0]])
    else:
        main()
