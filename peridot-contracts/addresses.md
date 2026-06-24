# Testnet Deploy (dev) — current no-$P deployment

Fresh no-rewards deployment from this branch. `$P` is intentionally not wired as a
controller reward token. SwapAdapter + MarginController were freshly redeployed after
the routed split-open budget and oracle-min overflow fixes, avoiding the 24h upgrade
timelock.

- Admin (dev): `GATFXAP3AVUYRJJCXZ65EPVJEWRW6QYE3WOAFEXAIASFGZV7V7HMABPJ`
- Alice: `GCOAFEN2VLTOAZR3RVSJ2QGLY4TCVMSFGVNWVI3YMQ6NLJJCCTAJT5TZ`
- Bob: `GBFSIHBLGGDU26EV6MZK64R5UEZU3WLRKM7VRVVCQTHBTI72HLPVUCRW`
- Controller (SimplePeridottroller): `CDMXPWG55776NECXQMWNBXMEQXZUAWA2AJBCQS7SU7SA64XHMO3KB3O6`
- XLM Vault: `CB32OVY4AADCHQT3DLKJYW5QVTWY5MOX7BBNZFT3SDHZ5HPSDDEA2THJ`
- USDT Vault: `CCEW6NSPCV7XUEQV75ZMII5HK5DGXK5JP2QOTGLV4UFLDBPEKRGO4Y4B`
- JumpRateModel: `CDF2GSHMMJR6OU3PBMHO642MSCEKIZV75SYOBP74Q4RZWDKK7VFOTKDZ`
- Mock USDT (7 decimals, open mint): `CDPXNHHVSLX3HFAHV7XOISM23MZH36WSXTO45RNDOBIDFZBGTSOVD4OY`
- XLM native asset contract: `CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC`
- SwapAdapter (pool-direct Aquarius single-hop build): `CCFXUYLPRFWLOSLT3KRVXZAXECZX5KN7KFQE5G7GDBS4Z2KGZRRYWFCZ`
- MarginController: `CB5UZHITW3G72PWTBSEBIY4WLB77LAG7RKAIZCD5URRANWRZ2J3OCHEU`
- Reflector Oracle: `CCYOZJCOPG34LLQQ7N24YXBM7LL62R7ONMZ3G6WZAAYPB5OYKOMJRN63`
- Controller oracle mappings:
  - XLM native SAC `CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC` -> Reflector `XLM`
  - Mock USDT `CDPXNHHVSLX3HFAHV7XOISM23MZH36WSXTO45RNDOBIDFZBGTSOVD4OY` -> Reflector `USDT`
- Controller price checks after oracle wiring:
  - XLM `get_price_usd` -> `[19170684406935, 100000000000000]`
  - Mock USDT `get_price_usd` -> `[99890500703762, 100000000000000]`
- Working Aquarius testnet router: `CBCFTQSPDBAIZ6R6PJQKSQWKNKWH2QIV3I4J72SHWBIK3ADRRAM5A6GD`
- Non-testnet/stale router to avoid on testnet: `CBQDHNBFBZYE4MKPWBSJOPIYLW4SFSXAXUTSXJN76GNKYVYPCKWC6QUK`

Aquarius pool status for current XLM/mock-USDT pair:
- Token order: `[XLM, Mock USDT]`.
- AQUA payment token: `CDNVQW44C3HALYNVQ4SOBXY5EWYTGVYXX6JPESOLQDABJI5FC5LTRRUE`
  (`AQUA:GAHPYWLK6YRN7CVYZOO4H3VDRZ7PVF5UJGLZCSPAEIKJE2XSWF5LAGER`, 7 decimals).
- `dev` created the AQUA trustline, swapped `100,000` XLM units into `10,683,815`
  AQUA units on Aquarius, then paid the `10,000,000` AQUA standard-pool creation fee.
- Pool ID: `9ac7a9cde23ac2ada11105eeaa42e43c2ea8332ca0aa8f41f58d7160274d718e`.
- Pool address: `CCMNSENXDBNJSY72BDIPH5CCXLLHBKZ4LXTRKDLKZN4UI2NJFQLWTLD6`.
- Pool was funded with `1,000,000,000` XLM units and `1,000,000,000` mock-USDT
  units, then skewed by swapping `100,000,000` mock-USDT units to XLM so XLM->USDT
  clears the controller's 1:1 oracle-min check.
- Pool was later rebalanced with an `87,000,000` XLM-unit swap into mock-USDT,
  returning `95,034,080` mock-USDT to `dev` and moving reserves to roughly 1:1
  (`999,912,914` XLM / `1,000,360,176` mock-USDT before the next long test).
- Final SwapAdapter binding allows this pool ID/address pair and also allowlists the
  pool address for estimates.
- Post-final-redeploy verification:
  - `cargo test --workspace` passed locally before deployment.
  - XLM and USDT vaults both read back `get_margin_controller =
    CB5UZHITW3G72PWTBSEBIY4WLB77LAG7RKAIZCD5URRANWRZ2J3OCHEU`.
  - New SwapAdapter readbacks: pool allowed `true`, pool binding allowed `true`.
  - Alice's temporary USDT margin-balance test was swept back out; final readback is
    `0`.
  - `begin_open_position_v2` routed short simulation on the final controller
    succeeded and returned simulated position id `1`.
- Full manual smoke after the pToken-finalize redeploy:
  - Bob XLM lending deposit of `10,000,000` units minted `10,000,000` pTokens;
    withdraw of `1,000,000` pTokens returned `1,000,000` XLM units.
  - Alice USDT borrow/repay of `1,000,000` units against existing XLM pToken
    collateral passed; debt returned to `0`.
  - Bob USDT borrow/repay passed after entering the USDT market; debt returned to
    `0`.
  - Routed leveraged short flow passed with the live Aquarius pool:
    `begin_open_position_v2` opened pending position `1`, borrowed `900,017` XLM
    units, adapter `swap_chained` returned `1,081,840` mock-USDT units, separate
    USDT-vault deposit minted `1,081,819` pTokens, and `finalize_open_ptokens_v2`
    opened the position with health factor `2,081,818`. `close_position_v2_repay_only`
    then closed it; `get_position(1)=null` and XLM margin debt returned to `0`.
  - Additional leveraged short stress: `3x` with `1,000,000` USDT margin pTokens
    opened position `2`, borrowed `1,800,034` XLM, swapped to `2,157,299` mock-USDT,
    deposited for `2,157,258` pTokens, finalized with health factor `1,578,628`,
    then closed; `get_position(2)=null`, Alice positions `[]`, and XLM margin debt
    returned to `0`.
  - Non-mutating `4x` short budget simulation also passed: simulated borrow
    `2,700,051` XLM and adapter estimate `3,220,085` mock-USDT out.
  - Before rebalancing, the reverse long route was blocked by current pool pricing,
    not budget: USDT->XLM estimate for `900,017` USDT was `747,327` XLM, below the
    5% oracle minimum `855,017`, so Aquarius rejected the swap.
  - After rebalancing, the full routed long flow also passed: `begin_open_position_v2`
    opened position `3`, borrowed `900,017` mock-USDT, adapter `swap_chained`
    returned `896,111` XLM, XLM-vault deposit minted `896,111` pTokens, and
    `finalize_open_ptokens_v2` opened the position with health factor `1,696,961`.
    `close_position_v2_repay_only` then closed it; `get_position(3)=null`, Alice
    positions `[]`, and USDT margin debt returned to `0`. Post-test USDT->XLM
    estimate for `900,017` USDT is `894,505` XLM, still above the `855,017` minimum.
  - Final exchange rates read after the smoke: XLM vault `1,000,000`, USDT vault
    `1,000,019`.
- Recommended live routed open flow is now:
  1. `begin_open_position_v2`
  2. adapter `swap_chained`
  3. direct deposit of received position asset into the position vault
  4. `finalize_open_ptokens_v2`
  The monolithic `open_position_v2` and the deposit-in-finalize
  `finalize_open_position_v2` path remain too heavy for the current live
  testnet route.

Superseded no-`$P` margin deploys:
- pool-direct pre-budget/overflow-fix pair: SwapAdapter `CARPR3UOIPGF7OIQITV5273SMEGKRCJP65MNKN2HLV32EKXTVUP72GBV`,
  MarginController `CAZOVGMZ4DAUI3ZX3OM243T3HLBGWS2KQYECFNT4LLG3IYX6BJAFSOEE`
- intermediate routed-budget redeploy superseded by oracle-min overflow fix:
  SwapAdapter `CARPRZBE5ICKOGEU4KDCDHIOAYRUPFDCWIXK7NOJZ27HSZY764YZ3SVJ`,
  MarginController `CBZFCLTSKHBDI6M7PNV6WBOCJZ7X7YVJ5IYSXPGYVAPFQCPCCYX57A5Z`
- split-open before pToken finalizer: SwapAdapter `CB6R6YZWWPACDX5JSPFLOF4PGH4BH5D6BHDC6IDVXH2DSXQCYCQJ62JB`,
  MarginController `CC2KR57IUIB3LZPGYK3INI4QWM4IKO3H2DXHM6SNQNC2UOBHEXBXZQTJ`
- direct-pool split-open attempt: SwapAdapter `CAMJEGX5BFZ5TVSUVJ5RDKFDSSLVMXRVK4JKPKP6GRQBNRBFVUT2HI5R`,
  MarginController `CDTP4VSVHHJRBGFWPSNQ65FQ3AFZAZEIWCDYLTC23AN7QAOBWC2QCZ5E`
- pre route-index redeploy: SwapAdapter `CCOFZJTN64COWDL46AJLZBSV7OJWLFXDL7YFWY2CD47SY2VW7TN3LN57`,
  MarginController `CD5PQCAWFM7LZS6U2BYEXNF3DUSRLTV66O5XL3EKNXEUG5ZIWGL3PQ5R`

## Previous testnet deployments (superseded)

# Testnet Deploy (dev) — stress-test v4, leveraged-fix + Almanax fixes

Fresh full deploy from the `leveraged-fix` line with all fixes from this work applied.
Fixes carried from v3 (still present):
1. Controller `#[contractimpl]` fix (claim_self / claim_all / portfolio exported).
2. `liquidate_position_v2` — `begin_margin_withdraw` bypass armed before each seize
   (re-entry fix) + per-vault price/rate/CF cached once (CPU-budget fix).
3. Almanax #3 — checked liquidation math (peridottroller + margin).
4. Almanax #5 — `set_market` rejects asset↔vault-underlying mismatch.
5. Almanax #4 — `token_balance` traps on negative balances.
6. Almanax #6 — margin withdraw bypass validates scope before removing.

New in v4 (this batch):
7. Almanax 27add6c7 — `collect_margin_fee` carries a `MarginFeeRemainder` so sub-unit
   fees accumulate instead of being dropped on `delta==0`.
8. Almanax bf888b9c — `accrue_market` only advances Supply/BorrowIndexTime when
   `delta>0` (or no suppliers/overflow), so rounding intervals fold forward instead of
   being discarded (kills permissionless-claim emission griefing).
9. Almanax 0c08187f — `sum_positions_usd` flags `collateral_indeterminate` for an
   unpriced collateral-only market (pbal>0, cf>0, debt==0) so liquidation fails closed
   instead of dropping that collateral and manufacturing a shortfall.
   (Almanax 49aa8d8e "expired borrow state erases debt" was a FALSE POSITIVE: borrow
   state is persistent, which archives on TTL expiry and forces restore — it never
   silently reads as 0.)

On-chain stress verification of this deployment (all PASS): deposit/borrow;
`account_liquidity` exact (collateral 90M − debt 40M = 50M liquidity); reward claim
accrued PERI on both supply (dev 2475) and borrow (borrower 195) sides via the reworked
`accrue_market`; liquidation repaid 8M XLM (debt 40M→32M) and seized 25,920,000 USDT
pTokens (8M × 1.08 bonus ÷ ⅓ price — exact); `set_market` accepted correct mappings and
rejected the asset↔vault mismatch. `collect_margin_fee` (margin fee remainder) is
covered by the 36 margin unit tests — a live margin open needs Aquarius liquidity for
the mock token, which testnet lacks, so it was not exercised on-chain.

XLM and Mock USDT both use a `[1,1]` fallback price on the controller. Vaults wired to
the margin controller; margin controller registered as a liquidation controller. Vault A
is NOT boosted (no DeFindex forwarding) unless explicitly set.

- Admin (dev): `GATFXAP3AVUYRJJCXZ65EPVJEWRW6QYE3WOAFEXAIASFGZV7V7HMABPJ`
- Controller (SimplePeridottroller): `CB5VERSVU273I37N6ROZQWP6JMHX3HGPXOJW2KGJLBNLRLORLE4UCQCC`
- JumpRateModel: `CCBP5KFPYIM4YZ2RCB6UN2IG5FXFP4JUGCRN4TWHWGUYELP6OZN74UYH`
- PERI Token: `CAVGNIPEPR4HUGYXKED4RCX2PSKRZBQWVSFWH5XE5KQTNWY5HHQMULLS`
- Mock USDT (open mint): `CD5WCVRHMUP3VLYYW4UGMJJ2WGN6MFWYLFYLXDXWBLWR5QEDPKOARIQ6`
- Vault A (XLM, non-boosted): `CD62XUGTHVR3DWFDLBFT6FOSGHEGYZIGD3GOBUYNH2FJBWXGKH5BEIQR`
- Vault B (USDT): `CAOYPA2G4EEZ3FIP4T7SHKEFPKIDIPSFMOJ2AWQ5JFPVQY3TQVVTGFAD`
- XLM native asset contract (TOKEN_A): `CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC`
- SwapAdapter: `CDNORMOHG5ANATLKOT3KQKY74SMHBVZ6JBPRT2JXEWQJD7UTLP4MD53L`
- MarginController (current): `CBOWU62EOI5GURA6J7YNZQODVBV5IBK7CDF3YJC7D3HTP52XVBHFXMFM`
- Aquarius Router (swap adapter router): `CBQDHNBFBZYE4MKPWBSJOPIYLW4SFSXAXUTSXJN76GNKYVYPCKWC6QUK`
- DeFindex XLM Vault (available boosted target): `CCLV4H7WTLJQ7ATLHBBQV2WW3OINF3FOY5XZ7VPHZO7NH3D2ZS4GFSF6`
- Test borrower identity: `GCR4RQVE5A6OJ73ZWTNSCPJMC5L3GB42NFFI664CF255F2R4PEA7C4LQ`

## Previous testnet deployments (superseded)

Stress-test v3 core (`CCZPTPSB...`, margin `CASOVXZF...`, swap `CC46IJGP...`, vaults
`CBP3CPUJ...`/`CCSVBWIW...`) had fixes 1–6 above but pre-dates the v4 Almanax fixes
(7–9); superseded by v4.

Stress-test v2 core (`CCYGQBDS...`) and its margin controllers `CBQLN5IF...`,
`CB4FJW5P...`, `CBERFOE7...` (and Vault C `CBA55N4N...`) are superseded.

## Previous testnet deployments (superseded)

Stress-test v1:

- Controller: CAVDLV7NHVRQZMVSELDRJJSSQEKONH5WV7UUY42XMBESGIHDUX5BUSX5
- XLM Vault: CC6QBFMUVB6Y2WOLE2ATAYPHXAO3RJXXYX3TDWRW5XO6QEIK3MP636IN
- USDT Vault: CDWSNIAAJ4I3W45KHPOYBJGGQWO4TOIQTHCOMCPLQQJGEA5KBMEC4VS4
- Mock USDT: CDYOVDSNL2XTKKNB762AVKPOZ2K7OOCB3EJRHE4A7JHPLI2SFLJEC3RU
- MarginController: CARHBTXIK3KDAN6T5FRQSXJALYOYDHKXEBCOHXAUC5ZDPJUY4SJQH2MP
- SwapAdapter: CDJILKXCSF74NY4VZX77OI6MBA6LEICJFKCJB7UY42UYXR7XNI6HBTNC

Older deployment:

- Controller (SimplePeridottroller): `CCBAEMMG4STILW6SYTNCIVG44OF4TQDDCYPU7GS3ZOEKLTC75ONTLCI2`
- JumpRateModel: `CCIDO7HBNBPUKFWEI3PRA6O6QU2JXUKVIZAERCZWBNGGK7LO7MFBKKOA`
- PERI Token: `CBCA56UIBQA3WT2JUIIG2BHW325CMLNAC7CKL33T37GHN25RCGR6SXPB`
- Mock USDT (open mint): `CDBWTU527WNACRCET2NF6RZFQ3WAPJOQM3OQ5VLUNHJRDQ6ICVO2JTJP`
- Testnet USDC: `CB3TLW74NBIOT3BUWOZ3TUM6RFDF6A4GVIRUQRQZABG5KPOUL4JJOV2F`
- Vault A (XLM): `CCHBN5RRP7KH4O7ICSIQTSYFFZBYFEBCF35UOQBGDI7GZZKKWXWVLLPX`
- Vault B (USDT): `CBP2U7FVTQ2EIAQ474CTYN74KCEU6YLCCGH6KRY2RAMQEDSKREKSAGSO`
- Vault C (USDC): `CBVTTRAXYESGIUYYK2XTQGUBWIXVZG6EMAJGFP2XFXB2N4SR5LEY6QT7`
- Vault D (BLEND USDC): `CCKPULOPSBOM6CWSDJGJ7K7I72BMPBOAEGPXPGM4NUKHTQ4HMOSB23ZU`
- MarginController: CAZQWGJDKG2JQYV66VV3ONBDLYAE77YVKSBUNWUY7MV6WVLLHT4URFX7
  SwapAdapter: CAGLARN3MUMRGCRNKXZ3SH7NVCZ3P3CDGHL2FQEEXIC4MPAGTQTACY6S
  SmartAccountFactory: CA7O44S46V3KTQKKDJ5DMIKIPBZOHXKYIKXRV4HMBA7OKZSMNUOB7DOG
  BasicSmartAccount (dev): CAJNDPSZ55K7CTIGQZXUHCXW3OI226Q2XQ5WYUEFQP4PUWVGDGOVV7P2
  Soroswap Factory: CDP3HMUH6SMS3S7NPGNDJLULCOXXEPSHY4JKUKMBNQMATHDHWXRRJTBY
  Soroswap Router: CCJUD55AG6W5HAI5LRVNKAE5WDP5XGZBUDS5WNTIVDU7O264UZZE7BRD
  Soroswap Aggregator: CC74XDT7UVLUZCELKBIYXFYIX6A6LGPWURJVUXGRPQO745RWX7WEURMA
  Aquarius Router: CBCFTQSPDBAIZ6R6PJQKSQWKNKWH2QIV3I4J72SHWBIK3ADRRAM5A6GD
  DeFindex USDC Vault (BLEND USDC): CBMVK2JK6NTOT2O4HNQAIQFJY232BHKGLIMXDVQVHIIZKDACXDFZDWHN
  DeFindex XLM Vault: CCLV4H7WTLJQ7ATLHBBQV2WW3OINF3FOY5XZ7VPHZO7NH3D2ZS4GFSF6

Testnet USDC: CB3TLW74NBIOT3BUWOZ3TUM6RFDF6A4GVIRUQRQZABG5KPOUL4JJOV2F
Reflector External CEX/DEX = CCYOZJCOPG34LLQQ7N24YXBM7LL62R7ONMZ3G6WZAAYPB5OYKOMJRN63

Mainnet:

- Soroswap/DeFIndex USDC Vault: CA2FIPJ7U6BG3N7EOZFI74XPJZOEOD4TYWXFVCIO5VDCHTVAGS6F4UKK
- Soroswap/DeFIndex EURC Vault: CCKTLDG6I2MMJCKFWXXBXMA42LJ3XN2IOW6M7TK6EWNPJTS736ETFF2N
- Aquarius Router: CBQDHNBFBZYE4MKPWBSJOPIYLW4SFSXAXUTSXJN76GNKYVYPCKWC6QUK
- XLM Contract: CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA
- USDC: USDC-GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN
- USDC Contract: CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75
- EURC: EURC-GDHU6WRG4IEQXM5NZ4BMPKOXHW76MZM4Y2IEMFDVXBSDP6SJY4ITNPP2
- EURC Contract: CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV

Reflector: CAFJZQWSED6YAWZU3GWRTOCNPPCGBN32L7QV43XX5LZLFTK6JLN34DLN

Controller : CCVUFGXKFVPAHWMMDDL6HXKUN2B2G73Z27VRM3WXZBBSQEUTNLI6YPEX
JRM Volatile: CCPJFBH5WSNZVMCUQCBM4X5334L6ZL3W4Q33XJAK45RCDHJ2JGJ5AP6A (XLM)
JRM Stable : CCI5LBBNYOASPQ62GIRY54PDEYWWURJB75HNRAFOU4LTOU3XBC73IB5I (USDC + EURC)
PERI ($P) : CDNJSOJKURHQUDBO7OHK7Z64R2CNMIAWXENHM24ALK7Y3H56EU6PUOKR
Vault XLM : CBU4Y7CJFOUZZE3QBOXTKM54UTUYW3SDJWTNMDGJBNCR5HS5UCEKV3BE
Vault USDC : CBVUJJIJTRJNOORPPCVH72DP7YDCOMDHI6WYKP3WOFVEPSCVP3TBXHIN
Vault EURC : CD3WN3PLW63HFZXE56OTRLMBV46WG54TFPGRL4RDQ43HQTTWVB4RPO3G
Oracle : CAFJZQWSED6YAWZU3GWRTOCNPPCGBN32L7QV43XX5LZLFTK6JLN34DLN
Peridot EURC VAULT Defindex: CBP2R5KYAWJCOCVDTSNTEVL3O6JBTWOOH7SZOX7DX5DLGVZCAMLBDZM3
Peridot USDC VAULT Defindex: CAB4JOLSCNELJVDQKZLVGHKWJCLXFDBZZMITJAFL4GBGTHIKWO47PYFH
Peridot XLM VAULT Defindex: CCB2AR5X3KP4WQKE7HNSUSDS7SHFMC2WPVSZ2ZXJ6DHXOKHFFKOZE6GK
