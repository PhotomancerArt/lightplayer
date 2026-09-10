# ADR: Studio's device stack over a virtual serial port

- **Status:** Accepted
- **Date:** 2026-09-09
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None
- **Amends:** [2026-08-03-studio-runs-n-device-sessions.md](2026-08-03-studio-runs-n-device-sessions.md)
  (its M9 item: what a device session is attached to can be an emulated board,
  and the pool does not learn a new kind) and
  [2026-06-22-browser-esp32-device-controller.md](2026-06-22-browser-esp32-device-controller.md)
  (the controller's boundary is the seam the shim installs under, unchanged)

Not to be confused with [2026-06-18-browser-serial-shim.md](2026-06-18-browser-serial-shim.md),
which is superseded and names the Rust-JS boundary. This ADR is about a virtual
*port*, not a boundary.

## Context

Studio's browser device stack — the chooser, the grant model, the readiness
engine, `esptool-js` flashing, the hotplug edges — could only be exercised with
an ESP32 board on a USB cable and a person to plug it in. That has cost the
project real time: nine of the ten committed device scenarios
(`scripts/device-scenarios/`) carry a `manual:` list of browser steps a human
performs, the JS layer carried a debt file
(`docs/debt/web-serial-js-untestable.md`) from 2026-07-10 saying it could not be
tested at all, and two registry defects live in that layer with coverage "none".

The ESP32-C6 emulator (`lp-emu/`) runs the shipped firmware, and plan one gave
it a host-side seam: two sockets per link, bytes on one and a line-oriented
control channel on the other, with `attach`/`detach`/`open`/`close`/`reset`
spelled the way the fake device already spelled them. A browser cannot open a
TCP socket, so plan two put a WebSocket door in front of it
(`lp-cli emu serve`) — and then asked the question this ADR answers: **at which
layer does Studio meet an emulated board?**

Three layers were available:

1. **A new Link provider** (`ByteStreamLink<EmulatorByteStream>`, a third
   provider kind). Studio would know the board is emulated and dress it
   honestly.
2. **A `navigator.serial` polyfill** — a virtual serial port, installed in the
   page, that Studio's existing `browser-serial-esp32` provider drives without
   knowing.
3. A protocol-level fake, which the repo already has
   (`providers/fake_device/`) and which never executes one instruction of the
   product image.

Level 1 is the sibling effort's **mode A**, and it has shipped: an emulated
device the *user* sees, honest dress and all. It answers a different question —
"can a user try LightPlayer with no hardware?" — and it deliberately does not
exercise the browser device stack, because it replaces it.

## Decision

**Studio's device stack runs unchanged over a virtual serial port.** Under
`?emu=<url>` the page replaces `navigator.serial` with a bus whose ports are
emulated boards; every layer above it — `browser_serial.js`,
`browser_esp32_device_controller.js`, `browser_esp32_flash.js`, the Rust
provider, the session pool, the readiness engine — is the shipped code, running
the shipped path, and cannot tell.

Five rules follow, and they are the decision.

### 1. The Studio JS/Rust device layer does not change to accommodate it

`lp-app/lpa-link/src/providers/browser_serial_esp32/**` (the Rust provider,
`browser_serial.js`, `browser_esp32_flash.js`) and
`lp-app/lpa-studio-web/public/lpa-link/browser_esp32_device_controller.js` are
**frozen against this work**. A change there would mean the claim ("the same
code runs") had quietly become "nearly the same code runs", and the value of
every gate under it would drop to nothing.

This is mechanical, not a promise: `just lint-browser-serial-js-frozen` pins the
three JS files by content hash and runs in CI. A milestone that needs one of
them to change has found something the premise did not survive, and its job at
that moment is to stop and report, not to make the change small.

The rule outlives the plan that produced it, which is why it is in an ADR.

### 2. The polyfill implements exactly eleven calls, and reproduces two scars

The entire contract the Studio layer has with Web Serial is
`navigator.serial.{requestPort, getPorts, addEventListener}` and `port.{open,
close, readable, writable, setSignals, getSignals, getInfo, forget}`.
`lp-app/lpa-studio-web/public/lpa-link/virtual_serial.js` implements those and
nothing else: more of the spec would be untested surface pretending to be a
browser.

Two behaviours of the real API are reproduced deliberately, each because Studio
carries a defect scar from it:

- **A re-enumerating device mints a NEW `SerialPort` object.** A USB-Serial-JTAG
  chip resetting looks exactly like a replug, and it is what the emulator does
  on `reset` / `download-mode`. A polyfill with one immortal port object would
  be *less* faithful than a board, and would hide the class of defect
  `adoptReenumeratedPorts` exists for (2026-08-31: a replugged C6 wallpapered
  the gallery).
- **A closed port is still a granted port.** `close()` releases the streams;
  only `forget()` revokes the grant (2026-07-22: deleting the grant handle on
  close broke flashing).

`getInfo()` reports **303a:1001** — Espressif native USB. An emulated C6 *is*
native USB, so this is not a disguise: the existing vid:pid paths (grant-aware
picking, a board's `usb_bridge`, re-enumeration pairing, `labelForPort`'s label)
run unchanged because the answer is true.

### 3. What it deliberately does NOT model: Chromium's own USB stack

The polyfill is a virtual *port*, not a virtual *browser*. Everything below is
out of scope by decision, and stays on hardware:

- **Loss reporting.** Chromium takes minutes to report some kinds of device
  loss; the shim's byte channel closes when it closes.
- **Grant revocation on reload**, and Brave's differences from Chrome's.
- **The real chooser** — its permission prompt, its "no devices found" state,
  its interaction with enterprise policy (`SerialAllowAllPortsForUrls`).
- **The USB layer's own timing**: enumeration delay, bus resets, hub behaviour.
  No assertion anywhere in this work is about a duration.

A shim walk is therefore **not** a substitute for a hardware walk of those
things. It is a substitute for everything above the port, which is where the
product's own logic lives.

A quieter consequence worth writing down: a polyfilled `navigator.serial`
**grants itself**. The Chromium policy profile, the standing WebSerial grant
(`just serial-grant`) and the reserved bench port block exist to remove the
chooser for an agent driving real hardware. Under the shim none of them are
needed, and none of them should be wired back in.

### 4. `EmulatorPort` is the shared object, and it has two backings

`lp-app/lpa-studio-web/public/lpa-link/emulator_port.js` is one emulated board
as an in-page object — bytes in/out, the control vocabulary as one method per
verb, flash get/put, reset, snapshot, probes. It is deliberately **not** a Web
Serial thing: the polyfill turns it into a `SerialPort`, and a `ByteStreamLink`
could turn the same object into mode A's device card, without either side
learning about the other. Its shape is the sibling effort's eight-point contract
(`_archive/2026-09-07-0118-studio-emulated-boards/notes.md` section 4); changing
that shape later is the sibling's call, not a breach of this ADR.

This work ships the **native** backing only — `lp-cli emu serve`'s WebSocket
door (`GET /boards`, `ws /board/<id>/bytes`, `ws /board/<id>/control`). Against
the eight points, as shipped:

| point | native backing |
|---|---|
| 3 — bytes both ways | `onBytes` / `write`, over the byte socket |
| 4 — control channel as methods | `attach`, `detach`, `open`, `close`, `signals`, `reset`, `downloadMode`, `state`, `pins`, `command` |
| 5 — flash `get()`/`put()`, snapshot | **refused**: `getFlash`, `putFlash`, `snapshot` reject with `NotSupportedError`. The door persists flash server-side (`--state-dir`) and reports a state word in `GET /boards`, but exposes no route for the bytes. `flashState()` answers what the registry said, and nothing pretends. |
| 6 — probe getters | **refused**: the door answers `state` and `pins`; a heap ledger is the wasm backing's |
| 1, 2, 7, 8 | wasm-build concerns (a byte-driven builder, `Instant` out of `run_until`, feature-gated codegen, deterministic default) — they are about a machine in the page, and the native backing has none |

**Where a backing cannot answer, it says so.** A lie inside the contract is
worse than a gap, and the two refusals above are the shape the wasm backing will
fill.

### 5. Dev mode is a URL flag, and the page is honest about it

`?emu=<ws url>` is read by an inline script in `index.html`, before the wasm
bundle boots. **Absent the flag the polyfill module is never fetched** — no
network request for it at all, which makes "this is a dev-mode capability"
checkable in the network panel rather than a claim.

The flag installs a **synchronous facade**: `navigator.serial` is defined
synchronously, and its methods await the dynamic `import()` behind it. A dynamic
import cannot promise to resolve before the bundle boots, and
`installSerialEvents` installs its hotplug listeners at most once per page, so
the object they land on has to be there from the first tick and has to stay. The
alternative — park the promise and have the Rust side await it, the way
`resolved_engine_urls()` awaits `__lpEngineAssets` — was rejected because it
puts the shim's existence on Studio's boot path, and Studio not knowing is the
whole premise.

Two consequences of the page owning the seam:

- **The picker is page chrome.** Under the shim `requestPort()` has no browser
  chooser to show, so the page draws one at the same seam, and
  `browser_esp32_device_controller.js:41` — the single call site — is unchanged.
  Studio's own Devices picker means something else (the device roster); giving
  it a second meaning would be exactly the change rule 1 forbids.
- **Honesty is page-level.** A device card under `?emu=` is deliberately
  indistinguishable from a real board's — that is the claim being tested — so
  the page carries a dev banner naming the shim and the backing URL. A
  card-level dress would be a behaviour difference, and mode A already owns
  honest dress. (Reversible: it is OQ2, and G1 asks Yona directly.)

`?emu=` and `?on=` (which device this tab is looking at) are **orthogonal
axes**. A page may carry both, and neither reads the other.

## Consequences

- A Studio device walk needs a browser and nothing else. What was a `manual:`
  list of steps for a human at a desk becomes something an agent can drive.
- The `web-serial-js-untestable` debt has a harness: the real-Chrome conformance
  suite (`lp-app/lpa-link/tests/browser_serial_conformance.rs`) runs the shipped
  JS over the polyfill over a scripted door, hermetically, in CI — and the same
  assertions run against a live `emu serve`.
- The three frozen JS files are now load-bearing for a CI lint. Legitimate
  future work on them has to move the hash deliberately, with the reason in the
  commit — which is the intent.
- Anything Chromium's USB stack owns is now *explicitly* hardware-only rather
  than accidentally untested. The list is in rule 3, and it is the honest
  residue of the debt file.
- Studio gained no code for any of this. The install seam is in `index.html` and
  `public/lpa-link/`; `lp-app/lpa-studio-web/src/` has no line that knows the
  shim exists.

## Alternatives Considered

- **A third Link provider (mode A's shape) for dev too.** Rejected for this
  purpose: it replaces the browser device stack instead of exercising it, so
  every defect that lives in `browser_serial.js` or the readiness engine would
  stay invisible. Mode A ships beside this one and answers the user-facing
  question.
- **Level 3 — an `esptool-js`-level fake.** Would test the flash flow and
  nothing under it, and would need its own protocol model that could drift from
  the ROM's. The emulator already runs the real ROM.
- **Installing the polyfill from Rust, on the boot path.** Cheaper to write, and
  it makes the install visible to Studio — precisely what the design is trying
  to avoid. See rule 5.
- **Making the emulated card visibly emulated (a card-level mark).** Deferred to
  G1 as OQ2, with the page banner as the current answer. Under mode B the card
  being indistinguishable is not a UX oversight — it is the assertion.

## Follow-ups

- **Amend when flashing is real** (plan two M5): what `esptool-js` needed from
  the ROM download console and the flasher stub, and whether the "runs
  unchanged" claim survived a write path.
- **The wasm backing** of `EmulatorPort` (the sibling's mode A) fills points 5
  and 6 above; when it lands, this table stops having "refused" rows for the
  browser case.
- **The golden-trace question** (plan two OQ3, G2): whether an emulator-captured
  trace is a fixture of the same standing as a silicon one.
