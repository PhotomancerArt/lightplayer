# esp-hal — LP fork

Vendored from crates.io **esp-hal 1.1.1**, verbatim except for the diff below.
Patched in through the root `Cargo.toml`'s `[patch.crates-io]`, the same way
`third_party/esp-alloc` and `third_party/esp-storage` are. The version stays
`1.1.1` on purpose: `esp-rtos`, `esp-radio`, `esp-storage` and the three
firmwares all depend on `esp-hal` by version, and the patch table only
substitutes a source, never a version.

## The diff: the interrupt dispatch helpers are `#[ram]`

`src/interrupt/mod.rs` gains `#[crate::ram]` on three functions:

| function | called from | size on the classic |
|---|---|---|
| `InterruptStatus::current` | `__level_1/2/3_interrupt` (Xtensa) and `handle_interrupts` (RISC-V), on every level-triggered peripheral interrupt | 80 B |
| `InterruptStatusIterator::next` | the same dispatch loops, once per pending source plus once to terminate | 127 B |
| `mapped_to_raw` | `should_handle`, once per pending source | 81 B |

Why: upstream marks the Xtensa level-N entries and the RISC-V
`handle_interrupts` `#[ram]`, but at `opt-level = "z"` these three helpers
stay outlined and land in flash `.text`, so every peripheral interrupt —
including the WS281x RMT refill interrupt on the classic ESP32's APP core —
runs ~288 B of dispatch code from the flash cache before it reaches the
registered handler. The ISR-in-RAM rule this repo follows (the full handler
path in RAM unless it costs a lot of RAM) wants that 288 B in `.rwtext`.
Filed as docs/debt/classic-iram-handlers-reach-flash.md, pay-down item 1;
the same note's jump-table item is handled on the firmware side by
`lp-fw/fw-esp32v3/rwdata_hook.x`, not here.

Nothing else is changed: no linker-script edits, no feature changes.

## Upstream

Candidate for an upstream PR to esp-rs/esp-hal (main still has the three
functions unmarked as of 2026-09-07). Once it lands and the firmwares move to
that release, drop this directory and the patch line.

## Re-syncing with upstream

Copy the new version out of the cargo registry, delete `.cargo-ok`,
`.cargo_vcs_info.json`, `Cargo.lock` and `Cargo.toml.orig` (mirroring
`third_party/esp-storage`), then re-apply the three attributes.
`grep -n 'crate::ram' src/interrupt/mod.rs` finds them in this copy.
