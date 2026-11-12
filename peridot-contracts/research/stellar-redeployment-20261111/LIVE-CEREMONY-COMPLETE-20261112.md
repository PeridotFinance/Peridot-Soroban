# Fresh Stellar cash lending — live ceremony completed

The operator completed creation, Ledger initialization/configuration, permanent seeding, ordinary deposits and approximately $10 withdrawals on all four fresh markets. The agent performed public reads, unsigned simulations and local evidence work only. No retired contract was invoked. Accepted contract WASMs remain unchanged from build commit `4c98bdd32b8fd076585b587b756c03b651d53fc0`.

| Market | Withdrawable backing remaining | Permanent seed | Actual market underlying | Deposit / withdrawal |
|---|---:|---:|---:|---|
| USDC | 90 | 1 | 91 | PASS / PASS |
| XLM | 617.1082991 | 5 | 622.1082991 | PASS / PASS |
| PYUSD | 90 | 1 | 91 | PASS / PASS |
| EURC | 81.1856294 | 1 | 82.1856294 | PASS / PASS |

Amounts above are underlying token units; do not infer receipt display decimals from raw share equality. Exact raw quantities and canonical SACs are in `fresh-deployment-manifest.json`. Final NAV equals raw total shares; exchange rate remains 1000000 in every market. Wallet credit checks include actual XLM fees, and all permanent shares remain receipt-owned.

Ledger holds every contract admin and the controller guardian role. No pending admin/upgrade is configured. All markets use the fresh controller; no oracle, router, rate model, strategy, margin controller or external allocation is bound. Borrowing and flash loans remain disabled by the durable receipt flag with a 10000 bps reserve; the separate controller pause is time-limited. Debt, reserve fees, admin fees and bad debt are zero. No maintenance-margin configuration or CL listing was performed.

## Recorded financial fees

| Step | Actual total XLM |
|---|---:|
| Five creations | 0.0891585 |
| Five initializations | 2.0695530 |
| 21 configuration calls | 0.4617483 |
| Four permanent seeds | 0.3324090 |
| Four ordinary deposits | 0.2506673 |
| Four withdrawals | 0.0115753 |

Code-upload fees, funding/trustline fees and any unrelated wallet transactions are separate. Withdrawal fees and the full before/after wallet accounting are preserved in `final-withdrawal-verification-20261112.json`.

## Evidence and remaining actions

`fresh-cash-post-withdraw-20261112.json` verifies final raw configuration and exact installed executable hashes. Phase-specific scripts record the earlier preconditions separately; do not rerun empty-market checks against funded markets. `verify-final-withdrawals.cjs` is reproducible against this checkpoint until further operator transactions change the expected balances or sequence. All simulation-only artifacts remain labelled unsigned preflights; successful public transaction outcomes establish actual application for the operator's ceremony.

Await additional top-up amounts per market in `TOP-UPS-PENDING.md`. There is no authorization to treat all wallet funds as available capital. The new-address-only DefiLlama change and draft PR text follow in the requested order; no PR is opened here. CL work and its unresolved gates are separate from this completed cash-lending ceremony.

A verified fresh manifest is available for `scripts/prepare_cash_keepalive.sh`; no keeper has been started or maintenance transaction signed by this checkpoint. Monitor code/instance/data TTLs and restore archived entries before resimulating. Future upgrades require storage compatibility testing against actual live state first. Old contracts remain retired permanently.
