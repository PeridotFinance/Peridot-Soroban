# Frontend draft: "PYUSD cash market earns nothing" (D1). DRAFT, not applied

## Where the frontend is

No local repository uses the fresh cash-market IDs. The following were searched read-only on 2026-11-18. The search covered
controller `CBMRS7LK…7RK`, PYUSD market `CD6OTGA6…OYZC` and the string "PYUSD" in `*.ts/tsx/js/mjs/json`, excluding `node_modules` and `.next`.

| Location | Result |
|---|---|
| `/Users/joshua/Peridot/Peridot-Soroban/peridot-dashboard` (Next.js) | An older dashboard. No PYUSD and no fresh IDs. |
| `/Users/joshua/Peridot/peridot-markets/app` (Next.js) | A separate hackathon product (isolated markets factory). Not the cash markets. |
| `~/Limit Order Frontend`, `~/Seltra Stellar POC`, other home folders | Unrelated |
| Only hits for the fresh IDs | `bots/cash-liquidator` and keeper evidence, which are not frontend code |

The integration contract the frontend team builds against is `docs/frontend-fresh-cash-markets.md`. The draft is therefore
a patch to that handoff: **`frontend-fresh-cash-markets.pyusd.patch`**. It is a unified diff that applies cleanly with
`git apply -p1` from `peridot-contracts/`. It has **not** been applied, because `docs/` is outside this task's write scope. The owner
should forward it to the frontend team or apply it.

## Copy strings (for the frontend team's string table)

| Key | Where | Text |
|---|---|---|
| `pyusd.card.badge` | PYUSD market card | **Deposit-only · earns 0%** |
| `pyusd.card.body` | PYUSD market card | This market holds your PYUSD and earns no interest. PYUSD can't be used as collateral or borrowed on Peridot. |
| `pyusd.card.vaultCta` | PYUSD card, only with flag `pyusdClVault` | Want PYUSD to earn? Try the PYUSD vault → |
| `pyusd.vault.disclaimer` | Under the CTA, only with the flag | The vault provides liquidity to the Aquarius PYUSD/USDC pool. Returns vary and can be negative. |
| `pyusd.deposit.debtWarning` | PYUSD deposit modal, if the user has any open borrow | While you have an open borrow, you won't be able to withdraw this PYUSD until you repay. |
| `pyusd.withdraw.blocked` | PYUSD withdraw button or tooltip, if any debt > 0; also mapped from the simulation error `Price unavailable` | Repay your loans first. PYUSD has no price on Peridot, so it can't be withdrawn while you have an open borrow. |
| `pyusd.apy` | APY cell | 0%, with no projection. It stays 0% after borrowing opens on USDC, XLM and EURC |

## Facts behind the copy

- **No price for PYUSD on the controller.** The controller oracle is the Reflector external USD feed `CAFJZQWS…34DLN`. It has no PYUSD entry: `Other(PYUSD)` and `Stellar(PYUSD SAC)` both return `None` (`evidence/oracle-bounds-20261118.json`). PYUSD is deliberately left without a symbol mapping (PLAN §2, option A).
- **PYUSD earns nothing.** The ceremony gives PYUSD no CF, no rate, no reserve factor and no borrow switch. Its cash reserve stays at 10000 bps, so nobody borrows PYUSD and nothing pays interest into the market.
- **Withdraw with debt.** Receipt `withdraw` with any outstanding debt calls `get_price_usd(token)` and panics `Price unavailable` (receipt-vault 1946–1955 at 4c98bdd3).
- **The CL vault is deployed but not yet invested** (`addresses.md`): vault `CD6N2EQJ…WL63`, staged receipt `CCGKTKPA…XBCI`. Hence the feature flag and no APY. The CTA's target URL is an open item for the owner.
