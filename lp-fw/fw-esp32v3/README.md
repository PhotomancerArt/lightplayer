# fw-esp32v3

LightPlayer firmware for the **classic ESP32** ("v3", WROOM-32E, Xtensa LX6).
Sibling of `fw-esp32s3` (Xtensa LX7) and `fw-esp32c6` (RISC-V).

Runs the LightPlayer app: `LpServer` over a **UART0** transport, backed by a
littlefs filesystem in the `lpfs` partition. Built through M2 (crate, 4 MB
partitions, RAM ledger, CI) and M3-P1 (app layer) of the classic-ESP32
bring-up roadmap
(`~/.photomancer/planning/lp2025/2026-07-31-1444-classic-esp32-bringup/`).

WS281x output landed in M4-P1: up to four concurrent RMT channels, sourced
from the board manifest — see "WS281x output" below.

The `lp-recovery` RTC crash ledger landed in M4-P2, pulled forward from M7
because the WS281x fault could not report itself over serial — see
`src/recovery/`. Safe mode, boot reports and the blame ledger all work; the
**RWDT is still M7**.

Still absent: radio (measure-only, behind `radio_ram_probe`). See "JIT status"
below for where on-device shader execution stands.

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
| `--features ws281x_telemetry` | app + periodic WS281x counter log (see below) |

Measured 2026-08-01 at M6, with the app layer, WS281x output and the recovery
ledger in: **1,707,792 B** image against the 3 MB `factory` partition —
**1,437,936 B of headroom, 46 % free**.

⚠️ **Flash is not this chip's constraint; RAM is.** At four channels the heap
has ~7 KB free where flash has 1.4 MB. LED-count claims must be quoted from
the RAM ledger, never from the RMT channel count — the measured ceiling is
**~240 LEDs comfortable / ~300 at the edge / 400 impossible**, at ≈89.5 B per
LED. Full flash and RAM ledgers, the cross-chip comparison and the
serde-surface go/no-go are in
[`docs/adr/2026-08-01-esp32v3-flash-budget.md`](../../docs/adr/2026-08-01-esp32v3-flash-budget.md).

### Release profile

Builds under `[profile.release-esp32v3]` (`inherits = "release"`,
`opt-level = "s"`), defined in the **repo-root** `Cargo.toml` next to
`release-esp32s3` (cargo honours profiles only on the workspace root
manifest). `"s"` rather than the repo's default `"z"` mirrors `fw-esp32s3`,
whose "z" build missed a 30 µs cold-frame WS281x deadline on the *faster*
LX7. As of M4 that reasoning applies here for real: the profile is part of
the RMT timing contract. This chip's deadline is a roomier 80 µs (64-word
halves), but the cold first frame is checked on silicon rather than assumed —
M4-P4.

`"z"` is not merely unvalidated here — it does not build. `esp-storage`'s
build script hard-errors on the classic ESP32 below `opt-level` 2/3/s, which
is why `just clippy-fw-esp32v3` passes `--profile release-esp32v3` where
`clippy-fw-esp32s3` gets away with `--release`.

## WS281x output

`src/output/` — the same three-layer split `fw-esp32s3` uses:

| Layer | File | Knows about |
|---|---|---|
| chip | `output/rmt/v3_rmt.rs` | RMT registers, RAM at `0x3FF5_6800`, the `INT_*` bit layout. Implements `lp_ws281x::RmtHw`. |
| sequencing | `output/rmt/shared_driver.rs` | the one `Ws281xDriver` static, the IRAM interrupt trampoline, the optional telemetry tap |
| seam | `output/rmt/esp32v3_rmt_ws281x_driver.rs` | `lpc-hardware` endpoints, leases, open-time pin binding |

Ported from the experiment repo's hardware-validated classic backend
(`2026-esp32s3-experiment`, `fw/led-lab-esp32/src/esp32_rmt.rs`). All
sequencing stays in `lp-ws281x`, whose host suite (`cargo test -p lp-ws281x`)
is the regression net; this crate adds no trait surface
(ADR `2026-07-31-lp-ws281x-multi-channel-driver-adoption`).

### The block plan, and where the four outputs come from

The RMT block plan is **computed at driver init from the board manifest's
declared `/rmt/ws281xK` count** (`v3_rmt::plan_for_declared`): each declared
channel gets `floor(8 / count)` of the chip's eight 64-word blocks, capped at
four (`tx_lim` is 9 bits — a 512-word window cannot be expressed). The
DOM-Z-102's four declared channels get two blocks each — slots `0, 2, 4, 6`
own memory, with 128-word windows halving into 64-word (80 µs) refill
deadlines, the exact split the old `BLOCKS_PER_CHANNEL = 2` constant
produced. A one-channel manifest gets a 256-word window (160 µs deadlines).

Two blocks for four outputs is not a taste call. The classic's *delivered*
interrupt rate saturates around 48 k/s regardless of demand (experiment
`findings.md` §12 — this is the root cause of the equal-start truncation
defect, and staggering does not fix it). At one block per channel each busy
output demands 25 k refills/s, so the chip runs out at **two**. Two blocks
halves the demand to 12.5 k/s and reaches four — validated on silicon at
G-M4, re-baselined with telemetry in the RMT-priority plan's P4.

**Absorbed slots are skipped by construction.** Manifest channel `K`
(`/rmt/ws281xK`) resolves to the plan's `K`-th available slot, and the
channel-creation loop only ever hands esp-hal a slot that owns memory. The
experiment harness did *not* do this — it kept asking for channels 0,1,2,3
and got `MemoryBlockNotAvailable` for the odd ones, which is why its
two-block configuration never ran.

Channel **count** comes from the board manifest and nowhere else: the
DOM-Z-102 declares four `/rmt/ws281xK` resources, and the endpoints offered
are its board-labelled GPIOs (IO18/IO16/IO14/IO2 → `ws281x:rmt:IO18` and so
on). No GPIO number appears in driver logic; the pin arrives with the
endpoint and is bound at `open` under a registry lease.

### Telemetry (`--features ws281x_telemetry`, off by default)

`lp-ws281x` keeps per-channel counters — guard trips, guard skips, TX errors,
refill-lag sum/count/max and a 9-bucket lag histogram — and this feature
prints them over the serial link, one line per configured channel roughly
every 10 s, from the frame-write path (never from the ISR):

```
[WS281X] t_ms=… ch=0 half=64 frames=… complete=… trips=… skips=… errors=… \
         refills=… wanted=… lag_avg=6.9 lag_max=21 over_half=0 hist=a:b:…:i
```

Read **`refills` against `wanted`** first — a refill that never arrives
leaves no lag sample behind, so `lag_max` can look comfortable while a third
of the frames truncate. `trips` is the direct truncation count. `wanted` is
`frames × ceil(total_bits / half)`, i.e. what an untruncated frame set would
have needed.

Off by default so the shipping image spends nothing on it: the module is
`cfg`'d out and the call site becomes an empty `#[inline(always)]` fn — not
even the timer read survives. `just clippy-fw-esp32v3` lints the feature on,
so it cannot rot.

### Frame dump (`--features frame-dump`, off by default)

The other tap on the same write path, answering a different question: not
"did the refills keep up?" but "were the pixels *right*?". A `frame-dump`
build prints one full hex dump per channel after open or resize, then a
checksum-and-lit-count summary about once a second:

```
[OUT] open endpoint=… bytes=192 leds=64 (frame-dump build)
[OUT] dump frame=1 leds=64 shown=64 crc=0x55772254 rgb=324a0208…
[OUT] frame=60 leds=64 crc=0x55772254 lit=64 first=(50,74,2) (8,55,106) …
```

Those line shapes are a **byte-for-byte port of `fw-esp32s3`'s**, deliberately:
`scripts/m4-hardware-walk.sh` and `lp-app/lpa-server/tests/shader_oracle_frame.rs`
parse both chips' transcripts with no per-chip branch, and the walk's whole
claim is that the two hex strings are equal. Change a format string here and
you must change it in all three places.

This is the M7 FINAL-gate instrument — "a shader compiles on-device into the
fixed SRAM1 code region and renders bit-exactly vs the host oracle". Run it
once the board is free:

```bash
just ci-prereqs                              # the oracle's rv32 engine needs this
scripts/m4-hardware-walk.sh --chip esp32     # or: ... --chip esp32 /dev/cu.wchusbserialNNNN
```

The walk flashes with `frame-dump`, pushes `examples/shader-oracle` (retargeted
from the XIAO's `D10` pad to this board's `IO18` — an endpoint picks a wire, not
a colour), reflashes to watch the device compile and render it, and diffs the
device's `rgb=` against the host's. Off by default for the same reason the
telemetry is: hex-formatting frames costs render time and floods the 921600-baud
UART0 the transport is also using. `just clippy-fw-esp32v3` lints it on.

## DRAM budget

esp-hal's `dram_seg` for this chip is `0x3FFB_0000..0x3FFE_0000` — **192 KB**
(the S3 has 341,760 B) — and `.data`, `.bss` (which holds the `esp_alloc`
arena) and `.stack` all come out of it. `.stack` gets the remainder, so the
heap constant in `src/main.rs` is one side of a zero-sum split.

Measured on the linked app image (2026-08-01, `HEAP_SIZE = 110 KB`, WS281x
output in):

| Section | Bytes | Δ vs M3-P1 |
|---|---|---|
| `.data` | 18,924 | +3,712 |
| `.bss` (incl. 112,640 B arena → ~21.6 KB static) | 134,280 | +24 |
| `.stack` | 43,400 | **−3,736** |
| total | 196,604 of 196,608 | — |

⚠️ **The WS281x driver was paid for out of the stack**, because the split is
zero-sum and `HEAP_SIZE` did not move: `.data` grew by 3,712 B (the shared
`Ws281xDriver` static with its eight `ChannelState`s, plus driver constants)
and `.stack` shrank by the same. 43,400 B is now well under fw-esp32s3's
proven 52,896 B. If a deep call — the recursive GLSL parser, a windowed-ABI
spill storm during an on-device compile *while frames are going out* —
overflows, `HEAP_SIZE` in `src/main.rs` is the knob that gives the stack back,
and M3's ledger says there was 18.8 k of heap headroom at render to spend.

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
bring-up roadmap). **Ratified at M6** —
[`docs/adr/2026-08-01-esp32v3-flash-budget.md`](../../docs/adr/2026-08-01-esp32v3-flash-budget.md) —
though the reasoning that produced it turned out to be wrong in a harmless
direction: the guess was that this chip is flash-constrained like the C6, and
it is not. The measured image is within 8,720 B of the **S3's**, not the C6's
(both are Xtensa and abort tier; the C6 is 1.15 MB larger). 3 MB is kept
because it is ample, not because it is tight, and unlike the S3 this chip
narrows supported hardware not at all — any 4 MB N4-class module runs it.

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
- **No radio in the app build.** Linked only behind `radio_ram_probe`, whose
  M2-P3 ledger measured 44,244 B of heap + ~390 KB flash for it.
