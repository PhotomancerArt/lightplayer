# lp-cloud-server

The cloud service's HTTP edge — **the** edge crate. `axum`, `tokio`, and
(from P08) `reqwest` live here and nowhere else in the workspace; everything
below this crate is sans-IO.

## The three planes

| Route | Plane | Auth |
|---|---|---|
| `POST /api` | control | session cookie → `Actor`; anonymous is a caller |
| `GET /b/{hash}` | content | none — the hash *is* the capability |
| `PUT /b/{hash}` | content | session required; body must hash to `{hash}` |
| `GET /t/{hash}` | content | none |
| `PUT /t/{hash}` | content | session required; manifest must *package* to `{hash}` |
| `GET /p/{share}` | page | none; OG tags only when the project is link-visible |
| `GET /auth/dev` | auth | localhost + `LP_CLOUD_DEV_AUTH` (404 otherwise) |
| `GET /healthz` | ops | none |
| anything else | page | a static file if it exists, else the SPA document |

Blobs and trees are separate routes because they are addressed differently:
a blob by SHA-256 of its bytes, a tree by `TreeManifest::package_hash()`.
See `src/content/tree_preimage.rs` for how a tree ends up stored at that
address in a store that only ever hashes what it is given.

## Running it

```sh
just cloud-serve                                     # mem + fs blobs + dev auth
LP_CLOUD_STORE=sqlite just cloud-serve               # persists across restarts
LP_CLOUD_STATIC_DIR=target/pages/studio just cloud-serve   # serve the real app
```

The port comes from `scripts/dev-port.sh` and the recipe prints the URL —
never assume one (AGENTS.md, "Dev server ports").

Configuration is all environment, parsed in one place: see the table at the
top of `src/config.rs`.

## The demo walkthrough (G1)

With `just cloud-serve` running at `$BASE` (the URL it printed):

```sh
# 1. sign in — dev auth, no Google needed; keep the cookie
curl -c /tmp/lp.jar "$BASE/auth/dev?email=you@example.com"

# 2. publish a project at a client-minted uid, link-visible
UID=prj_$(LC_ALL=C tr -dc '0-9A-Za-z' </dev/urandom | head -c 16)
curl -b /tmp/lp.jar -X POST "$BASE/api" -H 'content-type: application/json' \
  -d "{\"version\":1,\"request\":{\"publishProject\":{\"uid\":\"$UID\",\"visibility\":\"link\",\"slug\":\"zook-dome\"}}}"

# 3. upload a preview PNG on the blob plane (address = sha256 of the body)
HASH=$(shasum -a 256 preview.png | cut -d' ' -f1)
curl -b /tmp/lp.jar -X PUT --data-binary @preview.png "$BASE/b/$HASH"

# 4. read the project back ANONYMOUSLY — no cookie, link visibility only
curl -X POST "$BASE/api" -H 'content-type: application/json' \
  -d "{\"version\":1,\"request\":{\"getProject\":{\"uid\":\"$UID\"}}}"

# 5. the share URL, with its OG tags
curl -s "$BASE/p/zook-dome-$UID" | grep 'og:'

# 6. the blob, anonymously, cached forever
curl -sI "$BASE/b/$HASH" | grep -i cache-control
```

Steps 4 and 5 against a `"visibility":"private"` project return the same
shapes with nothing in them — `notFound` and a plain document. That is
deliberate: a private project must be indistinguishable from one that never
existed, or the uid space becomes searchable.

## Testing

`cargo test -p lp-cloud-server`. The edge tests drive the real router with
`tower::ServiceExt::oneshot` against mem-backed stores and a tempdir
"artifact" — no port is bound and no web build is needed. Domain rules
(visibility, membership, push validation) are `lp-cloud-domain`'s tests and
are not repeated here.
