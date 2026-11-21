# CL group: Mainnet signer for the CL liquidator (HG4) and keeper v6 (HG5)

Prepared on 21 Nov 2026 on branch `feat/cl-collateral-group`.
Status: **code and scripts only, review pending.**
- Nothing was signed, sent, deployed or upgraded.
- No Droplet and no key was created.
- No approval file and no `state-v6` directory were written.
- The live keeper v5 sources, the v5 config and `state-v5/` were not touched. The `rpc-key` file was not read.

| Commit | Content |
|---|---|
| `ffc64b3` | Keeper v6 baseline: verbatim copies of the v5 files. Review `ffc64b3..2f76786` to see exactly v5 → v6 |
| `2f76786` | Keeper v6 changes, config template and tests (103/103) |
| `12eb8b6` | CL liquidator Mainnet signer, heartbeat signer, trustlines, seized-unit exit, Droplet scripts and tests (48/48) |

Stream A's `a17d2ab` (CLI hash parse) committed the shared-worktree `main.mjs`. It already contained my Mainnet wiring hunks (`mainMainnet`, `mainnetLiveMode`, `onSigned` journalling). `12eb8b6` adds only the alert levels on top. The `chain.mjs`, `config.mjs` and `package.json` hunks also landed through stream A's commits. The code is identical either way; attribute those hunks to HG4 when reviewing.

**Almanax is expired. Nothing here was scanned.** These need a scan before use:
- `bots/cl-liquidator` at `12eb8b6`;
- `prototypes/cl-pyusd-vault-b-pilot-tools/keeper-{worker,loop,streak-probe}-v6.cjs`, range `ffc64b3..2f76786`;
- `research/cl-collateral-group-20261118/droplet/*.sh`.

---

## 1. HG4: CL liquidator Mainnet signer

### 1.1 Design (mirrors `bots/cash-liquidator`)

A config whose `networkPassphrase` is exactly Mainnet takes `src/mainnet.mjs`. The Testnet path is unchanged: the CLI-alias executor (`chain.mjs` `assertLiveAllowed`) still refuses Mainnet and its tests still pass.

| Property | Implementation |
|---|---|
| Secret | Two 0600 files on the Droplet, owned by service user `clliq` and created there by `src/keygen.mjs`, which prints only the public key and never overwrites. `loadKeyFile` refuses a symlink, a non-regular file, group or other permission bits, a foreign owner, or a public-key mismatch. Errors never contain file contents. Keys load lazily at the first signature and live only in memory |
| Key separation | Liquidator: `executorKeyFile`. Heartbeat: `heartbeatKeyFile` (`cl-hb-main`), with its own account and sequence. On Mainnet, `executorIdentity` and `heartbeatIdentity` (CLI aliases) are refused |
| Live switch | **Both** `--live` and `DRY_RUN=false` are required. `--live` alone, `DRY_RUN=false` alone, or any other value is a dry run or a refusal (`mainnetLiveMode`) |
| Network pin | Exact Mainnet passphrase in the config, and the RPC `getNetwork` must match. Re-checked on every send |
| Code pins | `pins.{vault 6fd8599a…, receipt 424454b9…, controller 642421f1…, usdcMarket 5630a701…}` (full hashes in `config/mainnet.example.json`). Checked at start and **re-read before every signature** in one `getLedgerEntries` call. A changed code refuses. A *pending* upgrade proposal is an alert only: the bot keeps liquidating on the pinned code, because refusing during the 24 h window would leave bad debt. Once the code changes, the next send refuses |
| Role pins | Refused if the liquidator or heartbeat key is: the admin Ledger GC4T…T7FP, the cash liquidator GDQA…5B5G, the cash pause key GBSK…PQX3, the keeper GBHW…ZON4, T3, each other, a listed borrower, the controller `PauseGuardian`, the receipt `stage_config.guardian`, the vault `stage_guardian`, `stage_investment_keeper`, or either `get_admin` |
| Pilot and retired contracts | CCGK, CD6N and the 20 retired ids are refused as a config contract and as a target |
| Trustlines | In live mode the bot refuses to start without **both** USDC and PYUSD trustlines. Without PYUSD, an in-kind payout of seized units reverts with SAC #13. The dry run reports them, so the soak can start before G44 |
| Single instance | `<dataDir>/run.lock` holds the pid and is taken over only if that pid is dead. A dry-run unit, a live unit and the exit tool can therefore never sign concurrently on the same sequence |
| DRY_RUN default | The unit runs `--watch` without `--live`, and the env file holds `DRY_RUN=true` |
| 12 h dry-run gate | `<dataDir>/dryrun-gate.json`. A live start requires all of:<br>- `dryRunGateHours` (default **12**, allowed 2–72) of error-free `--watch` dry-run cycles;<br>- no cycle gap over 120 s;<br>- the last cycle no more than 10 min ago;<br>- the same binding: sha256 of the config, excluding `borrowers`/`team3`, plus every `src/*.mjs`.<br>The first live start arms the record once, so a systemd restart of the live unit needs no new soak. A new release or a config change needs a new soak |

**What every send checks** (`makeSender`, the same for the bot and the exit tool):
1. **Allowlist** (`botAllow`), with exact argument names:
   - `checkpoint_nav`, `checkpoint_fallback`, `sync_seized(id)` on the receipt;
   - `checkpoint_rewards` on the vault;
   - `cache_price(token = USDC)`;
   - `approve(from = liquidator, spender = usdcMarket, 0 < amount ≤ repayCapRaw)`;
   - `liquidate(borrower ∈ borrowers, repay_market = usdcMarket, collateral_market = receipt, 0 < repay ≤ repayCapRaw, liquidator = self)`.

   Anything else is refused.
2. **Simulation** from the liquidator account. A failure is a plain failure: nothing is signed.
3. **Authorization:** every entry must use source-account credentials, its root must be exactly this contract and function, and there must be no sub-invocations. Controller `liquidate` does only `liquidator.require_auth()`, and the market pulls USDC by allowance, so a correct liquidation has no nested authorization.
4. **No restore preamble.**
5. **CPU:** declared CPU ≤ 97,000,000.
6. **Resource-fee ceiling per call**, in stroops, applied to the padded resource fee the envelope authorizes, rent included. Above the ceiling the call is refused with `alert` and is **never signed**. This addresses the Testnet case where the first `checkpoint_nav` on fresh code paid 70.97 XLM of rent; that case is the regression test.

   | Call | Ceiling |
   |---|---:|
   | `checkpoint_nav`, `checkpoint_fallback`, `cache_price`, `approve` | 5,000,000 (0.5 XLM) |
   | `checkpoint_rewards`, `liquidate`, `sync_seized`, exit calls | 20,000,000 (2 XLM) |
   | heartbeat | 5,000,000 (0.5 XLM) |
7. **Per-transaction fee cap:** `maxFeeStroops` 21,000,000. Inclusion bid 100,000. Ceiling plus bid must fit under the cap.
8. **Daily budget** per UTC day, persisted in `<dataDir>/fee-budget.json`:
   - liquidator 50,000,000 (5 XLM); heartbeat 50,000,000.
   - It is reserved at the maximum fee before signing and trued up to `feeCharged` when the outcome is final.
   - A rejected send (`ERROR` / `TRY_AGAIN_LATER`) is refunded. An unknown outcome stays charged.
9. **Signing and journalling:**
   - Sign. The journal note `{of: intent seq, hash, signed: true}` is written **before** `sendTransaction`.
   - The transaction is sent once and polled for 90 s through raw-JSON `getTransaction`, because SDK 16 cannot decode newer metadata.
   - No final status, or a throwing send, returns `unknown`. `main.mjs` then leaves the intent unresolved, which blocks a further L3 for that borrower, as on Testnet. Nothing is re-sent automatically.

**Heartbeat on Mainnet.** `src/heartbeat.mjs` (stream A, including 4a7b449) is used unchanged. Its injected `sign` is `makeHeartbeatSigner`, which accepts only:
- one `invokeHostFunction` of `receipt.checkpoint_nav()`, with no arguments;
- source = `heartbeatAccount`, and no authorization entries;
- resource fee ≤ 0.5 XLM and envelope fee ≤ `heartbeat.maxFeeStroops` (5,100,000);
- spend within the heartbeat daily budget, charged at the envelope fee when it signs.

Refusals are raised to `alert`. The heartbeat's RPC is the raw-`getTransaction` facade.

### 1.2 Tools

| File | Use |
|---|---|
| `src/keygen.mjs` | `node src/keygen.mjs /abs/<name>.secret` on the host: prints the public key only |
| `src/trustlines.mjs` | Adds **both** USDC (GA5Z…KZVN) and PYUSD (GDQE…U2V5), only the missing ones. Each classic asset's SAC id must equal `contracts.usdc` / `contracts.pyusd` (verified: CCW67…MI75, CCCRWH…HGU2). Dry run unless `CONFIRM_TRUSTLINES=yes`. Fee 10,000 stroops per operation. After SUCCESS it re-reads both trustlines over RPC |
| `src/exit-seized.mjs` (P9) | `show` reports units, beneficial units, commands, balances, trustlines and a 20 bps close quote.<br>`prepare-close [--units N]`: `stage_prepare_close(self, N ≤ stage_units, min_swap 0, deadline now + 3000)`.<br>`withdraw --id N`: only the liquidator's own kind-1 command at phase 10 with min_swap 0; it pays in kind.<br>Fallback: `emergency-exit --units N --accept-reward-waiver`, then `emergency-pay --id N` (phase 21).<br>Every command except `show` **refuses without both trustlines**. Dry-run simulation unless `CONFIRM_EXIT=yes`. Sends through the same guarded sender, with an allowlist of exactly the planned call. It takes `run.lock`, so **stop the service first** |
| `config/mainnet.example.json` | All public ids filled from `clg-mainnet.json` (deterministic: receipt CDOPEENZ…3UZO, vault CAGZA2TN…GVTB, controller CDSJ3ACU…TEOA, USDC market CCDYSYV5…BS2N). The liquidator and heartbeat keys are placeholders, which the config refuses; the bootstrap fills them. `borrowers` is `[]` and `team3` is null until G46 |

### 1.3 Tests (48/48, `node --test` in `bots/cl-liquidator`)

- The 29 existing tests are unchanged.
- 19 new tests in `test/mainnet.test.mjs`, offline with a fake RPC and throwaway temp keys:
  - **Config:** aliases refused on Mainnet; placeholders, pilot/retired ids and missing pins refused; every forbidden role refused; fee, ceiling and gate bounds.
  - **Live switch:** `--live` and `DRY_RUN`.
  - **Allowlist:** bounds for every stage.
  - **Sender order:** simulate → journal hash → send, with retval decode and fee true-up.
  - **Refusals before signing** (nothing signed, sent or charged): CPU, cap, ceiling, nested/foreign/other-root authorization, restore, failed simulation, code-pin change, daily budget.
  - **Outcomes:** ERROR refunds; timeout or a throwing send is unknown; FAILED.
  - **Per-call checks:** the network is re-checked on every call; pins and pending reads; role refusal against on-chain guardians.
  - **Heartbeat signer:** allowlist, cap, budget, ceiling.
  - **Key files:** mode, owner, symlink, mismatch, no overwrite.
  - **Gate:** soak, reset, gap, binding change, arm/re-arm.
  - **Run lock.**
  - **Trustlines:** SAC mismatch, only missing lines added, both required.
  - **Exit tool:** ownership, phase, min_swap, waiver flag, exact-call allowlist.
  - **The 70.97 XLM rent regression**, for both the bot and the heartbeat.
- A tarball from `git archive 12eb8b6` is reproducible (sha256 `dac74ded16c1510799d7f2ecf7588a8b951d5004c53b8f29b97acb3e2944487f` twice). Its own `npm ci` then `node --test` gives 48/48.
- A read-only Mainnet smoke run of `readPins`, `trustlineStatus` and raw `getTransaction` against live cash contracts decoded correctly. It found the real pending proposals on the cash XLM and EURC markets.

### 1.4 Points for the lead (decide; not picked silently)

1. **The 12 h gate conflicts with the LAUNCH timeline.**
   - LAUNCH §4 schedules a 2 h soak (12:30–14:30) and the first live cycle at 16:00 Sun.
   - With the default 12 h, the dry run must start by Sun 04:00 or earlier.
   - The contracts don't exist before G7–G10 (09:45–10:15), and `startChecks` refuses missing instances. So 12 h is impossible on Sunday.
   - Options:
     - (a) set `dryRunGateHours: 2` in `/etc/peridot-cl-liquidator/mainnet.json` before starting the soak (2 is the code minimum);
     - (b) move the first live cycle to Mon.

   The value is part of the gate binding, so changing it later restarts the soak.
2. **Heartbeat key form.**
   - LAUNCH §6.4 tells the operator to run `stellar keys generate cl-hb-main` and set `heartbeatIdentity`. On Mainnet that is now refused.
   - The bootstrap creates `$DATA/keys/cl-hb-main.secret` with `keygen.mjs` and writes `heartbeatKeyFile` / `heartbeatAccount`.
   - Replace those §6.4 lines with §3 below. The same applies to §6.2/§6.3 (`config/mainnet.local.json` becomes `/etc/peridot-cl-liquidator/mainnet.json`).
3. **Heartbeat budget sizing.**
   - At 12 s and about 0.004–0.007 XLM per call, a continuous D3 chain costs about 1.2–2.1 XLM/h.
   - The default daily budget is 5 XLM, about 2.4–4 h of chain per day. When it is exhausted, the signer refuses (alert), the chain lapses and the fallback becomes unreachable. That is safe, but liquidations then wait.
   - The 12 XLM account funding (G43) covers about 5–9 h.
   - Set `fees.heartbeat.dailyBudgetStroops` deliberately, and replace the per-call figure after G26 (LAUNCH P10).
4. **Resource ceilings vs first-touch rent.**
   - A first `checkpoint_nav`, `cache_price` or `sync_seized` that must create persistent entries can exceed its ceiling. The bot then refuses forever (alert) until someone acts.
   - Fallback:
     - run that one call from the Ledger with `clg-run.sh`; or
     - raise the single ceiling in the config. That is a new binding, so the soak restarts unless it is done before the soak.
   - Recommended: do G52 (`team.checkpoint_nav`) and a `cache_price` from the Ledger before the soak starts, so the bot only ever pays steady-state fees.
5. **Retval fallback.**
   - If Mainnet metadata cannot be decoded, a boolean return value falls back to the simulated one.
   - This only affects `lastL4Failure`, which is an alert path for L5 after a failed claim.
6. **One L3 per `--once --live`** is unchanged: at most 8 sends, one L3 for the largest shortfall (LAUNCH §7 asked to confirm this).

---

## 2. HG5: keeper v6

### 2.1 What changed (only this; everything else is the v5 text)

Files in `prototypes/cl-pyusd-vault-b-pilot-tools/` on this branch:
- **New v6 files:** `keeper-worker-v6.cjs`, `keeper-loop-v6.cjs`, `keeper-streak-probe-v6.cjs` and `keeper-config-v6.template.json`.
- **Unchanged v5 dependencies,** byte-identical: `keeper-plan-v4.cjs` (e1fd3987) and `keeper-inspect-v5.cjs` (17f7cfe7).

**Interface check.** `stellar contract info interface` on the 96e7b3c vault and receipt matches live CD6N and CCGK:
- every function the keeper calls, and every user type, is identical;
- that covers `stage_*`, `checkpoint_rewards`, `get_admin` and `stage_investment_*`.

So re-pinning is sufficient. The vault v2 still requires three consecutive-ledger observations (MAINNET-SIMS P1–P8).

| v6 change | Where |
|---|---|
| Config-pinned. The worker already took receipt, vault and code pins from the config. The **streak probe had compiled-in CD6N/CCGK/GBHW/e6638030 pins**; v6 has none:<br>- `configure(pinsFromConfig(config))` is called once per process;<br>- every entrypoint refuses while unconfigured;<br>- a second, different configure refuses;<br>- the retired pair and placeholders are refused. | streak-probe-v6, loop-v6 |
| Placeholders validated at start: any `PLACEHOLDER`, `<…>` or `FILL ME` value refuses. The template is refused as shipped | worker-v6 `validateConfig` |
| CCGK/CD6N refused as receipt or vault | worker-v6, probe-v6 |
| Own state directory: it must be absolute, named `state-v6*`, and not equal to or inside `…/state`, `state-v4` or `state-v5` | worker-v6 |
| **Pending upgrade → refuse.**<br>- Vault v2 and receipt StgV2 keep a proposal in persistent `DataKey::PendingUpgradeHash`; this was read from the sources.<br>- Execution reads both entries before inspecting.<br>- `prepare` re-reads them in the **same burst-1 `getLedgerEntries`** as the code instances, so the job-2 latency path gets no extra round trip.<br>- The streak refuses at start and before every attempt.<br>- On a refusal nothing is journaled or signed (`refusalCode PENDING_UPGRADE`). Read-only mode reports `pendingUpgrades` and `executionWouldRefuse`. | worker-v6, loop-v6, probe-v6 |
| Scopes `cl-vault-b-keeper-v6`, `cl-vault-b-keeper-v6-foreground-supervisor` and `cl-vault-b-keeper-streak-probe-v6`. Source bindings name worker-v6, plan-v4 and inspect-v5. v5 approvals cannot authorize v6, and v6 approvals cannot authorize v5 | all |

**Kept from v5:** the 97M host-preflight stop (AT-LIMIT from 98M), the journal and single-worker lock, approvals written only by the operator (agents never create them), fee/budget/floor caps, authorization checks, windows, the secure-store CLI signer, and the soft read-again codes.

**v6 hashes:**

| File | sha256 |
|---|---|
| worker-v6 | `d266e11f58ed2ab0a089d5d7dfa81290b049316fad2de57b9ec25bd3b714df71` |
| loop-v6 | `0b129a986a16712e017e1cb734a760acf2a84b91897563cc76d40bc61ec8f03b` |
| probe-v6 | `1b5060a3a4db622154a3c15357cc1107916b83eba7de74c4049fbdb9ef4af430` |

### 2.2 Tests

**v6 suite: 103/103** (`node --test tests/*.test.cjs`).

| File | Tests | Content |
|---|---:|---|
| worker | 46 | The v5 41 plus 5 v6 tests: retired/placeholder/state-dir refusals; v5 scope or v5 bindings refused; pending upgrade with fakes and on the production adapter (either contract); an upgrade proposed between the execution check and prepare is caught by the burst-1 re-read |
| loop | 24 | v5 21 + 3 |
| probe | 20 | v5 17 + 3, including configure guards, pending refusal at start and mid-run, and the key encoding |
| inspect-v5 | 8 | |
| latency | 3 | Includes the 40-seed PASS → invest handoff (MEASURED 40/40 first streak; LIVE 40/40 at O+2/O+3) |
| **job-2 latency regression** | 2 | v6 submits `stage_convert` for 8/8 seeds; a pending upgrade on the same job refuses with nothing signed |

- The v4 comparisons that reproduce the 17 Nov incident remain in the v5 suite. That suite is unchanged and **still 398/398** in the main checkout.
- `keeper-plan-v4.test.cjs` depends on `cl-pyusd-vault-b/keeper-plan.cjs`, which is not on this branch. It runs in the v5 suite.
- The v6 tests need `bots/margin-liquidator/node_modules` (SDK 16.3.1). I used a local, uncommitted symlink to the main checkout's install and removed it after the run.

### 2.3 Where v6 runs

The keeper resolves the SDK at `../../bots/margin-liquidator/node_modules` relative to the tools directory, and `runtimeBindings` pins that `package-lock.json`. Run v6 either:
- from the main checkout after this branch is merged; or
- from `.wt-cl-group/peridot-contracts/prototypes/cl-pyusd-vault-b-pilot-tools/`, after `npm ci --ignore-scripts` in `.wt-cl-group/peridot-contracts/bots/margin-liquidator`.

Signer: the same secure-store alias `peridot-pyusd-vaultb-keeper` (GBHW…ZON4) on the Mac. **v5 and v6 sessions must never overlap**, because they share the account sequence.

---

## 3. Operator steps

### 3.1 CL liquidator Droplet (lead decides: the existing `peridot-cashliq`, or a new Droplet)

Defaults assume the existing host `peridot-cashliq` (165.245.249.83) and volume `/mnt/cashliq`. On a new Droplet: same steps, without `SKIP_PACKAGES`, and with a DO volume mounted.

```bash
# Operator Mac
cd /Users/joshua/Peridot/Peridot-Soroban/.wt-cl-group
git archive --format=tar.gz --prefix=cl-liquidator/ 12eb8b6:peridot-contracts/bots/cl-liquidator > /tmp/cl-liquidator-12eb8b6.tgz
shasum -a 256 /tmp/cl-liquidator-12eb8b6.tgz     # expect dac74ded16c1510799d7f2ecf7588a8b951d5004c53b8f29b97acb3e2944487f
scp -i ~/.ssh/peridot-cashliq_ed25519 -o UserKnownHostsFile=~/.ssh/known_hosts_peridot_cash \
  /tmp/cl-liquidator-12eb8b6.tgz peridot-contracts/research/cl-collateral-group-20261118/droplet/{lib.sh,bootstrap-cl-liquidator.sh} root@165.245.249.83:/root/
# Droplet (root). Node is already installed there (v22.23.5, DROPLETS.md)
NODE_VERSION=v22.23.5 NODE_SHA256=cdeb8670a0c81f51b4dc4ac61919f9b1ad917b304bf8221eb939570663d240f8 \
SRC_TARBALL=/root/cl-liquidator-12eb8b6.tgz SRC_SHA256=dac74ded16c1510799d7f2ecf7588a8b951d5004c53b8f29b97acb3e2944487f \
SKIP_PACKAGES=1 START=0 bash /root/bootstrap-cl-liquidator.sh
```

**What the bootstrap does:**
- Host tests must pass (48/48).
- It creates user `clliq`, `/mnt/cashliq/cl-liquidator/{state,logs,keys}`, both keys, and `/etc/peridot-cl-liquidator/{mainnet.json,cl-liquidator.env}` (`DRY_RUN=true`).
- It installs the unit `peridot-cl-liquidator` (enabled; not started with `START=0`) and `/etc/needrestart/conf.d/peridot-cl.conf`.
- It prints the **two public keys** and never touches the cash services.

**Then:**
1. **HG6.** Enter the keys in `clg-mainnet.json`: `liquidator.publicKey` and `heartbeat.publicKey`.
2. **Gate decision §1.4-1.** If the timeline stays, set `"dryRunGateHours": 2` in `/etc/peridot-cl-liquidator/mainnet.json` now. It is root-owned, 0640; edit it as root.
3. **After G7–G10** (the contracts exist) and ideally after the Ledger has done the first-touch `checkpoint_nav` / `cache_price` (§1.4-4):
   ```bash
   systemctl start peridot-cl-liquidator
   systemctl show -p ActiveEnterTimestamp peridot-cl-liquidator
   ```
   The soak clock starts at the first error-free cycle.
4. **G43** funds the liquidator with 25 XLM and the heartbeat with 12 XLM. Then add the trustlines. The tool takes `run.lock`, so stop the dry run for this; that restarts the soak, so do it before step 3 if you can.
   ```bash
   systemctl stop peridot-cl-liquidator
   systemd-run --uid=clliq --gid=clliq --wait --pipe --collect --property=EnvironmentFile=/etc/peridot-cl-liquidator/cl-liquidator.env \
     --working-directory=/opt/peridot-cl-liquidator/current /usr/local/bin/node src/trustlines.mjs --config /etc/peridot-cl-liquidator/mainnet.json        # dry run
   # ... same with --setenv=CONFIRM_TRUSTLINES=yes   (once; check the printed hash before any retry; output must show USDC and PYUSD true)
   systemctl start peridot-cl-liquidator
   ```
5. **G44** pays 200 USDC. Then **G46**: add T3 to `borrowers` and `team3`. That is excluded from the gate binding, so the soak continues. Restart the unit.
6. **Gate:**
   - the log shows ≥ the gate hours of `dry-run gate` lines without `cycle failed`;
   - `cat /mnt/cashliq/cl-liquidator/state/dryrun-gate.json` shows `last - since` ≥ 3600 × hours;
   - `gate liquidator` passes on the Mac.
7. **Live** (from Sun 16:00, after G54). First the single cycle of LAUNCH §7, as the service user with the unit stopped:
   ```bash
   systemctl stop peridot-cl-liquidator
   systemd-run --uid=clliq --gid=clliq --wait --pipe --collect --setenv=DRY_RUN=false --working-directory=/opt/peridot-cl-liquidator/current \
     /usr/local/bin/node src/main.mjs --config /etc/peridot-cl-liquidator/mainnet.json --once --live
   ```
   The gate must have been satisfied **within 10 min** of the dry run stopping. It is armed now. Then switch the unit to live:
   ```bash
   mkdir -p /etc/systemd/system/peridot-cl-liquidator.service.d
   printf '[Service]\nEnvironment=DRY_RUN=false\nExecStart=\nExecStart=/usr/local/bin/node src/main.mjs --config /etc/peridot-cl-liquidator/mainnet.json --watch --live\n' \
     > /etc/systemd/system/peridot-cl-liquidator.service.d/live.conf
   systemctl daemon-reload && systemctl start peridot-cl-liquidator
   ```
   Back to dry run: `rm …/live.conf && systemctl daemon-reload && systemctl restart peridot-cl-liquidator`.
8. **Seized units (P9),** with the unit stopped:
   ```bash
   node src/exit-seized.mjs --config /etc/peridot-cl-liquidator/mainnet.json show
   ```
   Then `prepare-close`, then `withdraw --id <id from show>`. Each is a dry run first, then `CONFIRM_EXIT=yes`. Run them through the same `systemd-run --uid=clliq …` form.
9. **Kill switch K7:** `systemctl stop peridot-cl-liquidator`. The heartbeat stops with it.
10. **Logs:** `/mnt/cashliq/cl-liquidator/logs/cl-liquidator.log`. Look for `level: alert`: fee ceiling, budget, trustlines, unknown outcome, upgrade pending.

### 3.2 Keeper v6 (Mac, operator only)

1. After G7/G9 read back SUCCESS:
   ```bash
   cp …/cl-pyusd-vault-b-pilot-tools/keeper-config-v6.template.json /Users/joshua/Peridot/keeper-pyusd-vaultb/keeper-config-v6.json
   ```
   Fill in `receipt` and `vault` (expected CDOPEENZ…3UZO and CAGZA2TN…GVTB) and T3 in `pilotOwners`. Update `wasmSha256` if HG1 changed the bytes. Set `status` to `CONFIGURED_FOR_PILOT_EXECUTION` and `readOnly` to `false`. Keep `stateDirectory` `…/state-v6`.
2. Read-only check, which must show no refusal and `pendingUpgrades: []`:
   ```bash
   node keeper-worker-v6.cjs …/keeper-config-v6.json
   node keeper-worker-v6.cjs --runtime-provenance
   ```
3. Operator approval. Copy `write-approval-v5.cjs` to `write-approval-v6.cjs` and change only:
   - `ROOT`: the v6 tools directory;
   - `keeper-worker-v5` / `keeper-loop-v5` → `-v6`;
   - `CONFIG` → `keeper-config-v6.json`, `OUT` → `keeper-approval-v6.json`;
   - `scope` → `'cl-vault-b-keeper-v6'`.

   Streak caps and schedule stay unchanged. Run it yourself; agents never write approvals.
4. Session (never concurrent with v5):
   ```bash
   node keeper-loop-v6.cjs …/keeper-config-v6.json --execute --approval …/keeper-approval-v6.json --poll-ms 1000 --max-run-seconds 3600 --pending-timeout-seconds 120 --max-attempts 40
   ```
   For G49 (T3) and G59 (migration). A refusal `PENDING_UPGRADE` means an upgrade proposal exists on the new vault or receipt. Do not keep running: cancel the proposal, or finish the upgrade path first.
