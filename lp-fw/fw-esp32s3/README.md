# fw-esp32s3

LightPlayer firmware for the **ESP32-S3** (Xtensa LX7). Sibling of
`fw-esp32c6` (RISC-V).

> **Not yet a consumer of `fw-esp32-common`.** An earlier version of this file
> called the crate its "third consumer"; that was wrong at the dependency
> level and is corrected here. What was actually proven during M5 is weaker
> but still useful: `fw-esp32-common` *compiles* for `xtensa-esp32s3-none-elf`
> pulling no `esp-hal`, so the chip-agnostic seam holds against a second
> architecture. Wiring it in is the app-layer milestone's job.

Currently a **boot skeleton plus hardware harnesses**: clocks, heap, and serial
logging far enough to print the `[INIT]` marker family, and a JIT corpus runner
(below). The server, storage, and output stacks are the app-layer milestone of
`~/.photomancer/planning/lp2025/2026-07-30-s3-app-layer-with-jit/`; the Xtensa
backport roadmap that created this crate is closed.

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
littlefs on the `lpfs` partition, and abort-tier recovery.

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

The serial readout driver (`src/output/readout_driver.rs`) prints the frame
bytes instead of driving LEDs: one full hex dump when a channel opens or
resizes, then a checksum and lit-LED count once a second. That makes the render
path *comparable*, which an LED never is.

`examples/shader-oracle` is the project built for that comparison. It is
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
