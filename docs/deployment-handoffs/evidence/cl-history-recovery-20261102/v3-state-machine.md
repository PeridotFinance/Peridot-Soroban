# Public V3 state machine (source review; not every route is compiled-proof complete)

Authoritative implementation is `margin-controller/src/contract.rs`, `perps.rs`, `cl_custody.rs`, `funded_liquidation.rs`. Current order is **begin→swap→activate**, with `execute_open_position_v3` an alternate activation finalizer after swap. Shared00’s begin/execute/swap/activate ordering needs coordinated policy reconciliation; this report flags it and does not independently edit00.

| Public entrypoint | Before → after | Custody / debt | Deadline and permitted recovery |
|---|---|---|---|
| begin_open_position_v3 | no position → PendingOpen unswapped | reserves user margin pTokens; debt0; immutable route/vault snapshots | open expires +1800s; owner cancel or permissionless expiry can refund only unswapped/debt0 |
| swap_open_position_v3 | PendingOpen unswapped → PendingOpen execution stored | borrow+redeem cash+facade conversion atomic; controller holds underlying CL shares with beneficiary assignment; debtlive | must be before openexpiry; failedcall rollback; later activation or funded recovery, never unsecured cancel |
| activate_open_position_v3 | PendingOpen swapped → Open | controller shares deposited into receiptmarket, beneficial owner preserved; debt retained; freshhealth/reserve checks | expiry does not prohibitactivation; failure rollsback entire activation leaving swappedpending; funded recovery remains |
| execute_open_position_v3 | PendingOpen swapped → Open | same activation implementation | before swap rejects; after activation repeat rejects; no separate pre-swap execution stage |
| cancel_pending_open_v3 | PendingOpen unswapped → removed | debt must0 and execution absent; refund margin+openfee | owner only; rejects swapped and activated positions withoutmutations |
| expire_pending_open_v3 | expired PendingOpen unswapped → removed | debt0/executionabsent required; same refund | permissionless strictly timestamp>expires_at; swappedexpiry cannot discard collateral/debt |
| resolve_pending_open_v3 | expired PendingOpen swapped → Open | same activation with currenthealth, receiptcustody and reservechecks | permissionless strictly afteropenexpiry; failedhealth/claim leavespending |
| recover_pending_open_funded | PendingOpen swapped Long → removed | owner external full debt repayment precedes directsharesreturn atomically; no voluntaryrewardwaiver | validation-only; owner; no liquidation/liveposition bypass; maxrepay bounded; unavailableunswappedwithout execution |
| add_position_collateral_v3 | Open → Open | consumes existing free margin receiptpTokens and addspositioncollateral; debtunchanged | owner; no pendingclose; despite outdated ABIcomment it requiresOpen |
| repay_margin_position_v3 | Open/Closing → same | owner external repayment to debtmarket; collateral stayslocked; debt decreases | owner; amount>0; repayment doesnot byitselfrelease |
| release_debt_free_position_v3 | Open/Closing → removed | refreshinterest; requireszero debt; releases appropriate receiptpTokens/controllerassets to owner/free margin | owner; route dependscurrentclosephase; cannotrelease whiledebtremains |
| close_position_v3 | Open Long → removed | atomic withdrawal/conversion/repayment/release | owner; no pendingclose; Short requires splitroute; failuresrollback |
| begin_close_position_v3 | Open → Open + preparedclose | receiptcollateral locked; debtretained | owner; createscloseexpiry; cancelpreparation or withdrawbeforeexpiry |
| prepare_close_position_v3 | Open → Closing | atomic begin+withdraw; controller holdspositionunderlying withbeneficiary context; debtretained | owner; closeexpiry; cancel/expire only beforeconversion |
| withdraw_close_position_v3 | Open+preparedclose → Closing | receiptcollateral redeemed into controller underlying | owner; preparation notexpired; refreshcloseexpiry; no repeat |
| swap_close_position_v3 | Closing unconverted → Closing converted | underlying converted to debtasset heldcontroller; debt notyetrepaid | owner; fixed route/slippage; thenfinish; postconversion cancellationforbidden |
| swap_close_short_position_v3 | Closing Short unconverted → Closing converted | boundedcollateral swap coversdebt; remainder belongsowner | owner; minoutput coversdebt; finishreleases remainder; no wholesaleunnecessaryconversion |
| finish_close_position_v3 | Closing converted → removed OR Closing residual | repay fromheldproceeds; release onlyif debtzero; residualdebt staysrecorded | permissionless; interest canleave residual; fullrepayment/release or eligibleliquidationrecovery |
| cancel_close_position_v3 | Open prepared / Closing unconverted → Open | clears preparation or redeposits heldunderlying into receiptmarket | owner; onlybefore conversion; preservesdebt; restore itself mayfail claim/valuation atomically |
| expire_close_position_v3 | expired Open prepared / Closing unconverted → Open | same restoration as cancellation | permissionless strictlyaftercloseexpiry; convertedclose cannotbe discarded |
| replace_position_pool_v3 | Open/Closing/Liquidated → same | only routeconfiguration changes; custody/debt unchanged | admin; oldroute mustdisabled, replacementallowed; no arbitraryhealthyroute switch |
| liquidate_position_v3 | unhealthy Open OR expired swapped PendingOpen → settled | existing legacy atomic swap liquidation route; expired pending route atomically activates without initial health acceptance, then requires liquidation eligibility before closing | external liquidator cannot owner; pending route strictly after open expiry with execution present; all activation/liquidation effects rollback if healthy or any downstream call fails; distinct from funded in-kind route |
| begin_liquidation_v3 | unhealthyOpen / eligibleClosing → Liquidated pending | debtlive; receiptcollateral or heldunderlying pinned; maytakeoverexpired residualclose | liquidator cannotowner; deadline+liquidationTTL; healthyresidualclose recovery gets0 incentive |
| swap_liquidation_v3 | Liquidated Started → CollateralConverted | convert pinnedcollateral; debtasset proceeds heldcontroller; borrower retains surplus entitlement | original liquidator beforetakeoverdeadline; authorizedother after; no unbackedrelease |
| finish_liquidation_v3 | Liquidated CollateralConverted → removed | repayfromproceeds; existingroute canabsorb residualbaddebt, incentivefromsurplus, remainderborrower | originalliquidator beforedeadline or takeoverafter; this is NOT fundedroute noritsatomicdebt/seizureproof |
| liquidate_funded_v3 | liquidatableOpen → removed | external boundedfullrepayment + permittedreceiptpToken seizure atomically; remainingcollateralborrower; no swaps/baddebtwriteoff | validation-only; freshprices/interest, pause/lockguards; forced transfer recordsrecoverywithoutwaiver; repeat rejects |
| preview_open_fees_v3 | unchanged | viewonly feequote | no recoverytransition |
| preview_liquidation_v3 | unchanged | current riskquote; mayrefreshcaches/TTL | no seizurepermission byitself |
| get_pending_perps_open / get_pending_perps_open_execution / get_pending_perps_close | unchanged | views explicitintermediate records; TTLrenewal | no recoverytransition |
| get_perps_position / get_pending_liquidation / get_liquidation_takeover_after | unchanged | views activation/liquidation terms anddeadlines; TTLrenewal | no recoverytransition |

Compiled tests in `recovery_state_tests.rs`:
- `compiled_cancel_after_execute_is_rejection_then_repay_release`: explicitexecute-before-swap rejection; swap; explicitexecute activation; historical rewardcheckpoint; pendingcancel and repeatedexecute rejection with complete snapshots unchanged; authorized fullrepayment/release succeeds. Required `cancel_after_execute` binds this composite test to measured `cancel_after_execute_rejection`, not a successful cancellation.
- `compiled_pending_before_swap_expiry_and_abandonment`: prematureexpiry rejection; expiredswap rejection; permissionlessexpiry refunds with debt0 and positionremoved.
- `compiled_failed_activation_preserves_pending_recovery`: killedclaims makeactivationfail; snapshots prove debt/collateral/rewards rollback; pendingcancel and swappedexpiryreject; fundedrecoveryafterexpiry preserves positivebookedpending+fraction exactly.
- `compiled_expired_swapped_permissionless_resolution`: premature resolution rejects; permissionlessresolution afterexpiry activates; pendingcancel rejects; authorizedrepayrelease succeeds.

These tests retain exact scoped external auth and actual contract authorization. They do not introduce unsafe cancellation or change production maintenance margin. Routes outside this mapping’s tested subset are source-reviewed only; missing adversarial/live-state proofs remain explicit.
