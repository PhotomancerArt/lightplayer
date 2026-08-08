# The lp-cloud service: content-opaque message API on one origin

- Status: accepted
- Date: 2026-08-06
- Plan: lp2025/2026-08-05-1642-cloud-folders-sync

## Context

The share-by-URL feature (vision D20: make something on lightplayer.app,
copy the URL, send it — it opens running, no login) required the
project's first network service. The service's shape was settled in a
vision/planning round with decisions D1–D26 recorded in the planning
directory; this ADR records the load-bearing architecture choices for
the repo.

## Decisions

**The cloud is a place you sync with, not a storage backend.** The
local library (OPFS in the browser) remains the source of truth;
cloud interaction is push/pull of content-addressed snapshots over the
lineage machinery in `lpc-history` (see the sibling ADR
`2026-08-05-project-history-dag-joins.md`). Offline-first falls out:
everything pulled is local, and login gates only sync calls.

**The server never parses project content.** It stores blobs, tree
manifests (as their canonical `lph1` preimage, so the content address
IS the package hash on every backend), the full per-project event log,
and a client-computed sidecar (name, format version, preview PNG) that
feeds listing cards and OG tags. Format migration stays client-side in
`lpa-upgrade`; the server survives weekly format bumps unchanged.

**Message vocabulary, not REST.** `lp-core/lpc-cloud-api` defines
Request/Response types in Rust (`CloudCallSpec` binds each request to
its response type at compile time); the HTTP edge maps one POST
endpoint plus a blob/tree transfer plane. The API is versioned
(`CLOUD_API_VERSION`) under **version-and-refuse** — deliberately NOT
lpc-wire's no-compat policy, because a browser tab can sit open for
days across server deploys. Refusals travel as HTTP 200 `CloudError`
answers: they are message-plane answers, not transport failures.
The durable public contracts are elsewhere: share URLs
(`/p/<slug>-prj_<uid>`, uid-authoritative) and the persisted event
format are forever; the message vocabulary is the least durable layer.

**Blob reads are open; the hash is the capability.** `GET /b/{hash}`
and `/t/{hash}` require no session: content addresses are 256-bit
unguessable, there is no listing, and the no-login viewer and OG
`og:image` fetches have no session to present. Writes are
session-gated and hash-verified. Project *metadata* access is
visibility-checked (`private` reads as `NotFound` — no existence
leak).

**Hexagonal with first-class fakes.** `lp-cloud-domain` is sans-IO
behind ports (MetaStore/BlobStore/Clock/IdMint); `lp-cloud-store-mem`
is the complete dev/test backend; `lp-cloud-store-sqlite` is the
production adapter (WAL, fail-fast on backend errors), with a shared
conformance suite run against both. The client's test transport wraps
the real domain in-process — the flagship scenario suite runs two
clients and a server with zero network.

**One origin, one machine.** The fly.io app serves the static Studio
bundle, the API, and the share pages from one origin (same-origin
cookies, per-URL OG injection, real 200 deep links — GH Pages
hosting retired at the 2026-08-06 DNS cutover). Single machine +
volume by design: SQLite is single-writer; durability is Litestream
WAL replication to Tigris (30-day point-in-time) plus content-
addressed blobs already in the bucket; availability leans on
offline-first clients treating brief downtime as a sync retry. The
scale-out path, if ever needed, is a Postgres adapter behind the same
port — not LiteFS, not a second machine.

**No Terraform.** `infra/` is fly.toml + an idempotent bootstrap
script; the estate is ~5 resources. Revisit when it stops fitting on
one page.

## Consequences

- Auth is Google-only (server-side code flow + userinfo; sessions as
  hashed DB tokens). Membership is per-project by email, resolving at
  first sign-in. **Amended 2026-08-07**
  (`2026-08-07-provider-based-auth.md`): auth is now provider-based, not
  Google-shaped — `LoginProviders`/`LoginOptions` let the client render
  sign-in from server-reported connections (prod Google, local dev a
  passwordless picker, a self-host password method possible later
  without a client fork). The session mint stays exactly as described
  here (one `open_session` every provider converges on; the cookie
  carries no provider), which is what made the amendment additive
  rather than a rewrite.
- Deploys ride CI (`deploy-cloud.yml` on a green "Main push"),
  carrying the validated sha into `/healthz`'s build report.
- The share-envelope ADR's "no cloud provider and no account system"
  premise is amended (envelopes remain the anonymous copy-semantics
  path; cloud sync is the identity-preserving path).
- Known debts recorded in `docs/debt/`: abuse/quota posture before
  any public announcement; no metrics/APM by choice.
