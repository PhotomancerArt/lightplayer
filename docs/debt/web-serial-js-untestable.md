---
status: carried
since: 2026-07-10      # the JS controller predates the link rewire
logged: 2026-07-23
area: lpa-link/browser-serial
related:
  [
    "../defects/2026-07-22-flash-session-map-deleted.md",
    "../defects/2026-07-16-browser-serial-endpoint-lost.md",
    "chip task_c23330f9 (LpFs conformance suite — the sibling gap on the fs seam)",
  ]
---
# The Web Serial JS layer ships untested

**Shape** — The browser serial stack's JS half — the module-scoped
session map (`browser_serial.js`), the device controller
(`browser_esp32_device_controller.js`: open/reset/read-pump/DTR-RTS),
and the esptool flash bridge — has no test harness of any kind. Web
Serial itself needs a real user gesture and a real device, so neither
unit tests, the fake-device e2e (which swaps in a Rust fake provider),
nor story capture ever execute this code. Every contract between the
Rust provider and the JS layer (session-id stability, close-vs-release
semantics, grant-handle retention) is enforced by nothing but reading.

**Carrying cost** — Bugs in this layer ship silently and surface only
on physical hardware walks: two registry defects live here
(endpoint-lost 2026-07-16; flash-session-map-deleted 2026-07-22, whose
honest coverage line is "none"). Every change to the manage/flash flow
requires a human with a board to verify; agents cannot close the loop.

**Workarounds** —
- Treat any change touching `browser_serial.js` /
  `browser_esp32_device_controller.js` as hardware-gated: flag it for
  a Yona walk explicitly.
- Keep the JS layer as thin as possible; push logic into the Rust
  provider where the fake-device e2e can reach it.
- The comment discipline in `closePort` (why the entry stays) is the
  current substitute for a pinning test.

**Incident log**
- 2026-07-16 — endpoint-lost defect: Rust-side ownership bug, but the
  JS seam's semantics (module-scoped survival) were part of the
  confusion.
- 2026-07-22 — flash-session-map-deleted: `closePort` deleting the
  grant-holding entry broke flashing; caught only on hardware;
  regression coverage impossible ("none" in the entry).
- 2026-07-22 — the fix's verification required a full manual
  flash/name/push walk.

**Exit criteria** — A harness that executes the JS layer against a
scripted `SerialPort` double (fake `navigator.serial` in the existing
wasm browser-test suite, or a Node harness importing the modules),
covering: session-id stability across open/close/re-enumerate,
close-vs-release semantics, read-pump error paths, and the flash
bridge's port acquisition. The 2026-07-14 link-architecture notes'
"stage-2 esptool simulator" front-end (fake SerialPort under
esptool-js) is the natural vehicle if that work lands.
