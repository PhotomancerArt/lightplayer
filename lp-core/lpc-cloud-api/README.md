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
