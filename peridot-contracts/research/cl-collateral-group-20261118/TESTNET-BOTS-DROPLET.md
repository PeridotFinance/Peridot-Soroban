# CL Testnet bots on `peridot-cashliq` (temporary, 22 Nov 2026)

**Testnet only.** This moves the two CL Testnet liquidator soak bots (stack R1 and stack R2, see `TESTNET-REAL-POOL.md` §4) off the operator Mac onto the existing Droplet `peridot-cashliq` (165.245.249.83). They run there until the R2 upgrade test after the ETAs (≈ 2026-11-22 12:44:21 UTC). They are then removed.

Nothing here signs or sends on Mainnet. The Mainnet cash liquidator, the pause watcher, `peridot-cl-liquidator`, their keys, the env files and the firewall are not changed.

Scripts are in `droplet-testnet/`:

| File | Runs on | Purpose |
|---|---|---|
| `tnbot-ops.sh` | Mac | `ssh-ok`, `prepare`, `install`, `start`, `verify`, `stop-mac`, `status`, `fetch`, `teardown` |
| `bootstrap-tnbot.sh` | Droplet (root) | `install`, `put-key`, `put-data`, `check`, `start`, `status` |
| `teardown-tnbot.sh` | Droplet (root, sent via ssh stdin) | removes everything, then verifies |

## 1. What gets installed

**User `tnbot`.**
- System user and group, shell `/usr/sbin/nologin`, password locked.
- Member of no other group. Not in sudoers. Home `/opt/tnbot` (root-owned, 0750 root:tnbot).
- Root disk only: the bootstrap refuses to run if `/opt` is not on `/`.

**Layout.**

| Path | Owner / mode | Content |
|---|---|---|
| `/opt/tnbot/app/releases/<sha16>` + `current` | root, read-only | `bots/cl-liquidator` from `git archive 4b63939:peridot-contracts/bots/cl-liquidator`. The bot tree is identical to 8b71f32 (5f48b0d heartbeat + data-key fix + parallel borrower reads). `npm ci --omit=dev --ignore-scripts`; host `node --test` must pass (67/67 on the Mac rehearsal). |
| `/opt/tnbot/bin/stellar` | root 0755 | stellar-cli 26.1.0 x86_64-linux-gnu, the same version as the Mac. sha256 `e18d5a76…be5c0c` (GitHub asset digest). |
| `/opt/tnbot/rN/config.json` | root:tnbot 0640 | the Mac `testnet-real{,2}.local.json` with only `dataDir` changed to `/opt/tnbot/rN/data`. |
| `/opt/tnbot/rN/stellar/identity/*.toml` | tnbot 0600 (dir 0700) | the two Testnet identities of that bot |
| `/opt/tnbot/rN/data/` | tnbot 0700 | `journal.jsonl` and `state.json` (snapshot of the Mac files), `soak.log`, and `home/` (CLI spec cache) |

**Node.** The bootstrap uses the existing `/usr/local/bin/node` (v22, the cash hosts' pinned v22.23.5). It does not install or relink node.

**Units** `peridot-tn-clbot-r1` / `-r2`. Each runs `node src/main.mjs --config /opt/tnbot/rN/config.json --watch --live` with:
- `User=tnbot`, `MemoryMax=200M`, `MemorySwapMax=0`, `CPUQuota=25%`, `TasksMax=64`;
- `NoNewPrivileges`, `ProtectSystem=strict`, `ProtectHome=yes`, `PrivateTmp`, `PrivateDevices`, `ProtectProc=invisible`, an empty capability set, and the `Protect*`/`Restrict*` set;
- `ReadWritePaths=/opt/tnbot/rN/data` only;
- `InaccessiblePaths=-/mnt/cashliq -/mnt/pausewatch -/etc/peridot-cash-liquidator -/etc/peridot-cl-liquidator -/var/lib/cashliq -/var/lib/clliq -/opt/peridot-cash-liquidator -/opt/peridot-cl-liquidator`, plus the other bot's directory, so R1 cannot see R2's keys and vice versa. The `-` prefix is needed because `/mnt/pausewatch` is on the other Droplet; without it the unit would fail with 226/NAMESPACE.
- `HOME`/`XDG_*` point into the data directory. `STELLAR_CONFIG_HOME=/opt/tnbot/rN/stellar` is the only `STELLAR_*` variable the bot passes on to the CLI (`chain.mjs cliEnv`).
- `Restart=on-failure`, 15 s.
- Log: `/opt/tnbot/rN/data/soak.log`. There is no logrotate (a few MB/day) and no needrestart override; a needrestart restart is harmless.

## 2. Keys: Testnet only, never printed

Transferred: exactly `clgr-liq`, `clgr-hb` (R1) and `clgr-liq2`, `clgr-hb2` (R2). `clg-admin`, the borrowers, the issuer and every other identity stay on the Mac. The upgrade test needs `clg-admin` there.

Gates on the Mac (`tnbot-ops.sh prepare`/`install`). Any failure refuses the transfer:
1. The name is on the allowlist and matches `^clgr-(liq|liq2|hb|hb2)$`.
2. The TOML is 0600. `stellar keys address` equals the pinned public key, which equals `liquidator`/`heartbeatAccount` in the config.
3. Network: the account exists on Testnet Horizon (200) and does **not** exist on Mainnet Horizon (404). Result on 22 Nov: all four are 200 on Testnet and 404 on Mainnet.
4. The config `networkPassphrase` is exactly `Test SDF Network ; September 2015`, and every RPC host contains `testnet`.

Transfer: `ssh root@… bootstrap-tnbot.sh put-key rN <alias> < ~/.config/stellar/identity/<alias>.toml`. The content goes straight into ssh stdin and is not staged or echoed.

On the Droplet, `put-key`:
- accepts only an allowlisted alias for its own bot;
- checks the shape (exactly one `seed_phrase = "…"` or `secret_key = "S…"` line) without printing it;
- installs the file 0600 tnbot;
- re-derives the public key as tnbot. A mismatch, or a match with a Mainnet role key, deletes the file and refuses.

`check_config` re-asserts the Testnet passphrase literal and the absence of "Public Global". The bot itself refuses `--live` unless the passphrase is exactly Testnet (`assertLiveAllowed`, re-checked per call).

## 3. Why `tnbot` cannot read Mainnet keys (on paper; `check` verifies on the host)

- **`/mnt/cashliq/cash-liquidator/keys/liquidator.secret`.** The file is 0600 cashliq and its dir 0700 cashliq (DROPLETS.md, lib.sh `ensure_key`). tnbot is neither the owner nor in group cashliq, and has no capabilities, so DAC denies it. In the units, `/mnt/cashliq` is also an `InaccessiblePaths` mount: the path does not exist inside the unit.
- **`/etc/peridot-cash-liquidator`** (0750 root:cashliq) and **`/etc/peridot-cl-liquidator`** (0750 root:clliq). tnbot is not in either group, and both paths are inaccessible in the units.
- **CL Mainnet keys** `/mnt/cashliq/cl-liquidator/keys/*` (if bootstrapped) are 0600 clliq under 0700, and also under `/mnt/cashliq`.
- **`/root`** is 0700 and also `ProtectHome=yes`.
- **No escalation.** No sudo, `NoNewPrivileges`, no SUID (`RestrictSUIDSGID`), no login shell, password locked.

`bootstrap-tnbot.sh check` prints PASS/FAIL and opens no secret:
- `runuser -u tnbot -- test -r <f>` (access(2), no open) for every file under `/mnt/cashliq`, `/etc/peridot-*`, `/root` and `/var/lib/{cashliq,clliq}`. It also prints mode/owner (metadata only) of the three Mainnet key files.
- Inside each unit's exact sandbox (`systemd-run -p <same properties>`):
  - both aliases resolve to the pinned public keys;
  - `/mnt/cashliq`, `/etc/peridot-*`, `/root` and the other bot's `stellar/` are inaccessible;
  - the code is read-only and the data dir is writable;
  - one read-only Testnet simulation (`stage2_status`) goes through the CLI.

## 4. Operator steps

From `/Users/joshua/Peridot/Peridot-Soroban/.wt-cl-group/peridot-contracts`, with `OPS=research/cl-collateral-group-20261118/droplet-testnet/tnbot-ops.sh`:

```bash
bash $OPS ssh-ok && echo reachable        # firewall allows only 84.185.36.152; do NOT add a café IP
bash $OPS prepare                         # done 22 Nov 10:37 UTC: SRC_SHA256 5df323c4eaff26e1…c859, 4 identity gates pass
bash $OPS install                         # scp, bootstrap install, 4 keys, data snapshot, `check` must print CHECK PASSED
bash $OPS start                           # enables + starts both units (Mac bots still running: overlap of a few minutes)
bash $OPS verify                          # plan cycles + sent-L1a hashes; each hash checked successful on Testnet Horizon
bash $OPS stop-mac                        # TERM PIDs 74590/74595 (command line re-checked) and their caffeinate -w
bash $OPS status
```

**Acceptance for "heartbeating".** Both logs currently show `mode: normal` (pool in range). The fixed-timer D3 heartbeat (`clgr-hb*`) only fires while `checkpoint_nav` is blocked, so it is not exercised. Acceptance is per unit:
- `plan` lines advancing;
- at least one `sent L1a ok:true` hash from the Droplet log, `successful=true` on Testnet Horizon, with source = that bot's liquidator.

The heartbeat alias is verified by `check` (sandboxed `stellar keys address`).

**During the overlap.** The Mac and the Droplet share source accounts. An occasional `txBAD_SEQ` or failed L1a is expected and is not an install failure.

**If verification fails.** `ssh … systemctl disable --now peridot-tn-clbot-r1 peridot-tn-clbot-r2`, leave the Mac bots running, then teardown.

**Deadline.** If SSH is still blocked at 12:40 UTC, nothing moves: the Mac bots keep running.

**Upgrade test (Mac, after the ledger close time passes 1795351461 = 12:44:21 UTC).**
```bash
CLG_TN_DIR=/Users/joshua/Peridot/Peridot-Soroban/.wt-cl-group/peridot-contracts/target/clg/testnet-real2 \
  bash /Users/joshua/Peridot/Peridot-Soroban/.wt-cl-group/peridot-contracts/prototypes/cl-collateral-group/testnet-real/05-upgrade.sh execute
```
It is resumable: `once` keys are in `state.env`. Expect a non-empty snapshot diff: interest on debt, and the NAV checkpoint state moved by the bots' L1a.

**Removal (after the test).**
```bash
bash $OPS teardown      # fetch data back to target/clg/testnet-real{,2}/bot/droplet-<ts>/, then teardown-tnbot.sh via stdin
# not moved: bash $OPS stop-mac
```

`teardown-tnbot.sh`:
1. Disables and stops both units, deletes the unit files, runs daemon-reload and reset-failed.
2. Kills tnbot processes.
3. Shreds the identity files, then runs `rm -rf /opt/tnbot /root/tnbot-stage`.
4. Runs `userdel`/`groupdel tnbot`.
5. Verifies that none of these remain:
   - unit (LoadState `not-found`);
   - user or group;
   - path or `*tnbot*` name;
   - file with the old uid/gid on `/` or `/mnt/cashliq`;
   - `clgr-*.toml`;
   - process.
6. Prints the cash/pause/CL unit state, as proof they were untouched.

Remaining by design: journald lines of the two units, auth.log lines from useradd/userdel.

## 5. Log

Times are UTC on 22 Nov 2026.

- 10:27: SSH from the café IP times out, as expected (the firewall allows only 84.185.36.152).
- 10:37: `prepare` done. The release rehearsal on the Mac (`npm ci`, `node --test` 67/67, `--once` dry run on R1 and R2 against Testnet) passes. Plan: R1 L1a refresh simulated OK; R2 no stage.
- 11:25: SSH still blocked. 11:40:05: SSH works (operator updated the firewall at home).
- 11:40: `install`:
  - npm ci; host `node --test` 67/67;
  - 4 keys put, each re-derived to its pinned public key;
  - journal/state snapshots (R1 4,553, R2 2,083 journal lines);
  - `check` **PASSED**:
    - `liquidator.secret` is 600 cashliq:cashliq under 700 cashliq; `test -r` as tnbot: denied;
    - no file under `/mnt/cashliq`, `/etc/peridot-*` or `/root` is readable by tnbot;
    - in both sandboxes, `/mnt/cashliq`, `/etc/peridot-*`, `/root` and the other bot's keys are inaccessible;
    - CLI Testnet read ok.
  - `/etc/peridot-cl-liquidator` does not exist on this host. `peridot-cash-liquidator`: active since 2026-11-19 06:10:37, NRestarts 0. Unchanged.
- 11:41:26 / 11:41:27: `peridot-tn-clbot-r1` / `-r2` active, `live: true`, Testnet passphrase, mode normal.
- 11:42–11:44: first Droplet sends succeed on Testnet during the overlap:
  - R1 L1a fe32f504… (11:42:11) and 76d64f78… (11:43:21);
  - R2 206b4b38… (11:44:21).
- 11:44:59: `stop-mac`. PIDs 74590/74595 stopped by TERM, their `caffeinate -w` exited with them, and no Mac bot remains.
- 11:45–11:48: Droplet only, 0 warn/alert lines:
  - R1 4 L1a sends, e.g. 4476da92… and fbc0ccdc…;
  - R2 3, e.g. 09f17bbc… and b9ff9081….
  - All are `successful=true` with source = that bot's liquidator.
  - MemoryPeak 58 MB / 48 MB (limit 200M). NRestarts 0.
