# WP7: margin pass in the cash liquidator, Droplet rollout (written, NOT executed)

Code: branch `feat/margin-keeper`, `peridot-contracts/bots/cash-liquidator`.
- Commit 1, `5dedf2a`, is a verbatim import of deployed release 4f06aad, tree `230bad98`.
- Commit 2 adds the margin pass. The README section "Margin pass" describes the behaviour.

Host: liquidator Droplet `peridot-cashliq`, <keeper Droplet IP>, unit `peridot-cash-liquidator`, user `cashliq`. Logs go to
`/mnt/cashliq/cash-liquidator/logs/liquidator.log`.

The release process mirrors `research/cash-borrowing-ops-20261118/COMMANDS-21NOV.md` A2 and A5:
1. `git archive`;
2. sha256 check;
3. `install_code`, which runs the host tests and switches `current`;
4. restart.

**Owner gate (25 Nov).** Margin opens stay paused until this keeper runs LIVE on the Droplet (not the Mac), after the dry stage below, with
every alert shown firing. Nothing here signs a transaction except the switch to live (section 3), and even then it signs only when there is
something to do.

**Provenance caveat.** The deployed release `cb88d4dec23b4c82` = 4f06aad is taken from DROPLETS.md and COMMANDS-21NOV A2. It was not
re-verified on the host while this was written (SSH inspection was not permitted). Step 1.3 checks it before anything changes.

## 0. Prerequisites (all must hold before step 1)

1. **Contract.** The margin v2 contract is deployed from WASM `b1cb1cb21f02de1b1a9047d31fa60ffc066c2ddedc94c4dff2fcbdb0a7b1766f`, with opens
   paused. You know its `C…` id.
   - Its `Ctrl` is the cash controller `CBMRS7LK…7RK`, and its USDC/XLM markets are the pinned receipts. The keeper refuses anything else.
   - The keeper key `GDQARP…5B5G` is neither its admin nor its guardian.
2. **Scan.** Almanax has scanned `5dedf2a..<WP7 commit>` with no open findings, and the branch is pushed. Releases are built from a
   pushed commit only.
3. **Inventory.** The margin pass draws on the same USDC/XLM inventory as cash liquidations.
   - At the pilot caps ($25 equity, 5×, so at most about $100 of debt per position), one long liquidation needs at most about 102 USDC.
   - One short needs at most about $100 of XLM, about 805 XLM at $0.124.
   - The funded 500 USDC / 2,500 XLM covers this. The keeper alerts when it no longer does.
4. **Alert delivery.** Alerts are JSON lines with `"level":"alert"` in the log above. There is **no off-box forwarding for this bot today**.
   Decide who reads them, and how, before going live (open question in the WP7 report).

Shell variables on the Mac (no secrets):

```zsh
export LQ="<keeper Droplet IP, from your own notes>"
```

```zsh
export SSHQ="-i <path to your keeper SSH key> -o UserKnownHostsFile=<path to your known_hosts file for the keeper hosts>"
```

```zsh
export MARGIN=C...   # the deployed margin v2 contract id
```

```zsh
export WP7=<full WP7 commit sha on feat/margin-keeper>
```

## 1. Release and dry-run stage

### 1.1 Build the tarball (Mac)

```zsh
mkdir -p ~/Peridot/releases && cd ~/Peridot/releases
```

```zsh
git -C ~/Peridot/Peridot-Soroban archive --format=tar.gz --prefix=cash-liquidator/ $WP7:peridot-contracts/bots/cash-liquidator > cash-liquidator-${WP7:0:7}.tgz
```

```zsh
shasum -a 256 cash-liquidator-${WP7:0:7}.tgz
```

Record the hash as `SRC_SHA256`. Keep this exact file: a rebuild gives the same contents but a different gzip hash.

### 1.2 Copy it to the Droplet

```zsh
scp $=SSHQ cash-liquidator-${WP7:0:7}.tgz root@$LQ:/root/
```

### 1.3 Check what runs now (read-only)

```zsh
ssh $=SSHQ root@$LQ 'readlink /opt/peridot-cash-liquidator/current; systemctl show -p ActiveState -p ActiveEnterTimestamp -p NRestarts peridot-cash-liquidator; grep -E "^(DRY_RUN|MARGIN_)" /etc/peridot-cash-liquidator/liquidator.env'
```

Expected:
- `current -> …/releases/cb88d4dec23b4c82`;
- `ActiveState=active`;
- no `MARGIN_` lines.

Write down the `DRY_RUN` value. The margin stage leaves it **unchanged**, so cash liquidations keep running as they do now.

### 1.4 Install the release (runs the host tests, then switches `current`)

```zsh
ssh $=SSHQ root@$LQ 'SRC_TARBALL=/root/cash-liquidator-'${WP7:0:7}'.tgz NODE_VERSION=v22.23.5 SRC_SHA256=<SRC_SHA256> bash -c ". /root/lib.sh && install_code" && readlink /opt/peridot-cash-liquidator/current'
```

Expected:
- `sha256sum` prints OK;
- the host `node --test` passes (63 tests);
- `current` points to the new `releases/<first 16 hex of SRC_SHA256>`.

On any failure `install_code` stops before switching `current`. Nothing has changed, so stop here.

### 1.5 Margin overlay (enabled, contract id) and the margin dry-run flag

The overlay sets only `enabled` and `contract`. The WASM pin stays in the release. Setting the contract id also turns on the cash-pass
exclusion of the pooled account.

```zsh
ssh $=SSHQ root@$LQ "printf '{\"enabled\": true, \"contract\": \"$MARGIN\"}\n' > /etc/peridot-cash-liquidator/margin.json && chown root:cashliq /etc/peridot-cash-liquidator/margin.json && chmod 0640 /etc/peridot-cash-liquidator/margin.json && cat /etc/peridot-cash-liquidator/margin.json"
```

```zsh
ssh $=SSHQ root@$LQ 'grep -q "^MARGIN_" /etc/peridot-cash-liquidator/liquidator.env || printf "MARGIN_CONFIG_FILE=/etc/peridot-cash-liquidator/margin.json\nMARGIN_DRY_RUN=true\n" >> /etc/peridot-cash-liquidator/liquidator.env; grep -E "^(DRY_RUN|MARGIN_)" /etc/peridot-cash-liquidator/liquidator.env'
```

Expected:
- `DRY_RUN` is unchanged from 1.3;
- `MARGIN_CONFIG_FILE=/etc/peridot-cash-liquidator/margin.json`;
- `MARGIN_DRY_RUN=true`.

### 1.6 Restart and check the start lines

```zsh
ssh $=SSHQ root@$LQ 'systemctl restart peridot-cash-liquidator; sleep 60; systemctl show -p ActiveState -p ActiveEnterTimestamp -p NRestarts peridot-cash-liquidator'
```

```zsh
ssh $=SSHQ root@$LQ 'grep -E "\"msg\":\"(start|margin pass enabled|margin pin check failed at start|margin state bootstrapped|margin heartbeat|heartbeat)\"" /mnt/cashliq/cash-liquidator/logs/liquidator.log | tail -n 8 | cut -c1-400'
```

Expected:
- `start` (with the unchanged `dryRun`);
- `margin pass enabled` with `wasm: b1cb1cb2…766f` and `dryRun: true`;
- `margin state bootstrapped` with `counter` equal to the on-chain `get_position_counter` (0 before any open);
- `heartbeat`;
- `margin heartbeat`.

Any `margin pin check failed at start` line is an alert: stop, then roll back (section 4) or fix the overlay.

Record `ActiveEnterTimestamp`. **The dry stage clock starts there.**

### 1.7 Dry-stage pass criterion (all must hold)

Run the checks below at least **12 h** after `ActiveEnterTimestamp`. The stage passes when all of the following hold:
1. **Running.** The service is active with NRestarts=0.
2. **Cycles.** There are at least 1,400 `margin heartbeat` lines since the start: 30 s cycles plus early wakes, so about 1,440 or more in 12 h.
   Their `ledger` values strictly increase, with no repeats (CAP-88 key).
3. **Errors.** There are 0 `margin cycle failed` lines, and 0 margin `warn`/`alert` lines. Alerts tagged `alertTest` are excluded, so run
   section 2 after this criterion, or exclude them. While opens are paused and the book is empty, no real margin alert is expected.
4. **No margin sends.** The journal has 0 `"kind":"margin_` entries.
5. **No cash regression.** Cash `heartbeat` lines continue with no new cash `alert` lines (compare with the hours before the restart), and the
   pooled account is never selected for a cash liquidation.
6. **Reads are correct against the deployed contract.** These fields of the latest `margin heartbeat` must equal an independent read-only
   simulation from the Mac:
   - `active` = number of open positions (0 while opens are paused);
   - `bsDebtShares` = `{USDC: 0, XLM: 0}`;
   - `impaired` = `false`;
   - `backstopNavPt` / `backstopShares` = `backstop_nav`;
   - `hf` = `340282366920938463463374607431768211455` (u128::MAX, no pooled debt).

```zsh
ssh $=SSHQ root@$LQ 'systemctl show -p ActiveState -p ActiveEnterTimestamp -p NRestarts peridot-cash-liquidator'
```

```zsh
ssh $=SSHQ root@$LQ 'grep -c "\"msg\":\"margin heartbeat\"" /mnt/cashliq/cash-liquidator/logs/liquidator.log'
```

```zsh
ssh $=SSHQ root@$LQ 'grep "\"msg\":\"margin heartbeat\"" /mnt/cashliq/cash-liquidator/logs/liquidator.log | sed -E "s/.*\"ledger\":([0-9]+).*/\1/" | awk "NR>1 && \$1<=p {bad++} {p=\$1} END {print \"non-increasing ledgers:\", bad+0}"'
```

```zsh
ssh $=SSHQ root@$LQ 'grep -E "\"level\":\"(warn|alert)\"" /mnt/cashliq/cash-liquidator/logs/liquidator.log | grep -i margin | grep -vc alertTest'
```

```zsh
ssh $=SSHQ root@$LQ 'grep -c "\"kind\":\"margin_" /mnt/cashliq/cash-liquidator/state/journal.jsonl || true'
```

```zsh
ssh $=SSHQ root@$LQ 'grep "\"msg\":\"margin heartbeat\"" /mnt/cashliq/cash-liquidator/logs/liquidator.log | tail -n 1 | cut -c1-600'
```

The independent reads below are simulations only (`--send=no`) with the keeper's public key as source. Nothing is signed.

```zsh
stellar contract invoke --id $MARGIN --network mainnet --source-account GDQARPHRQFU7K52XMESPNBSKZE6GPCORZV54EBGDVSXJQBOIDUMO5B5G --send=no -- get_position_counter
```

```zsh
stellar contract invoke --id $MARGIN --network mainnet --source-account GDQARPHRQFU7K52XMESPNBSKZE6GPCORZV54EBGDVSXJQBOIDUMO5B5G --send=no -- backstop_nav
```

```zsh
stellar contract invoke --id $MARGIN --network mainnet --source-account GDQARPHRQFU7K52XMESPNBSKZE6GPCORZV54EBGDVSXJQBOIDUMO5B5G --send=no -- pooled_health
```

```zsh
stellar contract invoke --id $MARGIN --network mainnet --source-account GDQARPHRQFU7K52XMESPNBSKZE6GPCORZV54EBGDVSXJQBOIDUMO5B5G --send=no -- is_impaired
```

```zsh
stellar contract invoke --id $MARGIN --network mainnet --source-account GDQARPHRQFU7K52XMESPNBSKZE6GPCORZV54EBGDVSXJQBOIDUMO5B5G --send=no -- opens_paused
```

`opens_paused` must be `true` throughout the stage.

## 2. Alert test on the Droplet (no funds touched)

`src/margin-alert-check.mjs` forces `DRY_RUN=true` and `MARGIN_DRY_RUN=true`, and replaces `sign`/`submit` with functions that throw. It
reads the state file but never writes it. Each run does two things:
1. **Live read-only cycle.** One margin cycle against the deployed contract: pins, discovery, positions, backstop and `pooled_health`. It
   logs `margin alert test: live read-only cycle ok`, plus any real alert.
2. **Synthetic snapshot.** The synthetic snapshot goes through the **same** `marginAlerts` → `emitMarginAlerts` path as a live cycle. Every
   line is tagged `"alertTest":true`.

The run exits 1 if a requested alert did not fire. Output is appended to the service log, so the alerts land exactly where live alerts
land: `"level":"alert"` lines in `liquidator.log`.

Run each alert on its own, one block per alert, then confirm where it landed.

**Liquidatable for more than 10 min** (owner rule):

```zsh
ssh $=SSHQ root@$LQ 'systemd-run --uid=cashliq --gid=cashliq --wait --collect --property=EnvironmentFile=/etc/peridot-cash-liquidator/liquidator.env --property=StandardOutput=append:/mnt/cashliq/cash-liquidator/logs/liquidator.log --property=StandardError=append:/mnt/cashliq/cash-liquidator/logs/liquidator.log --working-directory=/opt/peridot-cash-liquidator/current /usr/local/bin/node src/margin-alert-check.mjs liquidatable'
```

**pooled_health below target** (hf < hf_target or hf_down < 1.10):

```zsh
ssh $=SSHQ root@$LQ 'systemd-run --uid=cashliq --gid=cashliq --wait --collect --property=EnvironmentFile=/etc/peridot-cash-liquidator/liquidator.env --property=StandardOutput=append:/mnt/cashliq/cash-liquidator/logs/liquidator.log --property=StandardError=append:/mnt/cashliq/cash-liquidator/logs/liquidator.log --working-directory=/opt/peridot-cash-liquidator/current /usr/local/bin/node src/margin-alert-check.mjs pooled_health'
```

**margin_impaired:**

```zsh
ssh $=SSHQ root@$LQ 'systemd-run --uid=cashliq --gid=cashliq --wait --collect --property=EnvironmentFile=/etc/peridot-cash-liquidator/liquidator.env --property=StandardOutput=append:/mnt/cashliq/cash-liquidator/logs/liquidator.log --property=StandardError=append:/mnt/cashliq/cash-liquidator/logs/liquidator.log --working-directory=/opt/peridot-cash-liquidator/current /usr/local/bin/node src/margin-alert-check.mjs impaired'
```

**Inventory below the largest debt** (USDC for longs, XLM for shorts; fires both):

```zsh
ssh $=SSHQ root@$LQ 'systemd-run --uid=cashliq --gid=cashliq --wait --collect --property=EnvironmentFile=/etc/peridot-cash-liquidator/liquidator.env --property=StandardOutput=append:/mnt/cashliq/cash-liquidator/logs/liquidator.log --property=StandardError=append:/mnt/cashliq/cash-liquidator/logs/liquidator.log --working-directory=/opt/peridot-cash-liquidator/current /usr/local/bin/node src/margin-alert-check.mjs inventory'
```

**Opens refused on unsettled bad debt** (#25):

```zsh
ssh $=SSHQ root@$LQ 'systemd-run --uid=cashliq --gid=cashliq --wait --collect --property=EnvironmentFile=/etc/peridot-cash-liquidator/liquidator.env --property=StandardOutput=append:/mnt/cashliq/cash-liquidator/logs/liquidator.log --property=StandardError=append:/mnt/cashliq/cash-liquidator/logs/liquidator.log --working-directory=/opt/peridot-cash-liquidator/current /usr/local/bin/node src/margin-alert-check.mjs opens_refused'
```

**Where they landed.** Each alert type shows as one `"level":"alert"` line with `"alertTest":true`, followed by
`margin alert test done … "missing":[]`.

```zsh
ssh $=SSHQ root@$LQ 'grep "\"alertTest\":true" /mnt/cashliq/cash-liquidator/logs/liquidator.log | tail -n 20 | cut -c1-300'
```

```zsh
ssh $=SSHQ root@$LQ 'grep "\"msg\":\"margin alert test: live read-only cycle ok\"" /mnt/cashliq/cash-liquidator/logs/liquidator.log | tail -n 5 | cut -c1-300'
```

**Optional real-data check of pooled_health.** This fires the alert from the deployed contract's real `pooled_health`, not the synthetic
snapshot:
- A one-off overlay raises `hfDownMin` above u128::MAX, so even the empty book's `hf_down` (u128::MAX) is "below" it.
- The overlay is only used for this one run; the service keeps `margin.json`.
- The line is a real alert (no `alertTest` tag) from the live read-only cycle.

```zsh
ssh $=SSHQ root@$LQ "printf '{\"enabled\": true, \"contract\": \"$MARGIN\", \"hfDownMin\": \"340282366920938463463374607431768211456\", \"realertSecs\": 0}\n' > /etc/peridot-cash-liquidator/margin-alertcheck.json && chown root:cashliq /etc/peridot-cash-liquidator/margin-alertcheck.json && chmod 0640 /etc/peridot-cash-liquidator/margin-alertcheck.json"
```

```zsh
ssh $=SSHQ root@$LQ 'systemd-run --uid=cashliq --gid=cashliq --wait --collect --property=EnvironmentFile=/etc/peridot-cash-liquidator/liquidator.env --setenv=MARGIN_CONFIG_FILE=/etc/peridot-cash-liquidator/margin-alertcheck.json --property=StandardOutput=append:/mnt/cashliq/cash-liquidator/logs/liquidator.log --property=StandardError=append:/mnt/cashliq/cash-liquidator/logs/liquidator.log --working-directory=/opt/peridot-cash-liquidator/current /usr/local/bin/node src/margin-alert-check.mjs pooled_health'
```

```zsh
ssh $=SSHQ root@$LQ 'grep "margin pooled_health below target" /mnt/cashliq/cash-liquidator/logs/liquidator.log | grep -v alertTest | tail -n 1 | cut -c1-300'
```

```zsh
ssh $=SSHQ root@$LQ 'rm /etc/peridot-cash-liquidator/margin-alertcheck.json'
```

## 3. Switch the margin pass to live

Preconditions:
- 1.7 passed;
- section 2 showed every alert;
- the owner approved;
- opens are still paused.

Only `MARGIN_DRY_RUN` changes. `DRY_RUN` stays as it is.

```zsh
ssh $=SSHQ root@$LQ 'sed -i "s/^MARGIN_DRY_RUN=true$/MARGIN_DRY_RUN=false/" /etc/peridot-cash-liquidator/liquidator.env && grep -E "^(DRY_RUN|MARGIN_)" /etc/peridot-cash-liquidator/liquidator.env'
```

```zsh
ssh $=SSHQ root@$LQ 'systemctl restart peridot-cash-liquidator; sleep 60; systemctl show -p ActiveState -p ActiveEnterTimestamp -p NRestarts peridot-cash-liquidator'
```

```zsh
ssh $=SSHQ root@$LQ 'grep -E "\"msg\":\"(start|margin pass enabled|margin heartbeat)\"" /mnt/cashliq/cash-liquidator/logs/liquidator.log | tail -n 4 | cut -c1-400'
```

Expected: `margin pass enabled … "dryRun":false`, then `margin heartbeat … "dryRun":false`.
- With opens paused and an empty book, a live cycle signs nothing.
- Reconcile is only simulated, and is sent only on `true`.
- Settle and clear-expired send only when there is debt or an expired request.

Only after this does the owner unpause opens; that is outside this runbook.

Watch live actions:

```zsh
ssh $=SSHQ root@$LQ 'tail -f /mnt/cashliq/cash-liquidator/logs/liquidator.log | grep --line-buffered -E "margin_(liquidate|settle|reconcile|clear_expired) submitted|\"level\":\"alert\""'
```

The journal records each intent hash before the send:

```zsh
ssh $=SSHQ root@$LQ 'grep "\"kind\":\"margin_" /mnt/cashliq/cash-liquidator/state/journal.jsonl | tail -n 10 | cut -c1-300'
```

## 4. Rollback

**Level 1: margin back to dry run**, keeping the release. Use this when a margin send misbehaves; the cash pass is unaffected.

```zsh
ssh $=SSHQ root@$LQ 'sed -i "s/^MARGIN_DRY_RUN=false$/MARGIN_DRY_RUN=true/" /etc/peridot-cash-liquidator/liquidator.env && systemctl restart peridot-cash-liquidator'
```

**Level 2: margin pass off**, keeping the release. The contract id stays in the overlay, so the cash exclusion of the pooled account
remains.

```zsh
ssh $=SSHQ root@$LQ "printf '{\"enabled\": false, \"contract\": \"$MARGIN\"}\n' > /etc/peridot-cash-liquidator/margin.json && systemctl restart peridot-cash-liquidator"
```

**Level 3: previous release.** Repoint `current` at release `cb88d4dec23b4c82` (4f06aad, kept by `install_code`).
- The old release ignores the `MARGIN_*` variables and the `margin` / `lastRoundTs` keys in `state.json`. Its loader only reads `cursor`,
  `startLedger`, `borrowers` and the refresh budget.
- It does **not** exclude the pooled account from cash liquidation. While margin positions are open, prefer level 1 or 2.

```zsh
ssh $=SSHQ root@$LQ 'test -d /opt/peridot-cash-liquidator/releases/cb88d4dec23b4c82 && ln -sfn /opt/peridot-cash-liquidator/releases/cb88d4dec23b4c82 /opt/peridot-cash-liquidator/current && readlink /opt/peridot-cash-liquidator/current'
```

```zsh
ssh $=SSHQ root@$LQ 'systemctl restart peridot-cash-liquidator; sleep 45; systemctl show -p ActiveState -p ActiveEnterTimestamp -p NRestarts peridot-cash-liquidator'
```

If a margin transaction is in flight (`UNKNOWN` status in the log), first reconcile its hash from the journal against Horizon. Do not
resend it.

```zsh
ssh $=SSHQ root@$LQ 'grep "\"kind\":\"margin_" /mnt/cashliq/cash-liquidator/state/journal.jsonl | tail -n 4'
```
