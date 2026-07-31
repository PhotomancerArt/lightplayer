# ADR: Boot-control sector

- **Status:** Accepted
- **Date:** 2026-07-30
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

A project can make its own device unrecoverable. The originating case: a
project bright enough to brown the board out at power-up. The LEDs come on,
the rail collapses, the board resets, auto-loads the same project, and loops —
never staying up long enough for Studio to connect and fix it.

`lp-recovery` already has a boot-loop ladder that skips project auto-load
after repeated incomplete boots, but it does not fire here, for **two
independent reasons** (both left for a follow-up plan):

1. `mark_boot_complete()` fires on the **first rendered frame**, so a board
   that renders one bright frame and then dies looks like a successful boot
   and resets the counter every time.
2. `ResetCause::Brownout` is excluded from `blames_code()`, so brownouts never
   increment the counter even when the counter is reached.

And a third problem that no amount of ladder tuning fixes: **the recovery
region lives in RTC fast RAM, which is wiped on `PowerOn`.** The instinctive
human response to a misbehaving board is to unplug it — which erases the
evidence that it was looping at all. A latch a user destroys by doing the
obvious thing is not a latch.

Separately, recovery tooling needs to tell a device "come up without loading a
project" while the device is in ROM download mode — i.e. while no firmware is
running to receive a message. The only channel available there is flash
itself.

These are the same requirement seen from two directions.

## Decision

Add a dedicated **4 KB `bootctl` flash partition** carrying a small record
that the firmware reads before it auto-loads a project. The format lives in a
new `no_std`, zero-alloc crate, `lp-base/lp-bootctl`, shared by every writer
and reader so they cannot disagree.

**Placement: the 4 KB comes out of `nvs`,** which shrinks from `0x6000` to
`0x5000`, putting `bootctl` at `0xe000`. Nothing reads NVS — no LightPlayer
code references it, and `esp-radio`'s `NVS` is a 15-word **RAM** array
(`common_adapter.rs`) plus stubbed ESP-IDF shims, not this partition. This
placement moves **no other offset**, so `lpfs` stays at `0x310000` and
existing devices' filesystem images remain valid. Both boards keep
byte-identical layouts so the offset can be a constant rather than a
partition-table lookup.

**Two writers, one format.** The host writes the record over esptool while
the device sits in ROM download mode — the path that works on a board that
cannot boot. The firmware will later write it too, to latch its own degraded
state across a power cycle (follow-up plan; today the firmware only reads and
consumes).

**Blank is safe.** A device that has never seen this feature reads `0xFF`
bytes. That, a bad magic, a bad CRC, a future format version, a short read,
and a torn write all decode to "boot normally". There is exactly one way to
get a non-default boot: a fully valid record that asks for one. A corrupt
sector can never *cause* a degraded boot — only fail to prevent one.

**Torn writes: one write, integrity by checksum.** The record is written to
an erased sector in a single operation, and its integrity rests on the magic
and CRC rather than on write ordering.

This is deliberately *not* the discipline `lp-recovery` uses. That crate
publishes RTC-RAM structures by flipping one visibility word last, and the
flash-native mirror — write the payload, then the magic — was the original
design here. **It does not work.** Every flash-write API that can reach this
sector issues the ESP ROM/stub `FLASH_BEGIN`, which *erases the sectors it is
about to write*; that is true of `espflash::write_bin_to_flash` and of
`esptool-js`'s `writeFlash` alike. A second write publishing a first would
erase it instead, leaving a valid magic over an erased payload — which fails
the CRC, boots normally, and makes the feature silently inoperative.

The CRC covers the magic, so every prefix of a partial write fails one of the
two checks. `no_partial_write_is_ever_honored` asserts that for all 16
truncation points.

**Consume on read, not on use.** The firmware erases a valid record the
moment it reads it, before anything acts on it. That makes the instruction
one-shot — the user asked for one recovery boot, not a permanent mode — and
means a crash *during* the recovery boot cannot make it sticky. The failure
mode traded for is an erase that fails, which strands the device reachable
with no project loaded: the safe direction, and itself recoverable.

**Unknown flag bits are ignored, not rejected.** A newer host asking for
something this firmware cannot apply still gets the instructions it does
understand.

**Amendment 2026-07-31 — the safe-mode clamp bits are assigned.** Bits
`8..16` now carry a safe-mode output clamp level (`0` = none, else a
brightness ceiling out of 255), with a precedence rule defined in the
format: *a firmware that implements the clamp loads the project dimmed and
ignores the skip bit* — a dim, visible board is a strictly better
degradation than a dark one. Studio's "Start in safe mode" writes BOTH the
skip and a dim clamp, so the same record means "nothing loaded" on firmware
that predates the clamp and upgrades to "project running dim" when the
clamp lands, with no format bump and no Studio change. Firmware consumption
of the clamp remains with the follow-up plan (it shares the output path
with the fixture mA limiter).

## Consequences

- **This is a breaking flash-layout change.** A device flashed with the old
  table has no `bootctl` partition; the region at `0xe000` is inside its
  `nvs`. Reading it yields whatever NVS had there — which decodes as `Blank`
  or `Invalid`, so an un-reflashed device simply never takes a boot-control
  instruction. Deployed boards must be reflashed with the new table to gain
  the feature, and that is safe because nothing reads NVS.
- `lpfs` is untouched, so reflashing does **not** cost the user their files.
- The firmware gains a flash read (and conditionally a 4 KB erase) on the boot
  path, before the filesystem mounts. Measured cost: image grew well within
  budget — 2 866 928 B of 3 145 728 B, 278 800 B headroom against a 65 536 B
  CI margin.
- `lp-bootctl` hardcodes the offset, so the two boards' partition tables must
  stay identical. `lp-base/lp-bootctl/tests/partition_layout.rs` enforces
  that, along with no-overlap, the 4 MB bound, and that no pre-existing
  partition moved. Those guards live in `lp-bootctl` rather than `fw-esp32c6`
  because the latter is RV32-only and excluded from host builds, so tests
  there would never run.
- Two skip-auto-load reasons now exist (boot-control record, and the
  RTC-resident incomplete-boot ladder). The boot log always names which one
  applied.

## Alternatives Considered

- **Shrink `lpfs` instead of `nvs`.** Rejected: changing the partition size
  changes littlefs's block count, invalidating every existing filesystem
  image. Recovery tooling that costs users their files is self-defeating.
- **Append after `lpfs`.** Impossible: the layout ends at exactly `0x400000`
  on a 4 MB part.
- **Store the flag in NVS proper.** Rejected: writing ESP-IDF's NVS format
  from a host over esptool means implementing that format host-side, which is
  far more work than a 16-byte record in a raw sector, for no benefit.
- **Store the flag in `lpfs`.** Rejected on two counts: the host would have
  to build and rewrite a littlefs image just to set one flag, and the flag
  would live in the same structure whose corruption is one of the things it
  needs to survive.
- **Keep the latch in the RTC recovery region.** Rejected: wiped on
  `PowerOn`, and unplugging the board is the single most likely user action
  in the scenario this exists for.
- **A `consumed` marker word instead of erasing.** Bit-clearing a word is
  cheaper than a 4 KB erase and avoids an erase cycle per recovery boot.
  Rejected for simplicity: it doubles the states to reason about in a
  safety-critical primitive, and the erase happens at most once per recovery
  boot, where tens of milliseconds are irrelevant.
- **Numeric partition subtype `0x40`.** Rejected because it does not work:
  espflash's `esp-idf-part` panics on any `data` subtype outside its enum.
  Used `undefined` (`0x06`), ESP-IDF's designated user-data subtype.

## Follow-ups

- The firmware as a *writer*: latch degraded state across a power cycle after
  repeated failures. Needs the boot-complete redefinition and the brownout
  blame policy fixed first — both deferred to the follow-up plan.
- The graduated output clamp that reserved flag bits `8..16` exist for.
- Mirroring a last-crash summary into the sector, so post-mortem is readable
  from a board that never boots (the RTC crash record is not).
- `lpfs` is declared with subtype `spiffs` but is littlefs; `esp-idf-part`
  does support a `littlefs` subtype. Left alone deliberately — cosmetic, and
  changing it would alter the partition-table binary for no functional gain.
