# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands

```bash
# Build all contracts (from peridot-contracts/)
bash scripts/build_wasm.sh

# Build a single contract (from peridot-contracts/)
stellar contract build --package receipt-vault

# IMPORTANT: Always use `stellar contract build`, never `cargo build` for wasm output.
# Wasm artifacts: target/wasm32v1-none/release/*.wasm

# Run all tests
cargo test

# Run a single contract's tests
cargo test -p receipt-vault

# Run a specific test
cargo test -p receipt-vault -- test_deposit_receives_ptokens

# Lint and format
cargo clippy
cargo fmt
```

## Architecture

This is the **Peridot DeFi Lending Protocol** — a Compound-style lending system on Soroban (Stellar smart contracts).

### Contract Dependency Graph

```
SimplePeridottroller (risk manager)
  ├── ReceiptVault (one per market, holds underlying tokens)
  │     └── JumpRateModel (dynamic interest rates)
  ├── PeridotToken (reward token, minted by peridottroller)
  └── Oracle (Reflector, external)

MarginController (leveraged trading, optional)
  └── SwapAdapter (Aquarius DEX wrapper)
```

### Core Contracts

- **`receipt-vault`**: Per-market vault. Handles deposit/withdraw (mints/burns pTokens), borrow/repay with interest accrual, flash loans, supply/borrow caps. Delegates risk checks to peridottroller.
- **`simple-peridottroller`**: Cross-market risk manager. Oracle pricing, collateral factors, account liquidity checks, liquidation coordination, pause controls, reward distribution.
- **`jump-rate-model`**: Utilization-based interest rate with kink mechanic. Called by vaults during `update_interest()`.
- **`peridot-token`**: Reward token with max supply cap. Admin (peridottroller) mints on reward claims.

### Supporting Contracts

- **`margin-controller`** / **`swap-adapter`**: Optional leveraged margin trading via Aquarius DEX. Legacy Margin V1 exports are disabled; new work must target Margin V2 entrypoints only.
- **`aquarius-lp-vault`**: ReceiptVault-gated boosted vault backed by an Aquarius concentrated-liquidity position, as an alternative to a DeFindex vault. Implements the same boosted-vault ABI, so it attaches through the unmodified `set_boosted_vault`; the strategy must then be bound back to that market with `set_receipt_vault`. **See `contracts/aquarius-lp-vault/CLAUDE.md` for the full handoff** — design decisions, reward compounding, the Aquarius auth-tree gotcha, transaction-footprint limits, and open review findings.
- **`mocks/mock-token`**, **`mocks/mock-lending-vault`**: Test-only mocks.

### Aquarius LP Rollout Invariants

- September18 approved yXLM reporter implementation is local, NOT activated.
  Router `Observed` source uses a separate reporter, time-window/replay/rate
  checks, on-chain two-way Aquarius cross-check and observation-end freshness.
  Configure required-observation dependencies for all3 LP settlement assets.
  LP-only controller now revalidates live oracle prices and rejects static
  fallbacks: warm caches otherwise bypass reporter invalidation (regression
  reproduced and fixed). Generic core behavior unchanged; repayment stays open.
  Separate `bots/aquarius-keeper/src/price-main.mjs` defaults to read-only Mainnet
  observation and rejects Mainnet publishing. First live window correctly halted
  for insufficient bucket volume; do not weaken candidate policy to force launch.
  See `bots/aquarius-keeper/PRICING.md` for trust, thresholds and release gates.
  User confirmed both price-router and aquarius-keeper Almanax paths in addition
  to the five LP contract paths. No seed transmission or keeper deployment.
- September18 fresh-stack canary migration is now user-approved, conditional on
  release checks and concrete bounds: settle legacy rewards, withdraw ONLY the
  small pilot positions to deployer, then redeposit via NEW Peridot receipts.
  No core/DeFindex or treasury migration. See `FRESH_STACK_MIGRATION.md` under
  `contracts/lp-receipt-vault/`. Nothing has been signed or deployed for this step.
  Compiled receipt/controller/strategy plus exact deployed pool WASM now pass
  simultaneous stable loans, exact borrower auth, reward conversion/payout and
  exits locally (latest peak240entries/197.13M CPU with live-price controller,
  bounded250/200M fixture). Mock oracle,
  reward routes and boost/plane remain; this is not a full live-dependency canary.
  `hybrid-validation` permits compiled research with Mainnet runtime fences;
  ordinary hybrid WASM remains blocked. Hybrid strategies fence BOTH legacy
  harvest and sweep_reward, plus raw primary rotation.
  Native Reflector CALI2BY...PLE6M now supplies PYUSD/USDC, not PYUSD/USD.
  New CrossQuoted router source multiplies it by upstream USDC/USD with metadata,
  age and positivity checks and no upside peg cap. yXLM feed still absent;
  keeper-published pricing was subsequently approved; implementation status above.
- September18 explicit LP lending interface: new `lp-lending-vault` exports only
  checkpointed lending/reward methods over the shared engine, not generic margin,
  flash-loan/recovery or uncheckpointed token selectors. New `lp-peridottroller`
  requires fresh zero-PERI state, rejects nonzero emission settings and admits
  at most3 LP-versioned markets. AQUA conversion/payout remains enabled. Approved
  CF targets are XLM500000/PYUSD800000/USDC800000; activation checks exact targets.
  Public config `config/lp-lending-mainnet.json` records targets/old pilot addresses,
  NOT live changes. Existing CFs remain0. Receipt bindings seal on activation;
  native Aquarius hybrid activation fences legacy harvest/raw primary rotation.
  Both new artifacts reject fresh Mainnet initialization. Generic hybrid/Aquarius
  WASM guards remain. Compiled receipt+controller local lifecycle passes with
  NATIVE strategy/mock pools, not full compiled-stack/Mainnet evidence. Full
  pricing, migration/restoration, claim-outage liquidation, exact-WASM resources,
  security and governed deployment gates remain. See LENDING_INTEGRATION.md.
- September18 USER SCOPE: PERI is not intended to pay rewards now or for the
  foreseeable future. New LP deployment must explicitly use zero supply/borrow
  emissions, retaining a token reference if deployment needs it. AQUA rewards,
  swaps and owner payouts remain enabled in the design. Do not confuse PERI
  emission speeds with liquidation bonuses, lending interest or keeper thresholds.
  Defer unfinished escrow PERI distribution for a verified zero-emission release;
  require an activation guard, legacy-liability reconciliation and a reviewed
  opt-in before PERI can ever be enabled. Do not erase earned historical claims.
  Native checkpoint fixes remain. Production ABI, lending/liquidation safety,
  migration, oracle policy and compiled testing still apply. Fresh Mainnet settings
  and existing deployed LP addresses are now recorded in addresses.md. Existing
  LP pause timers expired September12: fresh getters return false; CF remains0.
  No Mainnet setting or keeper was changed for this clarification.
- September18 native controller-incentive checkpoint follow-up: backing mint now
  accrues with OLD total supply/escrow balance; backing payout checkpoints escrow
  and owner BEFORE temporary share credit. Enabled-incentive regressions reproduce
  both old dilution and retroactive payout capture, then verify the correction,
  exact owner auth, atomic rollback, repeated minting, exited owners and distinct
  borrower emissions. Complete hints come only from local accounting, outside
  contractimpl; no public selector or production behavior changed. Escrow PERI
  accrual is retained by the controller, NOT attributed/paid to historical backing
  owners yet, and never added to principal NAV. Unsolicited permissionless claims,
  owner attribution across unit changes, liquidation incentive checkpoints and
  missing-index restoration remain release gates. Do not expose production ABI
  or treat this as complete controller-incentive integration. See
  contracts/lp-receipt-vault/LENDING_INTEGRATION.md and Agents.md.
- September18 native lending reward-settlement follow-up: the LP wrappers now
  select managed-only cash for borrowing, principal/reward withdrawal and admin
  reinvestment. Donated settlement stays untracked even through full exit; missing
  cash records/custody deficits fail closed before legacy helpers can reconstruct
  them. Core functions were refactored with an internal cash-policy parameter;
  existing public ABI always selects legacy policy. Native helpers are outside
  contractimpl, feature/test gated; no production LP selector or activation flag.
  Shared reward_settlement.rs now supplies the SAME conversion/recycling/reserved
  payout logic to lean and lending engines. Lending backing is priced using NAV
  including debt and redeemed through managed-only, health-checked withdrawal.
  New tests cover all settlement roles, both stable indices, deferred recycling,
  actual reward payout with debt, historical converted rights after liquidation/
  exit, donation isolation and exact auth/rollback. Native strategies/controllers
  with MOCK pools/oracle, not full compiled-stack evidence. Production ABI,
  incentives, fee/pair emissions, loss/oracle/liquidation policy, migration and
  full-WASM/resource gates remain. No chain or keeper changes. See Agents.md.
- September17 native Aquarius/LENDING integration now shares `reward_claims.rs`
  between generic lending and lean research engines; no copied claim rules or
  changed reward key encodings. Native `reward_lending.rs` checkpoints actual
  primary/gauge claims or observable IOUs around lending deposit/withdraw/borrow,
  transfer/delegated transfer and seizure. Core still supplies authorization,
  debt-inclusive NAV, collateral checks and liquidity protection. Withdrawal
  reserves historical raw rewards and checks actual payout against a user floor.
  Repayment remains Core's cash-for-debt path, independent of reward outages.
  New local three-market tests use native strategy/controller/JRM, real SACs and
  MOCK concentrated pools/oracle (stable settlements share one mock pool). Both
  stable loans actually unwind positions. No production hooks/ABI or artifact.
  The integrated exit exceeded200 research entries at205; fixture now bounds250
  while keeping all other SDK limits ON. This is NOT compiled/Mainnet fit proof.
  Regression also pins a LEGACY cash-policy gap: donations are excluded from NAV
  but live-cash sizing can spend them during payout, leaving more LP capital for
  remaining holders. Strict unmanaged-cash segregation is not established and
  must be resolved before activation. See LENDING_INTEGRATION.md and Agents.md.
- September17 native LENDING boundary work now lives in
  `contracts/lp-receipt-vault/LENDING_INTEGRATION.md`. The existing lending engine
  is the baseline; the lean supply-only principal engine is NOT being enabled for
  borrowing. New native reward_share_hooks checkpoint all retained streams around
  supply-neutral transfer/delegated-transfer/seizure and preserve earned rewards
  for old holders. Two-controller tests cover LP XLM collateral backing both stable
  debts, exclusion of identical-token collateral across groups, loan/interest NAV,
  liquidations and rollback. Controlled funded rewards and idle liquidity only:
  actual Aquarius all-stream/outage/unwind integration is still required. Native
  tests use an explicit200-entry research envelope after exceeding SDK25's bundled
  100 entries; CPU/memory/write limits remain enforced. Not compiled/network-fit
  evidence. No production hook, ABI, governance or keeper changes.
- September17 USER DIRECTION CORRECTION (supersedes the supply-only FINAL target
  in the prototype/migration notes below): user requests a NEW dedicated LP
  Peridottroller, isolated from core/DeFindex markets, WITH borrowing support.
  The current lean lp-receipt-vault omits borrowing, interest/JRM, collateral and
  liquidation and is therefore NOT a suitable deployment target as-is. Preserve
  its reward-accounting research, but redesign the receipt integration around
  lending-capable accounting and controller health/liquidation hooks. Existing
  records already identify a separate LP pilot controller CCZKDMAP...ENGC; this
  was not a shared core controller. New controller deployment does not itself
  add borrowing or safely migrate obligations. User CONFIRMED cross-market
  collateral WITHIN the LP group: deposit XLM as collateral and borrow PYUSD or
  USDC. One shared LP risk group, NOT separate per-market controllers; collateral
  and debt must never cross into the core/DeFindex group. Do not
  select risk parameters or enable borrowing under the temporary parity-alias
  oracle policy. Existing live safety gates remain until reviewed replacements,
  resources, liquidation/liquidity tests, security scan and governed activation.
  No deployment or keeper change was made for this clarification.
- September17 LOCAL migration specification and shadow rehearsal:
  `contracts/lp-receipt-vault/MIGRATION.md` defines in-place address/share
  preservation, external controller/incentive retirement, legacy-yield policy,
  fenced cutover and production ABI gates. Native `migration.rs` inspects local
  accounting only; success is NOT activation readiness. No activation method was
  added. Tests use actual generic native receipt deposits in an idle-only scratch
  fixture, then a TEST-ONLY marker insertion to compare raw shares, allowances,
  metadata, cash, donations and non-par redemption. Missing state, one-raw-unit
  liabilities and controller links reject. Individual debt can hide behind zero
  totals: tests explicitly demonstrate that local checks cannot prove absence.
  Normal controller delisting requires zero supply; nonempty retirement remains
  unresolved. Old pooled reward treatment requires approval; never silently
  reassign it as new individual claims. See Agents.md for checks/scan evidence.
  No production ABI, deployable artifact, keeper change or Mainnet mutation.
- September17 LOCAL lifecycle follow-up: native coordinator now pins primary
  denomination and supports admin-authorized, staged rotation. Preparation unwinds
  custody into receipt cash without changing shares/weights; completion rechecks
  zero strategy shares and actual pool liquidity, collects old rewards, rejects
  outstanding IOUs, and retains every old token's claims/history/routes. Registered
  new primary needs a valid route/floor; quoted IOUs stay disabled until an actual
  positive PRIMARY claim proves its denomination (gauge/zero/reverted claims do
  not). Pool-side configuration remains external; bootstrap primary remains a
  trust assumption. Out-of-band strategy primary changes fail the receipt pin.
  Durable per-user exit requests record nonce/shares/minimum without moving or
  locking shares. Execution needs fresh owner auth, current shares, fresh reward
  checkpoint and the stored minimum; failures preserve the request. This is NOT
  immediate exit during a total reward-data outage. Initial combined rotation
  exceeded100M instructions; staged exact-pool tests pass both settlement indices:
  preparation79 entries/~77.11M or85.11M CPU, completion67/~22.90M. Native
  receipt/strategy, real pool WASM, mocked ancillary dependencies—not a full
  compiled-stack or live token migration. WASM gates remain. See design/handoff
  for final tests/scan status; the prior2677209 scan does not cover these changes.
- September17 LOCAL outage follow-up supersedes the claim/stale-quote limitation
  in the earlier native coordinator entry below. Failed claims now checkpoint
  independently observable primary/gauge receivables; these are NOT cash/NAV and
  cannot be swapped or paid until actually collected. Recovery never indexes the
  same debt twice. Missing/decreasing observations still fail closed. Proportional
  principal exit burns only attributable strategy shares plus managed idle cash,
  checks actual deltas and the user's final minimum, and retains strategy guards.
  It never forwards an inflated user minimum to the strategy to draw others' cash.
  Six added outage regressions; lean bridge48 passed/2 explicit-WASM ignores.
  Full workspace595 unit/3 doc tests passed. Two separately enabled local tests
  using exact deployed concentrated-pool WASM also passed:87 entries each,87.33M
  and95.44M instructions (fresh/stale oracle respectively). These use native
  receipt/strategy, controlled oracle/plane/reward-route mocks and fresh local
  pool state, NOT a compiled-stack or Mainnet simulation; real gauge WASM remains
  untested. Full compiled budgets, multi-route worst cases, fees/pair emissions,
  rotation/restoration/migration, production ABI and security review remain gates.
  No deployment, keeper changes or Mainnet writes. See the design for scan scope
  and reproducible exact-pool test commands; do not deploy this native prototype.
- September17 LOCAL all-stream follow-up: native-only
  `lp-receipt-vault/src/reward_coordinator.rs` now splits actual new emissions
  between ordinary holders and escrow, derives pending inventory across up to4
  retained tokens, and settles old backing yield before new-unit issue/redemption.
  Deposit/transfer/withdraw wrappers checkpoint all retained streams; principal
  exits do not require a reward swap. Failed claims/quotes still block exits and
  remain a release blocker. Registration cannot reset a missing old registry.
  Thirteen functional coordinator tests plus one explicitly over-budget diagnostic
  added; lean bridge total42 passed. Shared backing total now18 tests; ordinary
  ReceiptVault171/1 intentional ignore, Aquarius93/2 intentional ignores, LP9 pass.
  Two-stream principal exit81 entries, staged reward payout82; four-registry staged
  recycling74–75. Combined four-route payout152/SDK100 is DIAGNOSTIC ONLY with
  limits disabled, not network-fit evidence. Staging always reclaims fresh rewards;
  it is not a continuous-emission liveness/budget proof. Actual pool-WASM/resource
  checks, dust/fees/pair-emission policy, outage recovery, production ABI and
  migration/controller obligations remain required. All WASM gates remain; no
  scan, commit/push, deployment, keeper or Mainnet change. Detailed evidence is in
  REWARD_SETTLEMENT_DESIGN.md and ignored Agents.md.
- September16 LOCAL LP-only split: `contracts/lp-receipt-vault/src/contract.rs`
  is a separate native supply-only principal engine. Generic lending/DeFindex is
  unchanged. No borrowing/JRM/flash loans/collateral/liquidation/margin/controller
  incentives; same development reward backing/ledger sources are reused. No public
  ABI or migration, and WASM compilation is blocked. Nine core tests and28 lean
  bridge tests pass, including both settlement indices. Native strategy-funded
  ownership deposit75 entries (generic106), exits66 each, conversion53 and separate
  reinvestment52, with transaction limits enabled. Correction:100 is the SDK fixture
  ceiling, NOT a fresh Mainnet-limit read. Actual pool-WASM budgets, all-stream/
  recycled yield, outage exit recovery, migration/controller obligations and review
  remain release gates. Existing receipt-address bindings cannot simply be changed.
  No scan, commit/push, deployment or keeper/Mainnet changes. See the design/handoff.
- September16 LOCAL persistent ownership follow-up: `receipt-vault/src/reward_ledger.rs`
  tracks per-token epochs, per-holder raw/converted claims and withdrawal reserves
  in persistent storage. Native receipt tests now derive allocations from real
  pToken weights and connect actual claims/swaps/backing without admin allocation.
  Thirteen added tests; totals ReceiptVault170/1 intentional WASM ignore and
  Aquarius93/2 intentional WASM ignores. IMPORTANT: one test is a diagnostic of
  the generic106-entry strategy-deposit footprint (SDK test ceiling100), not network-fit
  evidence. Functional exit/deposit tests use idle cash. Recycled escrow emissions
  and multi-stream coordination still fail closed; production mutation hooks are
  not wired. Both contracts reject hybrid-rewards WASM builds. No release/Mainnet
  action. Full evidence and limitations: REWARD_SETTLEMENT_DESIGN.md and Agents.md.
- September 16 LOCAL hybrid bridge work: native-only receipt-authorized primary/
  gauge claims and exact-input reward swaps now compose with real ReceiptVault
  backing and Aquarius reinvestment in tests. Explicit `hybrid-rewards` WASM builds
  are rejected; default production behavior is unchanged. Combined conversion and
  reinvestment hit104 ledger entries, so conversion now creates immediately backed
  idle cash via `finish_idle`, with reinvestment separate (77 entries each in the
  native mock-pool fixture). Twelve new bridge tests pass; targeted totals are
  ReceiptVault170/1 intentional WASM ignore and Aquarius80/2 intentional WASM ignores.
  Per-user allocation remains a test stand-in; ownership hooks, fees/pair emissions,
  migration/TTL, controller incentives and exact-WASM budgets still block release.
  No hybrid scan, release, keeper or Mainnet change. See REWARD_SETTLEMENT_DESIGN.md.
- September 15 hybrid reward work is LOCAL/DEVELOPMENT ONLY: receipt allocation
  model plus `reward_backing.rs` real-token custody/reinvestment library. Backing
  uses escrowed pTokens with separate claim units; it has no production entrypoints
  or activation and is excluded from default builds. Seventeen custody tests and
  sixteen allocation tests pass; full targeted suites: ReceiptVault170 passed/1
  intentional WASM ignore, Aquarius68 passed/2 intentional WASM ignores. Pool
  recognition, multi-token coordinator, controller incentive checkpoints, receipt
  mutation hooks, TTL/migration and exact-WASM/budget tests still need integration.
  No new scan, release or Mainnet change. See REWARD_SETTLEMENT_DESIGN.md; do not
  deploy this library or claim withdrawal-time AQUA settlement is implemented.
- September 15 keeper freshness release is LIVE: dedicated branch/tag
  `aquarius-keeper-v0.4.1` pins reviewed commit
  `be5fb6cf544cdf4926b3f490c82fbbff027cb8c9`. DigitalOcean deployment
  `8efb6e62-cb4b-4c30-be85-ad789529a4ff` is ACTIVE, exactly one worker,
  DRY_RUN=false, RUN_REBALANCE=true, RUN_HARVEST=true, HARVEST_ON_START=false.
  All 29 tests, syntax checks and production npm audit passed; existing exact
  Almanax scan was reverified COMPLETE with zero findings. Package metadata in
  the scanned source remains 0.4.0; identify this release by Git ref and SHA.
  Dry-run stage passed before live activation. First live cycle at 14:34 UTC
  confirmed six refreshes across XLM/PYUSD/USDC, independently checked on-chain,
  costing 0.0129135 XLM. Four cycles through 15:34:50 UTC report zero failures;
  the hourly XLM check also performed a confirmed automatic rebalance.
  Compounding, reward floors, 10000-raw threshold, range policies and cadences
  are unchanged. No contract upgrade or manual harvest. First post-restart harvest
  check is due around 20:34 UTC / 22:34 CEST, subject to existing guards.
  This release does NOT implement withdrawal-time reward settlement.
  Transaction hashes, staging evidence and the 9.7530717 XLM keeper balance
  snapshot are recorded in Agents.md. Preserve all unrelated Margin work.
- September13 follow-up: PYUSD and USDC AQUA conversion floors were approved,
  independently price-checked and changed to3350 (1e7 scale). XLM18194, routes,
  1% slippage,10000-raw harvest threshold and ranges are unchanged. Both setter
  transactions succeeded, total fees0.0019086 XLM; subsequent harvest simulations
  passed with no skips and expected settlement11640/11577 raw units. No manual
  harvest was submitted. Existing automatic compounding remains enabled.
  Keeper freshness candidate `be5fb6c` (deployed September 15, see above): actual NAV
  timestamp checks before dependent preparation/signing, fallback deferral without
  restart loops, and compact transaction error logging. All29 tests passed;
  Almanax scan81f882fd-2658-42f1-bba5-d77036d42018 returned zero findings.
  At that checkpoint the worker was v0.4.0/f3d8b3e. The user's agreement was interpreted
  as retaining compounding plus withdrawal settlement of eligible remaining rewards.
  See `contracts/aquarius-lp-vault/REWARD_SETTLEMENT_DESIGN.md` for the proposal,
  source integration points and unresolved accounting proof/release requirements.
  Do not disable compounding or claim withdrawal-time reward settlement exists.
  Complete evidence and transaction hashes are in Agents.md.
- All three +/-40 migrations completed on Mainnet on 2026-09-09, after the
  individual proposal timelocks matured. Strategies now run the reviewed
  `00a1e9097339cbd1ee194a7a7f938d8d72918a7c95f89520c23c5ea2d4b8162e` WASM;
  ReceiptVault binaries are unchanged. Actual positions: XLM `[-40,40]`, PYUSD
  and USDC `[-60,20]`, each 80 ticks total. Policies are half40/margin20,
  cooldown3600/divergence100, enabled. Independent post-migration pool snapshots
  matched strategy liquidity: 60426561125 / 12081792690 / 12081852089 respectively.
  All pToken supplies, strategy shares and tracked deposits were preserved.
  Deposits/redemptions are open, CF=0, borrows=0, borrow breakers renewed.
  Full-holder withdrawal simulations passed for all three after migration; no
  deposits or withdrawals were submitted. All 36 rollout transactions succeeded,
  costing 0.5377548 XLM total; deployer retained 48.5861082 XLM. No repeated upload.
  Keeper v0.4.0 remains on its existing single-worker deployment: XLM checks hourly,
  stablecoin checks and NAV/cache maintenance every twenty minutes. Reward floors,
  conversion routes and harvest threshold remain unchanged; no APY is guaranteed.
  First post-migration keeper cycle completed at 09:50:44 UTC with zero failures,
  six maintenance refreshes and one XLM harvest. Decoded harvest events confirm
  0.3470593 AQUA converted into 0.0006317 XLM, without a harvest_skipped event.
  EURC and the unrelated Margin V3 rollout were not modified. Complete upgrade and
  rebalance hashes are in Agents.md. The proposal-only records below are historical.
- PYUSD/USDC +/-40 extension approved on 2026-09-07: reuse the exact uploaded
  `00a1e909...` candidate below for BOTH existing stablecoin settlement strategies.
  `scripts/rollout_aquarius_stable40_mainnet.sh` requires LABEL=PYUSD or LABEL=USDC,
  defaults to read-only, verifies uploaded bytes without another upload, and pins
  each strategy/market/pool/settlement binding. Proposed policy is half40/margin20,
  cooldown3600/divergence100; existing twenty-minute stablecoin checks stay unchanged.
  Each strategy needs its own 24-hour proposal. Submission requires
  CONFIRM_MAINNET=PROPOSE_STABLE40; execution requires MODE=execute and
  CONFIRM_MAINNET=MIGRATE_STABLE40, both with PREFLIGHT_ONLY=false. Proposal resource
  plus inclusion fees are capped at 0.11 XLM each. Do not rerun the XLM proposal.
  No contract/keeper code changes; existing candidate scans and WASM tests apply.
  Both proposals are now confirmed: PYUSD tx `9d057b4a...`, safe execution
  2026-09-08 19:55:24 CEST; USDC tx `ddaf2861...`, safe execution 19:55:35 CEST.
  Combined proposal fees 0.0847930 XLM; no repeated WASM upload. Both were executed
  on 2026-09-09 as recorded above. See Agents.md for complete hashes and ETAs.
  EURC/USDC is assessment-only: real FX exposure, live concentrated pool spacing 60
  means minimum half-width 120, separate EURC/USDC oracle prices are available,
  but the checked EURC sale quote is 1.586% below oracle fair value. Investigate
  before launch; do not relax guards or repurpose the existing EURC market.
- XLM-only +/-40-tick pilot candidate (2026-09-07): aligned width changes now
  trigger the existing guarded rebalance even away from the old range's edge.
  Cooldown, oracle/quote checks, balance-delta checks and atomic rollback remain
  unchanged; no storage or ABI migration is added. Proposed policy is half-width
  40, margin 20, cooldown 3600, divergence 100 bps. The later approved extension
  applies the same policy to PYUSD/USDC, as recorded above.
  Keeper v0.4.0 adds `XLM_REBALANCE_INTERVAL_MS=3600000`; stable range checks and
  all NAV/cache refreshes remain on twenty-minute cycles. Harvest threshold and
  reward floors are untouched. This is not a guaranteed yield increase.
  `scripts/rollout_aquarius_xlm40_mainnet.sh` defaults to a read-only proposal
  preflight. It pins the XLM strategy, production admin and optimized target
  `00a1e9097339cbd1ee194a7a7f938d8d72918a7c95f89520c23c5ea2d4b8162e`
  (60,556 bytes, `aquarius_lp_vault.xlm40.optimized.wasm`). It refuses another
  pending target and does not reset a matching timelock. `MODE=execute` enforces
  maturity plus 30 seconds, pauses only XLM, simulates the exact live rebalance,
  checks the actual 80-tick position and accounting, then restores availability.
  Failure after pausing leaves XLM paused for inspection. Full-range Almanax scan
  `96041ede-a26e-4005-a87c-64dedb658f4b` (`4054dbd..f3d8b3e`) returned zero findings.
  XLM proposal `f222d9f4b44364db131c956e362077eaa4a728d2d5446df34cc7be5645e5173d`
  is confirmed; safe execution is 2026-09-08 19:07:08 CEST (ETA + 30 seconds).
  Execution completed on 2026-09-09 as recorded above. CLI contract-data
  output is CSV containing JSON columns, so the executor parses the ETA column
  strictly. Final full-range rescan `c7aa7b9b-ae99-4c71-813b-d9474665ed6b`
  (`4054dbd..a23fc82`) also returned zero findings. Keeper v0.4.0 immutable ref
  is pinned at `f3d8b3e`; live deployment `2a44ce76-f82e-454b-9cd9-13a1eb3bdecd`
  is ACTIVE with one worker. First live cycle confirmed six refreshes and three
  negative range checks with zero failures. Upload/proposal charged 66.8843791 XLM
  total; the inclusion-fee cap does not cap Soroban resource/rent fees. Deployer
  balance afterward was 35.6168927 XLM. See `Agents.md` for the transaction trail.
  That proposal-only checkpoint is superseded by the verified live migration above.
- Keeper v0.3.0 (`b5a4aea`, immutable branch/tag `aquarius-keeper-v0.3.0`) adds a
  harvest-only threshold: simulate the exact harvest and sign only when expected
  settlement funds reach `HARVEST_MIN_UNDERLYING_RAW=10000` (0.001 XLM/PYUSD/USDC).
  Include existing settlement cash, converted rewards and pool fees; exclude raw AQUA
  and failed nested calls. Below-threshold results defer to the next twenty-minute
  cycle without failing/restarting the worker. Successful harvests retain the six-hour
  interval; NAV/cache refreshes and range checks continue independently. This is a dust
  deployment gate, not a profitability guarantee or atomic on-chain minimum.
  Conversion skips are logged, and the on-chain harvest cooldown is checked before
  simulation. Missing simulation evidence fails closed. No reward floors were changed.
  All 17 keeper tests pass; Almanax full-range scan
  `506550e9-bf92-4897-91b5-f152ebb7d712` (`3895b7a..b5a4aea`) returned zero findings.
  Mainnet simulation at ledger 64317866 deferred all three harvests with expected
  settlement balances 4267/5329/5320 raw units and completed with zero failed steps.
  DigitalOcean dry-run deployment `640f70ff-db3c-427b-8ce9-3dc765ecf656` passed all
  three gates and all maintenance simulations. Live deployment
  `00ef5021-83da-40c1-b5bb-eb75adf0581a` is ACTIVE on the exact release commit,
  with one worker, DRY_RUN=false, HARVEST_ON_START=false and RUN_REBALANCE=true.
  Its first live cycle at 2026-09-07 15:07 UTC confirmed all six refresh transactions
  (ledgers 64317888..64317893), no rebalances and zero failures. No harvest was due
  during startup; the first scheduled threshold check is six hours after startup.
- Users enter only through ReceiptVault; AquariusLpVault is permanently bound to one
  matching market and rejects direct deposits. Its internal strategy shares are
  non-transferable and may grow only for that bound ReceiptVault; users transfer the
  ReceiptVault pTokens instead.
- XLM/yXLM uses an XLM-settled strategy. PYUSD and USDC use separate settlement
  strategies and ReceiptVaults that share the same concentrated PYUSD/USDC pool.
- `harvest()` converts the configurable primary reward (AQUA at launch) and gauge
  rewards into each market's underlying, then redeploys. Reward token and route are
  independently admin-configurable. Each reward also requires a governance-set,
  1e7-scaled minimum raw-underlying/raw-reward rate; a missing or breached floor leaves
  the reward idle instead of trusting the route pool's own quote. Empty or failed
  permissionless harvests do not consume the cooldown; it starts only after value is
  actually claimed, converted, or deployed.
- Root token-transfer authorizations required by the deployed Aquarius ABI are guarded
  by post-call input balance-delta caps, and every swap also verifies that its actual
  output-token balance increase meets the independently computed minimum. ReceiptVault
  persists and bounds the boosted strategy's output-vector length at binding, so a
  multi-asset strategy quote outage cannot change the authorized redemption shape.
  If that append-only key is absent on an upgraded/archived market, a failed live quote
  triggers a zero-share shape probe and persists the recovered count before redemption;
  the admin setter remains the fallback when the strategy cannot answer even that probe.
  ReceiptVault always marks live NAV losses down;
  fixed-cash redemption sizing first rejects quotes below 90% of independent accounting.
  If that exact, cash-bounded exit fails and the strategy supplied a lower positive live
  quote, ReceiptVault retries once with enough shares for the same nonzero cash minimum.
  This keeps withdrawals live after a genuine large loss without letting a dust quote
  force an otherwise healthy strategy into a full unwind.
- Use an isolated LP-market Peridottroller, CF=0, and borrow paused. Deployment scripts
  require the controller address explicitly. Reuse the appropriate existing JRM while
  the markets remain supply-only.
- The temporary supply-only rollout points the isolated controller and all strategies
  directly at Reflector. Oracle symbol aliases map XLM and yXLM to `Other("XLM")`, and
  PYUSD and USDC to `Other("USDC")`. This assumes both pairs remain at par; keep CF=0,
  borrowing paused, the executable-quote deployment gates, and runtime pool-divergence
  guards. PriceRouter remains the preferred depeg-aware follow-up before collateral or
  borrowing is enabled, and it returns no pegged-asset price whenever the executable
  observation pool is unavailable rather than assuming even the configured floor.
  Extreme pool/oracle values that exceed their final numeric range are treated the same
  way: PriceRouter returns `None`, while AquariusLpVault retains only its bounded
  last-good NAV root. AquariusLpVault uses exact Soroban `U256` products for mul-div
  pricing and share accounting, so a representable result never becomes a saturation
  sentinel merely because the intermediate product exceeds `u128`.
- Past the strategy NAV stale bound, public boosted quotes fail soft. ReceiptVault
  redeems from its cached/accounting estimate with a nonzero cash minimum; only that
  protected exit may use the last NAV ratio, and the Aquarius quote must still satisfy
  the configured divergence guard. This preserves supplier exits during an oracle outage
  without making the stale value eligible for fresh deposits or unguarded swaps.
- Concentrated positions use a keeper-refreshed snapshot of their actual token0/token1
  composition, valued only at the independent oracle ratio. The keeper recenters only
  near a configured edge and only after a tighter two-sided pool/oracle check. A
  rebalance withdraws the old position, derives the exact token ratio from Aquarius'
  quote for the new spacing-aligned range, requires that ratio to match the independent
  live-tick/range geometry within five percentage points, performs one guarded
  excess-leg swap, and
  atomically fails unless a new position is minted with at least 95% of pair value
  redeployed. All three now target half-width 40 ticks (roughly 0.8% total price
  width), with a one-hour recenter cooldown. Range centers follow the pool tick grid.
- The supply-only Mainnet rollout completed on 2026-08-27 after the clean Almanax
  scan and live pool/oracle/route preflights. A live deposit/partial-withdrawal smoke
  test passed for all three markets on 2026-08-28. Isolated controller
  `CCZKDMAP…ENGC` owns XLM market `CBRJTPI…ZECZ`, PYUSD market `CBNVNCP…MLMA`,
  and USDC market `CBIOHQF…AZP7`; their strategies are `CB3WLG4…H6RW`,
  `CANCOWO…5EKY`, and `CAQZ7XP…KGIN`. The deployer retains 24 XLM, 4.8 PYUSD,
  and 4.8 USDC pTokens backed by live concentrated positions. The 2026-09-04 upgrade
  installed the final-holder ReceiptVault build and first concentrated strategy build,
  but its exact live XLM rebalance simulation exposed a range-ratio defect before any
  position mutation. The reviewed recovery migrated all three positions on 2026-09-06:
  XLM ticks `[-200,200]`; both stablecoin strategies `[-120,80]`. CF remains 0;
  the controller's defense-in-depth borrow
  breakers expire automatically after 72 hours and must not be mistaken for the durable
  supply-only control. The single-worker DigitalOcean keeper is live for NAV, reward
  conversion, and cache
  refresh; range maintenance is now enabled. Do not enable borrowing or
  collateral until cross-market footprint, depeg-aware PriceRouter, and boosted-
  valuation staleness work is redesigned and re-audited. See `Agents.md` for complete
  IDs, hashes, transactions, and next steps.
- The concentrated-range and final-holder release is implemented in `f2fcdca` with
  balance-delta deposit hardening in `4bfe469`, both pushed on `leveraged-fix`.
  Almanax full-range scan `4f97a6f1-24d9-4887-9a69-d2c3d4995761` found one High
  that `4bfe469` fixes; exact fix scan `3a290a92-65af-4fbf-8951-d2d428d8c598`
  completed with zero findings. After one balance-rejected upload created no state, the
  funded 2026-09-03 retry uploaded both candidate artifacts and staged all six exact
  24-hour upgrades. All six upgrades executed on 2026-09-04. The guarded executor then
  stopped on a no-send XLM rebalance: an aligned narrow range at the live tick needed an
  approximately 0.8972:1 XLM/yXLM ratio, while the deployed code assumed equal value and
  would have left 5.138% idle, just above its hard 5% cap. The simulation rolled back;
  all three positions and liquidities initially remained unchanged full-range. Deposits/redemptions
  were reopened, borrowing remains paused, and CF/borrows remain zero. The range-aware
  strategy-only fix is pinned in guarded recovery scripts. Final Almanax scan
  `e09ce891-d72f-4504-b97a-a381b6997ae7` over `4c2e48d..96ee13b` completed with zero
  findings. After funding, the reviewed strategy was uploaded and all three recovery
  proposals confirmed on 2026-09-05. All three upgrades and rebalances completed after
  maturity on 2026-09-06, with 36/36 successful transactions costing 0.6531179 XLM.
  Accounting invariants and all three full-holder withdrawal simulations passed;
  deposits/redemptions are open. Do not repeat the completed proposals or migration.
  Keeper v0.2 deployment `908b8045-5ffb-41fe-abbf-8ded8c87b6f8` is active from exact
  clean-scanned commit `4bfe469` with one live-signing worker and
  `RUN_REBALANCE=true`; its first six refresh transactions and all three range checks
  completed with zero failures. Harvest remains scheduled, with no startup harvest.
- The separate existing XLM/USDC/EURC markets have the clean-scanned ReceiptVault
  borrow-footprint fix staged under their 24-hour upgrade timelocks. All three target
  hash `5f35bc16…04e1` and mature on 2026-08-29 between 11:04:44 and 11:04:54 CEST.
  They still run the old code until the required pause/execute/verify/unpause sequence.
  `scripts/execute_receipt_vault_borrow_fix_mainnet.sh` defaults to a read-only,
  pinned-state preflight and gates mutation until 11:05:24 CEST, including a 30-second
  safety margin. It is resumable only with explicit acknowledgement of an all-paused
  prior attempt, leaves markets paused on incomplete verification, restores the exact
  original policy only after all checks pass, and never submits the final borrow.
- Full implementation, operational details, verified addresses, tests, and remaining
  gates are in `contracts/aquarius-lp-vault/CLAUDE.md` and `Agents.md`.
- The production keeper is `bots/aquarius-keeper`: one process services XLM, PYUSD,
  and USDC serially with a dedicated, fee-only signer to avoid Stellar sequence
  collisions. `.do/aquarius-keeper.yaml` defines a single 512 MiB DigitalOcean worker,
  automatic deployments disabled, starting in dry-run mode. Never commit its secret or
  scale multiple live instances with the same key.

### Key Patterns

- **Fixed-point math**: `SCALE_1E6 = 1_000_000` for rates/percentages (e.g., `600_000` = 60%). Borrow index uses `1e18` scaling.
- **`#![no_std]`**: All contracts. No standard library, no randomness, fully deterministic.
- **Auth**: Admin functions use `admin.require_auth()`, user actions use `user.require_auth()`. Liquidation hooks (`repay_on_behalf`, `seize`) only callable when vault is wired to peridottroller.
- **Lazy interest accrual**: Interest updates happen on user actions (deposit/withdraw/borrow/repay), not on a schedule.
- **Re-entry protection**: Cross-contract aggregation uses exclusion parameters to skip the calling vault.
- **Oracle staleness**: Price stale if `price.timestamp + k*resolution < now` (k=2 default). Missing prices treat collateral as 0 USD.
- **Events**: Single-tuple topics: `(Symbol("event_name"),)`.
- **Checked arithmetic**: Use `.checked_add()`, `.checked_mul()` etc. to prevent overflow. `overflow-checks = true` in release profile.
- **Cross-contract safety (FIND-039)**: Use `try_invoke_contract()` instead of `invoke_contract()` for all external contract calls to prevent account lockout from TTL-expired or malicious markets. Apply conservative fallbacks: collateral failures → $0, debt failures → skip market, token/price failures → skip market. Critical for `sum_positions_usd`, `exit_market`, and liquidation flows.
- **Core lending footprint**: ReceiptVault `get_account_snapshot()` must use its
  narrow, key-specific TTL/read path. Calling the broad `ensure_initialized()`
  from this controller hot loop makes the XLM/USDC/EURC three-market borrow
  exceed Soroban's 100-entry footprint (132 entries versus 81 with the narrow
  path). Keep `bump_ttl()` as the separate permissionless global keepalive.

### Storage

Contracts use `env.storage().persistent()` and `.instance()` for key-value state. Key enums are defined at the top of each contract's `lib.rs`.

## Workspace

Soroban SDK version: **25.0.0** (workspace dependency in root `Cargo.toml`).

OpenZeppelin Stellar contracts are a git submodule at `../../openzeppelin-stellar-contracts`. If builds fail with missing deps, run: `git submodule update --init --recursive`

## Deployment

Deploy scripts are in `scripts/`. The main flow:

```bash
export IDENTITY=dev
bash scripts/build_wasm.sh
bash scripts/deploy_testnet.sh        # deploys full protocol
bash scripts/verify_testnet.sh        # checks state
bash scripts/teardown_testnet.sh      # pauses everything
```

Contract invocations require `--` before function args:
```bash
stellar contract invoke --id <id> --source-account dev --network testnet -- deposit --user <addr> --amount 1000000
```

## Troubleshooting

- **"reference-types not enabled"**: Wrong build target. Use `stellar contract build`, not `cargo build`.
- **Missing OpenZeppelin deps**: Run `git submodule update --init --recursive`.
- **Test snapshots changed**: Test snapshots live in `contracts/*/test_snapshots/`. These are auto-generated; commit updated snapshots after intentional contract changes.
