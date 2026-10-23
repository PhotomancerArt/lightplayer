# fw-esp32s3

LightPlayer firmware for the **ESP32-S3** (Xtensa LX7). Sibling of
`fw-esp32c6` (RISC-V).

> **Now a real consumer of `fw-esp32-common`, not just a compile target.** An
> earlier version of this file called the crate `fw-esp32-common`'s "third
> consumer" before any dependency existed; a later correction went the other
> way and said "not yet a consumer" while M5 had only proven that
> `fw-esp32-common` *compiles* for `xtensa-esp32s3-none-elf` pulling no
> `esp-hal`. Both are now stale: the app-layer milestone wired it in for
> real — `server_loop`, `boot`, `logger`, `lp_fs`, `transport`, and the
> chip-agnostic `Esp32OutputProvider` this crate's RMT driver plugs into
> (below) all come from `fw-esp32-common`.

Runs the LightPlayer app on device: `lp-server` over USB-Serial-JTAG, the tree
store on `lpfs`, GLSL compiled to Xtensa machine code by the on-device JIT, and real
WS281x output on 4 concurrent RMT channels (below) — plus the hardware
harnesses (clocks, heap, and serial logging far enough to print the `[INIT]`
marker family, a JIT corpus runner, and the RMT loopback self-test). The
server and storage stacks landed as the app-layer milestone of
`~/.photomancer/planning/lp2025/2026-07-30-s3-app-layer-with-jit/`; the
4-channel RMT output replaced the serial-readout stand-in that milestone
shipped with, in `~/.photomancer/planning/lp2025/2026-07-31-0720-s3-led-output-4ch/`.
The Xtensa backport roadmap that created this crate is closed.

Verified on hardware 2026-07-30 (ESP32-S3 rev v0.2, 16 MB flash): flashes and
boots to `[INIT] ready`, and runs the Xtensa JIT corpus (below) with 11/11
cases matching their goldens. The desk board is a 16 MB part, but the
firmware only requires **8 MB** — see [Partitions](#partitions).

## Hardware harnesses

Any `test_*` feature makes `build.rs` set the `fw_harness` cfg, which replaces
the boot path's park loop with a harness runner — the same mechanism
`fw-esp32c6` uses, deliberately not a second one.

### `test_xt_jit_corpus`

```bash
just fwtest-xt-jit-esp32s3 /dev/cu.usbmodemXXXX
```

Compiles shaders on-device through `lpvm-native`'s JIT (`isa/xt`) and prints
`PASS`/`FAIL` per named case. It was the first execution of LightPlayer-compiled
code on Xtensa silicon, and it is what proved the **exec-alias fix**:
intra-module call targets must hold the I-bus alias, not the D-bus address the
linker wrote through.

The corpus lives in `lpvm-native::xt_corpus` rather than here, and that
placement is load-bearing: the host golden test
(`lpvm-native/tests/xt_corpus_goldens.rs`) runs the **same** modules on
`lp-xt-emu` and on rv32, so a device mismatch is a real emulator-vs-silicon
difference rather than two harnesses disagreeing.

**Goldens are committed constants confirmed before any hardware run. A device
mismatch is a finding to triage — never a reason to edit a golden**, which
would turn the comparison into a tautology that passes forever.

Run the host oracle first:

```bash
cargo test -p lpvm-native --features xt-corpus,emu-xt
```

### `test_xt_fp_conformance`

```bash
just fwtest-xt-fp-esp32s3 /dev/cu.usbmodemXXXX                 # the whole corpus
just fwtest-xt-fp-esp32s3 /dev/cu.usbmodemXXXX signed_zero 50  # a smoke run
just fwtest-xt-fp-esp32s3 /dev/cu.usbmodemXXXX tables          # estimate ROMs
just fwtest-xt-fp-esp32s3 /dev/cu.usbmodemXXXX helpers         # divide-step helpers + probe2
```

The M6 hardware campaign's rig. Runs `lp-xt-fp-vectors`' 5 630-vector corpus on
this chip's FPU and prints `(result, FSR)` per vector as hex blocks; the recipe
captures the transcript to `target/fp-capture/` and diffs it with `just fp-diff`.

Three things about it are load-bearing:

**The device regenerates its own inputs.** It links the *same* generator crate
the host predicted with, so vector 4 137 is the same vector on both sides by
construction — no transfer protocol and no reflash per batch. Both sides print
the generator's fingerprint and the diff tool **aborts** on a mismatch, because
a disagreement there means the two sides ran different vectors and every
comparison after it is meaningless.

**The harness decides nothing.** No `PASS`, no `FAIL`, no comparison. The
predictions live in `lp-emu/lp-xt-emu/tests/fixtures/fp/` and were committed
before any board ran (M6 D2); classification is the host's job. A device that
graded itself would be the tautology the whole milestone is arranged to avoid —
so, as with `test_xt_jit_corpus`, **a disagreement is a finding to triage and
never a reason to edit a golden.**

**A truncated capture is an error, not a partial pass.** Every family ends with
a sentinel stating its row count, and the parse fails if the count and the rows
disagree or the final `END-ALL` never arrives. That rejection is asserted by
unit tests against deliberately damaged fixtures, not by having tried it once.

The instructions themselves are `global_asm!` kernels — one per operation shape,
plus one per `(conversion, scale)` pair since the scale is an instruction
immediate — and every vector is a *call* into one of them. That is what keeps
the kernel count near thirty instead of 5 630, and why the campaign needs no FP
emitter. `LP_FP_MODE=tables` switches to the estimate-table sweep, which reads
the implementation-defined lookup ROMs behind
`recip0.s`/`rsqrt0.s`/`sqrt0.s`/`div0.s` back exhaustively and run-length
encodes them.

Run the host oracle first; it replays the whole corpus with no board attached:

```bash
cargo test -p lp-xt-emu --test fp_conformance
```

Measured on the desk S3 (MAC `D8:3B:DA:47:29:70`, chip rev v0.2), 2026-07-31:
`CPENABLE` arrives as **`0x000000ff`** — every coprocessor enabled, not merely
the FPU's bit 0 — under this boot chain. M6 P1 established that it arrives
armed; this says how widely. The provenance is still unpinned (no write exists
in esp-hal 1.1.1 or xtensa-lx-rt 0.22), so the harness arms bit 0 explicitly
anyway and prints both sides.

### `test_backtrace_oracle`

```bash
just fwtest-backtrace-esp32s3 /dev/cu.usbmodemXXXX
```

Proves `lpc_shared::backtrace`'s Xtensa windowed walk on silicon. A backtrace
walker that returns plausible-looking garbage is worse than one that returns
nothing, so this asserts an **exact** frame count rather than eyeballing
output: a recursive `chain(n)` produces `n` frames that all return to the same
call site, so a correct walk contains a run of exactly `n` identical PCs. That
is checked at depths 5, 15 and 25 — all well past the point where the register
window ring wraps and the frames stop being reachable without a forced spill.
Corrupt save-area chains (cyclic, descending, torn, off-stack, unaligned) must
terminate at exact counts.

The harness also prints a **control** that runs the same walk with the window
spill skipped. On the run that established this it reported 19 frames where 25
were expected — six believable, correctly-typed, wrong addresses. That is the
failure mode the spill exists to prevent, and it is why the control is in the
transcript.

Run the host oracle first — it drives the same walk against synthetic stacks
built inside the real S3 DRAM window:

```bash
cargo test -p lpc-shared
```

### `test_loopback`

```bash
just fwtest-loopback-esp32s3 /dev/cu.usbmodemXXXX
```

The WS281x timing oracle, with no oscilloscope and no strips: each of the four
RMT TX channels (GPIO4-7) is routed into its own RX channel through the GPIO
matrix, so the firmware captures its own waveform at 12.5 ns resolution and
asserts it numerically while all four transmit at once — decode against the
sent bytes, per-bit high time and period within **±25 ns** of that channel's
own configuration, no cross-talk, the 300 µs latch, a 100-frame concurrent
soak with zero guard trips, and a guard-word truncation on one channel that
must leave the other three's frames intact.

It also probes the RMT RAM address on-chip (`E1:` lines) by making the
peripheral itself deposit a word through its APB FIFO port: the S3's RAM is at
`RMT_BASE + 0x800`, not the C6's `+0x400`, and getting that wrong transmits the
tail of the register file.

The `E4: MEASURE golden_*` block is the re-derivation of the committed
hardware golden `lp-fw/lp-ws281x/tests/golden/ws2812_grb_esp32s3.txt`. As
everywhere else here, **a device mismatch is a finding to triage, never a
reason to edit the golden.** Run the host oracle first — it drives the same
sequencing against a mock and the same classifier against that capture:

```bash
cargo test -p lp-ws281x
```

### `test_button`

```bash
just fwtest-button-esp32s3 /dev/cu.usbmodemXXXX
```

GPIO button diagnostic mode: D9 (GPIO8) with an internal pull-up, normally-open
button to GND. Prints a `BUTTON gpio=... seq=... kind=...` line per debounced
press/release. Ported from fw-esp32c6's `test_button`, but synchronous instead
of an embassy task — this chip's harness entrypoint never starts the embassy
runtime, so the poll loop busy-waits between samples instead of awaiting
`embassy_time::Timer`. Hardware verification (flash + jumper walk) happens at
the milestone gate that first has a use for the button; this harness exists so
the driver cannot rot uncompiled — see
`docs/defects/2026-07-28-fw-esp32-harnesses-rotted-uncompiled.md`.

### `test_seam_abi` (emulator only)

```bash
just build-fw-esp32s3 test_seam_abi   # `just test-emu-esp32s3-seams` builds and runs it
```

The Xtensa emulator-seam ABI under the real `release-esp32s3` build (S3
radio M5; `docs/adr/2026-10-05-emulator-seams.md` §13). It calls
`test_echo(1, 2, 4)`, drains `test_take`, and — when the network seam's
engaged byte reads 1 — `net_mac` and `net_link`, each through the shim
`lp_seam::seam_fn!` generates for Xtensa, with a value kept live across every
call; then, with the take engaged, it binds the S3's wake handler
(`seams::seam_wake_handler`, `SYSTEM.cpu_intr_from_cpu_3`) and sleeps in
`waiti`, draining the take on every wake. It is the only image with a seam
table (`src/seams/`); the shipped image declares none until it has a network
stack. On silicon it prints `echo=7`, `take engaged=false`,
`net engaged=false` and idles. The emulator's side is
`lp-emu/esp/lp-emu-esp32s3/tests/seam_abi_harness.rs`.

### `test_lcd_lanes` (emulator first)

```bash
just build-fw-esp32s3 test_lcd_lanes   # `just test-emu-esp32s3-lcd` builds and runs it
```

Five uneven WS281x lanes (12, 9, 7, 5 and 3 LEDs on GPIO1–5, the XIAO's
D0–D4) out of LCD_CAM's i8080 mode at 2.5 MHz, fed by GDMA from one
internal-RAM buffer, sent twice. It encodes its own frame (three 400 ns slots
per WS281x bit) so that it tests the emulator's LCD_CAM and GDMA models
(`lp-emu/esp/lp-emu-esp32s3/tests/lcd_cam_lanes.rs`, every lane byte-exact)
and not the product's encoder. On silicon it drives the same pads (nothing
need be attached) and prints `[LCD-LANES] frame N sent …` and `done`.

## Output

`src/output/rmt/` drives WS281x strips from the RMT peripheral on **up to four
channels at once**, over `lp-fw/lp-ws281x` — the portable transmitter whose
sequencing (ping-pong refill, bit cursor, guard word) is tested on the host and
shared with every chip. Three layers:

| File | Owns |
|---|---|
| `rmt/s3_rmt.rs` | the chip: seven register operations, the `0x800` RAM offset, the S3's by-event-then-channel interrupt layout |
| `rmt/shared_driver.rs` | the single `Ws281xDriver` static and the IRAM interrupt trampoline that feeds it |
| `rmt/esp32s3_rmt_ws281x_driver.rs` | the `lpc-hardware` seam: endpoints, leases, open-time pin binding |

The number of channels offered comes from the **manifest** — one per
`/rmt/ws281xK` resource, four on this board — never from a literal in driver
logic. Each channel gets one 48-word memory block, which is what makes four
outputs possible at all: a 48-word window halves into exactly one LED, the
tightest refill deadline the hardware can pose.

Pins are bound at `open`, not at boot. An endpoint is a board label
(`ws281x:local:D10`) and which one a project drives is authored data, so the
channel is configured up front and its pad connected when the endpoint opens,
under the registry lease that grants exclusive use of that GPIO.

Timing is WS2812-class (GRB, 300 µs latch) on every channel. A strip wired in
another colour order is the fixture node's `color_order`, above this boundary —
the driver stays GRB, exactly like the C6's.

### LCD_CAM output (feature `lcd-ws281x`, not the default)

`src/output/lcd/` drives **up to eight lanes** from LCD_CAM in i8080 mode,
fed by GDMA. Build it with `just build-fw-esp32s3 lcd-ws281x`. The RMT driver
above stays the default until the switch is ruled on (S3 radio roadmap M9;
`docs/adr/2026-10-22-s3-parallel-ws281x-output.md`).

| | |
|---|---|
| Lanes | the manifest's `/lcd/ws281xK` resources (K < 8); the XIAO S3 Plus declares 8 |
| Endpoints | the RMT driver's: one per board-labelled pad (`ws281x:local:D3`), shared in `src/output/ws281x_endpoints.rs`, so a project opens on either |
| Wire | 2.5 MHz bus, three 400 ns slots a bit (T0H 400, T1H 800, period 1200 ns), GRB, 300 µs latch inside the buffer |
| Lockstep | every lane clocks out together; a frame lasts as long as the longest lane, short lanes are held low |
| CPU | the encode only (`lp_ws281x::parallel`), once per frame; no interrupt, no refill deadline, so a frame survives radio interrupts, masked critical sections and flash writes (`background_tx_safe() = true`) |
| RAM | two frame buffers in **internal RAM, by capability** (`alloc_caps(Internal)`: never `dram2_seg`, never PSRAM), 72 B per LED of the longest lane + 750 B each: 9,678 B per buffer at 5 × 124, 36,750 B at 8 × 500. Sized by what is open; an open that cannot get them fails with a sentence |
| Pins | refused at open: GPIO19/20 (USB-Serial-JTAG), GPIO26–32 (flash), GPIO33–37 (octal PSRAM) |
| Image | +4,676 B flash, −2,616 B internal RAM against the RMT image (2026-10-22) |
| Encode | 651 µs at 5 × 124, 3,564 µs at 8 × 500 (FS3, 240 MHz); up to 5.5 ms when the radio pre-empts it |
| ⚠️ RAM today | on the Bluetooth image the `Internal`-tagged heap is nearly full once a project loads: a 2 × 310-LED project could not get one 23,070 B buffer (FS3, 2026-10-22), while the same project plays on the RMT. Two buffers when they fit, one when only one does; M10 (PSRAM) is what frees the room |

The silicon evidence and the harness are the LED-channels spike's:
`test_lcd_ws281x` (LCD out, the CAM half of the same peripheral sampling the
same pads as a CPU-free logic analyzer; build it without `ble`,
`--no-default-features --features esp32s3,test_lcd_ws281x`, or its 8 × 500 case
runs out of the radio image's arena), and `lcd_ws281x_probe` (the check beside
Bluetooth and a Wi‑Fi scan loop; it brings its own LCD_CAM, so it cannot be
built with `lcd-ws281x`).

## Input

`src/hardware/button.rs` is a board-manifest-driven GPIO button driver, ported
from `fw-esp32c6`'s `Esp32GpioButtonDriver` — same driver id
(`esp32-gpio-button`), same manifest-driven endpoint enumeration
(`GpioInput` capability + board-assigned label), same internal-pull-up wiring.
The only chip-specific piece is the GPIO range check
(`board::esp32s3::constants::MAX_GPIO`, 48 versus the C6's 30). On this
board's manifest it exposes D0-D10.

## Building

```bash
just build-fw-esp32s3
```

Xtensa has no upstream Rust target, so this crate carries its own
`rust-toolchain.toml` with `channel = "esp"` — Espressif's fork, installed by
`espup`. That per-crate quarantine is required by
`docs/adr/2026-07-29-per-chip-fw-toolchains.md`: the rv32 crates stay on the
shared pinned nightly and are unaffected.

**Needs esp Rust >= 1.90** (the workspace MSRV). On 1.88 `lpc-model` genuinely
fails to compile — 70 × E0716 from the `Slotted` derive's const-promotion of a
temporary — so an MSRV error here means `espup update`, not a code problem.
The recipe also puts the toolchain's bundled GNU binutils on `PATH`, because
the Rust target spec links through `xtensa-esp32s3-elf-gcc`.

### The split image (what ships)

What Studio flashes, CI gates and a release ships is the **split image**
(`lp-fw/builds/esp32s3-8mb.json`, `"split": true`; layout 2), the C6's
shape on this chip — one link cut by reachability into a **core** and an
**engine**, a RAM-only loader at `factory + 0`, two boot records:

```bash
just fw-esp32s3-split          # target/fw-split/esp32s3-shipped: core.bin, engine.bin, app.bin, merged.bin, split.json
just fw-esp32s3-size-check     # the split headroom gate (64 KB), and the core's distance to its next 64 KiB page
just test-emu-esp32s3-split-boot   # ROM-up through the IDF door to the engine's hello; core high; a project rendered
```

```text
0x000000  IDF bootloader       espflash 3.3.0's bundled one (the S3's only door)
0x008000  partition table      factory 0x10000+0x600000, lpfs 0x610000+0x180000
0x010000  loader               lp-fw/fw-esp32s3-loader, RAM-only (≤ 0x5000 B)
0x015000  progress record      ┐
0x016000  boot record 0        │ layout 1's offsets (lp_bootctl)
0x017000  boot record 1        ┘
0x018000  reserved             erased, never written (the price of the 64 KiB page)
0x020000  core                 an ESP image the loader starts
 page↑    engine               its header first; MMU entries 0x100.., both views
  …       free to factory's end (read from the flashed table)
```

- **The door.** `main.rs` cuts the boot into `core_boot` and
  `lp_engine_entry`. A plain build (`just build-fw-esp32s3`, the dev image)
  calls the door directly; a split build (`LP_SPLIT_LINK=1`, the `lp_split`
  cfg) reaches it only through the engine header's `entry`
  (`src/split.rs`), so everything only the door reaches — the server, the
  node graph, the on-device GLSL compiler — links into the engine.
- **Everything a board needs to stay reachable lives in `core_boot`**: the
  link, the filesystem, and every radio's bring-up. A board whose engine is
  missing runs core-only: its link up, its watchdog fed, and the update
  channel served on USB and on every Bluetooth link (below).
- **The engine's two views.** The S3's one MMU table serves both buses at one
  index, so the core maps the engine's flash pages once at entry `0x100`: its
  header and rodata read at `0x3D00_0000`, its code fetched at `0x4300_xxxx`
  (`lp_bootctl::s3_mmu`). The core reads the engine header through the
  cache, the boot records through esp-storage's low-level ROM calls (after
  `FlashStorage::new`, never before), and `factory`'s end from the flashed
  table.
- **Updates over the air** (the S3 updates plan's M7 P8): the image announces
  `ota: {layout: 2}` (chip 2, layout 2 in `lpc-update`'s code table), and the
  board side is the C6's, shared — `fw_esp32_common::split_core`, over this
  chip's `S3Split` (`src/split_core/`): its MMU (the engine at entries
  `0x100..`, the core's own hash through its scratch entries `0x1E0..0x200`,
  never the `lpfs` window), the ROM flash calls behind the shared write fence,
  the SHA accelerator (software while a boot hash holds it), the S3 reset
  table (`ResetKind::from_s3_reason`) and the RMT update light (never USB
  19/20, the flash and PSRAM pads 26–37). The boot is the C6's:
  `split_core::begin` marks a trial **attempted** as `mount_filesystem` reads
  the partition table, before the mount and before Bluetooth; `split::split_boot`
  marks it **started**; the first boot after a USB flash hashes the engine once
  (the engine guard); a host link coming up **confirms** a trial; a board with
  no engine it may enter, an engine that keeps crashing, or a core on trial
  runs **core-only**, serving the update session on USB and on each Bluetooth
  link (the radio links decide `Update` mode there). The running engine's
  USB transport and its link mux hand channel 3 to the same session.
  Bluetooth carries channel 3 through the C6's shared code; its walk on
  silicon is P12's.
- **Octal flash takes no update** (Q7): an eFuse that says octal flash makes
  `S3Split::updates_off` say so — `[OTA] updates off: octal flash: …` at
  boot, layout 0 in the hello's manifest, nothing written (not a mark, not a
  record), and the engine run as a plain image.
- Test images only, never in a build def: `fixture-trial-dies`,
  `fixture-trial-hangs`, `fixture-usb-untrusted` (the C6's).
- Measured (`just fw-esp32s3-size-check`, 2026-10-22, default features,
  Bluetooth on, the update board side in): core 791,440 B, engine
  2,055,057 B, `app.bin` 2,973,696 B; steady headroom 3,318,895 B, update
  headroom 4,521,984 B; 60,528 B to the core's next page. **The split is
  layout-sensitive on Xtensa:** the reachability walk follows shared literal
  pools (GNU ld coalesces them across functions), so a link where an engine
  function's pool lands next to a core one drags engine code into the core —
  main at `ccfa116b0` carried ~400 KB of `lpc_engine`/`lpc_model`/`lpc_wire`
  in a 1,325,216 B core that way (`lp-fw-split why` shows
  `core_boot` → `btdm_controller_init` → a `.literal` section of
  `lpc_engine`'s). Compare cores across builds with that in mind.

How the split is built (GNU ld's quirks on Xtensa, the literal pools, the two
views) is `tools/lp-fw-split/README.md`; the loader is
`lp-fw/fw-esp32s3-loader/README.md`.

### Release profile

This crate builds under `[profile.release-esp32s3]` (`inherits = "release"`,
`opt-level = "s"`), not the workspace's default `opt-level = "z"`. The `"z"`
codegen was slow enough on the cold first frame to miss the RMT loopback
harness's 30 µs refill deadline on one channel — see
`docs/defects/2026-07-31-opt-z-missed-rmt-drain-deadline.md`. `"z"` stays the
default for the C6, whose flash budget is genuinely tight; the S3 has ~4.6 MB
of headroom in its 6 MB app partition, so trading ~104 KB of image size for
codegen the timing-critical driver was actually validated at is free here.

## Flashing

```bash
just flash-fw-esp32s3 /dev/cu.usbmodemXXXX
```

The port argument is optional but usually wanted: several boards are typically
on the desk bus and auto-detection picks the first match, not necessarily the
S3. The S3 speaks **USB-Serial-JTAG**, not a UART bridge, so it enumerates as
`/dev/cu.usbmodem*` and **its port number changes** each time the chip
re-enumerates after a reset.

**Never hardcode the port — identify it.** Ask each candidate what it is:

```bash
for p in /dev/cu.usbmodem*; do echo "-- $p"; espflash board-info --port "$p"; done
```

`Chip type: esp32s3` is the one you want. This is not hypothetical: during M3
the S3 sat on `usbmodem1101`, was unplugged and replugged, and came back on
`usbmodem1301` — with a **C6** now answering to `1101`. espflash refuses a
chip mismatch, so the failure is safe rather than silent, but the confusing
error costs more time than the loop above. Docs here use `usbmodemXXXX` as a
placeholder for that reason.

Before concluding a board is dead: a stray `espflash` holding this port wedges
it *uninterruptibly* (`ps` STAT `Us+`; `kill -9` does not land, because the
process is blocked reading a device node that went away when the chip
re-enumerated). Only a physical replug clears it. Check `pgrep -fl espflash`
first. Bare `espflash monitor` cannot attach to a running app at all — it
always tries to sync with the bootloader — so use the flash path above.

### Partitions

`partitions.csv` targets an **8 MB floor**: 6 MB `factory` + 1.5 MB `lpfs`,
ending at 0x790000 with 448 KB of slack. It no longer mirrors the C6's table
(3 MB + 960 KB = exactly 4 MB), which stays as it is because the C6's own
budget is genuinely tight.

**This encodes a board assumption: ≥8 MB of flash.** An N4 (4 MB) module
cannot flash this image, and that is deliberate. The reasoning, with sourcing
data, is in `docs/adr/2026-07-30-esp32s3-partition-floor.md`; the short
version is that N8R8 and N16R8 are the two most-sourced ESP32-S3 WROOM-1
variants, so 8 MB covers the realistic hardware while 16 MB would strand N8
boards and inherit the longest lead times. 4 MB was ruled out on measurement:
the C6's RISC-V image already sits at ~2.86 MB of a 3 MB partition, and Xtensa
has no compressed instructions.

A consequence worth stating: the S3's app image size is a **trend, not a
budget gate**. `just fw-esp32s3-size-check` exists so Xtensa code density
stays a tracked number — useful for the C6 and the classic ESP32, which *are*
constrained — not so anyone has to fight for space here.

Two espflash flags are mandatory, and forgetting either produces a confusing
failure rather than an obvious one.

**`--partition-table`.** espflash **silently** substitutes a default table
whose factory partition is only 1 MB if it is omitted; the boot skeleton fits
that, so the mistake stays invisible until the firmware grows past it. This
matters more now, not less: our table has diverged further from any default, so
a silent substitution fails later and more confusingly.

**`--flash-size 8mb`.** espflash writes a flash-size field into the image
header and **defaults it to 4 MB**, and the bootloader validates the partition
table against *that header*, not against the physical chip. Omit it and even
this 16 MB desk board boot-loops:

```
I (45) boot.esp32s3: SPI Flash Size : 4MB
E (56) flash_parts: partition 2 invalid - offset 0x10000 size 0x600000
       exceeds flash chip size 0x400000
E (66) boot: Failed to verify partition table
```

`espflash board-info` reports the *real* chip size and is the quickest way to
tell a header problem from a genuinely small board.

Both the `.cargo/config.toml` runner and the justfile recipes pass both flags.
The justfile carries the size once as `s3_flash_size`; the cargo runner
duplicates it because it cannot read a justfile variable. Those two and
`partitions.csv` must move together.

A correct boot prints:

```
I (46) boot.esp32s3: SPI Flash Size : 8MB
I (82) boot:  2 factory          factory app      00 00 00010000 00600000
I (89) boot:  3 lpfs             Unknown data     01 82 00610000 00180000
I (153) boot: Loaded app from partition at offset 0x10000
[INIT] fw-esp32s3 boot
```

### The memory ledger

On a project load, a project unload, a stop-all and a client `runtime_status`
— and **never** from the five-second heartbeat — the console prints the
classic's three-line heap ledger, in the classic's exact field names and
order, so that one comparator reads all three chips:

```text
[stack] heartbeat: high-water 21504 B of 37248 B (15744 B headroom)
[MEM] free=203112 used=42648 largest_free=196608 retry_saves=0
[JIT] used=0 peak=0 cap=0 spans=0 peak_spans=0 allocs=0 frees=0 fails=0 largest_free=0
```

(Figures illustrative; `37248` is this image's measured main stack.) Two
fields are **structural** on this chip and invent nothing: `retry_saves` is
always `0` because there is no OOM retry allocator here, and the whole `[JIT]`
line is zeros because the S3 JITs out of the heap through SRAM1's I-bus alias
rather than out of a reserved code region — so that residency is already
inside `[MEM]`'s `used`. See `esp32_memory_stats` in `src/main.rs` and
`src/stack_probe.rs`.

## The radio: Bluetooth, ESP-NOW and Wi-Fi (features `ble`, `espnow`, `wifi`)

The C6's radio, on this chip (S3 radio plan, M3 and M4,
`lp2025/2026-10-20-2335-s3-radio-and-ota`), from the shared crate
`lp-fw/fw-esp32-radio`, the C6's feature tiers:

- **`radio`**: esp-radio and the radio blobs' C heap; it starts nothing on
  its own.
- **`espnow`**: the radio's one bring-up (`esp_radio::wifi::new`, the Wi-Fi
  blob) and the ESP-NOW driver (the Radio node), with the C6's Wi-Fi rule (while the board
  is set to use Wi-Fi, a Radio node says why it is off and the rest runs).
  The image adds `/radio/0` to the hardware manifest itself
  (`HwManifest::with_resource`; the board profile leaves it out for the
  radio-less image) and says `svc.radio-espnow`.
- **`ble`**: the BLE link — lp-link over a Nordic-UART GATT service, one
  untrusted link per connection into the link mux. The controller is BTDM,
  not the C6's NPL, so its configuration is this chip's
  (`src/ble/ble_controller_config.rs`): three activities — the two link slots
  plus the advertiser that offers the second one (BTDM counts every
  advertising set and connection against `max_connections`; the C6's NPL
  counts connections only:
  docs/defects/2026-10-21-the-s3-stops-advertising-after-its-first-connection.md)
  — and the controller task on core 0. Logins draw their nonce from the
  hardware RNG (`fill_random`), as on the C6.
- **`wifi`**: the station, the IP stack, the secure LAN link at
  `ws://<board>/link`, mDNS `lp-xxxx.local` and the relay's device leg, on
  `lp-net` (12 KB, priority 1, pinned to core 0: `src/net/net_chip.rs`). The
  relay's address is a build fact (`LP_RELAY_HOST`, `src/net/relay_config.rs`).
  The relay client is the C6's (`fw_esp32_radio::net::relay_task`): it dials
  only when joined, Cloud relay is on and the board holds an account key.
  Emulated, `just test-emu-esp32s3-relay` (`lp-cli/tests/emu_relay_link.rs`'s
  S3 arm) and `just walk-wifi-emu relay --s3` walk it: registered, edited as
  `relay:<mac>@<origin>`, taken over on the LAN, busy, a deploy, Cloud relay
  off and on, a reset key refused. It uploads no project there: before PSRAM
  (M10) the image cannot compile `projects/test/basic` with a relay session
  open (docs/defects/2026-10-22-the-s3-wifi-image-runs-out-of-heap-with-a-project-loaded.md).
  On silicon it is owed until the join defect is fixed
  (docs/defects/2026-10-22-the-s3-wifi-image-never-authenticates.md).

All of it comes up in `core_boot`, after the filesystem, in the C6's order
(the radio, then BLE, then `lp-net`), so a core-only board keeps its radio.
BLE follows the C6's rule: **on unless the device store (`/.lp/access.json`)
turns it off** — a fresh board starts it. The radios' interrupts (priority 1,
CPU0) sit under the link thread's priority-1 lock (`src/io_thread.rs`). On a
split image the radio links serve the engine once `split_boot` enters it; a
core-only S3 serves nothing over the air yet (updates are the updates plan's).

**`ble` is on by default** since its desk walk passed on FS3
(`docs/reports/2026-10-21-s3-bluetooth-walk.md`): every S3 image starts the
BTDM controller unless the device store turns Bluetooth off, emulated ones
included (they advertise to nobody, as an emulated C6 does). **`espnow` is off
by default**, unlike the C6: the Wi-Fi blob up costs about 37 KB of heap the
S3 does not have to spare beside a 620-pixel project, and when it starts is
Yona's call (the ADR's 2026-10-22 amendment). **`wifi` is off by default**
until its desk walk passes:
build it with `just build-fw-esp32s3 wifi` or `just fw-esp32s3-split
esp32s3,server,float-f32,wifi`. A radio-free image is `--no-default-features
--features esp32s3,server,float-f32,…`; a `radio_boot_probe` image runs the
probe's radio in place of the product's (`build.rs`: `s3_radio`, `s3_ble`,
`s3_espnow`, `s3_wifi` are the features minus the probe, and the source gates
on those).

**Memory** is `docs/adr/2026-10-21-s3-radio-memory.md`: the radio blobs' own
C heap (`src/c_heap.rs`; 32 KB on a Bluetooth image, 48 KB with the Wi-Fi
blob up), the arena cut around the boot stages' RAM and
`dram2_seg` untagged — no radio block may land where a boot stage runs after a
warm reset, and every block is checked as it is handed out — the Wi-Fi IRAM
classes in flash, a 168 KB arena on a Bluetooth image and 136 KB with the
Wi-Fi blob up, a ~52–54 KB main stack. With both radios up
and no project the heap is 262,160 B in total and ~131 KB free (emulated),
against 319,504 / 287,940 without a radio. A radio image prints its regions at
boot (`[INIT] heap region: …`) and the radio region's high water at the
heartbeat (`[radio-heap] …`).

On FS3 with an antenna, Mac Chrome as the central (2026-10-21, the Bluetooth
walk record, a `ble` image without M4's ESP-NOW): `[ble] up: controller +33400
B, host + GATT +3520 B`; connected, the radio region's high water is 26,084 of
32,768 B (0 B overflowed to main) and the main stack's 29,312 of 54,296 B.
Frame rate on `/projects/studio` (Peach (1D)): 37 fps radio off, 35–36
advertising, 32 connected and editing, inside the radio frame-rate budget.

## How this differs from fw-esp32c6

Per the per-chip-toolchains ADR, the recovery strategy is **per chip** and does
not migrate into `fw-esp32-common`:

| | fw-esp32c6 | fw-esp32s3 |
|---|---|---|
| Toolchain | shared pinned nightly | `channel = "esp"` |
| Panic strategy | `panic=unwind` + `unwinding` | **`panic=abort`** (abort tier) |
| Unwind tables | `.eh_frame` retained via a build.rs patch | none — nothing to retain |
| Recovery | `catch_unwind` around node render | RTC ledger + reset |
| Panic-path allocation | boxes a payload for `begin_panic` | none |
| esp-sync reentrancy guard | needed — see below | not needed — see below |
| Panic with no ledger installed | hangs in place | resets anyway |
| Linker | rust-lld, `-Tlinkall.x` | GNU ld, `-Wl,-Tlinkall.x` |

That is why this crate's `build.rs` is so small: apart from the wire-hello build
provenance and the `fw_harness` cfg, the C6's exists mostly to patch esp-hal's
`eh_frame.x`, which is meaningless without unwinding.

The last three rows all follow from the same fact: this panic handler allocates
nothing, so it cannot re-enter `esp-alloc`'s non-reentrant lock the way the C6's
`Box`-ing handler can. `src/recovery/panic_path.rs` carries the full reasoning,
including what replaces the C6's `is_esp_sync_reentrant_lock_panic` guard.

⚠️ The RTC watchdog (`src/recovery/watchdog.rs`) is armed by `boot_firmware`
immediately next to the `io_task` spawn, and nowhere else. Its feed policy is
deliberately conditional on a live `io_task`, so arming it anywhere the I/O task
does not yet exist would boot-loop the board every 8 s.

## The app layer

The default build is the LightPlayer app: `LpServer` over USB-Serial-JTAG,
the tree store on the `lpfs` partition, and abort-tier recovery.

### The filesystem: the tree store on `lpfs`

`lpfs` (1.5 MB at `0x610000`, 384 sectors of 4 KiB) holds the tree store
(`lp-tree-store`, the `fs-tree` feature, which `server` implies), as on the
C6 since its 2026-10 cutover; littlefs is gone from the image. The C6 is the
model, and the differences are only the chip's:

- **Reads go through the flash cache.** `src/tree_flash.rs` maps the
  partition at layout 2's `lpfs` window (`lp_bootctl::s3_mmu::LPFS_WINDOW`,
  MMU entries `0x1C0..0x1E0`, `0x3DC0_0000`, up to 2 MiB — the loader's
  scratch, unmapped before it jumps) and the store reads it as memory. After
  every program and erase the written range is dropped from the cache with
  the mask ROM's `Cache_Invalidate_Addr`; if the ROM ever refuses, reads fall
  back to ROM calls for the rest of the boot. Writes go through
  `fw_esp32_common::aligned_nor_flash` over esp-storage, as on the C6.
- **Records are hashed in software** (`lp_tree_store::SoftSha256`), not with
  the S3's SHA accelerator.
- **Octal flash is refused.** A part whose eFuse says octal flash serves the
  memory filesystem and says so (`[FS] octal flash: …`); the store has only
  been read on quad parts.

What it cost (`just fw-esp32s3-size-check`, against `main` at `05f3ab150`
with littlefs): the monolithic image grew 21,840 B (2,431,712 →
2,453,552 B). In the split image the core grew 28,848 B (the store mounts in
`core_boot`) and the engine shrank 5,000 B; the core now sits 17,248 B below
its next 64 KiB page (46,096 B before).

**A littlefs `lpfs` is held, never formatted.** A board that still has
littlefs files (the S3s flashed before M7 P9) boots with `fs: legacy_held`
and an in-memory filesystem: its files are untouched. There is no migration
(the S3 updates plan's ruling L7: the only S3 is the bench one). To adopt the
tree store, erase `lpfs` over USB (lease the board first; `board take`
prints its port) and re-push the projects:

```bash
espflash erase-region --port <port> 0x610000 0x180000
```

The next boot finds blank flash, formats the store (`[FS] tree store
formatted (384 sectors, …)`) and serves it.

### The link IO thread

**`io-thread`** (default on): the shared USB link task
(`fw-esp32-common::usb_link`) runs on its own esp-rtos thread — priority 1,
pinned to **core 0** (never `None`: an unpinned task can wake on core 1 and
raise that core's own SWI1), a 4 KB stack, its own embassy executor
(`src/io_thread.rs`, a per-chip copy of the C6's — `fw-esp32-common` names no
esp-hal thread API). The link is shared behind a priority-1
`RawPriorityLimitedMutex` (`UsbLinkShared::leak_locked`), and the server
answers a tick's requests before it renders
(`set_messages_first(true)`, only with the thread). The feature needs
`esp-rtos/esp-radio` **and** `esp-rtos/esp-alloc` — it links esp-rtos's small
esp-radio glue (the public `task_create` call lives there), not a radio
stack. Without the feature the old arrangement still builds: one thread-mode
executor runs both the link and the server loop, render first.

`io_thread_stack_diag` (off, never shipped) paints the thread's stack and
logs `[iostack] high-water …` at the heartbeat; the Xtensa port reads `sp`
from `a1` and scans the esp-rtos `Task` record, a scan that lives only in
this diagnostic feature. See
`docs/adr/2026-10-02-c6-link-io-thread.md`'s S3 amendment for the numbers,
the lock and the core-1 follow-up.

Two dependency lines carry almost all of its size, and both are choices a
careless edit would silently reverse:

| Line | Choice | Why it is written that way |
|---|---|---|
| `lpa-server` | `default-features = false`, **exactly two `node-*` gates** | each gate's runtime is 3–85 KB (see `lpc-engine/README.md`); the two listed are the two the board can actually run |
| `lp-gfx-lpvm` | the real JIT backend, **no ISA feature named** | the ISA is chosen by `target_arch`; naming one is how you pay for a backend the chip cannot execute (+26,448 B measured on the C6) |

The two enabled node kinds are `node-shader` and `node-fixture`. The second is
not decoration: `OutputNode` consumes a **control** product, `ShaderNode`
produces a **visual** one, and `FixtureNode` is the only runtime that converts
between them. A shader-only build compiles GLSL on device and still cannot show
it.

`lps-glsl` appears in `cargo tree` and is now genuinely linked — this build
compiles GLSL on the board.

### Verifying the render without LEDs

> **The serial readout driver is gone.** `src/output/readout_driver.rs` printed
> the frame bytes instead of driving LEDs, and it was replaced by the real RMT
> driver (above) when four-channel output landed. `scripts/m4-hardware-walk.sh`
> and `lp-app/lpa-server/tests/shader_oracle_frame.rs` still describe its
> `[OUT] dump` lines; that comparison needs a new source of frame bytes before
> the walk can be re-run.

`projects/test/shader-oracle` is the project built for that comparison. It is
deliberately clock-free, so every frame is identical and no time
synchronisation is needed, and it is sized to 64 LEDs so the readout's one-shot
dump covers the **whole** frame rather than a prefix. Its output node turns the
display pipeline's LUT, dithering, interpolation and brightness off, which
collapses that pipeline to a stateless `(v + 0x80) >> 8` — so any difference
between host and device is the shader, not the pipeline.

```bash
scripts/m4-hardware-walk.sh            # flash, push, render, compare
```

The walk renders the same project on two host engines and diffs both against
the board. Measured 2026-07-30 on the desk S3: **192 of 192 bytes identical**
across all three.

The two host engines are not redundant. `[ORACLE]` is wasmtime; `[ORACLE-RV32]`
is `lpvm-native`'s rv32 emulation — the *same code generator* the S3 JITs, one
ISA over. When the three disagree, which pair agrees is the entire diagnosis:
device+rv32 against wasmtime is a native-versus-wasm difference that the C6
shares, while a device disagreeing with both is Xtensa-specific. See
`docs/defects/2026-07-30-q32-native-vs-wasmtime-last-bit.md`.

## Board profile

The compiled-in fallback manifest is
`lp-core/lpc-hardware/boards/seeed/xiao-esp32-s3-plus.json`
(`default_esp32s3_hardware_manifest()`), matching the desk board. Until
2026-07-30 this crate fell back to the **C6's** profile and logged
`hardware manifest: seeed/xiao-esp32-c6`; the C6 pin map is wrong for this chip
in every particular, so a real output driver must never inherit it.

The profile is deliberately partial, because a missing entry is a gap and a
wrong GPIO number is a short circuit:

| Absent | Why |
|---|---|
| User LED | **Tested and not found on GPIO21.** Seeed documents GPIO21 for the plain XIAO ESP32-S3, and espboards.dev says GPIO22 for the *Plus*, which is impossible (the ESP32-S3 numbers GPIO0-21 and GPIO26-48). Driving GPIO21 in a 3-blink/pause pattern for 20 s on the desk board produced **no visible change**, so GPIO21 is ruled out for this variant. The pin stays out of the profile. Settling it properly wants the Seeed schematic or an S3 `test_gpio_calibrate` harness (see "Not yet ported"), not another guess — the yellow LED on this board appears to be a power/charge indicator unrelated to any GPIO we drive. |
| The nine 1.27 mm castellated pads | No published GPIO map. |
| GPIO26-37 | In-package flash and octal PSRAM. Never claimable. |
| `/radio/0` | No radio driver is registered here, so the resource would never open. |

`/gpio/19` and `/gpio/20` **are** listed, reserved: they are USB-Serial-JTAG
D-/D+, and driving them drops the host link until a physical replug — the S3's
version of the C6's GPIO12/13 trap.

A `/hardware.json` on the device overrides the compiled-in profile, which is how
a different S3 carrier board gets described without a rebuild.

## Workspace notes

A workspace **member** (so it shares workspace dependencies and the lockfile)
but not in `default-members`, and excluded from `clippy-host` — exactly like
`fw-esp32c6`. Both are cross-target-only, and including this one in host
clippy triggers a real `critical-section` feature-unification conflict
(multiple `restore-state-*` features) rather than a mere build failure.
