# fw-esp32s3

LightPlayer firmware for the **ESP32-S3** (Xtensa LX7). Sibling of
`fw-esp32c6` (RISC-V) and the **third consumer** of `fw-esp32-common`.

Currently a **boot skeleton**: clocks, heap, and serial logging far enough to
print the `[INIT]` marker family. The server, radio, and LED-output stacks
arrive in later phases of the Xtensa backport's M5.

Verified on hardware 2026-07-30 (ESP32-S3 rev v0.2, 16 MB flash): flashes and
boots to `[INIT] ready`.

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
just flash-fw-esp32s3 /dev/cu.usbmodem1101
```

The port argument is optional but usually wanted: several boards are typically
on the desk bus and auto-detection picks the first match, not necessarily the
S3. The S3 speaks **USB-Serial-JTAG**, not a UART bridge, so it enumerates as
`/dev/cu.usbmodem*` and **its port number changes** each time the chip
re-enumerates after a reset.

Before concluding a board is dead: a stray `espflash` holding this port wedges
it *uninterruptibly* (`ps` STAT `Us+`; `kill -9` does not land, because the
process is blocked reading a device node that went away when the chip
re-enumerated). Only a physical replug clears it. Check `pgrep -fl espflash`
first. Bare `espflash monitor` cannot attach to a running app at all — it
always tries to sync with the bootloader — so use the flash path above.

### Partitions

`partitions.csv` **mirrors the C6's table exactly**: 3 MB `factory` + 960 KB
`lpfs`, totalling precisely 4 MB. That is deliberate — the 4 MB floor is the
target, not the desk board's 16 MB, and matching the C6 means the storage
layer's offsets port across unchanged.

Passing it is not optional. espflash **silently** substitutes a default table
whose factory partition is only 1 MB if `--partition-table` is omitted; the
boot skeleton fits that, so the mistake stays invisible until the firmware
grows past it. Both the `.cargo/config.toml` runner and the justfile recipe
pass it.

## How this differs from fw-esp32c6

Per the per-chip-toolchains ADR, the recovery strategy is **per chip** and does
not migrate into `fw-esp32-common`:

| | fw-esp32c6 | fw-esp32s3 |
|---|---|---|
| Toolchain | shared pinned nightly | `channel = "esp"` |
| Panic strategy | `panic=unwind` + `unwinding` | **`panic=abort`** (abort tier) |
| Unwind tables | `.eh_frame` retained via a build.rs patch | none — nothing to retain |
| Recovery | `catch_unwind` around node render | RTC ledger + reset |
| Linker | rust-lld, `-Tlinkall.x` | GNU ld, `-Wl,-Tlinkall.x` |

That is why this crate's `build.rs` is nearly empty: the C6's exists mostly to
patch esp-hal's `eh_frame.x`, which is meaningless without unwinding.

## Workspace notes

A workspace **member** (so it shares workspace dependencies and the lockfile)
but not in `default-members`, and excluded from `clippy-host` — exactly like
`fw-esp32c6`. Both are cross-target-only, and including this one in host
clippy triggers a real `critical-section` feature-unification conflict
(multiple `restore-state-*` features) rather than a mere build failure.
