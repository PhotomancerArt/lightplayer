# ADR 2026-06-21: LPA Link Provider-Owned Resources

## Status

Accepted

## Context

`lpa-link` had a split ownership model. It exposed shared endpoint records, but
each provider returned a provider-specific session type, while browser serial
and browser worker resource ownership mostly lived in the old Studio runtime
crate and `lpa-studio-web` glue.

That made the boundary hard to reason about: the link provider looked like the
owner of endpoint/session identity, but Web Serial ports, worker lifecycles,
ESP32 probe/flash, and browser-side JavaScript were owned elsewhere.

## Decision

`lpa-link` providers own their concrete resources. `LinkEndpoint` remains a
provider-neutral endpoint snapshot. `LinkSession` is now a provider-neutral
session snapshot/handle. Provider-private endpoint/session state owns concrete
resources, and public provider operations accept endpoint/session ids.

Browser provider JavaScript that implements provider mechanics is owned by
`lpa-link`:

- `browser_worker` owns the worker wrapper and worker lifecycle.
- `browser_serial_esp32` owns Web Serial request/open/release/close.
- `browser_serial_esp32` owns ESP32 bootloader probe and firmware flash
  bindings.

Application-owned browser artifacts are passed to provider constructors as
same-origin paths, not as remote URLs or a general locator model.

`lpa-studio-ux` owns the active Studio controller layer above `lpa-link`. It may
adapt connected link sessions into `lpa-client`, but it does not own provider
resource identity or lifecycle.

## Consequences

- Provider behavior is discoverable in one crate.
- Browser serial endpoint-to-port state is provider-private.
- Browser worker lifecycle is provider-private.
- `lpa-client` continues to own request ids, response correlation, protocol
  errors, heartbeat/log events, and project deploy helpers.
- Web apps must provide same-origin sidecar paths for generated assets such as
  `fw_browser.js`, `fw_browser_bg.wasm`, firmware manifests, and esptool modules.
- Future providers should follow the same pattern: shared endpoint/session
  records, provider-private concrete state, and provider methods by id.
