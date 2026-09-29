# Isolated Testnet reporter Droplet

Deployed September26,2026 using DigitalOcean context `peridot2`. Never use the
`peridot` context for this worker: the existing production XLM keeper is separate.

## Deployment

- Name `peridot-depth-testnet-20260926`, ID `603935393`, Frankfurt `fra1`.
- IPv4 `164.90.178.15`, Basic `s-1vcpu-1gb`:1GiB RAM,25GiB local disk,$6/month
  base price verified through the API. No paid backups, extra volumes or IPv6.
  User ceiling is$10/month; taxes/transfer overages are not a hard billing cap.
- Dedicated firewall `005be16e-a085-48ef-a6dd-56032f5255b0`, attached ONLY to this
  Droplet. SSH inbound restricted to operator IP at deployment; if it changes,
  update this firewall's SSH source deliberately. Outbound TCP80/443/53 and
  UDP53/123 only. Password and keyboard-interactive SSH disabled.
- Dedicated SSH key ID `59614784`; private key remains in ignored
  `target/depth-droplet-20260926/ssh_ed25519`. SSH host key was pinned on first
  connection (TOFU); subsequent connections require an exact match. Fingerprint
  `SHA256:0hMg5wtCjzc0TgHwY8dgr+oJLcPvMhYLAckDqD8D3YA`.
- Image `sha256:cba821e46cb1300e5729a632e0b10f9d6317e2a964ccd4d193677e391d0d0383`,
  local tag `peridot-depth-testnet:reviewed`. Dockerfile pins its Node22 base;
  package lock pins JS dependencies. Source runtime is unchanged from scanned
  `85789496eb498f1c8cb53ca0a02d551ea4e1aef6`.

## Keys, state and execution

ONLY the dedicated Testnet reporter `GCQFJG4JVPI4SLBOAHMQOO27JGA6II6NZWCVUY2B5L6TKLFDPON3HND7`
was transferred over pinned encrypted SSH after validating its public key.
Root-only0600 `/etc/peridot-depth-testnet/reporter.env` supplies the container
environment by variable name, not a secret command-line argument. Root/Docker
administrators can access that environment; never print full Docker inspection,
systemd environment or process environment. No secret in cloud-init, image, Git
or logs. Admin, Mainnet, treasury and existing keeper keys were NOT transferred.

Service `peridot-depth-testnet.service` is enabled on boot. Container is non-root
UID/GID1000, read-only root filesystem, all capabilities dropped, no-new-privileges,
bounded memory/CPU/PIDs and no inbound application ports. It signs only fixed
Testnet observation operations after runtime network/code/config/auth/fee guards.

Canonical live journal is now `/var/lib/peridot-depth-testnet/publisher.jsonl`
on the Droplet's persistent disk (directory0700/file0600, UID/GID1000). Original
completed rehearsal journal was copied byte-for-byte before starting; local
`target/depth-testnet-replay` remains historical evidence and is NOT current.
Do not run the local reporter/harness concurrently or overwrite the remote journal
with the older local copy. Persistent disk survives process/reboot, not Droplet
deletion. No automatic journal rotation/backup was introduced; retain public
evidence before any later migration. Journal's1MiB limit still stops for review.

`Restart=no` is deliberate: unknown hashes, failed transactions or stale locks
require reconciliation, not blind retries. A graceful stop preserves history;
every new process must collect a fresh30-minute window. Systemd boot activation
is not proof that an unresolved old journal can resume. Public logs are in
`journalctl -u peridot-depth-testnet.service`; journald storage capped at200MiB.

## Verified result and limits

Service began September26 19:47:14UTC, run `21de8877-cbb2-4635-a3d9-f0f9d33648f8`.
First sample collected/agreed with unchanged guards; no missed slot or failure.
Expired prior report invalidated successfully on Testnet:
`82ee80487aa5beba260958fa3c515a56fe5dcf3b1e6de441d6b562428faad57b`, ledger4885692.
Fresh publication still requires a complete healthy window; earliest approximately
20:19UTC if observations continue qualifying. This is a controlled mock fixture,
not an independent Testnet yXLM market or live lending migration.

106keeper tests pass, including2deployment-constraint tests; remote service
syntax verification, Testnet network/manifest preflight, time synchronization,
copied artifact hashes and container restrictions verified. npm install audit
reported0vulnerabilities. Packaging commit `270e6b69863798d1f0ded3f69393a511f0d9ee8c`
is pushed and remote-verified on `leveraged-fix`. These deployment files have NOT
received a new Almanax scan: its MCP connection and shell API-key environment are
unavailable in this session. Prior runtime scan is not a scan of the new package.

Original timely lost-response restart remains unproven. Prior expired-report
governed recovery did complete; see `DEPTH_LIVE_REHEARSAL.md`. Mainnet migration,
economic/liquidation checks and explicit activation remain gated. No Mainnet
transaction, production-keeper change or spending from the400XLM ceiling.

Read-only status from the repository directory:

```sh
ssh -i target/depth-droplet-20260926/ssh_ed25519 \
  -o IdentitiesOnly=yes -o StrictHostKeyChecking=yes \
  -o UserKnownHostsFile=target/depth-droplet-20260926/known_hosts \
  root@164.90.178.15 \
  'systemctl is-active peridot-depth-testnet; journalctl -u peridot-depth-testnet -n 12 --no-pager -o cat'
```
