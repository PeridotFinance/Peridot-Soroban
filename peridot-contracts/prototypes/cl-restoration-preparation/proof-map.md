# Focused proof mapping

These are combined-candidate assertions, not substitutes for the required Stage 1
matrix. All generated sources and WASMs are pinned per run. Global lifecycle,
transaction and release gates remain independent and can fail when these pass.

| Concrete compiled test | Assertions and reached financial roots |
| --- | --- |
| `compiled_combined_repayment_aged_full_max_refund_release_and_beneficiary_claims` | Nonzero aged debt; exact maximum and surplus/refund; zero debt; zero-debt repeat; debt-free release; receipt/margin custody exit during both reward-call failures; beneficiary claim; delayed rewards remain quarantined; exact scaled conservation. |
| `compiled_combined_repayment_insufficient_max_unauthorized_and_closed_rollback` | Insufficient maximum, zero maximum, wrong owner, missing authorization, premature release, closed-position repayment and repeated release reject after setup; complete ledger/TTL rollback; authorized full repayment/release remains available afterward. |
| `compiled_combined_funded_liquidation_without_borrower_authorization` | Distinct funded liquidator authorizes; no borrower authorization; post-accrual repayment and permitted seizure remain atomic; borrower reward entitlement retained. The preparation branch additionally executes an independent Aquarius trade, then freshly simulates and invokes liquidation. |
| `compiled_atomic_qualification_swap_close_64_owners_128_changes_8_unresolved_5pct_trade` | Preserved compiled atomic mint/transfer/custody, activation and close; 64 owners, ownership changes, unresolved history and pool trade; all three claim-first boundaries retained. |
| `compiled_atomic_qualification_8_independent_liquidations_repeated_both_call_outages` | Eight independent debt positions; repeated claim and pending-view outages; actual accrued debt cleared atomically with permitted seizure; borrower history and quarantine conserved. |
| `compiled_atomic_qualification_8_independent_pending_recoveries_repeated_both_call_outages` | Eight expired pending positions; full funded repayment, released custody and preserved exited beneficiaries through repeated outages. This route is owner-authorized; the separate active-position liquidation test supplies borrower independence. |

`restoration::journey` wraps each supported successful financial root with an
independent cold-state preparation probe. It archives exactly the persistent
dependency keys discovered by the complete warm controlled-state preflight,
including code and instances. It measures every native batch, saves/reloads
between batches, verifies duplicate batches are empty, records the incomplete
prefix's failed financial attempt, and invokes the final root with each profile.
It requires no additional automatic restoration for a successful result.

`postconditions::check` compares the final compiled SDK and official-host
contract data/code and events, checks actual reward-token backing, and preserves
booked borrower/liquidator claims and existing historical records in liquidation
and release. Assertion reads run on separate clones after measurement. Other
beneficiary roots explicitly report that this additional assertion is unsupported;
their original fixture assertions remain separately identified.

Raw state hashes remain separate. The only allowed comparison substitution is
the exact fresh temporary authorization-nonce pair for a declared synthetic
external authorizer: the host value must match its serialized credential, the
SDK value must be its one fresh mock nonce, and all other entry fields/data and
all remaining state must match. This is not real account-signature evidence.

`test_restore_checker.py` mutates unsigned XDR, fees, rent, profiles, required key
sets, batch order, snapshot links, totals, return values and missing evidence.
Validation failure propagates through `check_runs.py` and `run.sh`. A successful
batch check is not a successful journey, and a successful unsigned journey is
not Core application, signature verification, live-state or release approval.
