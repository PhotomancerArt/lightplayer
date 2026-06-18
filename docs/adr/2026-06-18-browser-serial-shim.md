# ADR 2026-06-18: Browser Serial Shim

## Status

Accepted.

## Context

LightPlayer Studio needs a static web path that can connect to already-flashed
ESP32 hardware over Web Serial. The rest of the Studio stack should stay in
Rust: `lp-studio-core` owns actions/state, `lp-studio-runtime` owns protocol
flow, and `lpa-link` models device/link/session concepts.

The browser Web Serial API is available to JavaScript, but the `web-sys`
bindings for `Serial` and `SerialPort` are currently gated behind
`web_sys_unstable_apis`. Requiring that cfg for normal Studio wasm builds would
make the build/deploy path more fragile and would leak a browser-platform detail
into unrelated Rust validation.

## Decision

Use a tiny JavaScript shim for direct Web Serial stream ownership.

- `lp-app/lp-studio-web/public/browser-serial.js` owns
  `navigator.serial.requestPort()`, `SerialPort.open()`, stream readers,
  stream writers, line buffering, and close/cancel behavior.
- The shim installs a narrow global function surface before the Rust wasm module
  starts.
- `lp-studio-runtime` calls that function surface through
  `browser_serial_shim.rs`.
- Rust still owns Studio actions/effects/events, endpoint/session modeling,
  `M!` protocol framing, JSON request/response parsing, server-event handling,
  diagnostics, and demo project upload semantics.
- `lpa-link` models `browser-serial-esp32` as a provider/session/connection
  kind, but it does not own browser stream objects.

## Consequences

Studio can build with ordinary wasm settings while still using Web Serial in
supported browsers.

The boundary is intentionally narrow and replaceable. If stable `web-sys`
bindings become practical later, the shim can be collapsed into Rust without
changing the Studio action model or the `browser-serial-esp32` provider
vocabulary.

The cost is one small JavaScript file in the static web shell. Browser stream
edge cases such as reader cancellation, disconnects, and permission errors must
be handled and tested at that boundary.
