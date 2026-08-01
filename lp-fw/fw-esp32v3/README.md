# fw-esp32v3

LightPlayer firmware for the **classic ESP32** ("v3", WROOM-32E, Xtensa LX6).
Sibling of `fw-esp32s3` (Xtensa LX7) and `fw-esp32c6` (RISC-V).

Runs the LightPlayer app: `LpServer` over a **UART0** transport, backed by a
littlefs filesystem in the `lpfs` partition. Built through M2 (crate, 4 MB
partitions, RAM ledger, CI) and M3-P1 (app layer) of the classic-ESP32
bring-up roadmap
(`~/.photomancer/planning/lp2025/2026-07-31-1444-classic-esp32-bringup/`).

Still absent: on-device shader **execution** (M3-P2 — a shader compiles here
and cannot yet run, see "JIT status" below), WS281x output (M4), the
`lp-recovery` crash ledger (M7), and radio (measure-only, behind
`radio_ram_probe`).

## Workspace shape

A repo-root workspace **member** excluded from `default-members` — the same
shape as `fw-esp32c6` and `fw-esp32s3` (see `fw-esp32s3/README.md`'s
"Workspace notes").

M2-P1 originally stood this crate up as its **own** workspace, because it then
had zero lp2025-internal path dependencies and so needed nothing from the root
`workspace.dependencies` or `[patch.crates-io]` tables. M3-P1 made it a real
`fw-esp32-common` / `lpa-server` / `lpc-hardware` consumer, which is exactly
the condition that decision named for revisiting it: path dependencies reaching
into the root workspace from *outside* it resolve their own copies of every
shared crate and silently ignore the root `[patch]` forks. The cost — a
`--exclude fw-esp32v3` entry in the justfile's `clippy-host`, mirroring the
other two firmware crates — is now paid.

Build and lint commands still `cd` into this directory: `.cargo/config.toml`
here selects the Xtensa target, the linker flags and the espflash runner, and
cargo reads that file from the CWD upward. Artifacts land in the shared root
`target/`. `[profile.release-esp32v3]` lives in the **root** `Cargo.toml`,
next to `release-esp32s3` — cargo only honours profiles on the workspace root
manifest.

## Building

Xtensa has no upstream Rust target, so — like `fw-esp32s3` — this crate
carries its own `rust-toolchain.toml` with `channel = "esp"` (Espressif's
fork, installed by `espup`), per
`docs/adr/2026-07-29-per-chip-fw-toolchains.md`.

```bash
just build-fw-esp32v3        # or, by hand:
cd lp-fw/fw-esp32v3
# Put the esp toolchain's bundled GNU binutils on PATH first if
# xtensa-esp32-elf-gcc isn't already resolvable — e.g.
#   export PATH="$HOME/.rustup/toolchains/esp/xtensa-esp-elf/esp-*/xtensa-esp-elf/bin:$PATH"
cargo build --profile release-esp32v3
```

Other build shapes (all covered by `just clippy-fw-esp32v3`):

| Command | Entrypoint |
|---|---|
| default | server app over UART0 |
| `--no-default-features --features esp32` | M2-P1 boot-to-hello skeleton |
| `--no-default-features --features esp32,radio_ram_probe` | M2-P3 radio RAM ledger |

Measured 2026-07-31 with the app layer in: **1,675,120 B** image against the
3 MB `factory` partition (53%), and the DRAM split below.

### Release profile

Builds under `[profile.release-esp32v3]` (`inherits = "release"`,
`opt-level = "s"`), defined in the **repo-root** `Cargo.toml` next to
`release-esp32s3` (cargo honours profiles only on the workspace root
manifest). `"s"` rather than the repo's default `"z"` mirrors `fw-esp32s3`,
whose "z" build missed a 30 µs cold-frame WS281x deadline on the *faster*
LX7; this crate has no timing-critical driver yet, so revisit with real
measurements when output lands (M4).

`"z"` is not merely unvalidated here — it does not build. `esp-storage`'s
build script hard-errors on the classic ESP32 below `opt-level` 2/3/s, which
is why `just clippy-fw-esp32v3` passes `--profile release-esp32v3` where
`clippy-fw-esp32s3` gets away with `--release`.

## DRAM budget

esp-hal's `dram_seg` for this chip is `0x3FFB_0000..0x3FFE_0000` — **192 KB**
(the S3 has 341,760 B) — and `.data`, `.bss` (which holds the `esp_alloc`
arena) and `.stack` all come out of it. `.stack` gets the remainder, so the
heap constant in `src/main.rs` is one side of a zero-sum split.

Measured on the linked app image (2026-07-31, `HEAP_SIZE = 110 KB`):

| Section | Bytes |
|---|---|
| `.data` | 15,212 |
| `.bss` (incl. 112,640 B arena → ~21.6 KB static) | 134,256 |
| `.stack` | 47,136 |
| total | 196,604 of 196,608 |

Free heap at idle, read off the device heartbeat: **103,916 B free /
8,724 B used**. For scale, fw-esp32s3's first on-device GLSL compile OOM'd at
a 96 KB heap and needed 240 KB — a number this chip cannot reach at any
setting. That measurement is what gate G-M3 of the bring-up roadmap exists to
evaluate.

Raising `HEAP_SIZE` further trades stack 1:1, and the link only fails once
`.stack` would go negative: 160 KB overshoots by 4,064 B
(`stack.x:11 cannot move location counter backwards`), putting the hard limit
at ≈155.9 KB — where the stack is zero and the board cannot run. So "as high
as it links" is *not* the real ceiling. 110 KB is the setting that keeps
`.stack` near fw-esp32s3's proven 52,896 B, which the Xtensa windowed ABI's
large frames and the recursive GLSL parser both want.

The tempting next lever is esp-hal's `dram2_seg` (`0x3FFE_7E30`, 98,768 B) as
a second `esp_alloc` region. **It overlaps
`lpvm_native::codemem_esp32::CodeRegion::ESP32_DEFAULT`**
(`0x3FFE_8000..0x3FFF_F000` D-bus), so any such region must stop below
`0x3FFE_8000` or the allocator and the JIT will hand out the same bytes.

## Flashing

```bash
espflash flash --chip esp32 --partition-table partitions.csv --flash-size 4mb \
  --monitor --after hard-reset --port /dev/cu.wchusbserial1140 \
  target/xtensa-esp32-none-elf/release-esp32v3/fw-esp32v3
```

(`.cargo/config.toml`'s `runner` does the same thing when you `cargo run`.)

**Always pass `--port` explicitly.** The desk board is a **DOM-Z-102**
(domraem carrier, classic ESP32 rev v3.1, 4 MB flash) at
`/dev/cu.wchusbserial1140` as of 2026-07-31 — but several boards are
typically on the desk bus at once (an S3 session may be sharing it), and its
CH340K USB-UART bridge enumerates under a port name that is only stable per
physical hub location. Re-verify before flashing:

```bash
espflash board-info --port /dev/cu.wchusbserial1140
```

`Chip type: esp32` is the one you want.

### macOS driver note

The DOM-Z-102's CH340K bridge needs the **WCH CH34x VCP driver** installed
on macOS before it enumerates as a `/dev/cu.wchusbserial*` device at all —
unlike the S3 (USB-Serial-JTAG, no driver needed) or the C6. After
installing the driver, macOS may block the kernel extension under
**System Settings → Login Items & Extensions** until you explicitly allow
it. See the planning roadmap's notes.md and memory
`classic-esp32-ch340k-macos-driver` for the full procedure; this is a
one-time machine setup step, not a per-flash one.

Two espflash flags are mandatory here too, for the same reasons `fw-esp32s3`
documents: `--partition-table` (without it espflash silently substitutes a
1 MB-factory default table) and `--flash-size` (espflash defaults the image
header's flash-size field to 4 MB regardless of the physical chip, and the
bootloader validates the partition table against *that header*). This
board's table genuinely is the 4 MB shape below, so `--flash-size 4mb`
should match reality — naming it explicitly still turns a mis-flashed board
into a loud espflash refusal instead of a confusing boot-loop.

## Partitions

`partitions.csv` copies `fw-esp32c6`'s **4 MB** shape verbatim (Q7 in the
bring-up roadmap): this chip's flash budget is constrained like the C6's,
not the S3's 8 MB floor.

| Partition | Offset | Size |
|---|---|---|
| `nvs` | `0x9000` | 24 KB |
| `phy_init` | `0xf000` | 4 KB |
| `factory` (app) | `0x10000` | 3 MB |
| `lpfs` (spiffs) | `0x310000` | 960 KB |

Ends exactly at `0x400000` (4 MB) — no slack. An ADR for this table lands
with a later milestone (M6 in the roadmap), not here.

## UART baud gotcha

Classic ESP32 has **no USB-Serial-JTAG** peripheral (unlike the S3), so this
crate uses `esp-println`'s `uart` feature instead of `jtag-serial`. That
feature writes UART0's TX FIFO directly but **never programs the baud
divisor itself** — the ROM bootloader leaves a divisor set for its own
(pre-reclock) clock tree, and after `esp_hal::init()` reclocks to
`CpuClock::max()`, the stale divisor makes every `esp_println!` print
garbage at any standard host baud. This was diagnosed and fixed on real
classic-ESP32 hardware in the experiment repo (`fw/xt-runner-esp32`,
FINDINGS.md "C1"): construct an `esp_hal::uart::Uart` on UART0 once at boot
(115200 8N1, TX=GPIO1, RX=GPIO3). `board::esp32v3::init::init_board` does
exactly that, before the first `esp_println!` — and since M3-P1 that binding
is no longer a keep-alive hack, it is the actual server transport.

## Host link

The server speaks the same `M!`-prefixed line protocol as the other two
firmwares, over UART0 at 115200 8N1 instead of USB-Serial-JTAG. See
`src/serial/io_task.rs` for the two places the byte layer had to differ from
fw-esp32s3's copy (no connection monitor; RX drained between TX chunks so a
long write cannot overflow the 128-byte RX FIFO).

Round-trip verified on the desk DOM-Z-102, 2026-07-31:

```console
$ printf 'M!{"id":7,"msg":"hello"}\n' > /dev/cu.wchusbserial1140
M!{"id":7,"msg":{"hello":{"proto":4,"fw":{"package":"fw-esp32v3",...}}}}
```

⚠️ **`lp-cli` cannot connect to this board yet** — it fails the readiness gate
with `Incompatible { FrameBeforeHello }`. `lpa_client::transport_serial`'s
`reset_after_open` performs the ESP32-C6 USB-JTAG-serial reset dance, which
does not reset a CH340K-bridged classic ESP32 (it never asserts DTR, which
that bridge's two-transistor auto-reset circuit needs); the client therefore
attaches to a still-running device that sent its unsolicited hello minutes
ago. Host-side work, tracked for M3-P2/P3, which need a project push.

## What's deliberately not here yet

- **No shader execution.** `lp-gfx-lpvm` is linked, so a shader *compiles* on
  device — but classic DRAM has no I-bus view, so `rt_jit`'s in-place buffer
  is not fetchable and the first call to compiled code faults. The placed
  path (`lpvm_native::codemem_esp32` + `JitBuffer::Placed` +
  `link::link_jit_at`) is wired in M3-P2. See the ⚠️ block at the
  `TargetLpvmGraphics::new` call site in `src/main.rs`.
- **No `lp-recovery` RTC crash ledger and no RWDT.** The panic handler is
  print-and-reset only — the compiler-level posture (`panic=abort`, no
  unwinding) already matches `fw-esp32s3`'s abort tier, so adding the ledger
  later is additive, not a rewrite. Consequences today: no safe-mode gate on
  project auto-load (a project that crashes the boot keeps crashing it), and
  no watchdog backstop for a hung frame loop. The backend needs
  classic-specific RTC-fast-RAM and `SocResetReason` constants; M7.
- **No output driver.** No WS281x driver is registered on the
  `HardwareSystem`, so the manifest's four `/rmt/ws281xK` endpoints resolve to
  nothing and the output node fails cleanly. M4.
- **No radio in the app build.** Linked only behind `radio_ram_probe`, whose
  M2-P3 ledger measured 44,244 B of heap + ~390 KB flash for it.
