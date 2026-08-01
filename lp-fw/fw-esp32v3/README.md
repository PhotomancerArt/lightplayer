# fw-esp32v3

LightPlayer firmware for the **classic ESP32** ("v3", WROOM-32E, Xtensa LX6).
Sibling of `fw-esp32s3` (Xtensa LX7) and `fw-esp32c6` (RISC-V).

P1 of the classic-ESP32 bring-up roadmap
(`~/.photomancer/planning/lp2025/2026-07-31-1444-classic-esp32-bringup/`):
**crate skeleton only**. Boots to a serial hello and a once-per-second
heartbeat; nothing else. No `lp-server`, no littlefs, no WS281x output, no
radio, no board manifest — those are later phases (P2 hardware boot, P3 RAM
measurement, P4 CI + size-check, P5 board manifest + docs).

## Workspace shape

This crate is its **own Cargo workspace** (`[workspace]` in its own
`Cargo.toml`), unlike `fw-esp32s3`, which is a member of the repo-root
workspace (see `fw-esp32s3/README.md`'s "Workspace notes"). Two reasons:

1. P1 has zero lp2025-internal path dependencies — no `fw-esp32-common`, no
   `lp-recovery` — so there is nothing here that needs the root workspace's
   `workspace.dependencies` or `[patch.crates-io]` forks.
2. Joining the root workspace would also require adding this crate to the
   justfile's `clippy-host` `--exclude` list to keep `just check` green
   (both `fw-esp32c6` and `fw-esp32s3` need that entry) — a justfile edit
   this milestone's P1 was scoped to leave for P4. Standing alone needs no
   root `Cargo.toml` or justfile edit at all.

If a later phase gives this crate real lp2025-internal dependencies, revisit
whether folding it into the root workspace (the S3's shape) is worth the
justfile coupling at that point.

## Building

Xtensa has no upstream Rust target, so — like `fw-esp32s3` — this crate
carries its own `rust-toolchain.toml` with `channel = "esp"` (Espressif's
fork, installed by `espup`), per
`docs/adr/2026-07-29-per-chip-fw-toolchains.md`.

```bash
cd lp-fw/fw-esp32v3
# Put the esp toolchain's bundled GNU binutils on PATH first if
# xtensa-esp32-elf-gcc isn't already resolvable — e.g.
#   export PATH="$HOME/.rustup/toolchains/esp/xtensa-esp-elf/esp-*/xtensa-esp-elf/bin:$PATH"
cargo build --profile release-esp32v3
```

Verified 2026-07-31: builds clean (no warnings) under esp-hal 1.1.1 on the
`esp` toolchain, producing a 144 KB ELF (60 KB `.text`) — comfortably inside
the 3 MB `factory` partition below. `just build-fw-esp32v3` / CI wiring is
P4, not yet present.

### Release profile

Builds under `[profile.release-esp32v3]` (`inherits = "release"`,
`opt-level = "s"`), defined in this crate's own `Cargo.toml` (not the repo
root's, since this crate is its own workspace). Mirrors `fw-esp32s3`'s
`release-esp32s3` profile verbatim, including its choice of `"s"` over the
repo's default `"z"` — the task said to copy the S3's choice because it's
the timing-safe one, validated on real WS281x RMT timing there. This crate
has no timing-critical driver yet; revisit with real measurements once
output lands (M4-class work, not in this roadmap yet).

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
(115200 8N1, TX=GPIO1, RX=GPIO3) and keep it alive. `src/main.rs` does
exactly that (`_uart0`) — the divisor its construction programs is what
`esp_println!`'s raw FIFO writes ride out on, even though nothing calls
methods on that binding directly afterward.

## What's deliberately not here yet

- **No `fw-esp32-common` dependency.** `fw-esp32s3`'s minimal boot path
  didn't need it either at the equivalent stage (see its own git history);
  P1's boot-to-hello requirement is satisfiable with `esp-hal` +
  `esp-println` + `esp-alloc` alone. It becomes this crate's fifth consumer
  when a later phase needs the shared logger/transport/server-loop layer.
- **No `lp-recovery` RTC crash ledger.** The panic handler is print-and-reset
  only (see `src/main.rs`'s panic-posture doc comment) — the compiler-level
  posture (`panic=abort`, no unwinding) already matches `fw-esp32s3`'s abort
  tier, so adding the ledger later is additive, not a rewrite.
- **No board manifest, no output driver, no radio.** Board manifest (Q5)
  and `lp-core/lpc-hardware/boards/domraem/dom-z-102.json` are P5. RAM
  measurement radio-off/radio-linked is P3. WS281x output and radio bring-up
  are out of this milestone's scope entirely.
- **No justfile or CI wiring.** `just build-fw-esp32v3`,
  `just fw-esp32v3-size-check`, and the gated `Firmware build (esp32v3)` CI
  job are P4.
