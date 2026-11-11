> For the complete documentation index, see [llms.txt](https://docs.aqua.network/llms.txt). Markdown versions of documentation pages are available by appending `.md` to page URLs; this page is available as [Markdown](https://docs.aqua.network/aqua-and-ice/ice-tokens-locking-aqua-and-getting-benefits.md).

# ICE tokens

Lock AQUA to receive ICE — non-transferable tokens that carry voting power and boost liquidity rewards.

ICE is locked AQUA. Locking converts AQUA into non-transferable ICE tokens that carry liquidity-voting power, governance-voting power, and a [reward boost](/aqua-and-ice/ice-boosts-how-to-maximize-lp-rewards.md) for liquidity providers. ICE cannot be sent, traded, or deposited anywhere.

<figure><img src="https://3649863591-files.gitbook.io/~/files/v0/b/gitbook-x-prod.appspot.com/o/spaces%2F5QUy59hBUnouBhx8gNSS%2Fuploads%2FiCjrd2mtqnfxevQwrE1q%2FICE.png?alt=media&amp;token=d1f3f1bd-8b50-4018-af5c-3c6cd2fe10bb" alt="The ICE token symbol and Aquarius branding"><figcaption></figcaption></figure>

## How much ICE you get

The amount of ICE depends on how long you lock. The **lock multiplier** grows linearly with the time until unlock and caps at 10x at 1,095 days (3 years):

```
ICE = AQUA locked × (1 + 9 × min(days until unlock / 1095, 1))
```

| Lock duration   | Lock multiplier | 10,000 AQUA locked gives |
| --------------- | --------------- | ------------------------ |
| 3 years or more | 10x             | 100,000 ICE              |
| 2 years         | 7x              | 70,000 ICE               |
| 1 year          | 4x              | 40,000 ICE               |
| 6 months        | 2.5x            | 25,000 ICE               |

The lock multiplier is not the reward boost: locking determines how much ICE you hold, and the up-to-x2.5 [reward boost](/aqua-and-ice/ice-boosts-how-to-maximize-lp-rewards.md) is what that ICE earns you as a liquidity provider.

## How ICE melts

Your ICE balance is not fixed. The protocol recomputes the expected amount daily from the formula above, and as the unlock date approaches, `days until unlock` shrinks — so ICE decays linearly toward 1x the locked AQUA at the unlock date. Excess ICE is reclaimed from your wallet and from your votes in batches throughout the day.

For example: 10,000 AQUA locked for 2 years starts at 70,000 ICE, melts to 40,000 ICE with one year remaining, and reaches 10,000 ICE on the unlock date — when the AQUA itself becomes reclaimable.

To maintain voting power, extend your lock or lock additional AQUA before melting erodes your balance.

## Locking AQUA

Locking happens at [aqua.network/locker](https://aqua.network/locker) and is executed entirely on the Stellar blockchain: the locker creates a claimable balance that only your wallet can reclaim once the selected lock period expires. Until then, the locked AQUA cannot be retrieved. For the interface walkthrough, see [Locking AQUA into ICE](/user-guides/how-to-use-aqua-locker-tool-and-get-ice-tokens.md).

## The ICE token family

Locking mints three non-transferable tokens to your wallet:

| Token     | What it does                                                                                                                                                           |
| --------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| ICE       | Tracks how much AQUA you have locked and determines the distribution of upvoteICE and governICE. No direct operational use.                                            |
| upvoteICE | Votes for markets at [aqua.network/vote](https://aqua.network/vote) — eligibility thresholds are covered in [Aquarius voting](/voting-and-rewards/aquarius-voting.md). |
| governICE | Votes on [governance proposals](https://aqua.network/governance).                                                                                                      |

{% hint style="info" %}
A fourth token, downvoteICE, was used for downvoting markets. It was deprecated in June 2026 and can still appear in older wallets.
{% endhint %}

Delegation mints two more tokens — dICE and gdICE — covered in [ICE delegation](/aqua-and-ice/overview/tokens-for-delegated-voting-dice-and-gdice.md).

## Why lock

* **Vote.** Liquidity voting and governance both run on ICE — locking is how AQUA holders get a say. upvoteICE votes can be withdrawn at any time, and governICE is retrievable as soon as a proposal concludes.
* **Vote everywhere at once.** The separate tokens let you participate in liquidity voting and governance simultaneously.
* **Earn more.** Holding ICE [boosts your AQUA rewards](/aqua-and-ice/ice-boosts-how-to-maximize-lp-rewards.md) as a liquidity provider, up to x2.5.
