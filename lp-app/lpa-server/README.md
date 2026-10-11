# lpa-server

The LightPlayer application server layer.

This crate hosts one or more core engines behind the `lpc-wire` API and handles
project management, request routing, and server-side integration points.

Used by apps and firmware to provide LightPlayer server functionality. All
communications are abstracted: serial, websocket, HTTP, or other concrete
transports are supplied by the embedding app.

`no_std`, designed for embedding.

## The fs batch (a push is one transaction)

Every fs request is stateless (`file_sync`'s pull-model rule) with one
exception: a **batch** (`FsRequest::BeginBatch` … `CommitBatch`), the
server's one piece of fs state between requests (`batch_state.rs`; ADR
`docs/adr/2026-10-10-fs-push-boundary-and-deflated-writes.md`).

- At most one is open, owned by the link that began it. `BeginBatch` answers
  `atomic` from `LpFs::batches_are_atomic`; on a backend without
  transactions (littlefs, memory, the host fs) it opens nothing and answers
  `atomic: false`.
- While it is open, another link's fs mutations (writes, deletes, batch
  verbs) are refused ("batch busy"); reads and everything off the fs wire go
  on. The server's own writes (a project's saves, the startup choice) join
  the batch.
- It ends on commit (the only ending that lands it), abort, the owner link
  closing (`ServerTransport::take_closed_links`) or resetting
  (`take_reset_links`, which the USB, UART and radio transports report), a
  second `BeginBatch` from the owner, or 60 s with no request from the owner,
  counted on the frame clock with handling time excluded (the tick after a
  long `LoadProject` is not idle time). After the timeout the owner's
  mutations are refused, naming why, until it begins or aborts again.
- `WriteChunkDeflated` is stateless: its length is checked first, then
  `LpFs::write_deflated_chunk` checks the offset, inflates and writes
  (the tree store keeps the deflated bytes).

Host tests: `tests/batch_state.rs`, `tests/push_boundary.rs` (a real client
conversation, cut sweeps), `tests/lp_fs_wrapper_conformance.rs`.

## Access

Every request is classified (`access_gate.rs`) and answered `NotPermitted`
when its link's tier does not cover it. A link's trust comes from its
transport (`lpc_shared::transport::LinkTrust`):

- **Trusted** (USB, the host process): edit, always.
- **Untrusted** (BLE): nothing until an HMAC login (`LoginBegin` /
  `LoginAnswer`), else what the device is `open` to
  (nobody, play or edit; the higher of the two wins).
- **Keyed** (a secure lp-link network link): the tier of the access entry
  its handshake matched. The transport reports the handshake
  (`ServerTransport::take_secure_events`); the server answers each key lookup
  from the installed secrets (`AccessState::key_lookup`, the same base-fs
  read as a login), charges a wrong key to the device's backoff, grants on
  `Authenticated`, and the hello on that link reports the tier. The anonymous
  key grants nothing (`open` decides). On a keyed link `LoginBegin` returns
  the offers without registering a login (a typed-password client's salts),
  and `LoginAnswer` is refused (`LoginResult` refused, as with no challenge
  outstanding): the handshake is its only login, so a relay can never pass
  an HMAC login through. A transport with no secure links inherits empty
  defaults. Host end-to-end test: `tests/secure_link_access.rs` (with
  `tests/support/secure_link_transport.rs`, a device-side transport over a
  secure lp-link responder).

## Wi-Fi settings

`NetworkStatus`, `NetworkAdd`, `NetworkForget` and `NetworkSet` (edit
tier) read and change root `/.lp/network.json` through the base filesystem
(`network_store.rs`) and each answer `NetworkStatus` — the two switches
(`wifi`, `cloudRelay`), every saved network without its password, and the
station (an injectable probe on `LpServer`; unset, every image says
`unsupported`). A board keeps at most eight networks; adding a saved name
again changes its password in place. `NetworkScan` answers from a second
probe, `unsupported` when unset. The file is write-only on every link, like
the access files (`lpc_access::is_write_only_file_path`). A board holding
its files for the C6 layout change refuses add, forget and set.
Host tests: `tests/network_requests.rs`, `tests/access_gate.rs`,
`tests/access_file_resource.rs`.