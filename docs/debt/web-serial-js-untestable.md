---
status: retired
since: 2026-07-10      # the JS controller predates the link rewire
logged: 2026-07-23
area: lpa-link/browser-serial
related:
  [
    "../defects/2026-07-22-flash-session-map-deleted.md",
    "../defects/2026-07-16-browser-serial-endpoint-lost.md",
    "../adr/2026-09-09-studio-device-stack-over-a-virtual-serial-port.md",
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
honest coverage line was "none"). Every change to the manage/flash flow
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
- 2026-09-09 — **the harness landed** (emulator plan two, M2 → PR #642;
  ADR `../adr/2026-09-09-studio-device-stack-over-a-virtual-serial-port.md`).
  A `navigator.serial` polyfill (`lp-app/lpa-studio-web/public/lpa-link/virtual_serial.js`
  + `emulator_port.js`) puts a scriptable virtual serial port under the
  shipped JS, and `lp-app/lpa-link/tests/browser_serial_conformance.rs`
  drives that JS — the real files, one instance, served from Studio's
  own static root — through it. The suite runs hermetically in CI (the
  path-gated `validate-browser` job's "Browser wasm tests" step,
  `just lpa-link-browser-test`) and against a live `lp-cli emu serve`
  by hand (`just lpa-link-browser-test-live`). It is 15 `wasm_bindgen_test`s
  as of `2be6b6235` (14 at M3; M5 renamed the reboot test and added
  `a_chip_reset_does_not_re_enumerate` when it changed the re-enumeration
  model). The two related registry defects now have coverage: the
  flash-bridge/close-vs-release path that broke
  `flash-session-map-deleted` is pinned by
  `a_closed_port_keeps_its_session_and_a_forgotten_one_does_not` and
  `the_flash_bridge_acquires_the_live_generation` — its "Regression
  coverage: None" line was corrected to name them; `endpoint-lost`
  already carried registry/controller coverage and the read-pump reopen
  is now also pinned by
  `the_read_pump_reports_a_lost_device_and_the_port_reopens`.
- 2026-09-09 — **a residual harness flake, recorded not filed** (M3, DD24).
  `wasm-bindgen-test-runner` serves the suite's JS from a `tiny-http` server
  that can wedge under concurrent dynamic `import()`s as the served JS grows;
  the driver is SIGKILLed after its timeout and the failure reads as *"Failed
  to detect test as having been run. It might have timed out."* It is
  size-sensitive, not code-sensitive (main's own JS padded to M3's byte length
  hung 1/8). The fix in place: `lp-app/lpa-link/tests/js/conformance_support.js`
  `load()` imports its five modules serially, not through `Promise.all`; a
  sixth module means keeping it serial (the comment at `load()` says so). No
  new debt entry — a third-party harness behaviour with a working mitigation is
  below `README.md`'s filing bar.
- 2026-09-09 — **the flash bridge proved out through the shim** (M5 →
  PR #653). Studio's own **Flash firmware** verb, through the frozen
  `browser_esp32_flash.js`, through esptool-js 0.6.0, wrote the packaged
  `esp32c6-4mb` image into an emulated board that then booted it ROM-up
  and said hello — so the flash bridge's port acquisition, the chip
  guard and the post-write reset dance now execute in a test rather than
  only on hardware. The "stage-2 esptool simulator" the exit criteria
  hoped for is bettered: the target is the real mask ROM download
  console, not a fake `SerialPort` under esptool-js.

**Exit criteria** — A harness that executes the JS layer against a
scripted `SerialPort` double (fake `navigator.serial` in the existing
wasm browser-test suite, or a Node harness importing the modules),
covering: session-id stability across open/close/re-enumerate,
close-vs-release semantics, read-pump error paths, and the flash
bridge's port acquisition. The 2026-07-14 link-architecture notes'
"stage-2 esptool simulator" front-end (fake SerialPort under
esptool-js) is the natural vehicle if that work lands.

**Exit-criteria walk (2026-09-10, plan-two M7).** Every named coverage
is met by a test in `lp-app/lpa-link/tests/browser_serial_conformance.rs`.
Verdict: **retired.** The entry stays in place; the log is the history
(`README.md`).

| exit criterion | met by |
|---|---|
| session-id stability across open/close | `session_ids_are_stable_across_open_and_close` |
| … across re-enumerate | `session_ids_are_stable_across_a_re_enumeration`, `a_reboot_behind_our_back_is_noticed_and_does_not_re_enumerate`, `a_chip_reset_does_not_re_enumerate` |
| close-vs-release semantics | `a_closed_port_keeps_its_session_and_a_forgotten_one_does_not`, `openness_is_readable_or_writable` |
| read-pump error paths | `the_read_pump_reports_a_lost_device_and_the_port_reopens` |
| the flash bridge's port acquisition | `the_flash_bridge_acquires_the_live_generation`; and end to end through the real flow, M5 (PR #653) |
| the "stage-2 esptool simulator" hope | bettered by M5 — esptool-js against the emulator's real ROM download console, not a fake |

**The residue, named so "retired" is not over-read.** This entry was
about the *JS layer's own contracts* being untested, and those are now
tested. Two things it touched on are **not** closed, and were never in
these exit criteria — they are the honest hardware-only residue, chosen
rather than missed:

1. **Chromium's own USB stack stays untested, and now deliberately so.**
   The polyfill is a virtual *port*, not a virtual *browser*: minutes-long
   device-loss reporting, Brave's grant revocation on reload, the real
   chooser and its permission prompt, and the USB layer's own timing are
   out of scope by design. The list, and the reasoning, is rule 3 of the
   ADR (`../adr/2026-09-09-studio-device-stack-over-a-virtual-serial-port.md`).
   A shim walk is not a substitute for a hardware walk of those things;
   it is a substitute for everything above the port, which is where the
   product's own logic lives.
2. **The CI half runs in one browser (Firefox).** `wasm-bindgen-test-runner`
   picks geckodriver on the runner, where there is no Web Serial at all,
   so the polyfill is the whole `navigator.serial` and the JS-layer
   criteria are genuinely proven. The Chrome-specific fact (that
   `Object.defineProperty(navigator, "serial", …)` shadows Chromium's
   prototype getter and is removable) was proven in real Chrome by hand,
   with output quoted, and the live half runs in Chrome. Per Yona's E1
   ruling (2026-09-10) CI stays browser-agnostic; the suite is documented
   as runnable locally in Chrome and Brave (`AGENTS.md`, "Running the
   conformance suite in Chrome or Brave"), so a Chrome-only regression can
   be reproduced on a desk rather than only on CI.
