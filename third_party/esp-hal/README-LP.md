# esp-hal — LP fork

Vendored from crates.io **esp-hal 1.1.1**, verbatim except for the four diffs below.
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

## The third diff: upstream's USB-Serial-JTAG async fixes, back-ported

`src/usb_serial_jtag.rs` carries three upstream esp-hal PRs, all merged in
August 2026 and **released in esp-hal 1.2.0**. They are upstream's own
commits (MIT/Apache-2.0), applied verbatim to the 1.1.1 file:

| upstream PR | merge commit | what it changes |
|---|---|---|
| [#6089](https://github.com/esp-rs/esp-hal/pull/6089) | `5c2672becc9f6161da65c329ef3593ed770af629` | `async_interrupt_handler` writes `int_clr` with only the bits it handled, not both |
| [#6097](https://github.com/esp-rs/esp-hal/pull/6097) | `f05b3976f8f5c3aa31ede23ca09bdb9d8f2f925b` | `flush_tx_async` sets `wr_done` (a prerequisite of #6104's flush hunk) |
| [#6104](https://github.com/esp-rs/esp-hal/pull/6104) | `ab45d33cf69f941e5bd8b939b9a4ad0e68bd6b7d` | an `esp_sync::RawMutex` around every async-path `int_ena` read-modify-write; `wait_tx_ready` (a new `serial_in_empty` event, then `serial_in_ep_data_free`) after every `wr_done` |

Why: the handler cleared a TX edge it had not handled. A drain that raised
`serial_in_empty` between the handler's `int_st` read (for an RX interrupt)
and its `int_clr` write was lost, and the write future slept until the
caller's 250 ms timeout with the send buffer already empty. See
`docs/defects/2026-09-26-esp-hals-usb-isr-clears-a-tx-edge-it-did-not-handle.md`.

Left out of 1.2.0's copy of the file on purpose: the `WakeLock` fields
(1.2.0's light-sleep lock, which needs `rtc_cntl` changes this fork does not
have), the module's move to `usb::usb_serial_jtag`, and doc-comment
rewording. After this diff the file differs from 1.2.0's only in those.

**Drop on upgrade to esp-hal ≥ 1.2.0**: all three are in it. Nothing of ours
is mixed in.

## The fourth diff: the RISC-V dispatcher's own callees are `#[ram]`

`src/interrupt/riscv.rs` changes two things inside the RISC-V
`handle_interrupts` path, which is the C6's:

| what | upstream | here | size on the C6 |
|---|---|---|---|
| `change_current_runlevel` | plain `fn`, called on entry and exit of every peripheral interrupt | `#[crate::ram]` | 98 B |
| the per-source loop | a closure (`let handle_interrupts = \|\| …`) with a `filter` closure inside, passed to `riscv::interrupt::nested` or called directly | a named `#[crate::ram] unsafe fn dispatch(status, prio)`, called directly at `Priority::max()` and wrapped in a one-line closure only for `nested`; the filter is a `continue` | 178 B |

Why: the first diff put `handle_interrupts`' three helpers in RAM, but
`handle_interrupts` itself still left RAM twice on every interrupt. On the
`ws281x_telemetry` C6 image (main `113493d0b`) `rust-nm` put
`change_current_runlevel` at `0x42098462` and
`handle_interrupts::{closure#0}` at `0x42098a04`, and the closure called two
machine-outlined prologue/epilogue helpers (`OUTLINED_FUNCTION_119`/`_115`,
`0x422345..`) — about a dozen cold 32-byte flash-cache lines between
`Trap15` and `rmt_isr`, both of which are in RAM. A closure is its own
codegen item and does not inherit the enclosing function's `link_section`;
`#[inline]` would not help at `opt-level = "z"`. That cost lands on the
WS281x refill, whose deadline is a 24-word RMT half (30 µs) on the C6's
two-channel plan. See
`docs/defects/2026-10-01-the-c6-choker-truncates-most-ws281x-frames-on-silicon.md`.

After it, the whole path from `Trap15` through `handle_interrupts`,
`change_current_runlevel`, `dispatch` and the first diff's three helpers to
the bound handler is in `.rwtext`; the only flash targets left on it are
panic paths (`unwrap_failed`, `panic_bounds_check`). Cost: `.rwtext`
+272 B, `.text` −266 B. Behaviour is unchanged — the same sources, in the
same order, at the same runlevel, with nesting enabled on the same
priorities. Check it with `rust-nm -C -n` and `rust-objdump -d`, not by
reading the attributes.

## Nothing else

No other linker-script edits, no feature changes.

## Upstream

Candidate for an upstream PR to esp-rs/esp-hal (main still has the three
functions unmarked as of 2026-09-07; the fourth diff is the same kind of change
and would ride the same PR). Once it lands and the firmwares move to
that release, this directory can go — but only after the `links` key above has
somewhere else to live, since upstream does not provide it and
`fw-esp32c6/build.rs` refuses to build without it.

## Re-syncing with upstream

Copy the new version out of the cargo registry, delete `.cargo-ok`,
`.cargo_vcs_info.json`, `Cargo.lock` and `Cargo.toml.orig` (mirroring
`third_party/esp-storage`), then re-apply the first, second and fourth diffs (the third is
upstream's, so a ≥ 1.2.0 copy already has it; on a 1.1.x copy, re-apply it
too — `grep -n INT_ENA_LOCK src/usb_serial_jtag.rs` finds it here):

* the three `#[crate::ram]` attributes — `grep -n 'crate::ram'
  src/interrupt/mod.rs` finds them in this copy;
* the fourth diff — `grep -n 'LP fork' src/interrupt/riscv.rs` finds both
  hunks (`change_current_runlevel`'s attribute and `rt::dispatch`);
* `links = "esp-hal"` in `Cargo.toml` and the `cargo::metadata=linker-scripts`
  line in `build.rs` — `grep -n 'linker-scripts' build.rs`.
