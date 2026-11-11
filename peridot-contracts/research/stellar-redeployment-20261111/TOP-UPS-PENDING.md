# Separate top-up list — pending successful live deposit/withdraw checks

No top-up command or amount is prepared yet. Complete the final command list's deposit and approximately $10 withdrawal on **every** market first, retaining transaction hashes, receipts, underlying balance deltas and exchange-rate readbacks. This file is an execution-order tracker, not a passing checklist or permission to skip those steps.

| Market | Verified fresh receipt | Deposit hash/readbacks | Withdrawal hash/readbacks | Operator top-up amount |
|---|---|---|---|---|
| USDC | pending deployment | pending | pending | awaiting operator |
| XLM | pending deployment | pending | pending | awaiting operator |
| PYUSD | pending deployment | pending | pending | awaiting operator |
| EURC | pending deployment | pending | pending | awaiting operator |

Once all four pass and the operator supplies amounts, produce a separate deposit-only list. Re-read live state and roles, borrowing=false, no strategy, exact asset binding, permanent floor and current exchange rate. Simulate each named amount, obtain the operator's signature, and reconcile each transaction before the next. Do not add to the permanent seed or guess that all remaining wallet balances are available capital; retain gas and account reserves separately.

DefiLlama follows this step: prepare the adapter update against only the verified new addresses, remove old market addresses and use underlying balances for cash-market TVL. Do not count receipt supply again. Mark any subsequently listed CL Aquarius position as double-counted. The adapter PR remains a draft text/change until separately authorized to open it.
