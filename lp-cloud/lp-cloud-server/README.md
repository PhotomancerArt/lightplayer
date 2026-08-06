# lp-cloud-server

The cloud service's HTTP edge — **the** edge crate. `axum`, `tokio`, and
`reqwest` live here and nowhere else in the workspace; everything below this
crate is sans-IO.

## The three planes

| Route | Plane | Auth |
|---|---|---|
| `POST /api` | control | session cookie → `Actor`; anonymous is a caller |
| `GET /b/{hash}` | content | none — the hash *is* the capability |
| `PUT /b/{hash}` | content | session required; body must hash to `{hash}` |
| `GET /t/{hash}` | content | none |
| `PUT /t/{hash}` | content | session required; manifest must *package* to `{hash}` |
| `GET /p/{share}` | page | none; OG tags only when the project is link-visible |
| `GET /auth/google` | auth | none — starts the OAuth round trip |
| `GET /auth/google/callback` | auth | the `lp_oauth_state` cookie is the credential |
| `POST /auth/logout` | auth | the session cookie, if there is one |
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

## Google OAuth setup

Real sign-in is the authorization-code flow, hand-rolled in
`src/auth/google_auth.rs` — no `oauth2` crate, and no JWT validation, because
the access token is spent immediately at Google's userinfo endpoint over TLS.
The server needs two secrets and one thing registered on Google's side.

### 1. Register the OAuth client

In the [Google Cloud Console](https://console.cloud.google.com/), in the
project that owns the app:

1. **APIs & Services → OAuth consent screen.** User type *External*. Fill in
   the app name, the support email, and the developer contact. Scopes:
   `openid`, `.../auth/userinfo.email`, `.../auth/userinfo.profile` — all
   three are non-sensitive, so this app never enters the verification review
   that Drive- or Gmail-scoped apps do. While the screen is in *Testing*,
   only the listed test users can sign in; add yours.
2. **APIs & Services → Credentials → Create credentials → OAuth client ID.**
   Application type **Web application**.
3. **Authorized redirect URIs** — add both, exactly as written:

   ```
   https://lightplayer.app/auth/google/callback
   http://localhost:8080/auth/google/callback
   ```

   Google matches these **character for character, port included**. The
   deployed one is the production origin's. For the localhost one, register
   the port you will actually use and then pin it, because
   `scripts/dev-port.sh` hands out a different port per worktree and a
   redirect URI Google has not seen is a `redirect_uri_mismatch` refusal:

   ```sh
   LP_CLOUD_PORT=8080 LP_CLOUD_BASE_URL=http://localhost:8080 just cloud-serve
   ```

   Day-to-day local work needs none of this — use `/auth/dev` instead, and
   pin a port only when you are exercising the real Google flow.

   *Authorized JavaScript origins* stays empty — the app never uses Google's
   JS SDK, and every call is server-to-server.
4. Copy the client ID and client secret.

### 2. Give them to the server

| Variable | Value |
|---|---|
| `LP_CLOUD_GOOGLE_CLIENT_ID` | `…apps.googleusercontent.com` |
| `LP_CLOUD_GOOGLE_CLIENT_SECRET` | the secret; never logged, never in the repo |
| `LP_CLOUD_BASE_URL` | the origin whose callback you registered |

The redirect URI the server sends is always `LP_CLOUD_BASE_URL` +
`/auth/google/callback`, so **the base URL is what has to match the console
entry** — a mismatched one is refused by Google, not by us. Both credentials
are required together: with only one, `/auth/google` answers `503` rather
than sending the user to a Google that would refuse them at the callback.
Deployment wiring for these — they are fly secrets, not plain env — and the
console walk-through for the deployed origin live in `infra/README.md`, which
points back here for the flow itself.

### 3. What happens then

`GET /auth/google` → 302 to Google with a random `state` also written to a
10-minute `HttpOnly` cookie → consent → `GET /auth/google/callback` → state
check → token exchange → userinfo → **the email must be verified** → the
account is upserted *by Google `sub`* (a changed address is the same account)
→ session cookie → 303 to `/`, or to a `?next=` that has been validated as a
same-origin relative path. `POST /auth/logout` deletes the session row and
then clears the cookie.

`LP_CLOUD_GOOGLE_ENDPOINT_BASE` exists for the tests only: it points all
three Google URLs at a stub, which is how `tests/google_auth.rs` runs the
whole flow — real handler, real reqwest — with no network and no credentials.

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
"artifact" — no port is bound and no web build is needed. The one exception
is `tests/google_auth.rs`, which binds a loopback port for its stub Google so
the sign-in handler's two outbound calls are real HTTP. Domain rules
(visibility, membership, push validation) are `lp-cloud-domain`'s tests and
are not repeated here.
