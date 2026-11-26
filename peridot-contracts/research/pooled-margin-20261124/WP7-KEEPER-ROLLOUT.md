# WP7: margin pass in the cash liquidator, Droplet rollout (written, NOT executed)

## Code and scope

Code is on branch `feat/margin-keeper`, under `peridot-contracts/bots/cash-liquidator`:
1. `5dedf2a`: verbatim import of deployed release 4f06aad (tree `230bad98`).
2. The margin pass.
3. The fixes from the live-RPC smoke test.
4. The prune fix (cherry-pick of `e742359`).
5. The owner decisions of 25 Nov: Telegram alerts, the external dead-man, conversion of seized collateral, dust liquidations.

The README section "Margin pass" describes the behaviour. "Alerts: Telegram, and an external dead-man" describes alert delivery.

## Hosts

- **Keeper Droplet:** `peridot-cashliq`, <keeper Droplet IP>, unit `peridot-cash-liquidator`, user `cashliq`. Logs go to
  `/mnt/cashliq/cash-liquidator/logs/liquidator.log`.
- **Pause host:** `peridot-pausewatch`, <pause host IP>, a separate machine. It runs the second dead-man watcher, unit
  `peridot-keeper-watch.timer`, user `keeperwatch`. Its log is `/var/log/peridot-keeper-watch.log`.

## Release process

The release process mirrors `research/cash-borrowing-ops-20261118/COMMANDS-21NOV.md` A2 and A5:
1. `git archive`;
2. sha256 check;
3. `install_code`, which runs the host tests and switches `current`;
4. restart.

**Owner gate (25 Nov).** Margin opens stay paused until this keeper runs LIVE on the Droplet, after the dry stage, with every alert shown
arriving on Telegram and the dead-man gate test passed. Telegram delivery is set up before M16.

**Signing.** Nothing here signs a Stellar transaction, except the keeper itself after the switch to live (section 6).

**Provenance caveat.** The deployed release `cb88d4dec23b4c82` = 4f06aad comes from DROPLETS.md and COMMANDS-21NOV A2. It was not
re-verified on the host while this was written. Step 1.3 checks it before anything changes.

## 0. Prerequisites

1. **Contract.** The margin v2 contract is deployed from WASM `b1cb1cb21f02de1b1a9047d31fa60ffc066c2ddedc94c4dff2fcbdb0a7b1766f`, with opens
   paused.
   - Its `Ctrl` is the cash controller, its markets are the pinned USDC/XLM receipts, and its `Pool` is
     `CBBMQBNHB2FYVZYV7VNHOJHUMTFJLR4PUMRVQYNW6RHIKZO2NQMIBUCV`. The keeper refuses anything else.
   - The keeper key `GDQARP…5B5G` is neither its admin nor its guardian.
2. **Scan.** Almanax has scanned the branch range with no open findings, and the branch is pushed.
3. **Inventory.** It is shared with cash liquidations. The funded 500 USDC / 2,500 XLM covers the pilot caps (about 102 USDC per long and
   about 805 XLM per short). The keeper alerts when it no longer does.
4. **Owner accounts.** The owner has a Telegram account and a Healthchecks.io account (section 2).

Shell variables on the Mac (no secrets):

```zsh
export LQ="<keeper Droplet IP, from your own notes>"
```

```zsh
export PW="<pause host IP, from your own notes>"
```

```zsh
export SSHQ="-i <path to your keeper SSH key> -o UserKnownHostsFile=<path to your known_hosts file for the keeper hosts>"
```

```zsh
export MARGIN=CA6Q7HULJTGFMTB7YLJWFKPQVIIWAP46PQHAY3H5V2EZ2R7HBCW6N7NZ   # deployed 26 Nov (M2)
```

```zsh
export WP7=<full commit sha on feat/margin-keeper>
```

## 1. Release on the keeper Droplet, and the dry-run stage

### 1.1 Build the tarball (Mac)

```zsh
mkdir -p $HOME/Peridot/releases && cd $HOME/Peridot/releases
```

```zsh
git -C $HOME/Peridot/Peridot-Soroban archive --format=tar.gz --prefix=cash-liquidator/ $WP7:peridot-contracts/bots/cash-liquidator > cash-liquidator-${WP7:0:7}.tgz
```

```zsh
shasum -a 256 cash-liquidator-${WP7:0:7}.tgz
```

Record the hash as `SRC_SHA256`. Keep this exact file: a rebuild gives the same contents but a different gzip hash.

### 1.2 Copy it to both hosts

```zsh
scp $=SSHQ cash-liquidator-${WP7:0:7}.tgz root@$LQ:/root/
```

```zsh
scp $=SSHQ cash-liquidator-${WP7:0:7}.tgz root@$PW:/root/
```

### 1.3 Check what runs now (read-only)

```zsh
ssh $=SSHQ root@$LQ 'readlink /opt/peridot-cash-liquidator/current; systemctl show -p ActiveState -p ActiveEnterTimestamp -p NRestarts peridot-cash-liquidator; grep -E "^(DRY_RUN|MARGIN_)" /etc/peridot-cash-liquidator/liquidator.env'
```

Expected:
- `current -> …/releases/cb88d4dec23b4c82`;
- active;
- no `MARGIN_` lines.

Write down `DRY_RUN`. It stays **unchanged**, so cash liquidations keep running as they do now.

### 1.4 Install the release (host tests, then `current`)

**`/root/lib.sh` is the CL liquidator's copy now** (the CL group bootstrap overwrote it; it refuses this tarball with
"tarball is not bots/cl-liquidator", targets `/opt/peridot-cl-liquidator`, and leaves a `<sha16>.tmp` folder there). Copy the
reviewed cash `lib.sh` (sha256 `93b64e0cfb9f86825b2fd5ba4b46a12a2e02187be71bc7ca147eccae5becfe27`, from
`research/cash-borrowing-ops-20261118/droplet/lib.sh`) to `/root/lib-cash.sh`, check its hash, and leave `/root/lib.sh` alone.

```zsh
ssh $=SSHQ root@$LQ 'SRC_TARBALL=/root/cash-liquidator-'${WP7:0:7}'.tgz NODE_VERSION=v22.23.5 SRC_SHA256=<SRC_SHA256> bash -c ". /root/lib-cash.sh && install_code" && readlink /opt/peridot-cash-liquidator/current'
```

Expected:
- sha256 OK;
- the host `node --test` passes (81 tests);
- `current` points to the new release.

On failure nothing has changed, so stop.

### 1.5 Alert delivery before the restart

Do section 2 (Telegram, then Healthchecks.io) now. The new release reports `alert delivery NOT configured` at start until the file from
2.4 exists.

### 1.6 Margin overlay and the margin dry-run flag

```zsh
ssh $=SSHQ root@$LQ "printf '{\"enabled\": true, \"contract\": \"$MARGIN\"}\n' > /etc/peridot-cash-liquidator/margin.json && chown root:cashliq /etc/peridot-cash-liquidator/margin.json && chmod 0640 /etc/peridot-cash-liquidator/margin.json && cat /etc/peridot-cash-liquidator/margin.json"
```

```zsh
ssh $=SSHQ root@$LQ 'grep -q "^MARGIN_" /etc/peridot-cash-liquidator/liquidator.env || printf "MARGIN_CONFIG_FILE=/etc/peridot-cash-liquidator/margin.json\nMARGIN_DRY_RUN=true\n" >> /etc/peridot-cash-liquidator/liquidator.env; grep -E "^(DRY_RUN|MARGIN_)" /etc/peridot-cash-liquidator/liquidator.env'
```

### 1.7 Restart and check the start lines

```zsh
ssh $=SSHQ root@$LQ 'systemctl restart peridot-cash-liquidator; sleep 60; systemctl show -p ActiveState -p ActiveEnterTimestamp -p NRestarts peridot-cash-liquidator'
```

```zsh
ssh $=SSHQ root@$LQ 'grep -E "\"msg\":\"(start|alert delivery: telegram|alert delivery NOT configured|external dead-man NOT configured|margin pass enabled|margin pin check failed at start|margin state bootstrapped|margin heartbeat|heartbeat)\"" /mnt/cashliq/cash-liquidator/logs/liquidator.log | tail -n 10 | cut -c1-400'
```

Expected, in order:
1. `alert delivery: telegram` with `prefix: [Stellar]` and `healthcheckPing: true`;
2. `start`;
3. `margin pass enabled` with `wasm: b1cb1cb2…766f` and `dryRun: true`;
4. `margin state bootstrapped`;
5. `heartbeat`;
6. `margin heartbeat`.

There must be no `NOT configured` lines. Healthchecks shows the check as **up** within a minute.

Record `ActiveEnterTimestamp`. **The dry stage clock starts there.**

### 1.8 Dry-stage pass criterion (all must hold, at least 12 h after `ActiveEnterTimestamp`)

1. **Running.** The service is active with NRestarts=0, and the Healthchecks check has stayed up.
2. **Cycles.** There are at least 1,400 `margin heartbeat` lines, whose `ledger` values strictly increase.
3. **Errors.** There are 0 `margin cycle failed` lines, and 0 margin `warn`/`alert` lines that are not tagged `alertTest`.
4. **No margin sends.** The journal has 0 `"kind":"margin_` entries.
5. **No cash regression.** Cash `heartbeat` lines continue with no new cash `alert` lines.
6. **Reads are correct against the deployed contract.** These fields of the latest `margin heartbeat` must equal the read-only simulations
   below:
   - `active` (0 while opens are paused);
   - `bsDebtShares` = 0/0;
   - `impaired` = false;
   - `backstopNavPt` / `backstopShares` = `backstop_nav`;
   - `hf` = u128::MAX (no pooled debt).

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

The independent reads are simulations only (`--send=no`), with the keeper's public key as source:

```zsh
stellar contract invoke --id $MARGIN --rpc-url https://soroban-rpc.mainnet.stellar.gateway.fm --network-passphrase "Public Global Stellar Network ; September 2015" --source-account GDQARPHRQFU7K52XMESPNBSKZE6GPCORZV54EBGDVSXJQBOIDUMO5B5G --send=no -- get_position_counter
```

```zsh
stellar contract invoke --id $MARGIN --rpc-url https://soroban-rpc.mainnet.stellar.gateway.fm --network-passphrase "Public Global Stellar Network ; September 2015" --source-account GDQARPHRQFU7K52XMESPNBSKZE6GPCORZV54EBGDVSXJQBOIDUMO5B5G --send=no -- backstop_nav
```

```zsh
stellar contract invoke --id $MARGIN --rpc-url https://soroban-rpc.mainnet.stellar.gateway.fm --network-passphrase "Public Global Stellar Network ; September 2015" --source-account GDQARPHRQFU7K52XMESPNBSKZE6GPCORZV54EBGDVSXJQBOIDUMO5B5G --send=no -- pooled_health
```

```zsh
stellar contract invoke --id $MARGIN --rpc-url https://soroban-rpc.mainnet.stellar.gateway.fm --network-passphrase "Public Global Stellar Network ; September 2015" --source-account GDQARPHRQFU7K52XMESPNBSKZE6GPCORZV54EBGDVSXJQBOIDUMO5B5G --send=no -- is_impaired
```

```zsh
stellar contract invoke --id $MARGIN --rpc-url https://soroban-rpc.mainnet.stellar.gateway.fm --network-passphrase "Public Global Stellar Network ; September 2015" --source-account GDQARPHRQFU7K52XMESPNBSKZE6GPCORZV54EBGDVSXJQBOIDUMO5B5G --send=no -- opens_paused
```

`opens_paused` must be `true` throughout.

## 2. Owner setup: Telegram, Healthchecks.io, pause-host watcher

### 2.1 Telegram, option A: a new bot

1. In Telegram, open **@BotFather** and send `/newbot`.
2. Give it a name (for example "Peridot Stellar alerts") and a username ending in `bot`.
3. BotFather replies with the **bot token**. Keep it in your password manager only.
4. Open the new bot and press **Start**, or add the bot to the group chat that should receive alerts, and send it one message.
5. Get the chat id on the Mac. The token is read silently, and is never echoed, put in argv or written to shell history:

```zsh
read -rs "TG_TOKEN?bot token: " && echo
```

```zsh
printf 'url = "https://api.telegram.org/bot%s/getUpdates"\n' "$TG_TOKEN" | curl -s --config - | python3 -c 'import json,sys; [print(u["message"]["chat"]["id"], u["message"]["chat"].get("title") or u["message"]["chat"].get("username")) for u in json.load(sys.stdin)["result"] if "message" in u]'
```

The number printed next to your user or group name is the **chat id** (groups are negative).

```zsh
unset TG_TOKEN
```

### 2.2 Telegram, option B: reuse the existing Avalanche bot

Use the existing bot's token and chat id from where the owner keeps them for the Avalanche keeper. Do not copy them through the repo or a
command line.
- Every message from this keeper starts with **`[Stellar]`** (`alerts.prefix` in `config/mainnet.json`), so one bot and one chat serve
  both chains.
- To change the prefix, add `ALERT_PREFIX=[Stellar-mainnet]` to `/etc/peridot-cash-liquidator/liquidator.env` (not a secret) and restart.
- If the Avalanche bot uses a webhook, `getUpdates` (2.1 step 5) is refused. That does not matter here, because the chat id is already
  known.

### 2.3 Healthchecks.io (external dead-man, first channel)

1. Log in at healthchecks.io and create a project, for example "Peridot Stellar".
2. **Add Check**:
   - name `[Stellar] peridot-cashliq`, so its own messages carry the prefix too;
   - schedule **Simple**, **Period 1 minute**, **Grace 3 minutes**. The proposal is 3 min of grace for the 30 s cycle; the keeper pings
     after every healthy cycle, and the check goes down about 4 min after the last ping.
3. **Integrations → Telegram**: follow the instructions to connect @HealthchecksBot to the same chat, or add it to the same group.
   Enable that integration on the check.
4. Copy the check's **Ping URL** (`https://hc-ping.com/<uuid>`). It goes into the keeper's secrets file in 2.4.
5. **Settings → API Access → Create read-only API key.** It goes into the pause host's watcher file in 2.6. Use the **read-only** key,
   never the read-write key.

### 2.4 Write the keeper's secrets file on the Droplet (token, chat id, ping URL)

The values are typed at hidden prompts on the Droplet. They never appear on a command line, in shell history or in a log. The file is
`cashliq`-owned, chmod 600.

```zsh
ssh $=SSHQ -t root@$LQ 'umask 077; install -m 600 -o cashliq -g cashliq /dev/null /etc/peridot-cash-liquidator/telegram.env && read -rs -p "bot token: " T && echo && read -r -p "chat id: " C && read -rs -p "healthchecks ping URL: " P && echo && printf "TELEGRAM_BOT_TOKEN=%s\nTELEGRAM_CHAT_ID=%s\nHEALTHCHECK_PING_URL=%s\n" "$T" "$C" "$P" > /etc/peridot-cash-liquidator/telegram.env; unset T C P; stat -c "%a %U:%G %n" /etc/peridot-cash-liquidator/telegram.env'
```

Expected: `600 cashliq:cashliq /etc/peridot-cash-liquidator/telegram.env`.

### 2.5 Test: all eight alert types to Telegram (never signs)

The test sends one message per type through the real path: log → notifier → Telegram. It exits 0 only when Telegram confirmed all eight
(`alerts.channel: "telegram"`). With margin enabled it first runs one read-only cycle against the deployed contract.

```zsh
ssh $=SSHQ root@$LQ 'systemd-run --uid=cashliq --gid=cashliq --wait --collect --property=EnvironmentFile=/etc/peridot-cash-liquidator/liquidator.env --property=StandardOutput=append:/mnt/cashliq/cash-liquidator/logs/liquidator.log --property=StandardError=append:/mnt/cashliq/cash-liquidator/logs/liquidator.log --working-directory=/opt/peridot-cash-liquidator/current /usr/local/bin/node src/margin-alert-check.mjs; echo "exit $?"'
```

```zsh
ssh $=SSHQ root@$LQ 'grep "\"msg\":\"margin alert check done\"" /mnt/cashliq/cash-liquidator/logs/liquidator.log | tail -n 1 | cut -c1-400'
```

Expected:
- `exit 0`, and `"channel":"telegram"` with all eight types under `delivered` and `"missing":[]`:
  `liquidatableOver10min`, `pooledHealthLow`, `impaired`, `inventoryLow`, `opensRefusedBadDebt`, `keeperDown`, `conversionFailed`,
  `dustLiquidation`.
- On the phone: eight `[Stellar] ALERT [peridot-cashliq] <type>` messages.

A single type can be re-sent on its own, for example:

```zsh
ssh $=SSHQ root@$LQ 'systemd-run --uid=cashliq --gid=cashliq --wait --collect --property=EnvironmentFile=/etc/peridot-cash-liquidator/liquidator.env --property=StandardOutput=append:/mnt/cashliq/cash-liquidator/logs/liquidator.log --working-directory=/opt/peridot-cash-liquidator/current /usr/local/bin/node src/margin-alert-check.mjs conversionFailed; echo "exit $?"'
```

### 2.6 Pause-host watcher (second, independent dead-man channel)

**Least-privilege choice.** The watcher reads the keeper's Healthchecks check with a **read-only** API key:
- that key cannot ping, pause or edit checks;
- the watcher needs no Stellar key and no access to the keeper host;
- the pause key volume is made inaccessible to it.

Watching the keeper's on-chain activity through public RPC needs no secret at all. It was rejected because the keeper sends nothing on
chain in steady state (dry run, or nothing to do), so it cannot tell "idle" from "down".

The release is installed in its **own** directory, `/opt/peridot-keeper-watch`. The pause watcher's `/opt/peridot-cash-liquidator/current`
is not touched.

```zsh
ssh $=SSHQ root@$PW 'echo "<SRC_SHA256>  /root/cash-liquidator-'${WP7:0:7}'.tgz" | sha256sum -c -'
```

```zsh
ssh $=SSHQ root@$PW 'id keeperwatch >/dev/null 2>&1 || useradd --system --no-create-home --shell /usr/sbin/nologin keeperwatch; id keeperwatch'
```

```zsh
ssh $=SSHQ root@$PW 'R=/opt/peridot-keeper-watch/releases/$(echo <SRC_SHA256> | cut -c1-16); mkdir -p $R.tmp && tar -xzf /root/cash-liquidator-'${WP7:0:7}'.tgz -C $R.tmp --strip-components=1 --no-same-owner && find $R.tmp -name "._*" -delete && (cd $R.tmp && PATH=/opt/node-v22.23.5/bin:$PATH npm ci --omit=dev --ignore-scripts --no-audit --no-fund && /usr/local/bin/node --test) && chown -R root:root $R.tmp && chmod -R go-w,a+rX $R.tmp && mv $R.tmp $R && ln -sfn $R /opt/peridot-keeper-watch/current && readlink /opt/peridot-keeper-watch/current'
```

```zsh
ssh $=SSHQ root@$PW 'readlink /opt/peridot-cash-liquidator/current'
```

That must still print the pause watcher's own release, unchanged.

Secrets file: the Telegram token, the chat id and the read-only key, at hidden prompts. It is `keeperwatch`-owned, chmod 600:

```zsh
ssh $=SSHQ -t root@$PW 'install -d -m 750 -o root -g keeperwatch /etc/peridot-keeper-watch && umask 077 && install -m 600 -o keeperwatch -g keeperwatch /dev/null /etc/peridot-keeper-watch/keeper-watch.env && read -rs -p "bot token: " T && echo && read -r -p "chat id: " C && read -rs -p "healthchecks READ-ONLY api key: " K && echo && printf "TELEGRAM_BOT_TOKEN=%s\nTELEGRAM_CHAT_ID=%s\nHEALTHCHECKS_API_KEY=%s\nHEALTHCHECKS_CHECK=dummy-until-listed\n" "$T" "$C" "$K" > /etc/peridot-keeper-watch/keeper-watch.env; unset T C K; stat -c "%a %U:%G %n" /etc/peridot-keeper-watch/keeper-watch.env'
```

List the checks the key can see. This prints names, status and `unique_key`, never a secret:

```zsh
ssh $=SSHQ root@$PW 'systemd-run --uid=keeperwatch --gid=keeperwatch --wait --pipe --collect --setenv=KEEPER_WATCH_FILE=/etc/peridot-keeper-watch/keeper-watch.env --working-directory=/opt/peridot-keeper-watch/current /usr/local/bin/node src/keeper-watch.mjs --list'
```

Put the `unique_key` of `[Stellar] peridot-cashliq` into the file. This replaces the placeholder line:

```zsh
ssh $=SSHQ -t root@$PW 'read -r -p "unique_key of the keeper check: " U && sed -i "s/^HEALTHCHECKS_CHECK=.*/HEALTHCHECKS_CHECK=$U/" /etc/peridot-keeper-watch/keeper-watch.env; unset U; grep -c "^HEALTHCHECKS_CHECK=" /etc/peridot-keeper-watch/keeper-watch.env'
```

Install and start the timer:

```zsh
ssh $=SSHQ root@$PW 'cp /opt/peridot-keeper-watch/current/deploy/pausehost/peridot-keeper-watch.service /opt/peridot-keeper-watch/current/deploy/pausehost/peridot-keeper-watch.timer /etc/systemd/system/ && systemctl daemon-reload && systemctl enable --now peridot-keeper-watch.timer && systemctl list-timers peridot-keeper-watch.timer --no-pager'
```

```zsh
ssh $=SSHQ root@$PW 'sleep 90; tail -n 3 /var/log/peridot-keeper-watch.log'
```

Expected: `keeper-watch ok` with `"status":"up"`.

## 3. Dead-man gate test (stop the keeper, both alerts, restart, both recoveries)

**Cost.** This stops cash liquidations for about 6–10 min. Run it while margin opens are paused and the cash markets are quiet.

**Gate facts.** It produces exactly the gate facts
`deadman: {service: "healthchecks", pauseHostWatcher: true, stopTestAt: <ISO>, alertsReceived: true}`.

1. **Precondition.** Healthchecks shows the check **up**, and the watcher's last line is `keeper-watch ok … "status":"up"`:

```zsh
ssh $=SSHQ root@$PW 'tail -n 1 /var/log/peridot-keeper-watch.log'
```

2. **Stop the keeper.** The command prints the time it stops it. That printed ISO time is **`stopTestAt`**:

```zsh
ssh $=SSHQ root@$LQ 'date -u +%Y-%m-%dT%H:%M:%SZ; systemctl stop peridot-cash-liquidator; systemctl is-active peridot-cash-liquidator'
```

3. **Wait for both alerts** (within about 5 min: 1 min period + 3 min grace + the 1-min watcher timer). On Telegram:
   - from **Healthchecks**: `[Stellar] peridot-cashliq` is **DOWN**;
   - from the **pause-host watcher**: `[Stellar] ALERT [peridot-cashliq via pause host] keeperDown`.

```zsh
ssh $=SSHQ root@$PW 'grep "keeper-watch alert keeper_down delivered" /var/log/peridot-keeper-watch.log | tail -n 1'
```

4. **Restart the keeper:**

```zsh
ssh $=SSHQ root@$LQ 'systemctl start peridot-cash-liquidator; sleep 60; systemctl show -p ActiveState -p ActiveEnterTimestamp -p NRestarts peridot-cash-liquidator'
```

5. **Wait for both recoveries** (within about 2 min). On Telegram:
   - Healthchecks: `[Stellar] peridot-cashliq` is **UP**;
   - the watcher: `[Stellar] RESOLVED [peridot-cashliq via pause host] keeperDown`.

```zsh
ssh $=SSHQ root@$PW 'grep "keeper-watch resolved keeper_down delivered" /var/log/peridot-keeper-watch.log | tail -n 1'
```

6. **Record the gate.** It passes only if all four messages arrived:
   - `service: "healthchecks"`;
   - `pauseHostWatcher: true`;
   - `stopTestAt`: the ISO time printed in step 2;
   - `alertsReceived: true`.

   If any message is missing, record `alertsReceived: false` and fix it before going live.

The keeper restart in step 4 re-runs the start checks (pins, roles, allowances). Expect the same start lines as 1.7.

## 4. Seized-collateral conversion and dust (behaviour to expect live)

- **Long liquidation.** The keeper redeems exactly the seized XLM pTokens, then sells exactly the redeemed XLM for USDC on the pinned pool,
  with `out_min` = Reflector value less 1.5 %.
- **Short liquidation.** It redeems the seized USDC pTokens, and buys XLM only while XLM is below its funded minimum.
- **Below the floor.** The lot is held and retried with backoff. `conversionFailed` alerts after 3 attempts or 15 min.
- **Unknown outcome.** It is parked for operator review and never retried.
- **Journal kinds:** `margin_redeem`, `margin_swap`.
- **Dust.** A margin liquidation with an expected loss ≤ $2 (oracle USD, including the fee) is sent, with `dustLiquidation`. Above $2 it
  is not sent, and an `unprofitableSkipped` alert fires.
- **Cash pass.** It keeps its plain redeem, with no swap, and leaves margin lots' pTokens alone.
- **Live-pool check (25 Nov).** A read-only Mainnet simulation through the keeper's own `prepare` was run with the keeper key as source:
  5 XLM → USDC on `CBBMQ…BUCV`.
  - It ran in the default recording mode, with one source-account auth entry: root `swap`, with the `transfer` nested.
  - `get_tokens` = [XLM, USDC], and the return value decodes as a u128: 6,080,915 against an oracle floor of 6,000,637, about 18 bps
    below the oracle. It used 14.6 M instructions.

### Releasing a lot parked for review

A lot goes to `review` only after an UNKNOWN redeem or swap outcome. The keeper never retries it.

1. Stop the keeper.

```zsh
ssh $=SSHQ root@$LQ 'systemctl stop peridot-cash-liquidator'
```

2. Show the parked lots and the matching journal intents.

```zsh
ssh $=SSHQ root@$LQ 'python3 -c "import json; [print(json.dumps(l)) for l in json.load(open(\"/mnt/cashliq/cash-liquidator/state/state.json\")).get(\"convert\", {}).get(\"lots\", []) if l[\"stage\"] == \"review\"]"'
```

```zsh
ssh $=SSHQ root@$LQ 'grep -E "\"kind\":\"margin_(redeem|swap)\"" /mnt/cashliq/cash-liquidator/state/journal.jsonl | tail -n 6'
```

3. Look the intent hash up on Horizon (`https://horizon.stellar.org/transactions/<hash>`).
   - If it **executed**, the lot is finished: remove it, and the converted funds are already inventory.
   - If it **did not execute** (not found after the time bound passed, or failed), put the lot back to the stage it was in, so it is
     retried.

Remove an executed lot:

```zsh
ssh $=SSHQ -t root@$LQ 'read -r -p "lotId: " L && python3 -c "import json,sys; p=\"/mnt/cashliq/cash-liquidator/state/state.json\"; s=json.load(open(p)); s[\"convert\"][\"lots\"]=[l for l in s[\"convert\"][\"lots\"] if l[\"lotId\"]!=sys.argv[1]]; json.dump(s, open(p+\".tmp\",\"w\")); import os; os.replace(p+\".tmp\", p)" "$L" && chown cashliq:cashliq /mnt/cashliq/cash-liquidator/state/state.json'
```

Re-arm a lot that did not execute. Enter `swap` or `redeem`, matching the journal kind:

```zsh
ssh $=SSHQ -t root@$LQ 'read -r -p "lotId: " L && read -r -p "stage (swap|redeem): " G && python3 -c "import json,sys,os; p=\"/mnt/cashliq/cash-liquidator/state/state.json\"; s=json.load(open(p)); [l.update(stage=sys.argv[2], attempts=0, nextAt=0, alerted=False) for l in s[\"convert\"][\"lots\"] if l[\"lotId\"]==sys.argv[1] and sys.argv[2] in (\"swap\",\"redeem\")]; json.dump(s, open(p+\".tmp\",\"w\")); os.replace(p+\".tmp\", p)" "$L" "$G" && chown cashliq:cashliq /mnt/cashliq/cash-liquidator/state/state.json'
```

4. Start the keeper again.

```zsh
ssh $=SSHQ root@$LQ 'systemctl start peridot-cash-liquidator'
```

## 5. Watch live actions

```zsh
ssh $=SSHQ root@$LQ 'tail -f /mnt/cashliq/cash-liquidator/logs/liquidator.log | grep --line-buffered -E "margin_(liquidate|redeem|swap|settle|reconcile|clear_expired) submitted|\"level\":\"alert\""'
```

```zsh
ssh $=SSHQ root@$LQ 'grep "\"kind\":\"margin_" /mnt/cashliq/cash-liquidator/state/journal.jsonl | tail -n 10 | cut -c1-300'
```

## 6. Switch the margin pass to live

Preconditions:
- 1.8 passed;
- 2.5 delivered all eight types;
- 3 recorded `alertsReceived: true`;
- the owner approved;
- opens are still paused.

Only `MARGIN_DRY_RUN` changes.

```zsh
ssh $=SSHQ root@$LQ 'sed -i "s/^MARGIN_DRY_RUN=true$/MARGIN_DRY_RUN=false/" /etc/peridot-cash-liquidator/liquidator.env && grep -E "^(DRY_RUN|MARGIN_)" /etc/peridot-cash-liquidator/liquidator.env'
```

```zsh
ssh $=SSHQ root@$LQ 'systemctl restart peridot-cash-liquidator; sleep 60; systemctl show -p ActiveState -p ActiveEnterTimestamp -p NRestarts peridot-cash-liquidator'
```

```zsh
ssh $=SSHQ root@$LQ 'grep -E "\"msg\":\"(start|margin pass enabled|margin heartbeat)\"" /mnt/cashliq/cash-liquidator/logs/liquidator.log | tail -n 4 | cut -c1-400'
```

Expected: `margin pass enabled … "dryRun":false`. With opens paused and an empty book, a live cycle signs nothing. Only after this does the
owner unpause opens; that is outside this runbook.

## 7. Rollback

**Level 1: margin back to dry run.** The cash pass and the alerts are unaffected.

```zsh
ssh $=SSHQ root@$LQ 'sed -i "s/^MARGIN_DRY_RUN=false$/MARGIN_DRY_RUN=true/" /etc/peridot-cash-liquidator/liquidator.env && systemctl restart peridot-cash-liquidator'
```

**Level 2: margin pass off.** The cash exclusion of the pooled account stays.

```zsh
ssh $=SSHQ root@$LQ "printf '{\"enabled\": false, \"contract\": \"$MARGIN\"}\n' > /etc/peridot-cash-liquidator/margin.json && systemctl restart peridot-cash-liquidator"
```

**Level 3: previous release** `cb88d4dec23b4c82`.
- That release sends no Telegram alerts and no Healthchecks pings. Healthchecks and the watcher will report it **down**: pause the check
  in Healthchecks first, or expect those alerts.
- It does not exclude the pooled account from cash liquidation, and it does not know about open conversion lots. Prefer level 1 or 2
  while margin positions or lots are open.

```zsh
ssh $=SSHQ root@$LQ 'test -d /opt/peridot-cash-liquidator/releases/cb88d4dec23b4c82 && ln -sfn /opt/peridot-cash-liquidator/releases/cb88d4dec23b4c82 /opt/peridot-cash-liquidator/current && readlink /opt/peridot-cash-liquidator/current'
```

```zsh
ssh $=SSHQ root@$LQ 'systemctl restart peridot-cash-liquidator; sleep 45; systemctl show -p ActiveState -p ActiveEnterTimestamp -p NRestarts peridot-cash-liquidator'
```

**Watcher off (pause host):**

```zsh
ssh $=SSHQ root@$PW 'systemctl disable --now peridot-keeper-watch.timer'
```

**In-flight transactions.** If a margin transaction is in flight (`UNKNOWN` in the log, or a lot in `review`), reconcile its hash from the
journal against Horizon first. Do not resend it.

```zsh
ssh $=SSHQ root@$LQ 'grep "\"kind\":\"margin_" /mnt/cashliq/cash-liquidator/state/journal.jsonl | tail -n 4'
```

## Run log

- **26 Nov 2026, stage 1 (cash only, margin off):** tarball `cash-liquidator-6004b54.tgz`, sha256
  `fc83b902f96c1fa232e7080f3947a6ff9b8f846588d5fc92e4bfe203c908bacc` (81/81 tests). Installed as
  `/opt/peridot-cash-liquidator/releases/fc83b902f96c1fa2` with `/root/lib-cash.sh`. Telegram bot: a new bot (owner),
  chat "Peridot ONLY". Alert check 15:09:45 UTC: all eight types delivered, `missing: []`. Restarted 15:10:18 UTC
  (`DRY_RUN=false` unchanged, `healthcheckPing: true`, no margin pass). Healthchecks check `[Stellar] peridot-cashliq`
  up; pause-host watcher installed (`/opt/peridot-keeper-watch/releases/fc83b902f96c1fa2`), `status: up` from 15:11:27 UTC.
  Rollback target: `releases/cb88d4dec23b4c82`. Next: ceremony M1-M15, then 1.6-1.7 (dry stage clock), section 3.
- **26 Nov 2026, ceremony M1-M15 done** (M1 `b4cbb349…6a85`, M15 `e401c76f…7852`); `gate config` ok, backstop 750 pT /
  7,501,000,000 shares, opens paused, 0 positions, not impaired. The `stellar contract invoke` checks now pass an explicit
  `--rpc-url` (the CLI's built-in `mainnet` network has no RPC URL and fails with "Invalid URL").

