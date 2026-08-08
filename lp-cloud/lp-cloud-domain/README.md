# lp-cloud-domain

The cloud sync service's logic, with nothing plugged into it.

`CloudService::handle(actor, request)` answers every `CloudRequest` from
`lpc-cloud-api` against injected ports. There is no transport here, no
executor, no clock, and no randomness — which is what makes the whole
service testable as plain function calls, and what keeps the axum edge
(P07) free to be the only crate that knows what HTTP is.

`handle` is an exhaustive match and nothing else: every arm hands its payload
to a private handler that takes that request's struct and returns the one
response struct that answers it — `fn get_project(&self, actor, GetProject)
-> Result<ProjectInfo, CloudError>` — and only wraps the result back. So the
match is the single place in the crate that speaks in
`CloudRequest`/`CloudResponse` terms, and a handler that answered with the
wrong shape would not compile rather than reaching a client. The pairing it
honors is `lpc_cloud_api::CloudCallSpec`, which the client side reads from the
other direction.

## Ports

| Port | What it is |
|---|---|
| `MetaStore` | All service state: users, sessions, projects, membership, head refs, sidecars, the per-project event log, the blob index |
| `BlobStore` | Content-addressed bytes. Defined here, *not* held by `CloudService` — blob transfer is edge-level |
| `Clock` | `now() -> f64` epoch seconds |
| `IdMint` | Random bytes for `usr` uids and session tokens |

`MetaStore` is deliberately **one** trait. Those tables are one consistency
domain — a push appends events, moves the frontier, and replaces the sidecar
in one breath; a login upserts a user and resolves their pending
memberships. Each is one SQLite transaction once P04 lands, and a split
trait invites an adapter that can half-apply one.

Its methods are **infallible** on purpose. `CloudError` is the client-facing
vocabulary and has no backend-failure code (there is nothing useful to tell
a client about a disk that stopped answering), so the port is total from the
domain's point of view and an adapter whose backend can fail owns that
policy itself. That is what lets `handle` return exactly
`Result<CloudResponse, CloudError>` without inventing an error the API does
not have.

Adapters: `lp-cloud-store-mem` (dev and tests) and `lp-cloud-store-sqlite`
(P04). The trait is object-safe so one conformance suite can hold both to
the same behavior.

## The rules that live here

**Visibility.** A `Link` project is readable by anyone holding its uid,
anonymous included — the uid *is* the share link. A `Private` project is
readable by members. Writes always require membership.

A private project the caller cannot see answers **`NotFound`, never
`NotAuthorized`**. "This exists but is not yours" turns the API into an
oracle for which project uids are real, and the uid is the credential. The
one place `NotAuthorized` appears is an authenticated non-member writing to
a `Link` project, whose existence they could already see.

**Push is never blocked** (D5). A push whose parents do not match the
current head is not a conflict to refuse — it becomes an additional head,
and the frontier says so. One rule covers every case: a pushed commit
consumes every head it names as a parent and takes their place; whatever is
left over stays. Parents = the sole head → fast-forward. Parents = both
heads (a clobber join) → the frontier collapses back to one. Parents = a
stale base → a second head, reported as `PushOutcome::NewHead`.

**Event validation is two-tier**, because the server's log is the
*interleaving* of every client's line, not any one client's history. A
first push (empty log) must replay cleanly through
`lpc_history::ProjectHistory::from_events` — there is no other line to
blame. A later push is tried the same way, and success means the pusher
continued the server's line; failure is *not* an error, because a `Joined`
authored against another line legitimately fails to replay onto the
interleaving. Such a batch is instead checked for internal consistency —
finite timestamps, no second origin event, joins naming two distinct
versions — and accepted. Anything stricter would need the pusher's own base,
which a content-opaque server does not have. See `push_validation.rs`; the
tier a push landed in is reported as `PushValidation::{Linear, Divergent}`.

**The server is content-opaque** (D3). It never opens a tree manifest, so
the missing-blob check covers the hashes it was handed (the tree, and the
preview PNG if the sidecar names one) and not the files a manifest
references. `SidecarMeta` is stored verbatim — name, format version, and
preview hash are the client's word, and the service does not audit them.

**The client owns project uids** (D21). `PublishProject` records the uid it
was given; publishing a uid someone else owns answers `NotFound`, so the
endpoint cannot be walked to discover which uids exist. The service mints
only `usr` uids and session tokens, both from `IdMint` bytes.

## Beyond `handle`

Two things the auth edge cannot do for itself, because they mint identity:
`upsert_user` (which also resolves pending memberships — the moment an
invitation by email becomes access, Q4) and the session trio
`open_session` / `resolve_session` / `close_session`. Only the *hash* of a
session token is ever stored; the raw bytes are returned once, for the edge
to put in a cookie. Cookies, OAuth, and transport stay at the edge.

## Tests

Unit tests sit at the bottom of the files whose logic needs no store
(`push_validation.rs`, `model/project_refs.rs`). The service's own tests are
in `tests/cloud_service.rs` rather than at the bottom of `cloud_service.rs`,
because they run against the real in-memory adapters and
`lp-cloud-store-mem` depends on this crate: that dev-dependency cycle
resolves for an integration target but not for a `#[cfg(test)]` module,
which compiles a second copy of the crate. The alternative — a second,
hand-rolled store living here — is exactly the fake-drifts-from-real hazard
the shared adapter exists to prevent.

Scenario-grain tests (two clients, one in-process server) are P06's, in
`lpa-cloud-client`.
