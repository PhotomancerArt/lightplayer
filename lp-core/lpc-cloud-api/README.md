# lpc-cloud-api

Client↔cloud-service message vocabulary for LightPlayer's project sync.

This crate is the entire request/response/error vocabulary a client and the
cloud sync service exchange, as pure Rust types: `CloudRequest`,
`CloudResponse`, `CloudError`, the `CloudCall`/`CloudReply` envelope, and the
supporting building blocks (`Visibility`, `Actor`, `ProjectMeta`,
`HeadInfo`/`PushOutcome`, `SidecarMeta`). It carries no transport, no IO, and
no logic beyond the version-refusal helper in `version.rs`. The blob
*transfer* encoding is out of scope entirely — blobs move over a separate
plain-HTTP plane — this crate only carries the hashes (`HaveBlobs`,
`MissingBlobs`) that plane is keyed by.

`no_std` + `alloc`. Depends on `lpc-history` for `PrefixedUid` (uids and the
`Actor::User` identity) and `ContentHash` (blob/tree hashes), and for
`HistoryEvent`, which a `PushCommit` carries verbatim.

## Every message is a struct; the pairing is a compile-time fact

Each of the eleven requests is a struct in `request.rs` (`GetProject { uid }`,
`PushCommit { .. }`, and the two payload-free `WhoAmI` / `ListMyProjects`);
each response is a struct in `response.rs` (`ProjectInfo`, `Heads`,
`MissingBlobs`, …). `CloudRequest` and `CloudResponse` are the closed sets of
them — a unit variant where the message carries nothing, a newtype variant
wrapping the struct otherwise. The per-message structs stay behind
`request::` / `response::` rather than being re-exported at the crate root:
`Events` and `Heads` only read unambiguously with their module in front.

`CloudCallSpec` (in `call_spec.rs`) is the pairing table — one hand-written
impl per request naming its `Response` and how to `extract` it. Eleven impls
in one greppable file, deliberately not a macro. It is what lets a client
write `call(port, GetProject { uid })` and get a `ProjectInfo` back, and what
lets the service's handlers return the concrete response type; the "what if
the answer is the wrong variant" branch then exists once per request instead
of at every call site.

**This restructuring did not move a byte on the wire.** Serde's external
tagging writes a newtype variant exactly as it wrote the struct variant —
`{"getProject":{"uid":"prj…"}}` — and the enums' `rename_all = "camelCase"`
renames variants, never fields, so the message structs carry no `rename_all`
of their own and `next_since` stays snake_case. The pinned JSON literal tests
in `request.rs` and `response.rs` are the check.

## fw-graph-clean

Nothing in `lp-fw` may ever depend on this crate. Cloud sync is a client/
service concern; firmware talks the device wire protocol (`lpc-wire`) and
has no business speaking to the cloud service directly. If a firmware crate
ever needs to reach for a type here, that is a sign the type belongs
somewhere lower (`lpc-history`) or the firmware-side need should go through
the client instead.

## Version-and-refuse policy

Every `CloudCall` and `CloudReply` carries `version: u32`, checked against
this crate's `CLOUD_API_VERSION`. This is deliberately **not** `lpc-wire`'s
no-compat policy (see the wire/protocol compatibility section of the repo's
`AGENTS.md`): the wire protocol has no long-lived peers because firmware,
server, and client deploy together, so old wire forms are simply deleted.
The cloud API has no such lockstep guarantee — a browser tab can sit open
across a service redeploy — so a version mismatch must be a named,
first-class refusal (`CloudError::VersionMismatch { client, server }`) that
both sides can detect and report, never a silent decode failure, a field
alias, or a best-effort partial-compat decode. `version::check_version` is
the one place that decision is made; both client and server call it before
trusting a call or reply body.
