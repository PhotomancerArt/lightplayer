# infra/ — what runs lightplayer.app

One fly.io app, one machine, one volume, one bucket. There is no Terraform
and no state file: the resources are created once by a script that can be run
again safely, and the running shape lives in `fly.toml`. That is the whole
posture — the thing being deployed is a single binary with a SQLite file next
to it, and infrastructure-as-code ceremony would be heavier than the
infrastructure.

| File | What it is |
|---|---|
| `Dockerfile` | Multi-stage build: the server binary, the litestream binary, and the pre-built Studio bundle |
| `entrypoint.sh` | Restore-if-missing, then run the server under `litestream replicate -exec` |
| `fly.toml` | The deployed shape: region, volume mount, health check, one machine |
| `litestream.yml` | `/data/cloud.sqlite` → Tigris, prefix `litestream/` |
| `bootstrap.sh` | Create-if-absent: app, volume, bucket, secrets, IPs, certificate |
| `dockerignore` | Copied to `.dockerignore` at build time (see below) |

The pieces outside this directory: `.github/workflows/deploy-cloud.yml`
(builds the web artifact, then deploys) and
`docs/runbooks/godaddy-dns-cutover.md` (the DNS switch, by hand, once).

## Bootstrap order

Do these in order. Steps 1 and 3 are the only ones that need a human with
account access.

1. **Google console** — create the OAuth client and both redirect URIs. See
   "The two manual consoles" below.
2. **`./infra/bootstrap.sh`** — creates the fly app, the 3 GB volume in
   `sea`, the Tigris bucket `lightplayer-cloud`, sets the secrets (prompting
   for the Google values from step 1), allocates a shared IPv4 and an IPv6,
   and requests the TLS certificate for the apex. It prints the DNS rows at
   the end. Run it as many times as you like: every step is create-if-absent.
3. **Deploy** — either push to `main` (the workflow) or, by hand from the
   repo root:

   ```sh
   just studio-web-deploy-dir production target/pages/studio lightplayer.app
   cp infra/dockerignore .dockerignore
   flyctl deploy . --config infra/fly.toml --dockerfile infra/Dockerfile --remote-only
   ```

   The web artifact must exist *before* the deploy: the Dockerfile copies it
   from the build context and does not build it. Note the explicit
   `--config`/`--dockerfile` flags — flyctl resolves `build.dockerfile`
   relative to the working directory, so `fly.toml` deliberately has no
   `[build]` section.

4. **Smoke against `https://lightplayer.fly.dev`** — the full P11 checklist.
   ⚠️ For this pass, `LP_CLOUD_BASE_URL` in `fly.toml` must be
   `https://lightplayer.fly.dev`, or the OAuth redirect and the OG urls point
   at a domain that is still GitHub Pages. Flip it to
   `https://lightplayer.app` in the same commit as the cutover.
5. **DNS** — `docs/runbooks/godaddy-dns-cutover.md`. Not before the smoke
   passes: a half-working service on the apex has no rollback that is faster
   than DNS propagation.

## The two manual consoles

Everything else is scripted. These two are not, because they are accounts,
not resources.

### Google Cloud console — OAuth client

`console.cloud.google.com` → APIs & Services → Credentials → Create
credentials → OAuth client ID → Web application.

Both redirect URIs must be registered — the localhost one is what makes real
login testable in development:

```
https://lightplayer.app/auth/google/callback
http://localhost:8080/auth/google/callback
```

Google matches these character for character, port included. The localhost
port is pinned to 8080 for this purpose (dev-port.sh hands out a different one
per worktree, and Google will not have seen it) — see the server README for
the exact `just cloud-serve` invocation.

During the pre-DNS smoke you will also want
`https://lightplayer.fly.dev/auth/google/callback` registered, because the
redirect URI is always `LP_CLOUD_BASE_URL` + `/auth/google/callback` and that
base URL is fly.dev until the cutover. Keep it afterwards if fly.dev stays
useful as a staging front door; remove it if not. Record which you chose in
the P11 results.

The consent screen, the scopes, the flow itself, and why there is no `oauth2`
crate are documented under **"Google OAuth setup"** in
`lp-cloud/lp-cloud-server/README.md`. This file covers only the deployment
wiring: these two values are fly secrets, not plain environment.

The client id and secret go to `bootstrap.sh`'s prompt (or its
`LP_CLOUD_GOOGLE_CLIENT_ID` / `LP_CLOUD_GOOGLE_CLIENT_SECRET` environment
variables). Never into a chat, a commit, or a shell command line.

### GoDaddy — the DNS zone

`lightplayer.app` is a GoDaddy-hosted zone (`ns37`/`ns38.domaincontrol.com`).
The cutover is a hand-edit of nine records, done once, and it has its own
document with every value inline:
[`docs/runbooks/godaddy-dns-cutover.md`](../docs/runbooks/godaddy-dns-cutover.md).

## Secrets inventory

Nothing in this list is ever in the repo. `fly secrets list --app
lightplayer` shows the names and digests; the values are write-only.

| Secret | Set by | Read by |
|---|---|---|
| `AWS_ACCESS_KEY_ID` | `fly storage create` (bootstrap step 3) | blob store + litestream |
| `AWS_SECRET_ACCESS_KEY` | `fly storage create` (bootstrap step 3) | blob store + litestream |
| `BUCKET_NAME`, `AWS_ENDPOINT_URL_S3`, `AWS_REGION` | `fly storage create`, incidentally | nothing — the app reads the `LP_CLOUD_S3_*` values from `fly.toml` |
| `LP_CLOUD_GOOGLE_CLIENT_ID` | `bootstrap.sh` prompt | the OAuth redirect |
| `LP_CLOUD_GOOGLE_CLIENT_SECRET` | `bootstrap.sh` prompt | the token exchange |

And one that is not a fly secret:

| Secret | Where | Read by |
|---|---|---|
| `FLY_API_TOKEN` | GitHub repository secret | `deploy-cloud.yml`. Create with `fly tokens create deploy --app lightplayer`. Until it exists, the workflow's gate job skips the deploy instead of failing. |

`fly storage create` prints the Tigris secret access key exactly once.
`bootstrap.sh` captures that output into `~/.lightplayer/tigris-lightplayer.env`
(mode 600, outside the repo) rather than the terminal — move it into a
password manager and delete the file. You will want it the day you run
`litestream restore` from a laptop.

To rotate a Google credential:

```sh
fly secrets import --app lightplayer    # paste KEY=VALUE lines, then ^D
```

`import` rather than `set` on purpose: `fly secrets set K=V` puts the value in
argv, where every process on the machine can read it.

## Backup and restore

Litestream streams the WAL to Tigris continuously and restores on boot, so
the disaster-recovery path *is* the boot path — the only kind that stays
tested.

```sh
fly ssh console --app lightplayer -C "litestream snapshots /data/cloud.sqlite"
fly ssh console --app lightplayer -C "litestream generations /data/cloud.sqlite"
```

To restore somewhere else (with the Tigris credentials in your environment):

```sh
litestream restore -config infra/litestream.yml -o /tmp/cloud.sqlite /data/cloud.sqlite
```

Blobs and trees are not replicated and do not need to be: they are immutable,
content-addressed, and already in the same bucket.

## How to roll back DNS

If the apex has to go back to GitHub Pages, the Pages workflow
(`deploy-studio-pages.yml`) is still live and still deploying — that is
deliberate until P12 — so the only thing to undo is the zone. Re-enter these
nine records at GoDaddy, exactly as they were on 2026-08-05:

```
A     @    185.199.108.153
A     @    185.199.109.153
A     @    185.199.110.153
A     @    185.199.111.153
AAAA  @    2606:50c0:8000::153
AAAA  @    2606:50c0:8001::153
AAAA  @    2606:50c0:8002::153
AAAA  @    2606:50c0:8003::153
CNAME www  light-player.github.io
```

…and delete the two fly records (`A @` and `AAAA @`). Step-by-step, with
verification commands, is §5 of the runbook.

## Operations

The basics, each one command:

**What version is deployed?**

```bash
curl -s https://lightplayer.app/healthz
# {"status":"ok","build":"<git sha>","cloud_api_version":1}
```

The `build` sha comes from the image's `GIT_SHA` build arg (CI passes the
commit it validated); `dev` means a hand build. The Studio bundle shows its
own tag in the app chrome — the two should come from the same commit.

**Logs** (one line per request: method, path, status, ms — no query
strings, no cookies, no `/healthz` noise):

```bash
fly logs -a lightplayer
```

`fly logs` is live + recent only. There is deliberately no log shipping,
metrics stack, or APM yet — see the debt register before adding one.

**Deploys** ride CI: a green "Main push" run on `main` triggers
`deploy-cloud.yml`, which deploys exactly the sha CI validated. A red main
never ships. Manual redeploy (incident override):

```bash
gh workflow run deploy-cloud.yml
```

**Rollback** — fly keeps the image history:

```bash
fly releases -a lightplayer          # find the last good version's image
fly deploy -a lightplayer --image <registry.fly.io/lightplayer:...>
```

DB schema note: rolling back past a migration is NOT covered by image
rollback — that's what the Litestream point-in-time restore is for (§Backup
and restore). Migrations are additive so far; keep them that way.

**Restore drill** — a backup you have never restored is a hope, not a
backup. Quarterly, or after any storage change:

```bash
set -a; source ~/.lightplayer/tigris-lightplayer.env; set +a
litestream restore -config infra/litestream.yml -o /tmp/drill.sqlite /data/cloud.sqlite
sqlite3 /tmp/drill.sqlite "select count(*) from users; select count(*) from projects;"
```

Sane counts = the drill passes. Retention is 30 days of point-in-time
(24h snapshots), configured in `infra/litestream.yml`.

**Uptime**: fly's health check restarts a wedged machine, but nothing
external notices fly itself being down. Recommended (2 minutes, free): a
healthchecks.io or UptimeRobot ping on `https://lightplayer.app/healthz`.
