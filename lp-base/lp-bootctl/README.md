# lp-bootctl

The **boot-control sector**: a flash-persisted instruction to the firmware's
next boot.

One 4 KB flash partition (`bootctl`, at `0xe000` on every supported board)
holding a 16-byte record that the firmware reads *before* it auto-loads a
project. It exists so a device can be recovered when its own project is what
stops it from running — a project bright enough to brown the board out, a
shader that hangs the watchdog, anything that dies before the link is usable.

This crate is `no_std`, zero-alloc, and pure: encode, decode, and the byte
layout. It performs no IO. Flash access lives in the edges —
`lp-fw/fw-esp32c6/src/bootctl.rs` on the device, and the `lpa-link` providers
on the host.

Design rationale, alternatives, and the partition-layout change:
[`docs/adr/2026-07-30-boot-control-sector.md`](../../docs/adr/2026-07-30-boot-control-sector.md).

## Why not the recovery region

`lp-recovery`'s breadcrumb region lives in RTC fast RAM. It survives software
and watchdog resets but **not a power cycle** — and unplugging the board is
exactly what a person does to a device that is misbehaving. A latch a user
erases by doing the obvious thing is not a latch. This sector is flash-resident.

## Two writers, one format

| Writer | When | Why it needs this channel |
|---|---|---|
| **Host** (esptool / espflash) | Device is in ROM download mode | No firmware is running to receive a message; flash is the only channel |
| **Firmware** | *Not yet implemented* — follow-up plan | Latch its own degraded state across a power cycle |

Both share this crate, so they cannot disagree about the format.

## The rules that matter

**Blank is safe.** An erased sector, a bad magic, a bad CRC, a future format
version, a short read, and a torn write **all** decode to "boot normally".
There is exactly one way to get a non-default boot: a fully valid record that
asks for one. A corrupt sector can never *cause* a degraded boot — only fail
to prevent one.

**One write.** Write the whole 16-byte record to an erased sector in a single
operation. Do **not** split it: every flash-write API that can reach this
sector issues the ESP ROM/stub `FLASH_BEGIN`, which erases the sectors it is
about to write, so a second write meant to publish a first erases it instead.
Integrity comes from the magic and CRC — every prefix of a partial write fails
one of the two checks and decodes as "no record".

**Consume on read.** The firmware erases a valid record the moment it reads
it, before acting on it. The instruction is one-shot; a crash during the
recovery boot cannot make it sticky.

**Unknown flag bits are ignored, not rejected.** A newer host asking for
something this firmware cannot apply still gets the instructions it does
understand.

**Safe mode is skip + clamp, with clamp winning.** Bits `8..16` carry a
safe-mode output clamp level (`0` = none, else a brightness ceiling out of
255). Hosts set both the skip bit and a dim clamp; a firmware that
implements the clamp loads the project dimmed and ignores the skip, while
older firmware ignores the clamp bits and honors the skip. Same record,
best available behavior on each.

## Layout

| Offset | Size | Field |
|---|---|---|
| `0` | 4 | Magic `LPBC` — **written last** |
| `4` | 2 | Format version (LE) |
| `6` | 2 | Padding |
| `8` | 4 | Flags (LE) |
| `12` | 4 | CRC-32 over bytes `0..12` (LE) |

The rest of the sector is left erased.

## Usage

```rust
use lp_bootctl::{BOOTCTL_PARTITION_OFFSET, BootFlags, decode, encode_record};

// Host side: ask the device to start once in safe mode. Skip + clamp
// together, so the record does the best thing each firmware knows how to.
let flags = BootFlags::SKIP_PROJECT_AUTOLOAD.with_safe_clamp(BootFlags::DEFAULT_SAFE_CLAMP);
let record = encode_record(flags);
// erase the sector, then write `record` at BOOTCTL_PARTITION_OFFSET in one go

// Device side, at boot:
let outcome = decode(&sector_bytes);
if outcome.skip_project_autoload() {
    // come up reachable with nothing loaded
}
```

The sector must be **erased** before the write; NOR flash cannot turn a `0`
back into a `1`.

## Tests

`tests/partition_layout.rs` guards the hand-maintained agreement between
[`BOOTCTL_PARTITION_OFFSET`] and both boards' `partitions.csv`: that the
offsets match, that no pre-existing partition moved, that nothing overlaps,
that the layout still fits 4 MB, and that the two boards stay identical.

Those guards live here rather than in `fw-esp32c6` because that crate is
RV32-only and excluded from host builds — a `#[cfg(test)]` module there would
never run.

```bash
cargo test -p lp-bootctl
```
