# ADR 2026-06-22: Browser ESP32 Device Controller Boundary

## Status

Accepted

## Context

Studio's browser ESP32 path needs to do more than open a serial transport. The
same user workflow may need to select a Web Serial port, open the normal
LightPlayer JSON-lines protocol, reset the device, stream raw boot logs, detect
blank flash or ROM downloader mode, flash firmware through `esptool-js`, wipe
the device, and reconnect afterward.

Earlier code split normal protocol attach into separate browser operations:
open for reset, close, reopen for protocol, then wait for readiness in Rust.
That made boot output easy to miss, made reset signal failures look like
connection failures, and forced the standalone serial debug page to duplicate
Web Serial lifecycle code.

Web Serial lifecycle details are browser-native: `SerialPort` ownership,
reader/writer locks, `setSignals`, close races, and `esptool-js` handoff are
all JavaScript/browser concerns. `lpa-link` still needs to own the provider
semantics, while Studio UX should only see link capabilities, logs, progress,
and connection state.

## Decision

Use a browser-side `BrowserEsp32DeviceController` as the durable owner of the
Web Serial ESP32 device session.

The controller owns:

- the selected `SerialPort`;
- reader and writer locks;
- raw serial log pumping;
- best-effort normal reset signaling;
- explicit debug reset sequences;
- line buffering for the LightPlayer protocol adapter;
- safe close/release behavior;
- event/log/progress emission for browser consumers.

The controller is served by the Studio web app at:

```text
/lpa-link/browser_esp32_device_controller.js
```

The wasm-bound `browser-serial-esp32` provider adapter imports that module and
maps it into `lpa-link` sessions, logs, and protocol I/O. The standalone
`serial-debug.html` page imports the same module directly, so hardware debug
flows exercise the same Web Serial lifecycle as Studio.

Normal protocol attach now opens the port once, starts reading immediately, and
then attempts normal reset while serial output is already being captured. Reset
signal failures are diagnostic, not the source of connection truth. Readiness
is determined from raw serial output and LightPlayer protocol frames above this
controller boundary.

`esptool-js` remains the implementation mechanism for bootloader operations.
Provider management operations release normal protocol ownership before
flash/wipe takes bootloader ownership, then reconnect through the same
controller-backed protocol attach path afterward.

## Consequences

There is one browser-native place to fix Web Serial lock handling, reset timing,
close races, raw log capture, and small retry policy.

Studio UX remains independent of Web Serial details. It consumes link/provider
capabilities, logs, progress, and readiness results rather than owning browser
ports or signal sequences.

The debug page is more trustworthy because its normal connect/reset/read path
uses the same primitive as the app.

Apps using the browser serial ESP32 provider must serve the controller module at
the expected same-origin path, or configure an equivalent packaging strategy in
a later provider option.

The current controller is intentionally browser/client-side. It is not a new
client/server protocol boundary and is not meant to be serialized beyond logs
or textual agent/debug views.
