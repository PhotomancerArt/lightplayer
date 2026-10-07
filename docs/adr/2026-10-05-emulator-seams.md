# ADR: Emulator seams — what a host answer may claim

- **Status:** Accepted (2026-10-06); amended 2026-10-06 (§11, §12 — PR #993,
  Wi-Fi M6)
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
measured evidence when that plan closed (Accepted 2026-10-06, after G2).

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
| **Capability** | hardware the emulator cannot model | in every emulated run, testing included, once one exists |
| **Performance** | hardware it models faithfully but slowly | **only** on the emulated boards a user adds on Studio's Devices page. Never `?emu=` (ws or tab), `emu serve`'s defaults, the walks, CI or validate. Never in `validate record` |

`?seams=<atoms|none>` forces the setting either way on Devices-page boards,
for testing.

*(amended 2026-10-06, §11): the first capability seam, `net=lan`, shipped
with PR #993 (Wi-Fi M6) — soft, in the default set, so this row's "every
emulated run" is now true in practice, not only in the rule.*

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

*(amended 2026-10-06, §11, §12): `net=lan` is the first capability seam in
the default set, and its default is **soft** — every emulated C6 run whose
image carries the seam engages it, so the label `lp-emu:esp32c6:<grade>+net=lan`
is now the common case, not the exception this section was written against.
A run whose pace was set explicitly carries `@pace=<mode>` after the seam
atoms (`…+net=lan@pace=realtime`); it is never a `+` atom, a pace moves no
grade, and `validate` refuses to record or replay a paced composite.*

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
decision.

*(amended 2026-10-06, §11): the last sentence above is now false — the
network seam shipped the firmware wake handler (plan P10; no Bluetooth seam
had shipped one first). The handler, rule (a) and rule (b) are proven as
written: see §11.*

### 8. Many boards

No seam state is static. Each engaged capability seam has a host-side
**endpoint**, addressed `<board>/<seam>`; a **medium** connects endpoints of
several machines, and the lockstep runner is its deterministic driver. Real
media (a virtual LAN, a Bluetooth air) arrive with their seams.

### 9. The LTO rule

Firmware never hand-writes a seam function or its call. `lp-seam`'s
`seam_fn!` generates both: the seam function is `#[inline(never)]
extern "C"`, exported, and holds a unique non-`pure` hint
(`addi zero, zero, <id>`) so it cannot be deleted or folded; the call shim
reaches it only from an `asm!` `call` with the arguments bound to `a0..a7`
and `clobber_abi("C")`, so LTO cannot drop the argument set-up or fold the
result (M0's K1 finding).

### 10. The review rule

Code that imports `esp_radio`, `trouble_host` or the RMT driver from outside
a seam needs a reason in review. The seam is where the emulator can see the
boundary; code that goes round it is code the emulator cannot answer.

### 11. The network seam (2026-10-06, PR #993)

`net=lan` is the first **capability** seam to ship. It is a seam in the
**switch** shape (§3): at network bring-up `fw-esp32c6/src/net/net_bringup.rs`
reads the engaged byte (`net_mac`'s, FD7) once. On silicon it reads 0 and
PR B's radio path runs unchanged (`esp_radio`'s station interface,
`EspStation`, the existing frame device) — ESP-NOW and BLE are not touched
either way. On an emulated board that engaged `net=lan` it reads 1, and a
seam-backed `SeamStation` and `SeamFrameDevice` plug in under the **same IP
stack** instead.

**Below the IP stack.** The seam hands over everything at and below the frame
device: raw Ethernet frames in and out, link state, scanning, joining and
leaving a network, station events (associated, auth failed, not found, link
lost, scan done) and the station's own MAC. Everything above it is unchanged
and real: `embassy-net`, lp-link, the secure LAN endpoint, mDNS and
`lp-server` all run exactly as they do on silicon, against whichever frame
device bring-up chose.

**Nine calls, pull-only, and the wake.** `lp-base/lp-seam` declares nine
calls in one block (`net_mac`, `net_take_frame`, `net_give_frame`,
`net_link`, `net_scan_start`, `net_scan_take`, `net_connect`,
`net_disconnect`, `net_event_take`; `lp_seam::net` carries the shared numbers
— event codes, the scan record layout, `MAX_FRAME_LEN` 1514 B,
`MAC_LEN` 6 B). Pull-only, as §7 requires: the emulator answers only what a
call asked for and writes only the buffer the call handed it. **The network
seam ships the firmware wake handler** (plan P10) — no Bluetooth seam had
shipped one first, so §7's "no firmware wake handler ships until the
Bluetooth seam does" no longer holds (see the amendment at §7). The handler
binds `FROM_CPU_INTR3` at priority 1, **only when the seam is engaged**
(never on silicon), costs 78 B of IRAM, and is entirely in RAM except the two
waker calls it makes when a bit was set (`AtomicWaker::wake`, the critical
section and the executor's wake leave RAM, but only on an emulated board). The
pending word's bit 0 means a frame is waiting, bit 1 means a station event is
waiting; any bit wakes both waiters. Its consumers are the two tasks on
**`lp-net`** — embassy-net's runner and the station task — never the main or
render executor, which is G0 rule (a) in code.

**The virtual LAN.** The medium under the seam
(`lp-emu-esp-common::seam::net`) is one Ethernet segment several emulated
boards join: a gateway that answers ARP and hands out DHCP leases, a set of
access points a board can hear and join, a host TCP port forwarded to each
board, and a host-side probe that answers `_lightplayer._tcp` lookups. It
never pretends to be a radio — no airtime, fading, retransmission or
coexistence — and it has **no uplink**: nothing on it reaches a real network.
`SharedLan` is the handle every board, host and runner on one LAN shares, with
one of three drivers: `LanDriver::Runner` (the lockstep runner delivers at its
quantum boundaries — deterministic, the only form CI or a multi-board test
uses), `LanDriver::SelfDriven` (one board's own machine pumps it on its guest
clock — `emu run`, the tab; deterministic for that one board), and
`LanDriver::WallClock` (`emu serve`'s boards, each on its own OS thread,
driven by the host's clock — not deterministic, and never what a test
asserts on). The wake is paced per G0 rule (b): never two raises outstanding,
a bounded queue per endpoint, a cap per take (512 B by default, below a full
Ethernet frame).

**`net=lan` is a capability default, soft** (§2, §4's soft-request rule):
every emulated C6 run whose image carries the network seam engages it, test
runs included, with no flag asked; an older image, or a run built without
the seam's declarations, prints `SEAM none engaged: …` and runs with no
network, exactly as §4 specifies for a soft request that cannot engage. This
is the first time a capability default changed what an ordinary emulated C6
run's label is: **every emulated C6 run carrying the image is now
`lp-emu:esp32c6:<grade>+net=lan`.** Plan P12 found and fixed what keyed on the
old, bare label: `validate record`/`run` for the C6 now always states
`--seams` explicitly (the composite's atoms, or `none` for a seam-free run),
so a plan and its sidecar never disagree; transcripts, `validate replay` and
`--against` resolve and file by the run's own label, sorted atoms; the chip
heap gate, `bless-chips` and `apply-ci-figures` read each run's label rather
than a hard-coded one. No existing committed transcript moved (all are
seam-free), and a board that joins nothing costs +32 B of heap against
`--seams none`.

**The overlay.** `validate.toml` gains a `[[seam]] net=lan` entry, but it
moves **no class's grade**: the schema has no class for signal strength,
airtime, throughput or BLE/ESP-NOW coexistence, so there is nothing in it to
mark `absent` — those are stated in the overlay's description instead, and a
`radio` class is proposed for when the first radio-claiming payload lands.
`memory` keeps the base grade with the A7 caveat carried forward: a heap
figure taken off a *joined* board does not speak for silicon, because the
Wi-Fi driver's own join allocations never happen when the seam answers.
Everything above the frame device — the IP stack, the link, the server —
stays at the base grade, because it is the shipped image's own code running
for real.

**What stays desk-only.** The seam answers nothing about the radio itself:
signal strength, airtime and over-the-air throughput, coexistence with BLE
and ESP-NOW, the Wi-Fi driver's own heap and join timing, and the
**warm-reset DMA class** — whether the radio's DMA buffers land outside
`dram2_seg` and whether repeated warm resets (RTS and a requested reboot,
alternating) stay crash-free with the station joined, traffic flowing and
BLE on (plan AC6; the emulator models no DMA target placement and nothing
about its bootloader crashes from one). Anything that depends on Chromium's
own USB/serial stack or its Local Network permission prompt is a desk
sitting away too, as is `.local` resolution in Chrome, which the walk report
names as unverified. A difference on anything the seam *does* model (the
frame device, the IP stack above it) is a fidelity defect, not a thing to
work around.

### 12. The pace axis (2026-10-06, Yona's decision)

An idle emulated board's guest clock runs far ahead of a wall-clock peer's (a
`wfi` skips straight to the next timer — measured ~8× a host's, PR #993): a
host's 1–2 ms round trip through a LAN forward was 10–17 ms of the board's,
so lp-link's resend timer and the reply deadline fired on frames the host had
not yet had time to acknowledge. Yona's ruling, relayed to the seams roadmap
(`lp2025/2026-10-05-1026-emulator-seams/notes.md`):

> "clock advance should be controlled. we may want an emulator that runs as
> fast as possible, but we may not. if we want 1x (which we do for a lot of
> things, like testing patterns) — then it should not run fast. if we don't
> care (like running some unit tests) then great — run as fast as possible.
> that should be an option."

**What shipped is the first slice of that option**, scoped to what the
network seam's virtual LAN already paces: a named, opt-in axis,
`lp_emu_esp_common::seam::net::Pace`, on `lp-cli emu run --pace realtime|max`
and `emu serve --pace …` (per-board `pace=`, beside `lan=` and `seams=`).

- **Unset** (every existing run, CI, the lockstep runner, every test, the
  tab): **realtime while a host is attached through a LAN port forward,
  otherwise max** — today's behaviour everywhere nothing has asked
  differently.
- **`realtime`**: held to wall time (1×) for the whole run, host or no host —
  what an interactive host (a person watching a pattern, Studio, a wall-clock
  peer) gets. It needs the network seam engaged on a LAN the board drives
  itself; a `--seams none` run or a runner's LAN (lockstep, CI) refuses it.
- **`max`**: never paced, even with a host connected — what CI, the
  lockstep runner, tests, validate and the benches stay at.

**The label.** A set pace shows after the seam atoms, never as one:
`lp-emu:esp32c6:t1+net=lan@pace=realtime`. An unset pace adds nothing, so
every label that existed before this axis is unchanged. `validate record`
and `validate run` refuse any paced composite — `realtime` is wall-clock
dependent and a transcript must be a pure function of the image; `max` is
what every validate run already is.

**The number, honestly.** `realtime` is **not** exactly 1×: a host's sleep
overshoots what it asked for and the overshoot is never made up, so an idle
board held to a connected host runs at **≈0.66×** wall speed (measured
0.65×). A catch-up variant that repaid the overshoot did reach 1.00×, but
surfaced `[RECOVERY] io task silent > 2000 ms` in 3 of 3 runs
(`docs/defects/2026-10-06-the-io-task-goes-silent-for-2-s-under-paced-lan-traffic.md`,
open, cause not yet named) and never showed it without catch-up (0 of 2) —
so the catch-up variant stays **out** until that stall is understood, and the
shipped `realtime` is the slower, no-catch-up one.

**Left for later** (the seams roadmap notes, filed from this PR): generalise
the axis to every host — the USB door (`serial:tcp://`, same ~8× race, 54
resends/upload, mostly hidden by the C6's 200 ms USB resend floor),
`emu serve` boards Studio opens through `?emu=`, and the in-tab board
(`?emu=tab`); move the mechanism off the LAN's pump so a `--seams none` run
can take a pace too; and decide the end-user default (pattern viewing
probably wants `realtime` by default on Devices-page and `?emu=` boards —
Yona's call, at planning).

## Consequences

- One firmware still runs everywhere. On silicon the LED seam is one call,
  one no-op hint and one return per spin iteration.
- A seam-off emulator run is today's machine: no scan, no patch, one
  boolean test a slice, no moved figure.
- End users get a lighter emulated board; nobody else does, so no test,
  walk or validate run measures a seamed machine by accident.
- Every new seam is an ABI change (a new identity). Old images lose their
  seams until updated, and say so.
- A performance seam's numbers carry their label everywhere they go, and
  cannot become a transcript.
- **The flash cost is not free, and it compounds.** The network seam's
  firmware half (§11) cost **+4.9 KB** of flash against plan P10's own
  +2 KB estimate (`.text` +4,368 B, `.rodata` +448 B, `.rwtext` +76 B,
  `.data` +16 B, `.bss` +64 B — mostly the seam station's state machines
  inlined into the station task and the second `Driver` arm's glue). The
  split image's core is now **1,152 B short of its next 32 KiB page**, so
  the next roughly 1.2 KB of core growth costs a full 32 KiB of steady
  flash headroom, not 1.2 KB. **The relay (Wi-Fi M7, PR B) will likely be
  the change that crosses that page** — it should check the core's page
  margin before it ships, not after.

## Evidence (planning `lp2025/2026-10-05-1709-seams-foundation-led`, PR #987)

**What the firmware pays** (`just fw-esp32c6-size-check`, main against the
branch): core +80 B, engine +8 B, room left −8 B (358,056 → 358,048 B).
`.rodata` +64 B, `.text` +20 B, `.engine_text` +8 B; `.data`, `.bss`,
`.rwtext` and `.trap` unchanged, so **0 B of RAM and 0 B of IRAM**. In the
release image `LP_SEAM_TABLE` is 0x58 B of core rodata (0x42000b28),
`lp_seam_ws281x_wait_step` is 0x14 B of core text (0x42065b80) carrying its
hint `addi zero,zero,1`, and its one call site is inside
`Esp32C6RmtWs281xOutput::write`'s spin loop. The `test_seam_abi` harness
prints `echo=7` seam-off and `echo=1587544071` (0x5ea00007) seam-on, so the
arguments and the result survive LTO (§9).

**Honesty** (`lp-cli/tests/emu_seam_led.rs`, in `just test-emu-c6-cli`):
`render-basic` on the shipped image, seam off and `led=fast`, renders 869
frames each, **byte-identical frames and identical heap**, over 56,469 seam
calls. `seam_off_is_today` holds the seam-off machine to today's, and no chip
figure moved.

**The split image** (`seam_split_rom_up`, `seam_two_tables`,
`seam_restart_rescans`): ROM-up from the reset vector through the real IDF
bootloader and the RAM-only loader, the seam waits for the app, arms only
once the hart runs from the flash window, against a live table whose own
address is below the engine window (0x4240_0000) and whose flash offset is
past the core offset the loader prints (`[LOADER] core @`). The bootloader's
checksum of the app passes, which is the K2 lesson held. Two identity-matching
tables arm the live one; a restart rescans.

**What it buys — provisional.** The end-user row (the tab module, ROM-up,
the split image with `render-basic` in lpfs, USB attached, t2, 5,500 ms
emulated; `lp-emu:esp32c6:t2` against `lp-emu:esp32c6:t2+led=fast`, lp-emu
`b77b5a4a0`), best of 3, user seconds: **node/V8 13.85 → 12.03 (−13.1 %)**,
**bun/JSC 15.03 → 10.52 (−30.0 %)**, UART identical on every leg. **These
were taken at load average 30–85 and are provisional**: they sit either side
of the spike's −22.4 % / −21.0 %, and will be retaken in a quiet window (and
on the phone) before they are quoted as the seam's effect.
`docs/emulator-perf-ledger.md` §3 carries the row.

**Studio** (G1, passed 2026-10-05): a Devices-page board journals `emu: LED
fast mode on (led=fast)` once per start; `?seams=none` journals `emu: LED fast
mode off (?seams=none)` and runs today's machine; `?emu=tab` and
`?emu=ws://…` boards journal nothing about seams (`just walk-no-board --tab`).

**Silicon** (G2, the desk check, passed 2026-10-06; silicon, not the
emulator): the emulator walk and the hardware walks of main (`cc20d0f6d`) and
of the seamed image (`bfa53d88e`) on a desk XIAO ESP32-C6 matched the host
oracle byte for byte (crc `0x55772254`). On a second XIAO C6, the packaged
split images ran 60 s each: **38 fps** median of the last five readings on
main and **38 fps** with the seam, so the seam costs silicon nothing a frame
counter can see. Both boards were restored byte for byte afterwards.

**Accepted** 2026-10-06: Yona approved the ship after G2 ("happy for you to
ship it and move on"). The end-user speed row above stays provisional until
it is retaken.

**PR #993 (Wi-Fi M6, the network seam and the pace axis, 2026-10-06).** The
emulated Wi-Fi walk (`just walk-wifi-emu lan`,
`docs/reports/2026-10-06-wifi-emulator-walk.md`) passed **10/10, twice** (runs
11 and 12) at the shipping tree `52a3e6549` — configuration
`lp-emu:esp32c6:t1+net=lan`, default pace, lp-emu `52a3e6549` — every step on
the boards' own console words, never a page string something else could
satisfy. W10 (`lp-cli link rtt lan:127.0.0.1:<forward>`) with `button-sign`
rendering at ≈92 emulated fps: request p50 6.747 / p90 7.908 frames (run 11),
p50 4.735 / p90 5.958 frames (run 12), no reply-deadline trips and no
watchdog resets in either run. `just walk-no-board --serve-release` stayed
6/6 at the same tree. CI carries two cells in `test-emu-serve` (the
Emulator C6 job, dev profile): `lp-cli/tests/emu_lan_lockstep.rs` runs two
boards deterministically through the lockstep runner — each saves the network
over USB, joins with a lease by 0.20 s emulated, resolves the other's
`lp-xxxx.local` through the LAN probe by 3.30 s, and gives the identical
frame log twice, with a third board ending `wrongPassword` then `notFound`;
`lp-cli/tests/emu_lan_link.rs` joins one board over its forward and uploads a
project through it. Before the pace axis (§12), four uploads of
`projects/test/basic` over one fresh board's LAN forward resent 95, 84, 74
and 83 frames each (nothing lost — TCP had delivered all of them); with the
pace, the same experiment resent 9, 14, 12 and 13
(`docs/defects/2026-10-06-an-emulated-boards-clock-outran-its-lan-host.md`),
and a whole `lp-cli link rtt` session's resends fell from 42 to 2.

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

- The Bluetooth link seam. *(amended 2026-10-06, §11): the firmware wake
  handler and rule (a)'s API already shipped with the network seam, ahead of
  Bluetooth — this follow-up is now just the Bluetooth seam itself, reusing
  that handler and adding its own consumer, not building the wake.*
- ~~The network seam, inside Wi-Fi M6.~~ *(amended 2026-10-06): shipped, §11.*
- Xtensa seams (roadmap M7): a windowed-ABI return.
- Retake the end-user bench row in a quiet window and on the phone; the
  numbers above are provisional.
- **Generalise the pace axis to every host** (filed 2026-10-06 from PR #993,
  §12): the USB door, `emu serve` boards Studio opens through `?emu=`, and
  the in-tab board; move the pacing mechanism off the LAN's pump so a
  `--seams none` run can take a pace too; decide the end-user default
  (pattern viewing likely wants `realtime` on Devices-page and `?emu=`
  boards); keep the catch-up variant out until its io-task stall is
  understood.
- **Later idea, not planned:** the seam's state is a journal line, not on the
  device card. Yona at G1: "no one will care. later we may want to add
  additional info about the emulator in some detail popup or similar, where
  we can point it out." If such a popup is built, it is where the seam
  belongs.
