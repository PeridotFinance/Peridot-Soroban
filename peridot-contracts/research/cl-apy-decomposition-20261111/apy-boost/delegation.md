> For the complete documentation index, see [llms.txt](https://docs.aqua.network/llms.txt). Markdown versions of documentation pages are available by appending `.md` to page URLs; this page is available as [Markdown](https://docs.aqua.network/aqua-and-ice/overview/tokens-for-delegated-voting-dice-and-gdice.md).

# dICE & gdICE

dICE and gdICE represent delegated ICE for markets and governance. They let delegates vote on behalf of holders.

When ICE is delegated, it is represented by special non-transferable tokens inside the Aquarius system. These tokens allow delegates to vote on behalf of others without ever taking ownership of their ICE.

There are two kinds of delegated tokens:

***

### dICE – delegated upvoteICE

* **Purpose**: Market voting
* **What it represents**: The upvoteICE delegated by holders to a delegate.
* **How it’s used**: Delegates use their dICE to vote on which liquidity pools receive AQUA emissions.
* **Rewards**: Any voting rewards flow back to the original ICE holder (the delegator), not to the delegate.

***

### gdICE – delegated governICE

* **Purpose**: Governance voting
* **What it represents**: The governICE delegated by holders to a delegate.
* **How it’s used**: Delegates use their gdICE to vote on DAO proposals, parameter changes, or upgrades to the protocol.

***

### Shared properties

* **Non-transferable** – dICE and gdICE cannot be moved or traded; they only exist within Aquarius to reflect delegated power.
* **Minted/Burned automatically** – created when ICE is delegated and removed when delegation is withdrawn.
* **Delegate-only** – visible in the delegate’s balance and usable only for casting votes.
* **Control stays with the delegator** – delegators can revoke or reassign at any time (with a 24-hour cooldown).

***

### Why this design matters

* **Transparency** – anyone can see how much delegated power a delegate holds.
* **Security** – delegates never own the underlying ICE; they only get temporary voting tokens.
* **Flexibility** – by separating dICE and gdICE, delegators can assign different delegates for markets and for governance.

***

### Token addresses

* [dICE on Stellar Expert](https://stellar.expert/explorer/public/asset/dICE-GAXSGZ2JM3LNWOO4WRGADISNMWO4HQLG4QBGUZRKH5ZHL3EQBGX73ICE-1)
* [gdICE on Stellar Expert](https://stellar.expert/explorer/public/asset/gdICE-GAXSGZ2JM3LNWOO4WRGADISNMWO4HQLG4QBGUZRKH5ZHL3EQBGX73ICE)
