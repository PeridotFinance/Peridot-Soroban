# Fable high-effort review of the initial plan excerpt

The lead disposition and revised design are in handoff27. This review predates those corrections.

# Stage1 ABI/key-inventory review

**Verdict:** I would not approve the plan as written. There are five blockers, all fixable on paper before build. This is based on the excerpt only; I could not see the "00" document, the accepted rows' XDR, or any code, so inferred items are marked.

## Blockers

**B1. Resource feasibility is closed only for the nine measured roots.**
- **Gap:** `cancel` (four phase variants), `recover_in_kind` (with and without waiver), the expiry executor and holder emergency redeem have no accepted row. These are the roots that must work in an outage.
- **Worst case:** `recover_in_kind` with waiver is probably `directinkind` (78 SDK total, 52 keys, 26 RW) plus Waiver, WaiverIndex, Boundary and quarantine writes. Each new read-write key can count two or three times against SDK total, so about eight new keys reaches 100.
- **Invest headroom:** invest sits at 95.1M of 100M CPU, measured on the prototype tick range, across a 29.0.0/29.0.1 mismatch, before versioned commands and namespaced fields.
- **Action:** add a pre-build upper-bound delta per root from the logical families, with a stop condition. That is a bound, not an invented count. Add accepted rows for every recovery root, and add memory, read-bytes, write-bytes and event-size columns.

**B2. "Receipt is sole state authority" leaves the vault unable to bound a single job.**
- **Gap:** `Stage1Reserved(token)` and `Stage1Locked(owner)` are aggregates, and per-job amounts live only in the receipt. One receipt phase bug, or a receipt upgrade, can refund or unlock against other owners' funds.
- **Unspecified:** who can set or rebind `Stage1Receipt`, whether it is set once, and who can upgrade receipt wasm. "Reviewed code trusted" holds for a code hash, not an address.
- **Action:**
  - Make the binding set-once in both directions.
  - State the upgrade authority and the rule for upgrading with open jobs.
  - Split Reserved into three sub-fields in the same instance entry (unminted cash, execution reserve, post-burn claims), which costs no extra key.
  - Decide whether a minimal per-id vault record is worth one key; `Stage1Allocation(id)` already exists on the close side.

**B3. Holder emergency redeem and legacy routes bypass the receipt.**
- **Gap:** emergency redeem is a direct vault entrypoint, so users can reach vault state without the receipt. `Stage1Reserved` sits outside legacy Config, so legacy code does not know it exists.
- **Action:**
  - Enumerate every reader of NAV or idle balance (emergency redeem, fee checkpoint, any sweep or rescue) and prove each uses Book minus Reserved.
  - Prove emergency redeem honours `Stage1Locked`.
  - Prove it shares the fee-allocation-before-burn and waiver code path with `recover_in_kind`.
  - Define the identity between token balance, Book, Excluded and Reserved.
  - Define who bears a shortfall (for example an issuer clawback): reserved claims or NAV holders.

**B4. Cash-first mint allows free-riding on incumbents.**
- **Gap:** an owner can set `max_strategy_cost` to zero (or other unsatisfiable terms) with a long deadline. Their shares are minted and take a pro-rata share of fees and rewards while the cash never earns. They then cancel (unlock only) and exit, and can repeat.
- **Action:** choose one: start reward and fee weight at investment, cap the deadline and floor `max_strategy_cost`, let idle common cash be invested under vault-level bounds, or accept and document it.

**B5. `accept_reward_waiver: bool` is consent to an unknown amount.**
- **Gap:** a signed auth entry with `true` can be submitted later by anyone holding it, when the unknown interval is larger.
- **Action:** replace the bool with typed consent naming the interval or boundary id and the history revision. Revert if they differ at execution.

## ABI and authorization

- **A1. Close-side caller is unstated.** `withdraw_close` and `convert_close` do not say whether they are permissionless. G3 implies they are; say so.
- **A2. Permissionless priced stages can be sandwiched atomically.** A contract can swap, call the stage, and swap back in one transaction.
  - Extraction is bounded only by the user's minima and the oracle band.
  - `mint_cash` needs an incumbent-side bound: `min_shares` protects the newcomer, but nothing stops over-minting at an undervalued NAV.
  - A manipulated strategy cost can take the full `max_strategy_cost` for incumbents, so present it as the worst-case cost.
- **A3. Owner signature at funding.** Ensure the owner's nested auth on the vault call covers a terms hash and revision, not just the id.
- **A4. One revision source.** Any change to vault Params, oracle or wiring must bump the revision that jobs store. Otherwise jobs proceed under limits the owner never signed.
- **A5. Exits under pause.** State that `prepare_close` in InKind mode is never blocked by pause, cap or revision.
- **A6. Deadline boundary.** Define it so that exactly one of progress or permissionless cancel is valid at any ledger.
- **A7. Missing ABI artifacts.** Add the phase × entrypoint × caller matrix, a stable error-code table (already-done versus transient, for keeper reconciliation), typed return values and a versioned event schema.

## Recovery

- **R1. Recovery must not depend on the oracle or a swap.** If fee allocation before burn needs a price (I cannot tell), a stale oracle blocks `recover_in_kind`. Assert by footprint.
- **R2. Reward tokens should accrue, never be pushed, in ownership transactions.** Otherwise a missing reward-token trustline blocks exits (inferred from `RewardHolder` pending).
- **R3. Undeliverable payouts stall the whole job.** A missing, deauthorized or limit-capped trustline on either token blocks the mixture payout, and an expiry cancel then reverts indefinitely. Decide between per-token residual claims and a documented "owner action required" state the keeper stops retrying.
- **R4. Pre-mint and post-mint cancel must have no Aquarius or oracle keys in the footprint.**
- **R5. Job discovery without an indexer.** RPC event retention is short. Document scanning `Stage1Job(0..Stage1Next)` or add a per-owner key.
- **R6. TTL policy is missing.** Add a per-key table: who extends, threshold and payer, especially for the History families that are never pruned.

## Key inventory

- **K1. Concrete collision candidates.** These variant names appear in more than one family in the excerpt:
  - `Interval`, `Totals`, `Count` and `Proposal` in both reward and History;
  - `Allocation` in both Custody and Stage1;
  - `Busy` and `Initialized` in several contracts' enums.

  Add a test that serializes every key variant per contract and durability and asserts uniqueness.
- **K2. Vault instance size.** Reward Interval, Open and Totals live in the instance. Prove they are bounded, since every root reads and writes that entry.
- **K3. Legacy receipt keys.** `Initialized` and `UnderlyingToken` cost two reads and two archival risks per root. Consider carrying them in `Stage1ConfigV1`.
- **K4. History access must be constant per root.** `Weight(owner,index)` and `Settled(segment,owner)` grow forever. State that recovery reads only the latest checkpoint, and that an exited holder's entitlements survive a zero balance.
- **K5. Proposal and Evidence authority is unspecified.** State who writes them, the cap (the quarantined amount only) and any delay.

## Facade fail-closed (your question)

- **Enforce at the vault's legacy entrypoints.** They are off the Stage1 path. Use a deploy-time immutable mode in the already-loaded instance, or compile them out. Either adds zero hot-path keys.
- **Do not leave the facade deployed but uninitialized.** Anyone could front-run `initialize`. Either do not deploy it, or initialize it with a null binding and leave it permanently paused.
- **`swap` must revert with the typed error.** It must never succeed with zero output. `estimate_swap` may return 0.
- **Guard it in CI.** Add a footprint diff asserting zero facade keys in all roots.

## Test additions

- **G1:**
  - emergency redeem against locked shares and reserved cash;
  - R1 and R4 footprint assertions;
  - waiver consent replayed after the interval grows;
  - payout with a missing, frozen or limit-capped trustline per token;
  - a table-driven test over every matrix cell.
- **G2:**
  - the B4 free-rider;
  - the mint and invest sandwich at the band edge, with incumbents' NAV per share not decreasing;
  - dust left after final exit followed by a first mint;
  - per-category Reserved equal to the sum over jobs.
- **G3:**
  - the worst-case `recover_in_kind` with waiver, cold, with long history;
  - a Params change between every pair of stages;
  - deep-auth amounts derived in the call after the intervening trade.
- **G4:**
  - rows for every recovery root;
  - mismatched receipt and vault binding, and a rebind attempt;
  - receipt upgrade with open jobs;
  - facade negative tests.
