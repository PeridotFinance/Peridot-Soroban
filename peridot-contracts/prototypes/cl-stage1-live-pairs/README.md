# Current Aquarius dual-pair private measurement preparation

Isolated reuse of the accepted Stage 1 cash-first and signed-local harnesses.
No production adoption, actual deployment artifact, configured cap or frozen ABI
implementation is asserted. Existing accepted artifacts stay intact. The operator
has now selected a 1,000 USDC-equivalent aggregate cap and unrestricted deposits;
neither setting is implemented by this fixture.

Current code was independently read at ledger 65421210. The pool now depends on
the captured configuration, reward vault and gauge WASMs, and its initializer
adds `config_storage_addr`. Those actual compiled dependencies are included.
Their configuration is set through their real ABI in controlled local state.

The pair fixtures use the captured token code/issuer/order, fee, protocol fee,
slot and report wide band. Token balances and liquidity histories remain
controlled. Initialized built-in SAC instances with genuine asset metadata are
imported at their public IDs into controlled genesis to preserve canonical token
order; private-network-derived IDs sort differently. This is not a live-state
fork or an asset deployment proof. Private Core import remains explicit. Pool and
configuration IDs are the public IDs in an isolated ledger, not deployments.
The oracle remains the compiled synthetic fixture populated with primary numeric
observations. Real Reflector code, feed routing, timestamp behavior and the
proposed depeg guard are **not measured or qualified** here.

PYUSD uses DEX/USDC numeric prices; EURC uses external/USD USDC and EURC prices.
EURC's captured approximately 74 bp basis exceeds the proposed 50 bp entry guard;
that guard has not been implemented or relaxed. Existing fixture checks retain
their existing values. A legitimate validation failure must remain a failure.

The vault's range and inward-rounded sqrt constants are the only isolated
financial source changes: PYUSD [-2020,2000], EURC [-3060,1020]. No ownership,
claim-first, conservation, reserve, quote, signature or pause check is removed.
The fixture's 10-unit swap minimum is a fresh quote less 1%; minimum minted shares
remains the baseline 190,000,000. This is a bounded small controlled workload,
not a $10,000 capacity or stressed liquidity proof.

Preparation (already done for v1; fresh output required):

```sh
python3 prototypes/cl-stage1-live-pairs/prepare.py --pair pyusd-usdc --output target/cl-stage1-live-pairs/pyusd-usdc-v1
python3 prototypes/cl-stage1-live-pairs/prepare.py --pair usdc-eurc --output target/cl-stage1-live-pairs/usdc-eurc-v1
```

Lead build and unsigned capture, each pair separately:

```sh
python3 prototypes/cl-stage1-live-pairs/build.py target/cl-stage1-live-pairs/pyusd-usdc-v1
bash prototypes/cl-stage1-live-pairs/capture.sh target/cl-stage1-live-pairs/pyusd-usdc-v1 target/cl-stage1-live-pairs/pyusd-usdc-capture-v1
```

For each of the nine generated `private-inputs/*` directories, record, sign ONLY
the hardcoded private fixture, enforce and apply in existing Core29:

```sh
bash prototypes/cl-stage1-live-pairs/run-host.sh BUILD INPUT/input.json FRESH_RECORD_DIR
node BUILD/sign-local.cjs INPUT
bash prototypes/cl-stage1-live-pairs/run-host.sh BUILD INPUT/enforce-input.json FRESH_ENFORCEMENT_DIR
CL_CORE_FORBID_SIGNING=1 CL_STAGE1_PRIVATE_CORE_INPUT=ABSOLUTE_INPUT/core-input.txt target/cl-stage1-signed-local/core-v6/stellar-core-private test '[cl-stage1-private-signed]' --version 29 --rng-seed 20261110 --ll warning
```

No command in preparation signs. Lead invokes signing only after the unsigned
pair-specific capture succeeds. Core test source-account signatures are actual;
setup issuer authority is explicitly assigned to the synthetic admin in controlled
genesis storage and is not an issuer signature proof. This avoids SDK registration
of a mock authorization contract at a G address. Financial owners still require
actual private-ledger signatures. Keep every failed build/capture/application log.
Never reuse prior table results or overwrite accepted cold/signed artifacts.

No cold replay is claimed by this preparation. The later close fork must discover
actual live state and validate real backing, current tick records and prices.

Completed checkpoint: both v6 captures pass two complete controlled journeys.
All nine roots per pair pass actual private source-signature Core application;
`apply-private.py`, `summarize.py` and `replay-gates.py` preserve and reconcile the
18 applications and 20 exact gate/error reports. Results and archive are in
`research/cl-apy-decomposition-20261111/lead/` and `execution-evidence/`.
Global lifecycle/transaction/checkpoint gates remain failed. Actual primary guards,
detached authorization variants, current-code cold restoration and a cap-sized
live-state rehearsal remain unqualified. No new broad study is implied.
