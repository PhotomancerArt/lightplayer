# ADR: The C6 emulator runs in the tab: one Worker, two doors

- **Status:** Accepted
- **Date:** 2026-09-10
- **Deciders:** Photomancer
- **Plan:** lp2025/2026-09-10-1707-c6-emulator-in-tab (P1 #685, P2 #687, P3
  #709, P4 #714 — this diff carries all four until they merge)
- **Amends:** [2026-09-09-studio-device-stack-over-a-virtual-serial-port.md](2026-09-09-studio-device-stack-over-a-virtual-serial-port.md)
  (its two "refused" rows close for the wasm backing) and
  [2026-09-07-always-a-device-target-real-emu-sim.md](2026-09-07-always-a-device-target-real-emu-sim.md)
  (D43's last clause and D44 replaced by D1 below) — the reciprocal, dated
  amendments live in those files
- **Supersedes:** None

## Context

Studio's device model shipped with an emulator-shaped hole in it
(`vision.md` of this plan, grounded on `main` `20f5c152bea`… `20f6b5124`):
`EMULATED_TARGETS` was `&[]`, so every C6 project resolved to a sim, and the
only emulator Studio could reach at all was a **native process** behind a
WebSocket (`lp-cli emu serve`) driven through a `navigator.serial` polyfill —
mode B, for agents. A user with no board and no terminal had no emulated
board, and `EmulatorPort`'s header already declared a second backing whose
four methods (`getFlash`, `putFlash`, `snapshot`, `probes`) simply rejected,
because the native door has no route for them
(2026-09-09-studio-device-stack-over-a-virtual-serial-port.md rule 4).

This plan fills that hole: the real ESP32-C6 firmware now boots inside a
Studio tab, flashed and reaching Ready with no board on the desk, as a device
a user picks beside the sim (**mode A**) and as the server-less form of the
emulator-first walk (**mode B**, `?emu=tab`). One dedicated Web Worker is the
wasm backing of `EmulatorPort`; a `ByteStreamLink` and the `navigator.serial`
polyfill are its two consumers.

## Decision

### 1. One dedicated Worker is the wasm backing of `EmulatorPort`; the Link and the polyfill are its two consumers (D2, D16)

`lp-app/lpa-studio-web/public/lpa-link/emulator_worker.js` holds one instance
of the `lp-emu-esp32c6` wasip1 module in a static slot and answers everything
the native door's two WebSockets answer, over `postMessage` instead of a
socket. `emulator_port.js` is **not forked**: `TabEmulatorPort extends
EmulatorPort` and overrides only the four methods the native backing had to
refuse (`getFlash`, `putFlash`, `probes`, `flashState`), leaving `snapshot()`
refused on both backings — there is no bytes format for a `Snapshot` (its own
module says in-memory only, on purpose), and a lie inside the contract is
worse than a gap.

- **Mode A** (users): `lp-app/lpa-link/src/providers/emulator_tab/`
  (`EmulatorTabStream`, `EmulatorTabControl`) is a `ByteStreamLink` over the
  page's Worker, reached through `LinkProviderKind::EmulatorTab`
  (`lp-app/lpa-link/src/registry/kind.rs`). The page-side handoff is
  `lp-app/lpa-link/src/providers/emulator_tab/emulator_tab_bridge.js`, which
  imports `emulator_tab.js` behind a function
  (`tabModulePath()` → `import(tabModulePath())`) rather than a bare
  specifier — see Consequences.
- **Mode B** (agents): `emulator_tab.js`'s `navigator.serial` polyfill
  install under `?emu=tab`, unchanged in kind from plan two's native-backed
  polyfill — same `virtual_serial.js`, same eleven-call contract, a
  different backing underneath.

Neither consumer learns of the other; both are two thin readers of the same
in-page object (plan two's rule 4, restated for a second backing).

### 2. The Worker paces and never sprints; the machine never sees wall time; dilation is reported, not built around (D8, D13)

`emulator_worker.js`'s pacing loop runs `emu_run(budget_cycles)` slices with
no wall timeout inside them; the conversion from wall time to a cycle budget
happens once, on the host side of the wall. When the guest falls more than
one slice behind (a stall, a hidden tab), the loop **drops the deficit and
re-anchors to now** rather than handing the guest a catch-up sprint. The one
number this exists to produce is **dilation** (guest µs per wall µs), and G1
found it had been silently broken since the loop's first cut — the pacing
loop emptied the dilation window on every re-anchor, and a guest slower than
real time re-anchors on every slice, so `dilation` was `null` forever. Only a
reboot empties the window now.

Measured (G1/G2 handoffs, `lp-emu` at the plan's pins): boot-to-hello 400 ms
of guest time, 0.88 s of wall in node; dilation 0.45× for that boot and
0.34× for a blank chip spinning on `invalid header`; a hidden tab falls to
about 0.009×; a live desk tab in this build read 0.4×, moving to 0.7× while a
flash ran. `speed_word(dilation)` (`runtime_band.rs`) formats the number for
the band: one decimal at or above 0.1, two below it, and `None` drops the
clause rather than guess at a non-number. The two-decimal branch's exact
threshold is an **open G2 question**, carried in Follow-ups, not settled
here.

### 3. The wasip1 CLI module is the one artifact; `emu_*` exports sit beside `_start` (D3, D17)

`lp-emu/esp/lp-emu-esp32c6/src/tab_abi/` (`mod.rs`, `exports.rs`, `tests.rs`)
adds twenty-three `#[unsafe(no_mangle)] pub extern "C"` exports
(`emu_create`, `emu_run`, `emu_control`, `emu_usb_write` /
`emu_usb_read` / `emu_uart0_read`, the `emu_flash_*` family, `emu_destroy`,
…) to the same CLI binary the bench rig builds; `_start` is untouched and the
JS host never calls it. `scripts/emu/build-tab-wasm.sh` (`just emu-c6-wasm`)
holds the export list and **parses the built module's own export section**
after linking, failing loudly if a name is missing — the drift guard a
native test cannot see. `lp-emu/esp/README.md` §"The tab host: the `emu_*`
slice ABI" is the exports' normative home; this rule only restates that it
is one artifact, not a fork.

### 4. The flash image lives in worker-owned OPFS, outside the library store; Forget deletes it (D15)

`emulator_worker.js` persists each board's chip at `emu-flash/<key>.bin`
(`FLASH_DIR = "emu-flash"`) through a sync access handle, on a
`PERSIST_EVERY_MS = 2_000` dirty cadence (the native door's own
`FLUSH_EVERY` rule) and at power-off — never through the library's `LpFs`
mirror, which would multiply a 4 MiB image by every open tab. **This is
where the design does not yet hold**: `deleteFlash(key)` resolves
successfully and deletes nothing (`dir.removeEntry(...).catch(() => {})`
swallows the rejection). It is filed below as an open defect, not claimed
fixed.

### 5. Sim or emu is an explicit, sticky choice; no default flip (D1) — amending the AAD ADR's D43/D44

Every C6-capable project's Devices picker offers both rows, tagged `emu` /
`sim`, with **no modifier key** — `EMULATED_TARGETS = ["seeed/xiao-esp32-c6"]`
gated further by `emu_offered_for(board_id)` (`runtime_backing.rs`), which is
true only when the board is in that table **and** a served build exists for
it, so a row is never offered that would mint a record and then fail its
package fetch.

The record is one sidecar type with a kind: `RuntimeKind::{Sim, Emu}`
(`sim_record.rs`), serialized as the `kind` field it has always carried
(`"sim"` / `"emu"`) — **no format bump**, because an older reader treats
`"emu"` as a foreign kind and reports absence, which is the documented
posture. `CatalogOp::CreateRuntimeDevice` is the one atomic op for both
kinds. The controller's fork is `RuntimeSession::{Sim, Emu}` and
`PoweredRuntime { uid, kind }` (`studio_controller.rs`).

`?on=emu` / `?on=sim` resolve through `DeviceHint::Emu` / `::Sim`
(`device_hint.rs`) into `ResolvedHint::Resolve { prefer: Option<RuntimeKind>
}` (`web_app.rs`), which becomes `HomeOp::OpenPackage { key, prefer }` — a
**field** on the existing op, not a sibling op, because opening with a
preferred kind is the same gesture as opening with none, narrowed.
`resolve_open_device`'s rungs (`studio_controller.rs`) are D1 written as
code:

1. the record that **last ran this project**, of any kind, when the address
   asked for no kind or the kind it already is;
2. an idle record of the asked kind and target, else mint one;
3. no kind, no history: **exactly today's behaviour** — an idle sim of the
   target, else a fresh sim.

Rung 3 is the literal "no default flips": a fresh C6 project with a bare
address lands on a sim exactly as it did before this build could emulate
one. This **amends** the AAD ADR's D43 (dropping its last clause, "no
`?on=` → emu when a module exists") and **replaces D44 outright** (the
emu-is-default row and its `⌥` modifier are gone; the two rows themselves
are the choice). `?on=emu` on a target with no module or no served build
stays `Unbacked`, reworded to name the board (D23) — the hint grammar is
unchanged, only the resolver's answer widens.

### 6. Mode A flashes by direct write; mode B keeps the ROM path (D5/D24)

The mode-A Flash verb (`BrowserEmuLinkSource`, `runtime_backing.rs`,
`device_transport.rs`, `browser_emu_source.rs`) fetches the served package
at the same manifest offset esptool's own path uses — `0x0` for
`esp32c6-4mb`, hardcoded at `lp-cli/src/commands/firmware/package.rs:99` —
writes it directly through `emu_flash_write`, and pulses reset; Erase is
`emu_flash_erase_chip()` then reset. Users do not sit through a ROM
download. Mode B's polyfill is untouched: `esptool.js` still flashes the
tab-hosted board through the real mask ROM, because there the ROM path is
the thing under test. The born-flashed fetch is resolved against the **site
root**, not the page (`76d8acce9`, this stack) — the worker's own base URL is
`/lpa-link/emulator_worker.js`, which made a page-relative `./firmware`
resolve to a 404-shaped `index.html` before that fix.

### 7. `LinkTransport::Emu` is a wire fact (D14)

`runtime_session.rs`'s `LinkTransport` gains a third arm, `Emu`, documented
in-place as in-process but **not free** — the bytes cross an emulated
USB-Serial-JTAG FIFO the guest drains at its own modelled rate, so it takes
the **device** cadence (150 ms), not the sim's (33 ms). This is a fact about
the wire, never about the device (`lpa-devices` gains no kind), matching the
AAD ADR's own framing of `LinkTransport`. **Where it is not yet a fully
separate fact**: `session_control` (`studio_controller.rs:1891`) still maps
`Emu | Serial => DeviceFace::Wire`, so an emu's card currently reads the same
verb text a serial device's does (`Disconnect`) at the session-control
surface, while the card itself calls the same action `Power off` — an open
G2 question, named in Follow-ups, not resolved here.

## Consequences

- **Two "refused" rows close.** Against the eight-point contract in the
  2026-09-09 ADR, points 5 (flash get/put) and 6 (probes) are answered by
  `TabEmulatorPort`; `snapshot()` stays refused on every backing. The dated
  amendment to that ADR records this.
- **`DeviceByteStream: Send` bent the design once, and it is the one place.**
  `lpa-client`'s `DeviceByteStream` is `Send` and synchronous; a Rust struct
  cannot hold a `JsValue`. So the port object lives in
  `emulator_tab_bridge.js`'s own registry, keyed by a small integer, and Rust
  holds the integer — the same shape `browser_serial.js` uses for its port
  ids, adopted rather than invented.
- **A site-relative `import()` must be wrapped in a function, twice over
  now.** esbuild constant-folds a bare `import("/lpa-link/...")` literal at
  bundle time and fails to resolve it when nothing in that crate sits behind
  the path; `browser_serial.js:224`
  (`controllerModulePromise ??= import(controllerModulePath())`) already
  wraps its device-controller import this way, and
  `emulator_tab_bridge.js`'s `loadTabModule()` /
  `import(tabModulePath())` now does the same for `emulator_tab.js`. Getting
  this wrong does not fail loudly: it fails the whole bundling step, `dx`
  falls back to copying wasm-bindgen snippets to the wrong path, and Studio
  hangs on "Loading Studio…" — measured at 0 stories discoverable before the
  fix, 600 after.
- **No persisted-format change.** `RuntimeKind::Sim` still serializes to
  `"sim"`; every sidecar on disk round-trips byte for byte. The new bytes are
  the OPFS flash images, which are not `catalog/` or `project.json` and are
  not covered by the persisted-format rule.
- **The band's speed clause sits where the sim's granted tier goes** (an emu
  grants no shader tier), and it is honest by construction: `None` when
  nothing has been measured yet, never a placeholder number.

## Alternatives considered

- **`wasm32-unknown-unknown` + wasm-bindgen.** Would fork the module from
  M7's browser host (`host_browser.rs`, `jit-host.js`, the `emu_host`
  imports), which is written against the wasip1 target. Rejected (D3):
  keeping wasip1 and adding a thin `emu_*` export surface beside `_start`
  costs one build script and no fork.
- **A native `ws:` backing for mode A first.** The door
  (`lp-cli emu serve`) already exists and plan two proved it flashable from
  a browser. Rejected for v1: it needs a server process on the user's
  machine, which defeats "no board on the desk"; filed as future work
  (`?on=ws:`, notes.md §8.4).
- **The default flip on a measured bar.** The vision's T5 considered
  flipping the default to emu once a module exists (the AAD ADR's original
  D43/D44 shape). Rejected outright by D1: no default changes when a module
  lands; the choice is the user's, always.
- **Flash bytes through `LpFs`.** The library's in-memory mirror (`LpFsOpfs`)
  already exists and boards could persist there. Rejected (D15): a 4 MiB
  image per record riding the library's flush would multiply page RAM by
  every open tab; a worker-owned sync access handle touches the bytes from
  exactly one place.

## Follow-ups

- **The M7 hook (S1 of this plan's P5) is deferred.**
  `scripts/emu/bench-web/jit-host.js` is on `origin/main`, so the condition
  for landing the hook (D26) is met — but it edits `emulator_worker.js`,
  which two open branches (`claude/emu-c6-tab-loose-ends` #710, and a sibling
  W5) are also editing, and this plan's own P1–P4 stack is unmerged. The
  director deferred S1 to a follow-on phase once that stack lands; the seam
  documented at P2 (a named import slot, `?emujit=1` for mode B) is
  unchanged and still where it lands.
- **Reset replays the board's lifetime** — `Esp32C6Machine::run_until`
  (`lp-emu/esp/lp-emu-esp32c6/src/machine.rs`) fixes `stop_cycle` once, as an
  absolute guest cycle, before its loop; a reset inside the slice zeroes the
  clock via `reboot()` and the loop continues against the *old* absolute
  bound, so the one slice that carries a reboot replays the board's entire
  guest lifetime. Measured (#710): a 0.5 s-old board's reboot slice costs
  4.6 s of wall; 8 s old costs 46 s. Escalated (E4); fix brief at
  `lp2025/2026-09-11-0911-tab-emulator-loose-ends/w4-reset-replay-fix.md`,
  not dispatched. See the defect entry below.
- **Forget leaves the OPFS image behind.** `deleteFlash` resolves and
  deletes nothing; a sibling effort (W5) is in flight on it as of this ADR.
  Not claimed fixed here. See the defect entry below.
- **The tab walk's upload step flaked once in three runs** (`WentQuiet`
  ~30 s mid-push, heartbeats alive) — undiagnosed; filed as a defect, not
  chased in this phase.
- **Open G2 questions, named and not resolved here:**
  - Power off vs Disconnect: `session_control` routes `Emu` to
    `DeviceFace::Wire` (same as `Serial`) while the card's own verb reads
    "Power off" — two surfaces disagree on the word for the same gesture.
  - The Hardware settings row keeps one row per board (D41), with no
    emu/sim tag; whether it should grow a second row too is open.
  - `speed_word`'s two-decimal branch (below 0.1×) is an interim ruling; a
    plainer form (e.g. "1/25 speed") was proposed at G1 and not chosen.
- **`just walk-no-board --tab` (acceptance criterion 1) is not fully met on
  this stack.** G1 found it stops at the flash step; a sibling, unmerged
  branch (#710) shows it passing 6/6 on a **young** board and traces the
  failure to the reset-replay defect above on an **old** one. Neither fix is
  on this branch. `just walk-no-board --tab` was **NEVER RUN** by this
  phase, by director instruction (S5).
- Snapshot resume, the pin-honest probe view, AAD-D8 deadline scaling, a
  native `ws:` backing, more or ELF-booted tab boards, classic/S3 modules,
  and two devices per tab remain future work (`notes.md` §8.4).
