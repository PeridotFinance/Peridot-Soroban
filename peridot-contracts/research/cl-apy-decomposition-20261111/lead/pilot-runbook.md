# Capped open-deposit pilot ceremony plan — unsigned, unexecuted

Scope: fresh receipt market and vault, no facade, no compromised/retired Peridot address reuse, no lending or margin binding, borrowing disabled. Keeper runs the permissionless financial stages and restoration preparation. No Mainnet signature/submission is authorized for the agent. The operator has named a **1,000 USDC-equivalent aggregate cap**, targeting **500 per leg** at the chosen pair. Pair remains unset. After the frontend limitation was explained, the operator explicitly chose **all addresses permitted on-chain** with a frontend password. There is no on-chain allowlist; describe this as an **open-deposit prototype intended for team use**, not enforced team-only access. No personal addresses were supplied or inserted. The keeper gets a fresh keypair generated on its host by the operator, with XLM funded by the operator; no secret is requested or generated here. Operating XLM is separate from deposited capital, loss reserves and future liquidator funding.

G1 outage-mixture recovery, G2 stressed histories, G3 interrupted/re-expired preparation and G4 Core accounting parity remain **unresolved proofs**. The latest explicit unrestricted-deposit decision supersedes their earlier use as allowlist-removal gates; with no on-chain allowlist they cannot prevent public deposits. Retain and publish their actual results, never label them passed. An over-limit complete signed row stops deployment. Closing on a fork of the chosen live state is rehearsed before any operator Mainnet signature, as requested.

## Actual artifact status

The present accepted prototype has `stage_*` methods and validation-network guards. Frozen package1 names such as `prepare_open` are interface declarations, not deployed methods. Its `stage_bind` calls `require_validation_network`, which rejects Mainnet. It also lacks the complete per-command frozen liability implementation. An on-chain allowlist is no longer requested; the aggregate cap must still be enforced across all callers and in-flight contributions. The local dual-pair overlays do not remove that restriction or invent an allowlist setter. Therefore the command forms below are a concrete **ceremony plan**, not an executable release manifest. A final command file must be emitted from the actual chosen artifact ABI after these explicit release-artifact differences exist and are measured; passing old bytes cannot qualify changed bytes.

Before sealing: record source commit, compiler/CLI versions, receipt/vault WASM hashes, selected live Aquarius pool/config/gauge/reward-vault hashes, token issuers/SAC IDs, range, oracle contracts/base/decimals/freshness, cap denomination and amount, access policy (unrestricted deposits), admin Ledger public address, keeper public address, upgrade disposition, permitted receipt/vault binding, resource/fee ceilings and chosen recovery mode. Record Mainnet passphrase literally. Every address-based contract ID is newly derived from the network ID, deployer's public address and committed salt; the executable WASM hash is separately bound and verified, not part of that ID preimage. No old address aliases are accepted.

## Command forms and signatures

Installed CLI help was inspected; commands below were **not run**. Use an empty dedicated CLI configuration directory and explicit public addresses; no local key identities, `--auto-sign`, `--ignore-checks` or key-reading commands. Public RPC captures are read-only. Never place signed Mainnet envelopes in Git or AGENTS.md.

For each receipt/vault WASM upload:

```sh
stellar contract upload --wasm "$WASM" --optimize=false --build-only \
  --source-account "$DEPLOYER_PUBLIC" --rpc-url "$RPC" \
  --network-passphrase 'Public Global Stellar Network ; September 2015' > "$STEP.unsigned.xdr"
stellar tx decode "$STEP.unsigned.xdr" > "$STEP.decoded.txt"
stellar tx hash "$STEP.unsigned.xdr" \
  --network-passphrase 'Public Global Stellar Network ; September 2015'
```

For each deployment, use the **verified uploaded** hash, precommitted unique salt, and constructor arguments from that artifact (none may be guessed):

```sh
stellar contract deploy --wasm-hash "$UPLOADED_HASH" --salt "$SALT_HEX32" \
  --build-only --source-account "$DEPLOYER_PUBLIC" --rpc-url "$RPC" \
  --network-passphrase 'Public Global Stellar Network ; September 2015' > "$STEP.unsigned.xdr"
```

For each setup/financial invocation, build with the actual ABI and explicit argument list. `METHOD` and arguments must come from the sealed method inventory, not an invented setter:

```sh
stellar contract invoke --id "$CONTRACT" --source-account "$SOURCE_PUBLIC" \
  --build-only --rpc-url "$RPC" \
  --network-passphrase 'Public Global Stellar Network ; September 2015' \
  -- "$METHOD" "$@" > "$STEP.unsigned.xdr"
```

Use RPC `simulateTransaction` with the unsigned envelope; inspect every returned authorization entry, restoration preamble, resource declaration and error. Do not use a CLI path that signs during simulation. A fresh simulation must match the sealed IDs/method/amounts/recipient/deadline and permitted auth tree. If restoration is required, prepare bounded **RestoreFootprintOp** batches with the measured constructor, simulate each, then re-simulate the original operation. Restoration is not permission to replay a stale financial envelope.

For each reviewed envelope the **operator**, not the agent, performs the Ledger signature and then deliberate submission:

```sh
stellar tx sign "$STEP.unsigned.xdr" --sign-with-ledger \
  --network-passphrase 'Public Global Stellar Network ; September 2015' \
  --rpc-url "$RPC" > "$STEP.signed.xdr"
stellar tx send "$STEP.signed.xdr" --rpc-url "$RPC" \
  --network-passphrase 'Public Global Stellar Network ; September 2015'
```

This form is suitable only when the reviewed source-account authorization covers the operation. Any detached Soroban authorization must be explicitly inspected and signed in the Ledger-supported flow, including invocation tree, nonce and expiration. Do not substitute a source signature for a required detached signature. Private fixtures measure both forms; hardware-wallet ceremony compatibility must be checked for the selected form before use. No automatic retry after a timeout: first query the exact hash with RPC `getTransaction`, reconcile source sequence and state, then decide whether a fresh envelope is necessary.

## Ordered ceremony and read-backs

|Step / signer|Before signature / abort condition|After confirmed success / read-back|
|Receipt upload / deployer Ledger|Pinned receipt bytes, fee/resource ceiling, source sequence; abort if changed|RPC getLedgerEntries code hash/bytes/TTL; upload hash equals manifest|
|Vault upload / deployer Ledger|Same for vault|Same code identity check|
|Receipt deployment / deployer Ledger|Committed salt, executable hash, exact constructor/admin; abort any unknown existing address|New contract instance/executable/admin matches derived ID|
|Vault deployment / deployer Ledger|Same, selected pool/token/range/oracle constructor if present|New instance and exact binding inputs|
|Initialization and binding, each actual ABI invocation / admin Ledger|No deposits yet; immutable reciprocal receipt/vault binding; selected pair; no facade|Exact config getter/storage, binding digest, correct admin, zero assets/shares/jobs, new-risk state|
|Cap/borrow-off configuration / admin Ledger|Only implemented methods from final ABI; no guessed setters; cap must be 1,000 USDC-equivalent across all users|Unrestricted deposit policy; aggregate cap enforced across wallets and pending jobs; borrowing disabled and no margin route|
|Enable capped deposit path / admin Ledger if actual ABI requires it|Selected live-WASM signed rows below all limits, fresh oracle guard, fork close rehearsal, final byte identities|Fresh public config proves intended open-deposit state and aggregate cap|
|O1 prepare / team owner Ledger|Fresh price, amount, swap limit, shares floor, execution funding, deadline and cap reservation|Command ID, owner and immutable terms; no asset movement|
|O2 fund / same owner Ledger|Exact command/terms; token transfer amount = contribution + bounded reserve; issuer flags/trustline|Exact owner debit, vault cash/liability and command phase; no borrowed debt|
|O3 convert / keeper source|Fresh prices/issuer/swap pause, original minimum, available deadline and untouched liability|Actual input/output deltas, segregated mixture, next phase; abort/reconcile on failure|
|O4 fee checkpoint + cash-backed mint / keeper source|Required claim-first boundary, fee-inclusive fresh NAV, command conditions|Old entitlement preserved, minted shares/fraction/rounding, pending-investment lock, cash backing|
|O5 investment + finalization / keeper source|Fresh valuation/cap/deadline, actual venue/dependency hashes, transaction resource ceiling|Liquidity delta, preserved NAV/rewards, command terminal, share unlock, execution-charge/refund exact; duplicate rejects|
|C1 prepare close / owner Ledger|Owner's available shares only, exact quantity/mode/minimum/deadline|Shares locked and command terms; no burn yet|
|C2a in-kind withdrawal / keeper source|Claim-first; incumbents' fee entitlement allocated **before any burn**; issuer/recipient checks|Exact mixture payout to owner, fee allocation, share burn, conservation and terminal phase|
|C2b withdraw for conversion / keeper source|Alternative to C2a, same before-burn invariant|Segregated owner exit mixture, burned shares, pending conversion; no double payout|
|C3 optional conversion / keeper source|Fresh price and original minimum, fixed recipient, no expired stale quote|Actual output and recipient balance, zero consumed liability, terminal command|

The nine measured roots are O1–O5, C1, C2a, C2b, C3. A normal journey uses either C2a or C2b+C3; it is not nine sequential transactions. Keeper source signatures pay inclusion/resource fees but confer no authority to redirect owner assets. Keeper automation requires explicit infrastructure/account setup later; no secret is generated or uploaded here. It must simulate, enforce fee/deadline limits, store submitted hash/sequence, query status before retry, and batch restoration with re-simulation. No “keeper always alive” assumption.

Every post-read is a fresh ledger-qualified capture: `getTransaction` result/meta/fees, source sequence/balance, contract code/instance IDs, stage job, balances/shares/locks/liability, pool position and relevant reward counters. Record exact ledger and hash. Getter calls use `contract invoke --send=no` with explicit public source, or direct RPC read-only simulation. An unknown result, wrong hash/admin/recipient/amount, missing guard, unexpected authorization or resource overflow halts the ceremony at that step. Do not proceed with the next signature hoping to repair it.

## Close rehearsal and interruptions

Import authentic chosen-pool code, instances, ticks, position, reward/config dependencies and price feeds into the supported private Core fixture, preserving a captured-ledger manifest. Add only the explicitly marked private fixture owner/source accounts and balances. Rehearse open→fee allocation→in-kind close and optional close conversion using the exact intended pilot quantity and final byte hashes; compare financial state and fee accounting. Controlled pools with matching code are useful compatibility evidence but **not this live-state fork**. The selected 1,000 USDC-equivalent cap has not yet been rehearsed. The nine-root controlled fixture is smaller. A 500/500 target is a value allocation, not 500 EURC tokens; account for range asymmetry, residual cash and fees. Cash-first flow accepts token0 contribution then converts; it does not silently adopt a two-token deposit ABI. Deposited principal and entrant execution reserve must have explicit separate cap treatment.

Before mint, cancellation/expiry returns segregated cash or mixture to the original owner. After cash mint, cancellation unlocks owned shares and refunds unused execution reserve; it must not also refund their principal. In-kind close is a distinct operation. Prepared-close cancellation unlocks shares; withdrawn-mixture recovery pays the owner. Terminal commands reject replay. Live interrupted-state handling must use the artifact's actual supported state machine, not an invented unsafe cancel after successful investment. G1–G4 outstanding behaviors stay documented; unrestricted deposit access does not make those proofs pass.

After the team pilot is live, scope 2× share margin with conventional oracle-triggered take-profit via V3 close and the common EVM-style registry interface. Start from the narrow risk/recovery ABI. Entry/write overflows in activation/recovery remain; this runbook does not authorize that build or borrowing.

## Keeper funding estimate

**Planning amount: 550 XLM; do not treat this as the final live-code funding quote yet.** Generate the fresh keypair only on the selected host, under the operator's control, and provide only its public address for the manifest/read-back. No private key, seed phrase or remote secret is needed by this research.

The auditable planning calculation is one prior measured fully cold journey (max 417.0785571 XLM), 50 warm in-kind lifecycle equivalents (50×0.5744714 =28.72357 XLM), and a provisional 1 XLM account minimum: 446.8021271 XLM; plus 20% operating contingency =536.16255252, rounded up to **550 XLM**. At the earlier $0.14599065/XLM mark this is ~$80.29 of operating cash, **not** a loss reserve or supplier capital and **not** an expenditure forecast. Warm journeys are a conservative proxy because the owner pays some steps. Do not sum several independently cold probes as routine lifecycle costs.

The accepted cold figures use the old code and captured rent schedule. New live Aquarius has extra configuration/gauge/reward-vault dependencies, so a fresh cold journey may require more. Final required funding is `round_up(account reserve + measured new-code cold recovery + desired warm-run buffer + explicit contingency)`. Check the live base reserve and keeper subentry count, actual restoration batches/TTL/rent and fees before requesting funding. If the result exceeds 550, report it rather than skipping restoration or assuming proactive maintenance. Deployment/admin transaction fees remain in the deployer budget, separately. No XLM transfer was prepared or submitted here.
