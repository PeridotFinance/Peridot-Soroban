# Independent Claude Fable5.2 review

Requested alias `fable`, effort `max`; CLI resolved `claude-fable-5-2`.
Tools disabled; reviewed only the user-approved bounded tables/design excerpts.
The model did not access or verify original archive bytes or all84 call nodes.
No contract execution, signing or broadcasting.

**Verdict: NOT QUALIFIED.** The table arithmetic holds and the largest-call statement is valid as an observation, but no stage of the proposed design has a complete cost. The proposed order (mint against cash, invest later) also shifts losses onto incumbents; reversing the two steps removes most of that.

This is hand arithmetic and reasoning on the payload only. I did not check the archive hash, the 84-node claim, or any Aquarius behaviour.

## Measurement

- **Sums reproduce.** All six column totals match: 167,831,028 / 162,817,228 (open), 103,955,308 / 100,785,452 (later open), 141,545,386 / 137,759,524 (close). No listed path is a prefix of another, so adding inclusive figures does not double count.
- **Not checkable here.** The tables show 31 rows (12 + 11 + 8), not 84 nodes. Child nodes and the reconciliation to root are not in the payload.
- **Label to fix before publishing.** Activation is quoted as 181 SDK / 145 unique in one place and 158 SDK in another.

**Is the largest-call conclusion valid?** Yes, worded as "largest observed in this capture": `deposit_position` 64,233,503 and `withdraw_position` 62,561,356. Three limits belong next to it:

1. **Single sample.** Each call appears once. The same payload shows same-function spread elsewhere: `swap` 6,953,963–11,100,517 and `estimate_swap` 4,957,303–9,167,680.
2. **Pre-state.** By path order, both ran right after `claim_all_position_fees` in the same transaction. The proposed INVEST and close run them with fees unswept, which was never measured.
3. **Basis.** The figures are CPU only, on the SDK basis. The prior mint read 131.864M SDK against 132.305M host, and nothing here gives memory, entries, unique keys or writes per call.

The stronger conclusion the data supports is pairwise. Fee sweep plus deposit is 118,185,789 and fee sweep plus withdraw is 115,815,792, so in this state the sweep cannot share a transaction with either position change.

## Stage budgets from measured components only

| Stage | Measured venue calls | Sum | Left for everything else |
|---|---|---:|---:|
| MINT-CASH | claim + fee sweep + composition estimate | 64,919,682 | 35,080,318 |
| INVEST | kill check + deposit estimate + deposit + composition before and after | 79,381,249 | 20,618,751 |
| Close withdraw-and-burn | claim + withdraw | 68,507,403 | 31,492,597 |

"Everything else" (oracle reads, share and reward accounting, command update, auth) is unmeasured for these shapes. For scale only, the prior mint's non-venue remainder was about 23.3M (131.864 − 108.6), which is more than INVEST has left. The fee getter the close depends on has no row in any table.

## Does mint-then-invest avoid the dependency without shifting losses?

It removes the budget coupling, and the prior numbers agree it had to: with the witness, swap quote and composition read all deleted, the old mint is still about 101.3M. It does not remove the economic coupling.

1. **Carry.** Let S be incumbent shares and s the entrant's. Between MINT-CASH and INVEST, the entrant's shares take s/(S+s) of fees and AQUA earned entirely by incumbents' liquidity. This meets the letter of claim-before-mint and sweep-before-mint, not their purpose. The entrant also gains by delaying INVEST, and an underfunded reserve blocks it indefinitely ("assets remain cash-backed"). "No free entrant upside" cannot be proven for this order, only bounded by window length.
2. **Deposit price risk.** Once the cash is common NAV, a deposit made while the pool price is pushed off market costs every holder when the price reverts, after the INVEST transaction. A before/after NAV built from withdraw-estimate amounts priced at the oracle will show no loss. That mark is at its minimum when pool and oracle agree and overstates otherwise. Charging the reserve correctly needs the position valued at the oracle price, or a pool-versus-oracle band, which adds unmeasured work to the tightest transaction.
3. **Oracle exposure at mint.** A NAV-priced mint with free composition lets the entrant contribute whichever token the oracle currently overvalues. A composition-matched mint needs no oracle and, by my algebra, can only overcharge an entrant who mints at a pushed price.
4. **Contradiction.** The cash is "committed to common NAV" yet INVEST uses a per-command "allocation". Any exit in between takes its pro-rata slice of that cash. Either it is common and there is no allocation, or it is earmarked and the new shares are not backed like the old ones. Unused tokens also become idle cash every holder carries.

## Can withdraw collect before the burn and apportion fees?

The formula is right. With burned liquidity floor(L·q/S) and the exiting holder taking floor((I+F)·q/S), remaining holders' per-share liquidity and cash cannot fall. Sweep plus withdraw does not fit, so this is the only single-transaction route, and it rests on four venue facts not evidenced here:

- **Payout:** `withdraw_position` pays all pending fees. The captured withdraw ran after the sweep, so it shows nothing either way.
- **Single position:** "All position fees" suggests several are possible. A pro-rata burn across N positions needs N withdraws at about 62.5M each, and fees on untouched positions would stay uncollected at the burn.
- **Getter:** an exact pending-fee getter exists, is cheap, and rounds the same way as the payout. If it reads low, the difference goes to the exiting holder.
- **Cost:** withdraw with fees pending has not been measured.

## Other gaps

- **Lenders.** Owner-set minimums protect the owner, not lenders. At 2x a borrower can set loose minimums, trade against their own conversion or deposit, and leave the shortfall to lenders, because health is rechecked only at activation. Each value-moving step on a levered command needs a protocol-enforced oracle floor or its own health check.
- **Rescue in every state.** Prior rescue was 50 or 52 writes and 152–176 entries. A possibly smaller shape is for the rescuer to repay and have the command record reassigned or split, with no asset movement. A reward claim would be needed only when minted shares change beneficial owner. This is a hypothesis, unmeasured.
- **Rewards outage.** The payload does not say how a historical entitlement is computed when the claim amount cannot be observed, or how a mid-open command with claim boundaries still ahead exits. The outage branch must be reachable only from venue state, never from anything an executor controls.
- **Stuck states.**
  - Pinned config revision should gate forward steps only, not cancel, repay or rescue.
  - A stale oracle with collateral in two tokens blocks conversion to the debt asset.
  - There is no terminal state when proceeds fall short of debt and no payer appears.
  - "Refund unused reserve only after completion" strands the reserve on cancel.

## Revised minimum approach (hypothesis, not proof)

Reverse the order: deposit segregated first, then sweep and mint.

1. Fund and optionally convert, as proposed.
2. **Deposit, no shares.** Command cash goes into the vault's position. The command is credited with the liquidity ΔL the venue reports, and unused tokens stay the command's. The share set is unchanged, so the rules as written require no claim or sweep. Unminted liquidity earns nothing for its owner; all fees and AQUA keep flowing to existing holders.
3. **Sweep and mint.** Claim, sweep fees into NAV, then mint floor(S·ΔL/L) shares, where L is already-minted liquidity. The entrant tops up its pro-rata slice of common idle cash in kind.
4. Deliver, custody and activate, as proposed.

The entrant then bears carry, deposit price, rounding and remnants by construction rather than by measurement. The incentive is to finish quickly, and incumbents need no oracle at mint. On measured components, the deposit stage has 30,160,723 left and the mint 39,851,304.

Three caveats:

- **Policy choice.** It embeds "pre-mint liquidity earns nothing", which is yours to approve, not mine.
- **Still unproven.** A deposit with fees unswept must preserve them, at unknown cost. Exits during the window must use minted liquidity only. Cancel before mint needs the same principal/fee separation as the close.
- **Leverage.** Levered commands still need the price band at deposit.

## What would qualify it

- Deposit and withdraw measured with fees pending, over several price and tick states.
- Fee getter cost and rounding parity, withdraw payout semantics, and a single-position invariant.
- CPU, memory, entries, unique keys and writes for every stage and every cancel, rescue and outage path, on the host basis.
- The new activation and rescue ABIs.

Until those exist, every stage in the published table should read NOT QUALIFIED.
