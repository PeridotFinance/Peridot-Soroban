# Separate top-up list — live deposit/withdraw checks passed

All four fresh markets passed their live seed, ordinary deposit and withdrawal checks. Exact wallet payouts, charged fees, permanent receipt-owned shares, cash/NAV and exchange rates reconcile. `fresh-deployment-manifest.json` records the fresh identities and final readbacks; `final-withdrawal-verification-20261112.json` records withdrawal evidence. This does not authorize any unspecified top-up amount.

| Market | Fresh receipt | Deposit | Withdrawal | Additional top-up amount |
|---|---|---|---|---|
| USDC | `CDW3GDNKLUEPPJLEUVGLJSKGYLB3YYDQBUY7NBHJILGQWNN55AYFHY3F` | [PASS](https://stellar.expert/explorer/public/tx/412a5c78944135a25dde81bf1ef22c3a08063ec28aa4a752c8ed5d0b345147c1) | [PASS](https://stellar.expert/explorer/public/tx/b8a827fedceabf1f8a6582ef9280811288e0eb687b22a6b84c8cc41269547eaf) | **Awaiting operator** |
| XLM | `CBHMG4CNJO2CRGADUMIO3UJ45C5MTWGCBFECMR6QE2TDDCDBB3N24WMS` | [PASS](https://stellar.expert/explorer/public/tx/f7d382d8f659132dfbcccc33f06298e2e3281507203328402790e27989f1b0e1) | [PASS](https://stellar.expert/explorer/public/tx/a5827d9fa66365a2088bae41c667386296d515d8c1cea49cf78784c1e58718f0) | **Awaiting operator** |
| PYUSD | `CD6OTGA6J6UIEKXTXA5CLPLSJA4VQ5HEOYP3CA375CZEHI3KMIL2OYZC` | [PASS](https://stellar.expert/explorer/public/tx/0bb6f97238e3a9df13fe2b5cb4b562db9baf09a7cf94947a8cca472e3e944530) | [PASS](https://stellar.expert/explorer/public/tx/3463b6ef25ce57bcc78b432d4ebb5e34f00905ecfa272e2af4c1bb5fb30b3e70) | **Awaiting operator** |
| EURC | `CAXWP5M2SF74DW22EQAYOXR4ZHN2FO2HGL737KQ2WFPEZTA4Z3W5E2M4` | [PASS](https://stellar.expert/explorer/public/tx/aadbcabe53a80b785fc824fafa0cdc52ffe16393adb86ece0875f8e9c28f807b) | [PASS](https://stellar.expert/explorer/public/tx/7363d95b52290425f3a7cdf1ca5e83e9fc2533eb9dd4c1ad02e74c93f836318c) | **Awaiting operator** |

Once the operator supplies amounts, prepare a separate deposit-only list. Re-read live roles, borrowing=false, no strategy, canonical asset binding, permanent floor, wallet funds and current NAV/share ratio. Simulate each named amount, have the operator sign, and reconcile the result before continuing. Never add top-ups to the permanent seed; never assume all remaining wallet balances are available capital. Keep gas and account reserves separate.

DefiLlama follows: prepare the adapter change using only these new receipt addresses, drop retired addresses, and count underlying contract balances without counting receipts again. A later CL Aquarius position must be marked double-counted. No adapter PR is opened by this checkpoint; user review and the requested follow-on order remain in force.
