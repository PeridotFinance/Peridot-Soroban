#!/usr/bin/env python3
"""Offline attribution of preserved SDK call trees; never executes contracts.

Run from any directory. Outputs only the two sibling attribution reports.
Input hashes are deliberate pins: a different experiment needs explicit review.
"""
import collections
import hashlib
import json
from decimal import Decimal, getcontext
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
PINS = {
    "target/cl-storage-candidate-roots-live/calls.jsonl": "b36abf622dd434ef80826f3b4fa2014be8c834f608a6bd1d7b2076c71e5aed2d",
    "target/cl-storage-baseline-data-archive/calls.jsonl": "500e9f76e9dbfdc801c17d83739d99305a76b28a20798425aba5a9cef0e8ae19",
    "target/cl-storage-candidate-roots-live/transactions.jsonl": "25fa617a8e47339e5bd5a42af88665894141341161a45105e1038141da0e451d",
    "target/cl-storage-candidate-roots-data/transactions.jsonl": "3694b2d286190dea5f5884a01e306a24623e601338602fdedb45671e88a34733",
    "research/cl-reserve-planning/results.json": "61e87ccb2c3f9d5aa5459a2ccd8a04201572bcad9c3bde20d4aae87d81255f20",
    "contracts/cl-share-pool-facade/src/resource_profile.rs": "ab6714725e3af998071e3b9d205fb2e51284e26ea70d4d7517a5ecbf33e0f7ec",
    "contracts/cl-share-vault/src/contract.rs": "7f5a868af47d40021904b6cc010ecb5ba1697a8e7fdb63cb12690c3f4e28d93d",
}


def load(path):
    data = (ROOT / path).read_bytes()
    actual = hashlib.sha256(data).hexdigest()
    if actual != PINS[path]:
        raise ValueError(f"source changed: {path}: {actual}")
    return ([json.loads(line) for line in data.splitlines()] if path.endswith("jsonl")
            else json.loads(data) if path.endswith("json") else data)


def summarize(row, line):
    assert row["invocation_ok"] and row["exclusive_reconciles"]
    assert row["diagnostic_descriptors_complete"]
    assert row["runtime_protocol"] == 29 and row["run_mode"] == "acceptance"
    calls = row["calls"]
    paths = {c["path"]: c for c in calls}
    assert len(paths) == len(calls) and paths["0"]["depth"] == 0
    by_role = collections.Counter()
    for call in calls:
        children = [c for c in calls if c["path"].rsplit(".", 1)[0] == call["path"]
                    and c["path"] != "0"]
        assert len(children) == call["direct_child_count"]
        exclusive = call["inclusive"]["instructions"] - sum(c["inclusive"]["instructions"] for c in children)
        assert exclusive == call["exclusive"]["instructions"] and exclusive >= 0
        by_role[call["role"]] += exclusive
    assert sum(by_role.values()) == row["root_instructions"]
    venue = [c for c in calls if c["role"] == "aquarius"]
    # A venue call cannot contain another selected venue call: inclusive totals
    # are a disjoint partition only after this condition is established.
    assert not any(b["path"].startswith(a["path"] + ".")
                   for a in venue for b in venue if a is not b)
    venue_total = sum(c["inclusive"]["instructions"] for c in venue)
    return {
        "source_line_1based": line, "flow": row["flow"],
        "root_sdk_instructions": row["root_instructions"],
        "exclusive_by_role": dict(sorted(by_role.items())),
        "aquarius_disjoint_inclusive": venue_total,
        "aquarius_exclusive": by_role["aquarius"],
        "outside_aquarius_subtrees": row["root_instructions"] - venue_total,
        "aquarius_calls": [{k: c[k] for k in ("path", "function", "inclusive", "exclusive")} for c in venue],
        "claim_count": sum(c["function"] == "claim" for c in venue),
        "claims_inclusive": sum(c["inclusive"]["instructions"] for c in venue if c["function"] == "claim"),
        "peridot_exclusive": sum(by_role[r] for r in ("v3", "vault", "facade", "cash_receipt", "share_receipt", "lending_controller", "adapter")),
        "all_calls": [{k: c[k] for k in ("path", "role", "function", "inclusive", "exclusive")} for c in calls],
    }


def main():
    loaded = {p: load(p) for p in PINS}
    flows = ("swap_open_position_v3", "swap_close_position_v3")
    rows = {}
    for variant, path in (("baseline", "target/cl-storage-baseline-data-archive/calls.jsonl"),
                          ("candidate", "target/cl-storage-candidate-roots-live/calls.jsonl")):
        rows[variant] = [summarize(r, i + 1) for i, r in enumerate(loaded[path]) if r["flow"] in flows]
        assert len(rows[variant]) == 3
        assert [r["claim_count"] for r in rows[variant]] == [3, 3, 2]
    for a, b in zip(rows["baseline"], rows["candidate"]):
        assert [(c["path"], c["role"], c["function"]) for c in a["all_calls"]] == [
            (c["path"], c["role"], c["function"]) for c in b["all_calls"]]
    comparisons = []
    for name, a, b in zip(("aged_open_with_actual_pool_deposit", "later_open_without_pool_deposit", "swap_close"), rows["baseline"], rows["candidate"]):
        comparisons.append({"case": name, "baseline": a, "candidate": b,
                            "sdk_cpu_saved": a["root_sdk_instructions"] - b["root_sdk_instructions"],
                            "role_exclusive_savings": {r: a["exclusive_by_role"].get(r, 0) - b["exclusive_by_role"].get(r, 0)
                                                       for r in sorted(set(a["exclusive_by_role"]) | set(b["exclusive_by_role"]))}})
    tx = {}
    for state in ("live", "data"):
        tx[state] = [{"flow": r["flow"], "instructions": r["resources"]["instructions"],
                      "test": r["test_name"], "snapshot_sha256": r["snapshot_sha256"],
                      "success": r["success"], "limits_pass": r["limits_pass"]}
                     for r in loaded[f"target/cl-storage-candidate-roots-{state}/transactions.jsonl"] if r["flow"] in flows]
        assert len(tx[state]) == 3 and all(r["success"] for r in tx[state])
    getcontext().prec = 70
    decomposition = []
    for case in loaded["research/cl-reserve-planning/results.json"]["cases"]:
        if Decimal(case["gross_cl_usdc_initial_usd_par"]) != 1000 or case["outage_hours"] != 24:
            continue
        t = case["timeline"][1]
        gross = Decimal(t["original_deposit_hold_value_usdc"])
        basket = gross / 2 * (Decimal(t["pyusd_usd"]) / Decimal(t["usdc_usd"]) + 1)
        lp = Decimal(t["modeled_lp_value_usdc_before_position_exits"])
        hold_loss, lp_loss = gross - basket, basket - lp
        covered = max(Decimal(0), hold_loss + lp_loss)
        assert abs(covered - Decimal(t["aggregate_lp_shortfall_usdc_before_position_exits"])) < Decimal("1e-16")
        assert abs(max(Decimal(0), lp_loss) - Decimal(t["basket_il_diagnostic_usdc_NOT_coverage"])) < Decimal("1e-16")
        decomposition.append({"strategy": case["strategy"], "path": case["path"], "boundary_hour": t["hour"],
                              "gross_usdc": str(gross), "basket_value_usdc": str(basket), "lp_value_usdc": str(lp),
                              "signed_50_50_hold_loss_usdc": str(hold_loss), "additional_lp_loss_usdc": str(lp_loss),
                              "total_shortfall_vs_original_usdc": str(covered)})
    out = {"schema": "cl-preserved-cpu-attribution-v1", "release_approval": False,
           "policy_changed": False, "contracts_executed": False, "inputs_sha256": PINS,
           "script_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
           "attribution_scope": "SDK invocation modeled CPU; distinct from fresh unsigned host execution/restoration and real signature costs",
           "comparisons": comparisons, "fresh_unsigned_transactions_separate": tx,
           "reserve_decomposition_at_initial_shock": decomposition}
    (HERE / "cpu-attribution.json").write_text(json.dumps(out, indent=2) + "\n")
    md = ["# Preserved CPU attribution and reserve benchmark decomposition", "",
          "Stage 1 remains incomplete; no policy, production code, stages or caps changed. This is an offline analysis of pinned existing evidence, not a new execution or qualification pass.", "",
          "## Attribution method and scope", "",
          "The SDK29 trees use the captured protocol29 cost model. For every node, this script independently checks exclusive CPU = inclusive CPU minus direct-child inclusive CPU, and verifies the sum of all exclusive nodes equals the root. Role totals below are exclusive. Aquarius inclusive totals include its token/plane descendants; selected Aquarius calls are proven disjoint, so their inclusive totals may be summed. Never add the two accounting views together. Outside-Aquarius costs include Peridot, tokens, oracle, interest-model and synthetic external-auth work; they are not all removable Peridot overhead.", "",
          "The baseline file name contains data-archive, but its call trees describe the original SDK invocation, not its separate archived companion. SDK diagnostics/shadow work and cumulative cost-type trackers are not added to these trees. Memory counter contributions are not allocation or peak-memory attribution. Recording-only unsigned replay does not prove real signatures or full transaction application.", "",
          "## Matched SDK roots", "",
          "| Case | Baseline CPU | Candidate CPU | Saved | Candidate Aquarius inclusive | Candidate Aquarius exclusive | Outside Aquarius subtrees |", "|---|---:|---:|---:|---:|---:|---:|"]
    for c in comparisons:
        a,b=c["baseline"],c["candidate"]
        md.append(f'| {c["case"]} | {a["root_sdk_instructions"]:,} | {b["root_sdk_instructions"]:,} | {c["sdk_cpu_saved"]:,} | {b["aquarius_disjoint_inclusive"]:,} | {b["aquarius_exclusive"]:,} | {b["outside_aquarius_subtrees"]:,} |')
    md += ["", "The later open reaches estimate_deposit_position but does not call deposit_position; it is not evidence for the full venue-deposit path. The existing deposit_balances code has explicit unchanged/idle returns after an unavailable, invalid or zero-liquidity estimate. The call trace alone does not establish which return predicate fired. Use the aged open with actual deposit when assessing full deposit feasibility.", "",
           "## Candidate exclusive CPU by contract role", "", "| Role | Aged open | Later open | Close |", "|---|---:|---:|---:|"]
    for role in sorted(set().union(*(c["candidate"]["exclusive_by_role"] for c in comparisons))):
        md.append("| " + role + " | " + " | ".join(f'{c["candidate"]["exclusive_by_role"].get(role,0):,}' for c in comparisons) + " |")
    md += ["", "These are call-boundary ownership labels, not source-line profiles. Unlabelled nodes include the interest model, synthetic external __check_auth and token balance calls. V3/facade/vault inclusive values overlap their descendants; the following view must not be summed with the role table or Aquarius totals.", "",
           "## Peridot call boundaries: inclusive versus exclusive", "",
           "| Case | Path / role / function | Inclusive CPU | Exclusive CPU |", "|---|---|---:|---:|"]
    for c in (comparisons[0], comparisons[2]):
        for node in c["candidate"]["all_calls"]:
            if node["role"] in ("v3", "facade", "vault", "cash_receipt") and node["function"] in (
                    "swap_open_position_v3", "swap_close_position_v3", "withdraw", "borrow_for_margin_to_controller",
                    "swap", "quote", "deposit_underlying", "transfer", "custody_assign", "custody_prepare_transfer", "redeem"):
                md.append(f'| {c["case"]} | {node["path"]} / {node["role"]} / {node["function"]} | {node["inclusive"]["instructions"]:,} | {node["exclusive"]["instructions"]:,} |')
    for c in comparisons:
        md += ["", f'## {c["case"]}: individual Aquarius calls', "", "| Call path | Function | Baseline inclusive | Candidate inclusive | Candidate exclusive |", "|---|---|---:|---:|---:|"]
        for a,b in zip(c["baseline"]["aquarius_calls"],c["candidate"]["aquarius_calls"]):
            assert a["path"] == b["path"] and a["function"] == b["function"]
            md.append(f'| {b["path"]} | {b["function"]} | {a["inclusive"]["instructions"]:,} | {b["inclusive"]["instructions"]:,} | {b["exclusive"]["instructions"]:,} |')
    md += ["", "## Distinct fresh unsigned host measurements", "", "| State | Aged open CPU | Later open CPU | Close CPU |", "|---|---:|---:|---:|"]
    for state, values in tx.items():
        md.append("| " + state + " | " + " | ".join(f'{v["instructions"]:,}' for v in values) + " |")
    md += ["", "These are separate complete host invocations from disposable snapshots, including their selected restoration work, not a sum of SDK subtree quantities. No fresh-host call-level tree was captured. The SDK breakdown therefore identifies measured dependencies without asserting an exact per-contract allocation of the unsigned transaction total or its difference from SDK costs.", "",
           "## What remains necessary for the unchanged 100M limit", "",
           "The aged open's Aquarius-only exclusive CPU is already 162,817,228; close is 137,759,524. Even an impossible zero-cost Peridot wrapper would not fit those unchanged venue paths under 100M. Inclusive venue subtrees total 167,831,028 and 141,545,386 respectively. A helper or compact Peridot records alone cannot establish compliance.", "",
           "Aged open retains claim-first deposit, share transfer and custody assignment (three claims: 6,196,410 + 5,684,682 + 5,672,237 = 17,553,329 inclusive), current principal valuation via estimate_withdraw_position (4,770,986), guarded conversion and exact spend authorization. The vault's oracle-derived NAV root is age-checked cached state on this path; the trace has no direct oracle call. This is not a measurement of the expired-cache/fresh-oracle branch. The fee checkpoint precedes principal refresh; estimate_swap (8,785,882) and estimate_deposit_position (4,538,505) remain inside the measured invocation. No valuation work is dropped from this attribution.", "",
           "Its fee collection and actual pool deposit cost 53,952,286 and 64,233,503 inclusive; together 118,185,789 before claims, quotes, swap or Peridot accounting. Close retains claim-first transfer and redemption, fee collection and actual withdrawal; fee collection plus withdrawal totals 115,815,792. Those call pairs alone exceed 100M. The third open ownership boundary has not been suppressed merely because close has a different two-claim call tree.", "",
           "Reaching 100M requires a measured change to the necessary venue work: fee-collection/position mutation implementation or ABI that reduces duplicate work while preserving fee-inclusive valuation, the three open claim boundaries, exact spend checks and atomic funded repayment/seizure. This is a dependency, not a claim that a new ABI exists or saves enough. Removing valuation or deferring mandatory claims is not an optimization. Different transaction stages would need a complete custody/debt/recovery design and are not introduced here. Existing 100-entry/50-write limits, restoration/code-byte ceiling, real signature overhead and missing lifecycle proofs remain separate failures.", "",
           "Most measured aged-open SDK savings are cash-receipt exclusive CPU (31,314,820 to 17,892,058, saving 13,422,762). Aquarius source is unchanged; smaller observed venue interval differences are not evidence of an Aquarius algorithm optimization. Exact source hashes and full call nodes are in the JSON.", "",
           "## Original-USDC reserve benchmark decomposition", "",
           "At one boundary with initial gross G, relative PYUSD/USDC price r and costless initial 50/50 conversion: basket = G(1+r)/2. Signed loss against original USDC decomposes exactly as G − LP = (G − basket) + (basket − LP). Coverage clips the sum at zero, not each summand separately. Basket loss may be negative when PYUSD gains relative to USDC. This is a boundary diagnostic; path-peak reserve still includes already-paid claims and cannot be reconstructed by adding independent maxima.", "",
           "Existing model numbers for G=1,000 USDC at the first shock (no fee/reward credits, costs still unknown):", "",
           "| Strategy / path | Signed 50/50 hold loss | Additional LP loss | Original-USDC shortfall |", "|---|---:|---:|---:|"]
    for r in decomposition:
        if r["path"] in ("pyusd_down50_persistent", "pyusd_down75_persistent", "both_down25_persistent"):
            md.append(f'| {r["strategy"]} / {r["path"]} | {Decimal(r["signed_50_50_hold_loss_usdc"]):.6f} | {Decimal(r["additional_lp_loss_usdc"]):.6f} | {Decimal(r["total_shortfall_vs_original_usdc"]):.6f} |')
    md += ["", "For full range at PYUSD0.50/USDC1, 292.893219 USDC total loss = 250 underlying-basket depeg loss + 42.893219 additional LP loss. At PYUSD0.25/USDC1, 500 = 375 + 125. Calling the additional LP term the entire product loss would underfund the original-USDC benchmark. Equal depegs leave the relative price unchanged and can produce zero USDC shortfall while losing dollar purchasing power. Actual zap/unwind/restoration costs, reserve-asset impairment, timing and capacity remain unproved; no production seed or cap is approved.", "",
           "Reproduce without contract execution: `python3 prototypes/cl-restoration-preparation/cpu-attribution.py`. It fails on changed input hashes or non-reconciling trees and rewrites only its JSON/Markdown reports. Required preserved inputs and exact hashes are listed in JSON; it does not download, sign or broadcast anything.", ""]
    (HERE / "cpu-attribution.md").write_text("\n".join(md))
    print(json.dumps({"sdk_trees_reconciled": 6, "matched_call_shapes": 3, "reserve_boundaries": len(decomposition), "new_contract_executions": 0}))


if __name__ == "__main__":
    main()
