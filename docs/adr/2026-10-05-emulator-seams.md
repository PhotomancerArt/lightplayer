# ADR: Emulator seams — what a host answer may claim

- **Status:** Proposed
- **Date:** 2026-10-05
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

The ESP32-C6 emulator runs the shipped firmware image, byte for byte, and
the rules that make that worth anything are strict. The architecture ADR
(`2026-09-06-esp-soc-emulator-architecture.md`) says the ROM hook table
starts empty and a hook is a last resort: try the real path first. The
virtual-air ADR (`2026-09-08-virtual-air-claim-policy.md`) says an event the
emulator originates says so, the air is off unless asked for, and a trust
grade moves only with a transcript.

Two things push against those rules:

- **Hardware the emulator cannot play at all**: Wi-Fi, Bluetooth, later a
  camera. A project that uses them does not run in an emulator today.
- **Hardware it plays faithfully but slowly**: the LED output (RMT). A user
  who adds an emulated board on Studio's Devices page runs it in a browser
  tab, where host time is the product.

The roadmap (planning `lp2025/2026-10-05-1026-emulator-seams`) asked for one
firmware with a few **named places where the emulator may answer**, called
emulator seams. Its M0 spike (draft PR #983, never merged) proved the
mechanism on the C6 and measured it: the LED seam saves 22.4 % host CPU
(node/V8) and 21.0 % (bun/JSC) on the end-user row, with identical frames,
fps and heap, for 96 B of flash and no RAM. Yona accepted it at ~22 % (G0).

This ADR records the exception that seams are, and the rules that keep it
narrow. It is written before the code (planning
`lp2025/2026-10-05-1709-seams-foundation-led`, P1) and finalised with the
measured evidence when that plan closes.

## Decision

### 1. A seam is a named exception, not a hook

A seam is a function in the one shipped firmware, `lp_seam_<name>`,
declared once in `lp-base/lp-seam`. On silicon its real body runs. The
emulator answers it only when a run asked for it (it is **engaged**). It is
a named exception to the architecture ADR's "the hook table starts empty /
try the real path first": the ROM hook table **stays empty**, and seams are
their own list, with their own counters, in the firmware's own descriptor
table. It is also a named exception to the virtual-air rules, kept as close
to them as it can be:

- **an answer says so** — every engaged seam announces itself (§5);
- **off unless asked** — no seam runs on a run that did not ask, and the
  emulator does not even scan for the table;
- **grades move only with a transcript** — a seam composes its own trust
  overlay onto the base configuration's, and a performance seam never makes
  a transcript at all (§5).

### 2. Two kinds, and where each is on

| Kind | For | On |
|---|---|---|
| **Capability** | hardware the emulator cannot model | in every emulated run, testing included, once one exists (none ships yet) |
| **Performance** | hardware it models faithfully but slowly | **only** on the emulated boards a user adds on Studio's Devices page. Never `?emu=` (ws or tab), `emu serve`'s defaults, the walks, CI or validate. Never in `validate record` |

`?seams=<atoms|none>` forces the setting either way on Devices-page boards,
for testing.

### 3. Two shapes, and charging

- **Replace:** one real function; the emulator patches its entry and
  answers. Fits a synchronous boundary (the LED wait).
- **Switch:** an **engaged byte** in flash `.rodata` (`0` on silicon) that
  the firmware checks at start-up to plug in a seam-backed adapter whose own
  calls are seams. The emulator patches the byte to `1` in the cache window,
  exactly as it arms code. On silicon it is one load and one branch.

**A seam always charges the skipped work's time.** The LED seam (L1) seams
only the render thread's busy wait between polls of the RMT driver: engaged,
it returns and the hart parks until an interrupt it would wake for. The RMT
model, its refill interrupts and the done interrupt all still run, so the
wire time is billed by emulated time passing. That is the answer to the perf
ledger's rejected R3′ ("turn the output off, and fps lies"): fps stays
honest because the time is billed, not skipped.

### 4. Identity: an exact match, and no cross-build compatibility

`SEAM_ABI_ID` is FNV-1a 64 of the `declare!` invocation's tokens without
whitespace, computed by the compiler; `doc:` text counts. Firmware and
emulator engage seams only on an exact match. **There is no cross-build
compatibility: an image built from different seam declarations runs with
no seams engaged, and the emulator says so.**

A request is **strict** (`--seams`: a seam that cannot engage is a hard
error) or **soft** (`--seams-prefer`: engage what the image allows, else one
loud line and run with none). Studio's boards ask softly, because a saved
board can hold firmware older than the Studio serving it. Capability
defaults, when one exists, are soft. With an empty request nothing scans.

### 5. Announcements, labels and composed grades

- A line at the start of every run and every chip start:
  `SEAM <atom> engaged (<kind>, abi <id>, <sites>)`, or
  `SEAM none engaged: <why>` for a soft request that could not engage.
- A `SEAM <atom> <verb>` line per call under `--trace`; `seam_calls` and
  `seam_arms` in the snapshot.
- The run's configuration label is the base name plus its engaged atoms,
  sorted: `lp-emu:esp32c6:t2+led=fast`. With none engaged, exactly the base
  name. `real` is the reserved not-engaged implementation.
- `validate.toml` gains `[[seam]]` overlays. A composite name resolves to the
  base configuration plus each atom's overlay; an overlay replaces a class's
  grade or marks it absent, and `--strict` refuses an absent class.
  Nobody writes one table per combination.
- A transcript sidecar carries `seams` and `seam_abi` only when a seam was
  engaged, so every existing sidecar stays byte-identical.

### 6. The split image: a core root, the live MMU, two tables

The shipped C6 image is a loader, a core and an engine
(`2026-10-04-c6-split-link-firmware-loader-and-boot-records.md`).

- **The seam table is a core root.** `LP_SEAM_TABLE` joins the split tool's
  core roots, so the table and the seam leaves it names land in the core.
- **Arming goes through the live cache MMU.** A site's flash bytes are found
  by translating its address through the mapping the running firmware set
  up, not by reading image headers. The table carries its own address; a
  table is **live** when its own address translates to the flash offset it
  was scanned at.
- **Several tables in flash are normal** (after an update, two cores).
  Every identity-matching table is a candidate; the live one is chosen at
  arm time; two live tables is an error.
- **Seams resolve on every chip start** — build, reboot, power cycle — and so
  after a flash write plus reset (Studio's Update firmware). Each start
  waits until the hart first runs from the flash window, because the
  bootloader reads the app through that window to verify it.

### 7. Pull-only, and the wake

The emulator only answers calls and writes memory a call handed it. The one
exception is the **wake**: one pending word (its address in the table) and
one line, `FROM_CPU_INTR3` at priority 1. The host sets bits, then raises;
the guest's handler clears the line, then swaps the word to zero. Two rules
from G0:

- **(a)** whatever a capability seam wakes runs on the firmware's **IO
  thread**, never the main or render executor;
- **(b)** the emulator **paces** what it produces: never two raises
  outstanding, a minimum spacing, a bounded queue per endpoint, a cap per
  take.

The revisit trigger **R-WAKE** stands: a lost or doubled wake the protocol
cannot explain, a second wake line, a host callback into guest code, a line
collision, or wake latency over 100 µs emulated sends the rule back to a
decision. No firmware wake handler ships until the Bluetooth seam does.

### 8. Many boards

No seam state is static. Each engaged capability seam has a host-side
**endpoint**, addressed `<board>/<seam>`; a **medium** connects endpoints of
several machines, and the lockstep runner is its deterministic driver. Real
media (a virtual LAN, a Bluetooth air) arrive with their seams.

### 9. The LTO rule

Firmware never hand-writes a seam function or its call. `lp-seam`'s
`seam_fn!` generates both: the seam function is `#[inline(never)]
extern "C"`, exported, and starts with a unique non-`pure` hint
(`addi zero, zero, <id>`) so it cannot be deleted or folded; the call shim
reaches it only from an `asm!` `call` with the arguments bound to `a0..a7`
and `clobber_abi("C")`, so LTO cannot drop the argument set-up or fold the
result (M0's K1 finding).

### 10. The review rule

Code that imports `esp_radio`, `trouble_host` or the RMT driver from outside
a seam needs a reason in review. The seam is where the emulator can see the
boundary; code that goes round it is code the emulator cannot answer.

## Consequences

- One firmware still runs everywhere. On silicon the LED seam is one call,
  one no-op hint and one return per spin iteration.
- A seam-off emulator run is today's machine: no scan, no patch, no
  per-slice check, no moved figure.
- End users get a lighter emulated board; nobody else does, so no test,
  walk or validate run measures a seamed machine by accident.
- Every new seam is an ABI change (a new identity). Old images lose their
  seams until updated, and say so.
- A performance seam's numbers carry their label everywhere they go, and
  cannot become a transcript.

## Alternatives Considered

- **L0: `wfi` in the wait on silicon too, no seam.** Saved 13.7 % and
  changes silicon's refill deadline; G0 chose L1.
- **L2: answer the whole LED send.** Fails "charge the skipped work" by
  analysis, and would need the pin model off.
- **Image-header arming** (the spike's `esp_app_image` walk). Reads the
  loader's header on the split image and knows one format; the live MMU
  knows every layout.
- **A build-commit identity.** Dirty dev builds never match each other.
- **Version ranges or shims across builds.** Rejected (§4); revisited only
  when emulated boards must run firmware from a different seam definition.

## Follow-ups

- The Bluetooth link seam, with the firmware wake handler and rule (a)'s API.
- The network seam, inside Wi-Fi M6.
- Xtensa seams (roadmap M7): a windowed-ABI return.
- Evidence (device cost, honesty test, end-user bench row, split-image
  arming facts) is filled in when the plan closes.
