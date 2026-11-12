# Stage1 ABI/key-inventory review — bounded approved design payload
Review only this excerpt. Fable HIGH effort, tools disabled. No implementation, files,
network, signing or independent verification. Return actionable issues in ABI, key
inventory, authorization, recovery and proposed tests. You review; the lead decides.

Accepted7f66fb05 closes resource feasibility. Aquarius stays, Option1 CLOSED. Stage1
1x PYUSD/USDC ONLY; no debt or margin custody. Limits100MCPU/41943040memory/100SDKtotal/
100uniqueSorobankeys/50writes. Cash-first mint BEFORE later keeperinvestment approved;
deposit-first rejected. No build until plan approved. LegacyMainnetretired, no realkeys/
funds, only privatefixture signing permitted. Fullrange/simplewide, noactiveoptimizer.
Prototype fixed[-2000,2000]/equaldecimals areTESTassumptions, not production settings.

Accepted warm rows CPU/SDKtotal/keys/RW:
prepare7397126/16/13/3;fund3754855/17/12/5;convert31245907/30/22/8;
feecheckpoint+cashmint72062767/69/50/19;invest95144812/61/43/18;
prepareclose2724937/13/10/3;directinkind78728736/78/52/26;
allocateforconversion78061427/74/50/24;convert+payout23786729/32/23/9.
Nine ROW TYPES,7txdirect or8txconvertedjourney. Coldinvest95205766/61/43/18;
coldexit78791924/78/52/26 after3signedRestoreFootprintbatches each SAMECoreledger.
416.6882987/417.0785571syntheticXLM independentcoldjourneys, notadditive lifecyclecost.
Actual Core29.0.0 application/signatures vs host29.0.1; full financialparity outstanding.
Nativeenvelopesignatures outsideSorobanCPU; nativeRestoreCPU/memory notSorobanmeasured.

## Proposal: ownership and ABI
Measuredcallpath receipt->vault->Aquarius/tokens/oracle. NO facadecall in9roots.
Receipt stores command, vault holds cash/LP/reserve/exitclaims, direct owner holds
shares lockeduntilinvestment/cancel. No receipt->vault->receipt callback (reentry).
Receipt passes its stored typed command under real contract authorization. Vault
requires exactlyboundreceipt, validateskind/phase/wiring/assets/bounds/beneficiary;
receipt validates persistedphase/deadline/revision. Atomic stateupdates/rollback.
No duplicated percommand vault guard planned: receipt is sole state authority;
reviewedboundreceipt code trusted, no usercaninvoke internalstage with fabricatedjob.

Proposed publicreceiptABI (namesprovisional):
- stage1_config(),stage1_job(id): read-only, no incidental broadTTLrenewal.
- prepare_open(owner,OpenTermsV1)->id ownerauth; fixedowner=payoutrecipient; amount,
  swap_in,min_swap,min_shares,max_strategy_cost,deadline,configrevision.
- fund_open(id) ownerauth exactnestedtransfer(owner,vault,contribution+reserve).
- convert_open(id),mint_cash(id),invest_cash(id): permissionless under storedterms,
  currentprotocolchecks; no caller-suppliedjob, recipient, route ormutablelimits.
- prepare_close(owner,shares,CloseTermsV1)->id ownerauth; explicitmodeInKind or
  ConvertToUnderlying replacing min_swap==0sentinel; tokenminima,deadline.
- withdraw_close(id): two measuredbranches, directmixturepayment orreservedmixture.
  Feeallocation+burn atomic. Directpayoutsamecall, nothiddenlaterstage.
- convert_close(id): freshquote/oracle/band, minoutput, fixedownerpayout.
- cancel(id): ownerbeforeexpiry, permissionlessafterexpiry; NEVERwaiverparameter.
  beforemintreturnsactualsegregatedmixture+reserve; aftermintrefundsreserve+unlocks
  sharesONLY, notmislabelledmixtureexit; closelockedunlocks; postburnmixturepaysowner.
- recover_in_kind(id,accept_reward_waiver): ownerauth; allowedpostmintlocked or
  closelocked only; actualproportionalcommonbacking (not old cashamount), samefee
  allocationbeforeburn, authorizedrewardrecovery. False+missingauthenticboundary
  revertsRewardWaiverRequired per00; truewaivesonlyownerunknowninterval, notoldbooked
  claims/others/lateraccrual. Absentowner: expiryexecutorunlocks/refunds, cannotwaive
  orforceburn. Investedterminal ownerusesnewclose/holderemergencyredeem; no idreuse.
- No termamendmentinfirstbuild: cancel/newcommandiflimitschange. Expiry/revision/
  pause/capblocksnewrisk, not fixedbeneficiarycashpayout/unlock orauthorizedrecovery.

Vault internalABI mirrorscheck_open/accept_funds/convert_open/cash_mint/invest_cash/
lock_close/withdraw_kind/convert_exit/cancel_kind +authorizedrecoveryvariant. Bound
receipt,id,versionedcommand under require_auth. Fundalsoownerrequire_auth; vault
exactauthorize_as_current_contract fordeeptransfers. Normalmint/burn/transfer claim
first in SAMEownershiptransaction includingzeroresult; custodyhooks retained but
notartificiallyexercisedin1x. Investmentnotownershipchange: no gratuitousclaim and
no earlierclaim substitutes laterrequiredone. Alltransfer_from/burn/legacyroutes
respectlockedshares. Existingatomicentrypointsnotenabledonnewstageddeployment;
preservebaseline. No arbitrarypausedfullblockonrecovery exceptactualtoken/venuefail.

Facadecurrent synchronous initialize/get_tokens/estimate_swap/swap/pause/controller/
sweep stays separate. No newstagedexecutionABI or extra facadehop for1x. NewStage1
releaseleavesfacadeunbound/disabled; quote0 or typedAsyncStageRequired ifattempted
routing to stagedvault. It holdsnoStage1funds/claims. Oldswap cannotpromiselateroutput.
Howbest enforcefailclosed compatibility withoutaddinghotpathcost? Reviewthis.
Mainnetvalidationguard retained. Later2xABI isdocument-onlyseparatemilestone.

## Key inventory
LogicalRust enumvariants DON'T automaticallynamespaceScValkeys. NewStage1variants
explicitlyprefixed. Fields inonecontractinstance are ONEphysicalkey; totalSDKmemory
reads+diskreads+writes distinctfromuniqueSorobankeys; nativefee/seq andTTL distinct.

Receipt existingpersistent Initialized,UnderlyingToken read in everycapturedroot.
Prototypeinstance Vault,Next,Busy +persistentJob(id). Plannedinstance Stage1ConfigV1
(vault,schema,pair/configrevision),Stage1Next,Stage1Busy; persistentStage1Job(id):
boundedversionedowner/terms/phase/amounts/reserve/shares/result. Solecommandauthority.
No onchainvectorofjobs/holders. Eventindexoffchain; exactonchainrecordauthoritative.
No firstbuildterminalpruning; absolutelynopruningunresolvedentitlements/liveclaims.

Vaultexistinginstance Config(pool,tokens,decimals,index,ticks,oracle),Params(caps,
pauses,oracleage,slippage/band),State(supply,liquidity,root),Admin,Initialized;
accountingBusy/Book(token)/Excluded(token), rewardIndex/Count/Interval/Open/Totals.
Stage1Receipt andStage1Reserved(token) newnamespacedinstancefields, outsideserialized
legacyConfig. Reservedaggregates=unmintedcash+unusedexecutionreserve+postburnclaims;
excludedNAV. MintreleasescontributedcashintocommonNAV, neverdoublecountreserve.
Postmintj.a/j.b aredeploymentintent, NOTsegregatedproperty. Otherexitsmayconsumeidle
cash; insufficientinvestmentcash permitsretry/cancel, nointerimfundingguarantee.

Vaultpersistent Shares(owner),Stage1Locked(owner),Stage1Allocation(id) boundedfee
record (oldsupply,shares,LPburn,fees,principal,allocated,retained),PositionAmounts,
OracleSymbol(token),primaryreward/wiringasperexistingtypes. RewardHolder(owner):
debt/pending/fraction;Boundary(id),Bucket(interval),Evidence/Proposal asneeded.
History persistent HistoryEnabled/Revision/Tracked/Count(owner)/Weight(owner,index)/
Segments/Segment(id)/Interval(id)/Waiver(interval,n)/WaiverIndex(interval,owner)/
Settled(segment,owner)/Proposal(segment)/Totals. Rootappendboundedaffectedholder
checkpoint/segment, neverenumerateallhistory; separatesettlementboundedcontinuations.
Recovery cannotreject merelybecauseunresolvedhistorygrew. No unboundedpackedrecord.
CustodyRegistered/Allocation/Total/Beneficial/Unassigned/Transfer existingseparate
future2x; directholderchecks mayreadrelevantkeys, notinventcustodytransfers.

FacadeexistinginstanceConfig/Busy/NextController/ReadyAt andcodeTTL; ZERO1xhotpathkeys.
External: Aquariusinstance/code,userposition/vaultindex,range/ticks,rewardindex/user
reward,killedflags/fees/plane; tokenSACinstances+vault/pool/recipientbalance orclassic
trustlines; rewardSACwhenclaimed; primaryoracleinstance/feedkeysforpricedsteps.
Detachedauth addsaccount/noncekeys; nativefee/sequence separate. Code/instance/dataTTL
keys inrestoration inventory. ExactphysicalXDRsetsfromacceptedrows plusnewbuild
simulation; these arelogicalfamilies, notinventedpostbuildnumericcounts.

## Keeper and four gates
Keeperinvests+finalizes withsamefinancialtransaction (commandcomplete,refund,unlock),
neverhiddenfinalizerstage. Persistentcommanddiscovery; freshprices/nonce/sequence;
unknownsubmitresultreconciledbefore retry, deterministicidempotency. Restorebatches
<=50RW/100keys/200000readbytes/132096writebytes thenresim originalfinancialroot.
ActualperkeyTTL/fees/rent, no keeperalwaysavailableassumption. Executionreserve is
strategycostintoken0; XLMfees/restoreoperatingbalance separate from lossreserve,
suppliercash andlaterliquidatorcapital. No approvedproductionamounts inferred.

G1 compiled_stage1_outage_mixture_recovery: realAquariusbytecodecontrolledclaimkill+
pendingviewfailure, staleoracle/killedswap; beforemint/postmintlocked/closelocked/
postburnmixture. Waiverfalseunprovenrollback; ownertrueonlyunknowninterval; keeper
cannotwaive; oldcoownerexit+newcomerjoinbeforelaterclaim redistributionhistoriceligible
only; missingboundaryquarantine. INCUMBENT_FEE_ENTITLEMENT_ALLOCATED_BEFORE_SHARE_BURN
oneunitzeroprincipal/allfees; failedsecondpayoutfullyrollsbackwithdraw/burn/reward/
command/liabilities. Withdrawkilled/tokenissuerrefusal keepsclaim, notcashpromise.
G2 compiled_stage1_stressed_histories:64holders/128changes/8outages reusefixtures,
256/1024/32 scalingcandidate (testboundsnotproductioncaps), exitedholders/waivers/
newcomers/emissionchanges/tickpaths/firstmint/finalexit/concurrentcommands. Exactscaled
conservation, proportionalshortfall, no newcomeroldrewards, boundedrecoveryaccess and
restoration withlonghistories, no pruning/admissionhistoryblocker onexistingrecovery.
G3 private_core_stage1_interrupted_restore_resume_reexpire: actualCorepartialbatch
stop/resume persistedsameledger, interveningrealAquariustrade, time/deadline/revision/
pausechanges, duplicates, actualTTLexpiry/eviction+restoreagain, thirdpartykeeper
withoutownerforpreauthorizedsteps orfixedexpiryrelease. Ifactualevictionunsupported,
gatefails; no TTLedit substitution. Alljourneyfeesincluded.
G4 private_core_stage1_financial_parity: continuous7txdirect+8txconverted Coreledger;
identicalperstepprestates host/SDKvsCorealltoken/trustline/supply/LP/command/locks/
reserves/rewards/history/quarantine/returns/events/TTLs; explicitnativefee/sequence
allowlist notentireaccountomission. Wrongevelopesignature nocharge/seq; validenvelope
badhostauth consumesfee/seq butfinancialrollback. Nonce/expiry/replay/wrongnestedauth/
amount/recipient. Everyrootcheck_budget, signedfee+resourcechecks, fixedlimits.
