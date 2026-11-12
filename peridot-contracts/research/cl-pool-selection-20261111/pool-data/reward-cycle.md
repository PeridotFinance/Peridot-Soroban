> For the complete documentation index, see [llms.txt](https://docs.aqua.network/llms.txt). Markdown versions of documentation pages are available by appending `.md` to page URLs; this page is available as [Markdown](https://docs.aqua.network/voting-and-rewards/reward-cycle.md).

# Reward cycle

When votes are counted and rewards are paid — the continuous, hourly, daily, weekly, and monthly rhythms of Aquarius.

Aquarius runs on several overlapping cycles. Votes are counted continuously, reward allocations refresh daily, Stellar DEX (SDEX) rewards pay hourly, AMM rewards accrue every block, voting incentives run in weekly rounds, governance votes in weekly slots, and delegate rewards pay monthly. This page shows when each thing happens; the mechanics live on the linked pages.

## The rhythm at a glance

| When (UTC)              | What happens                                                                                                                                                                            |
| ----------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Continuously            | Votes are tracked; [AMM rewards](/voting-and-rewards/aquarius-amm-rewards.md) accrue every block; [Pool Incentives](/for-projects/pool-incentives.md) stream over their campaign period |
| Every hour              | [SDEX rewards](/voting-and-rewards/sdex-rewards.md) are calculated shortly after the hour and paid out around half past                                                                 |
| Once a day              | Reward allocations resync from the latest [vote standings](/voting-and-rewards/aquarius-voting.md) — at an unpredictable time within the day, so the update moment can't be gamed       |
| Once a day, random time | Each market with voting incentives takes its payout snapshot and pays [voting incentives](/for-projects/what-are-voting-incentives.md) to voters                                        |
| Sunday 18:00            | Deadline to submit voting incentives for the following week                                                                                                                             |
| Sunday 19:00            | Voting incentives for the coming week are collected and validated                                                                                                                       |
| Monday–Sunday           | Approved voting incentives are distributed linearly, one payout per day                                                                                                                 |
| Monday–Sunday           | One governance or asset proposal votes per weekly [voting slot](/user-guides/how-to-use-aquarius-governance.md#proposal-queue-and-voting-slots), Monday 00:00:00 to Sunday 23:59:59     |
| Monthly                 | [Delegate rewards](/aqua-and-ice/overview/reward-program-for-delegates.md) are paid, computed from daily delegation snapshots                                                           |

## What this means in practice

* **Your vote doesn't move rewards instantly.** Votes are counted as soon as they confirm, but reward allocations refresh once a day — expect a market's rewards to reflect new votes within a day, not within minutes.
* **SDEX rewards arrive hourly, automatically** — your wallet needs an AQUA trustline at payout time, or the payout skips you.
* **AMM rewards accrue every block but are not pushed to you** — claim them in [My Aquarius](/user-guides/dashboard.md) or through the [claim function](/developers/code-examples/claim-lp-rewards.md).
* **Voting incentives snapshot at a random time each day.** Keep your votes on the market with voting incentives around the clock and hold a trustline for the voting incentive asset — adjusting votes can mean missing that day's snapshot.
* **Voting incentives operate on whole weeks.** A voting incentive submitted before Sunday 18:00 UTC starts paying the following Monday; submitted after, it waits an extra week.
