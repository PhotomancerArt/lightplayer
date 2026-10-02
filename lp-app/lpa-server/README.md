# lpa-server

The LightPlayer application server layer.

This crate hosts one or more core engines behind the `lpc-wire` API and handles
project management, request routing, and server-side integration points.

Used by apps and firmware to provide LightPlayer server functionality. All
communications are abstracted: serial, websocket, HTTP, or other concrete
transports are supplied by the embedding app.

`no_std`, designed for embedding.

## Access

Every request is classified (`access_gate.rs`) and answered `NotPermitted`
when its link's tier does not cover it. A link's trust comes from its
transport (`lpc_shared::transport::LinkTrust`):

- **Trusted** (USB, the host process): edit, always.
- **Untrusted** (BLE): nothing until an HMAC login (`LoginBegin` /
  `LoginAnswer`), else play when the device is `open`.
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