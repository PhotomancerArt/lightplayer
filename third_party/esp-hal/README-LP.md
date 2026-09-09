# esp-hal — LP fork

Vendored from crates.io **esp-hal 1.1.1**, verbatim except for the two diffs below.
Patched in through the root `Cargo.toml`'s `[patch.crates-io]`, the same way
`third_party/esp-alloc` and `third_party/esp-storage` are. The version stays
`1.1.1` on purpose: `esp-rtos`, `esp-radio`, `esp-storage` and the three
firmwares all depend on `esp-hal` by version, and the patch table only
substitutes a source, never a version.

## The first diff: the interrupt dispatch helpers are `#[ram]`

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

## The second diff: a `links` key

`Cargo.toml` gains `links = "esp-hal"` and `build.rs` gains one line beside the
`rustc-link-search` it already emits:

```rust
println!("cargo::metadata=linker-scripts={}", out.display());
```

esp-hal links no native library. The key is here for the two things cargo
attaches to it:

1. **An ordering edge.** Cargo runs a `links` package's build script before the
   build scripts of everything that depends on it. Without one it orders
   nothing, and build scripts of sibling packages run concurrently.
2. **`DEP_ESP_HAL_LINKER_SCRIPTS`**, which hands direct dependents the exact
   `OUT_DIR` the generated linker scripts land in.

`lp-fw/fw-esp32c6/build.rs` needs both. It patches the generated `rodata.x`
(merging `.rodata_desc` into `.rodata`, so the image keeps to the two
ROM-mapped segments the ESP32 bootloader accepts) and flattens `eh_frame.x`.
Before this key it *guessed* the directory by scanning
`target/<triple>/<profile>/build/esp-hal-*/out`, and skipped quietly when the
scan found nothing — which on a **cold** target dir is what happened, because
its script could run first. The result was that build 1 and build 2 of one
commit were different images, and a CI tree is always cold. See
`docs/defects/2026-09-08-cold-target-dir-links-esp-hals-stock-rodata.md`.

⚠️ **This diff must outlive the one above.** The `#[ram]` diff goes away when
upstream takes it; the `links` key does not, because nothing upstream provides
it. If this fork is ever dropped for a stock esp-hal release, `links` and the
metadata line have to be re-applied to whatever replaces it — or
`fw-esp32c6/build.rs` has to stop needing them. That build script fails loudly
with this file's name in the message rather than linking the stock layout, so
the mistake cannot be silent, but it will stop the build.

## Nothing else

No other linker-script edits, no feature changes.

## Upstream

Candidate for an upstream PR to esp-rs/esp-hal (main still has the three
functions unmarked as of 2026-09-07). Once it lands and the firmwares move to
that release, this directory can go — but only after the `links` key above has
somewhere else to live, since upstream does not provide it and
`fw-esp32c6/build.rs` refuses to build without it.

## Re-syncing with upstream

Copy the new version out of the cargo registry, delete `.cargo-ok`,
`.cargo_vcs_info.json`, `Cargo.lock` and `Cargo.toml.orig` (mirroring
`third_party/esp-storage`), then re-apply **both** diffs:

* the three `#[crate::ram]` attributes — `grep -n 'crate::ram'
  src/interrupt/mod.rs` finds them in this copy;
* `links = "esp-hal"` in `Cargo.toml` and the `cargo::metadata=linker-scripts`
  line in `build.rs` — `grep -n 'linker-scripts' build.rs`.
