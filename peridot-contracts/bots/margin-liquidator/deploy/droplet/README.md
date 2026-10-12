# Single-Droplet Pilot

Deployment candidate, not a completed deployment. Use a dedicated Ubuntu 24.04
Droplet, Frankfurt, Basic Regular 1 vCPU / 1 GiB / 25 GiB ($6/month before tax
and backups). Check memory under realistic load; resize only with approval.
This host is NOT highly available. Its outage stops liquidation processing.
Core-cache maintenance and all other bots are outside this deployment.

## Access and Release Gates

The configured `peridot` DigitalOcean context currently returns 403 for Droplet
and SSH-key inventory. It cannot yet safely provision this host. Either grant
the needed Droplet/SSH-key/firewall permissions locally or create the host in
the DigitalOcean dashboard. Never paste API tokens or private keys into chat.
Use SSH keys, restrict inbound TCP 22 to the operator's IP and permit no public
database port. Verify the host-key fingerprint through the Droplet console
before connecting. Do not use `StrictHostKeyChecking=no`.

Before deployment, commit/review/scan the exact keeper candidate and this
package. Build the keeper image from that release outside the 1 GiB server,
then transfer/load it or pull a digest-pinned registry image. The existing
App Platform dry-run uses an older runtime; do not treat it as this release.

## Prepare

Install Docker Engine and its Compose plugin using the official Ubuntu
instructions. Do not execute a downloaded shell installer. Copy this directory
to `/opt/peridot-stellar-margin` on the new server. No secret goes in the image
or repository. Only the keeper and PostgreSQL containers use this host.

Set `KEEPER_IMAGE` to the reviewed image digest or local `sha256:` image ID.
Set `POSTGRES_IMAGE` to a reviewed `postgres:16-bookworm@sha256:...` digest.
Retain those two public values in the deployment directory's `.env` file.
Before preparing credentials, verify that the selected PostgreSQL image uses
UID/GID 999 (`docker run --rm "$POSTGRES_IMAGE" id postgres`).

```bash
cd /opt/peridot-stellar-margin
sudo bash prepare-host.sh
sudo docker compose config --quiet
sudo docker compose up -d postgres
sudo docker compose up -d keeper
sudo docker compose logs --tail 50 keeper
```

Preparation refuses to overwrite configuration or an existing journal volume.
It generates database passwords and a self-signed TLS trust anchor with the
`postgres` DNS SAN; the client verifies that certificate and hostname. The
keeper role is not a PostgreSQL superuser. The database has no host port and
lives on an internal Docker network; only the keeper also has outbound access.
Do not dump `docker compose config`, container environments, or secret files
into logs/chat. `config --quiet` validates without printing credentials.

Both named volumes survive container replacement. The PostgreSQL journal is
canonical; the keeper's index is disposable. NEVER run `docker compose down -v`,
prune these volumes, or start a new empty journal for an existing signing key.
Restart policy is for service recovery, not for bypassing fatal journal stops.
Monitor restart count and cycle progress, not just container running status.

## Before Signing

1. Validate the actual Docker deployment, verified TLS and DB role permissions,
   persistence across replacement, duplicate-worker rejection and fee caps.
   Keyless dry-run does NOT exercise the database journal or real submissions.
2. Complete isolated Testnet transaction tests and the exact-release security
   scan. Generate/configure a dedicated Mainnet key on the server only after
   approval. Never copy the treasury/deployer key. Funding is a separate action.
3. Set up encrypted off-host backups, test restoration into an isolated DB,
   and alert on a stale heartbeat, fatal errors, low XLM, disk/memory pressure
   and certificate expiry. Local volume persistence is NOT a backup. No paid
   backup service is enabled by this package; approve its cost/destination first.
4. Verify the other operator's always-on core-cache maintenance, then complete
   separately guarded contract wiring and canaries. Keep frontend trading off
   until those pass. Nothing here performs maintenance/admin/contract calls.
5. Stop the old App worker and verify its absence before live signing. Remove
   the obsolete $5 App only after replacement validation and explicit approval;
   until then both services are billed. Preserve unrelated Apps and servers.

The root-only `/etc/peridot-stellar-margin/keeper.env` starts with `DRY_RUN=true`
and NO Stellar secret. Do not flip it simply because container startup succeeds.
The private database key and keeper password are root:999 mode 0640, needed by
the PostgreSQL process; the config directory itself is root-only. A Docker/root
administrator can access all container secrets and must be trusted.

## Health Alerts

`monitor.sh` runs on the host, independently of the keeper container. It needs
Bash, jq, curl, GNU date, timeout and flock. The systemd service/timer checks once
per minute for stopped containers, unhealthy PostgreSQL, a stale cycle heartbeat,
recent keeper errors and disk usage >=85%. With `KEEPER_PUBLIC_KEY` configured,
it also checks the Mainnet native wallet balance (default warning below 5 XLM).
This is a total-wallet warning, not the transaction client's spendable-reserve
calculation. An unavailable Horizon account also produces an alert condition.

Create the Telegram bot through BotFather and start a chat with it (or add it to
the intended group). Store its token and numeric destination chat ID directly
on the server in the root-only files named by `monitor.env.example`. Never paste
tokens into chat, command-line arguments, Git or images. Set file modes to0600.
Install the example as `/etc/peridot-stellar-margin/monitor.env`, configure the
dedicated keeper PUBLIC address, and leave `NOTIFY=false` until delivery testing.
Do not use treasury/deployer balances to assess keeper funding.

The current staging installation uses `NOTIFY=false`: it logs health but sends
NO alerts. Once configured, a first run sends a health message, changed states
send notifications immediately, and ongoing faults repeat at most hourly.
Failed delivery is NOT recorded as successful and is retried by the next timer.
Messages contain fixed health labels only, never raw application logs or XDRs.
The token is passed to curl over stdin, not in its argument list.

```bash
# On Linux; tests mock Docker and HTTP, never send messages or stop services.
bash test-monitor.sh
systemd-analyze verify peridot-margin-monitor.service peridot-margin-monitor.timer
```

A host-local monitor cannot report loss of the entire host or its internet
connection. External dead-man monitoring is a separate live-readiness gate.
Do not equate a running timer or a mocked send test with actual Telegram delivery.

## Backup Proposal

As checked2026-10-12, Cloudflare R2 Standard includes10GB-month,1million ClassA
and10million ClassB requests monthly. Hourly encrypted journal dumps with bounded
retention should fit this allowance for the pilot, subject to actual data size
and other usage on the account. This is an estimate, not an unlimited free plan.
An alternative is DO weekly Droplet backups at20% of the$6server ($1.20/month),
but a weekly image is not a frequent transaction-journal backup.

No bucket, paid backup plan, backup credential, scheduled uploader or retention
deletion has been created. Obtain approval and a private destination first.
Proposed implementation: consistent PostgreSQL dumps, client-side encryption
to an offline-held recovery key, timestamped immutable object names, bounded
retention and verified off-host restore rehearsal. Never back up raw Stellar
secrets into an unencrypted bucket or silently restore an old journal and sign.
The existing local synthetic DB dump/restore test is NOT off-host backup proof.

Sources: [R2 pricing](https://developers.cloudflare.com/r2/pricing/),
[DO backup pricing](https://docs.digitalocean.com/products/backups/details/pricing/).

## Recovery Procedure

Retain configuration, database volume and the reviewed image manifest together.
Rotate the 365-day TLS certificate before expiry with a controlled stop/restart;
update both server certificate and client trust, without resetting journal data.
Never disable TLS verification to work around expiry.

An older backup can omit already signed intents. After any restore, keep signing
disabled and reconcile the signing account's chain history and unresolved hashes
before resuming. Missing RPC history or an elapsed deadline is not proof a
transaction never executed. Do not automatically retire uncertain intents.

References: [Droplet creation](https://docs.digitalocean.com/products/droplets/how-to/create/),
[Docker Ubuntu installation](https://docs.docker.com/engine/install/ubuntu/),
[PostgreSQL TLS](https://www.postgresql.org/docs/16/ssl-tcp.html).
