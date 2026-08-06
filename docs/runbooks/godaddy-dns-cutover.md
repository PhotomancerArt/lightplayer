# Runbook — point lightplayer.app at fly.io (GoDaddy)

**What this does:** moves the apex `lightplayer.app` from GitHub Pages to the
fly.io service, and deletes `www` on purpose.

**Who:** Yona — the GoDaddy account is his and nobody else can do this step.

**How long:** ten minutes of clicking, then up to an hour of waiting for the
TLS certificate. The DNS itself moves in minutes at TTL 600.

**Blast radius:** lightplayer.app is unreachable for whatever your resolver
has cached of the old records — a few minutes. Rollback is §5 and is the same
ten minutes in reverse.

Every value you need is in this document. You should not have to look
anything up.

---

## 1. Preconditions

All four, in order. Do not start §3 with any of them unchecked.

**a. `infra/bootstrap.sh` has been run**, and you have its two addresses.
Re-run it to print them again — it changes nothing:

```sh
./infra/bootstrap.sh
```

Or read them straight from fly:

```sh
fly ips list --app lightplayer
```

Expected: one `v4` (shared) and one `v6`. Write them down here before you
start clicking:

```
IPv4  ____________________        (looks like 66.241.xxx.xxx)
IPv6  ____________________        (looks like 2a09:8280:1::xx:xxxx:0)
```

**b. The certificate is requested and waiting for DNS.**

```sh
fly certs show lightplayer.app --app lightplayer
```

Expected right now: status `Awaiting configuration` / "Your certificate for
lightplayer.app is being issued", with the DNS instructions listed. That is
the correct state *before* the cutover — the ACME challenge validates through
the A/AAAA records you are about to create.

**c. The service is fully smoked on `https://lightplayer.fly.dev`** — the
whole P11 checklist, every item green: `/healthz`, the app loads and the sim
runs, dev-auth refused, a real Google login round-trip, publish + anonymous
read + blob GET, `/p/…` OG tags in the HTML, deep links 200, and a machine
restart with the data surviving.

Never cut over on a partial smoke. The apex has no faster rollback than DNS
propagation.

**d. `LP_CLOUD_BASE_URL` is being flipped to the apex.** In `infra/fly.toml`
it reads `https://lightplayer.fly.dev` for the smoke pass. Change it to
`https://lightplayer.app`, commit, and let the deploy land **before** you edit
DNS — the service should already believe it is lightplayer.app when the first
real request arrives. (It keeps answering on fly.dev either way; only the
absolute OG urls and the OAuth redirect change.)

---

## 2. Where the records are

1. <https://godaddy.com> → **Sign In** (Yona's account).
2. Top right, your name → **My Products**.
3. Under **Domains**, find `lightplayer.app` → the **DNS** button next to it.
4. You land on the **Records** table. Direct link:

   <https://dcc.godaddy.com/control/portfolio/lightplayer.app/settings>

   (If that opens the settings overview rather than the table, there is a
   **DNS → Manage Zone / Manage DNS** link on that page.)

The table lists Type / Name / Value / TTL with an edit (pencil) and delete
(trash) control per row. GoDaddy applies each row edit on its own **Save** —
there is no transaction, so the zone is briefly inconsistent while you work.
That is fine and it is why §3 does the additions last.

---

## 3. The changes

Nine deletions, two additions. Everything else in the zone —
`NS`, `SOA`, any `TXT`, any mail records — **stays untouched**.

### 3a. DELETE the four `A @` records (GitHub Pages)

Verified live 2026-08-05. All four have Name `@`:

| Type | Name | Value |
|---|---|---|
| A | @ | `185.199.108.153` |
| A | @ | `185.199.109.153` |
| A | @ | `185.199.110.153` |
| A | @ | `185.199.111.153` |

### 3b. DELETE the four `AAAA @` records (GitHub Pages)

| Type | Name | Value |
|---|---|---|
| AAAA | @ | `2606:50c0:8000::153` |
| AAAA | @ | `2606:50c0:8001::153` |
| AAAA | @ | `2606:50c0:8002::153` |
| AAAA | @ | `2606:50c0:8003::153` |

GoDaddy may display these in compressed form with a different case
(`2606:50c0:8000::153`). Match on the last group `::153` and the
`8000`/`8001`/`8002`/`8003` — there are exactly four and they are all going.

### 3c. DELETE the `www` CNAME — and do not replace it

| Type | Name | Value |
|---|---|---|
| CNAME | www | `light-player.github.io` |

**This is deliberate.** The ruling, in Yona's words: *"I don't think anyone
uses www.lightplayer.app — we have no users, so now's the time to break it."*
The fly certificate covers the apex only, so a surviving `www` record would
point at a host that cannot serve it — a broken page is worse than no page.
`www.lightplayer.app` returning NXDOMAIN afterwards is the intended outcome
and a G2 pass criterion.

### 3d. ADD the two fly records

Use the addresses from §1a. **Add** → Type → Name `@` → Value → TTL: choose
**Custom** and enter `600` seconds (GoDaddy defaults to 1 hour; 600 is what
makes a rollback take ten minutes instead of sixty).

| Type | Name | Value | TTL |
|---|---|---|---|
| A | @ | *your IPv4 from §1a* | 600 |
| AAAA | @ | *your IPv6 from §1a* | 600 |

One of each. Do not add four of anything — GitHub Pages needed four A records
because it is anycast across four edges; fly needs one.

### 3e. Leave alone

`NS` (`ns37.domaincontrol.com`, `ns38.domaincontrol.com`), `SOA`, and every
`TXT`, `MX`, or `_domainkey` row in the table. Nothing in this cutover
touches domain ownership, mail, or verification records.

---

## 4. Verification

Run these in order. Each one says what to expect.

### 4a. The apex resolves to fly

```sh
dig +short lightplayer.app A
```

Expect **exactly one line**: your fly IPv4 from §1a.

If you still see `185.199.10x.153`, your resolver is caching. Ask an
authoritative server directly — this answers immediately, no cache:

```sh
dig +short @ns37.domaincontrol.com lightplayer.app A
```

Then the v6 record:

```sh
dig +short lightplayer.app AAAA
```

Expect exactly one line: your fly IPv6.

### 4b. www is gone

```sh
dig +short www.lightplayer.app
```

Expect **no output at all**. Empty is success here.

### 4c. The certificate issues

```sh
fly certs show lightplayer.app --app lightplayer
```

Expect `Status = Ready` and a `Certificate Authority` / issued line. This
usually lands within a couple of minutes of DNS propagating but can take up
to an hour; ACME validates through the records you just created, so it cannot
succeed before 4a does. Re-run until it flips — there is nothing to poke.

### 4d. The service answers on the apex

```sh
curl -sI https://lightplayer.app/
```

Expect:

```
HTTP/2 200
content-type: text/html; charset=utf-8
cache-control: no-cache
```

`no-cache` on the document is correct and deliberate: the HTML is dynamic
(OG injection, SPA fallback) while the hashed assets under `/assets/` carry
long-lived caching.

Health endpoint:

```sh
curl -s https://lightplayer.app/healthz
```

Expect: `ok`

### 4e. Deep links return real 200s

```sh
curl -sI https://lightplayer.app/p/anything-prj_test
```

Expect `HTTP/2 200`. A share URL answers 200 whether or not the project
exists, is private, or is malformed — identical responses, on purpose, so the
route cannot be used to enumerate which project uids are real. This is the
thing GitHub Pages could not do: its 404.html deep-link hack served an actual
HTTP 404 to crawlers.

An asset, to confirm the static plane and its caching:

```sh
curl -sI https://lightplayer.app/index.html | grep -i cache-control
```

### 4f. In a browser

- Open <https://lightplayer.app> — the app loads and the sim runs.
- An old bookmark with the hash route, e.g.
  `https://lightplayer.app/#/library`, redirects to the new path form.
- <https://www.lightplayer.app> fails to resolve. Expected.
- GitHub Pages is no longer reachable through the apex (`light-player.github.io`
  still works directly — that is fine and is the rollback path).

### 4g. The unfurl

Publish a real project, copy its share URL, and paste it into a link-preview
checker — <https://www.opengraph.xyz> — or straight into a Discord or iMessage
draft. Expect the project's name, the description line, and the preview image.
This is the feature the whole cutover exists for; do not skip it.

---

## 5. Rollback

If the apex is broken and you want GitHub Pages back, this is the whole
procedure. The Pages workflow (`deploy-studio-pages.yml`) is still live and
still deploying on every push to `main` — that is deliberate until P12 — so
Pages is current the moment DNS points back.

At <https://dcc.godaddy.com/control/portfolio/lightplayer.app/settings>:

**1. Delete the two fly records** you added in §3d (`A @` and `AAAA @`).

**2. Re-enter these nine, exactly as they were on 2026-08-05.** TTL: 1 hour
(GoDaddy's default) or 600 — either is fine, 600 propagates faster if you
need to move again.

| Type | Name | Value |
|---|---|---|
| A | @ | `185.199.108.153` |
| A | @ | `185.199.109.153` |
| A | @ | `185.199.110.153` |
| A | @ | `185.199.111.153` |
| AAAA | @ | `2606:50c0:8000::153` |
| AAAA | @ | `2606:50c0:8001::153` |
| AAAA | @ | `2606:50c0:8002::153` |
| AAAA | @ | `2606:50c0:8003::153` |
| CNAME | www | `light-player.github.io` |

Copy-pasteable, same values:

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

**3. Verify:**

```sh
dig +short lightplayer.app A
```

Expect the four `185.199.10x.153` addresses back.

```sh
curl -sI https://lightplayer.app/
```

Expect `HTTP/2 200` with a `server: GitHub.com` header.

**4. Leave the fly certificate alone.** `fly certs show lightplayer.app` will
go back to awaiting configuration; it costs nothing and it is one less thing
to redo when you cut over again.

**5. Nothing on the fly side needs undoing.** The service keeps answering on
`https://lightplayer.fly.dev`, the data is untouched, and litestream keeps
replicating. Only the front door moved.

---

## Appendix — the zone as it was, 2026-08-05

Dug live before the cutover, for the record:

```
$ dig +short lightplayer.app NS
ns38.domaincontrol.com.
ns37.domaincontrol.com.

$ dig +short lightplayer.app A
185.199.108.153
185.199.109.153
185.199.110.153
185.199.111.153

$ dig +short lightplayer.app AAAA
2606:50c0:8000::153
2606:50c0:8001::153
2606:50c0:8002::153
2606:50c0:8003::153

$ dig +short www.lightplayer.app CNAME
light-player.github.io.
```

`ns37`/`ns38.domaincontrol.com` confirm the zone is GoDaddy-hosted, which is
why this runbook is about GoDaddy's UI and not a registrar transfer.
