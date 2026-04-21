# Phase 02 — Expose gpio4 from init_board

**Tags:** sub-agent: yes, parallel: 1

## Scope of phase

Add `GPIO4` to the `init_board()` return tuple in
`lp-fw/fw-esp32/src/board/esp32c6/init.rs` and update every destructure
site in the codebase to consume the new element (named `_gpio4` where
unused).

### Out of scope

- Any code in `tests/fluid_demo/`.
- Any code in `tests/msafluid_solver.rs` or `tests/test_msafluid.rs`
  beyond the destructure update.
- Any non-esp32c6 board code.
- Any change to `start_runtime` or any other helper in `init.rs`.

## Code organization reminders

- Granular file structure, one concept per file.
- Place abstract things, entry points, and tests near the **top** of each
  file.
- Place helper utility functions at the **bottom** of each file.
- Keep related functionality grouped together.
- Any temporary code must have a `TODO` comment so it can be found later.

## Sub-agent reminders

- Do **not** commit. The plan commits at the end as a single unit.
- Do **not** expand scope. Stay strictly within "Scope of phase".
- Do **not** suppress warnings or `#[allow(...)]` problems away — fix
  them.
- Do **not** disable, skip, or weaken existing tests to make the build
  pass.
- If something blocks completion (ambiguity, unexpected design issue),
  stop and report rather than improvising.
- Report back: what changed, what was validated, and any deviations from
  this phase plan.

## Implementation details

### 1. `init_board()` change

In `lp-fw/fw-esp32/src/board/esp32c6/init.rs`:

Current return type ends with `esp_hal::peripherals::FLASH<'static>`.
Append a 7th element `esp_hal::peripherals::GPIO4<'static>`. The full
return type becomes:

```rust
pub fn init_board() -> (
    SoftwareInterruptControl<'static>,
    TimerGroup<'static, impl TimerGroupInstance>,
    esp_hal::peripherals::RMT<'static>,
    esp_hal::peripherals::USB_DEVICE<'static>,
    esp_hal::peripherals::GPIO18<'static>,
    esp_hal::peripherals::FLASH<'static>,
    esp_hal::peripherals::GPIO4<'static>,
)
```

Inside the body, alongside the existing `let gpio18 = peripherals.GPIO18;`,
add:

```rust
let gpio4 = peripherals.GPIO4;
```

And return `gpio4` as the 7th tuple element.

### 2. Update every destructure of `init_board()`

Use grep to find every site:

```sh
rg --files-with-matches 'init_board\(\)' lp-fw/fw-esp32/
```

Expected sites (verify with the grep above):

- `lp-fw/fw-esp32/src/main.rs`
- `lp-fw/fw-esp32/src/tests/test_msafluid.rs`
- `lp-fw/fw-esp32/src/tests/test_dither.rs`
- `lp-fw/fw-esp32/src/tests/test_rmt.rs`
- `lp-fw/fw-esp32/src/tests/test_gpio.rs`
- `lp-fw/fw-esp32/src/tests/test_usb.rs`
- `lp-fw/fw-esp32/src/tests/test_json.rs`

For each site, find the destructure (e.g.
`let (sw_int, timg0, _rmt, usb_device, _gpio18, _flash) = init_board();`)
and append `_gpio4` as the 7th element. Use `_gpio4` (with leading
underscore) at every site since none of them consume it. Phase 5 will
update the `test_fluid_demo` runner to bind it without underscore — but
that runner does not exist yet in this phase.

If a site already binds without underscore (e.g. `_rmt` becomes `rmt`),
preserve the existing naming; only add the new 7th element.

### 3. Do not invent new gating

Some `#[cfg(not(any(...)))]` blocks in `main.rs` may need a `test_fluid_demo`
guard added — that is **phase 5's responsibility**, not this phase's. If
clippy in the existing feature combos passes after just the destructure
updates, you are done.

## Validate

From `lp-fw/fw-esp32/`:

```sh
cargo clippy --features esp32c6 \
    --target riscv32imac-unknown-none-elf \
    --profile release-esp32 \
    -- --no-deps -D warnings
```

```sh
cargo clippy --features test_msafluid,esp32c6 \
    --target riscv32imac-unknown-none-elf \
    --profile release-esp32 \
    -- --no-deps -D warnings
```

```sh
cargo clippy --features test_dither,esp32c6 \
    --target riscv32imac-unknown-none-elf \
    --profile release-esp32 \
    -- --no-deps -D warnings
```

```sh
cargo clippy --features test_rmt,esp32c6 \
    --target riscv32imac-unknown-none-elf \
    --profile release-esp32 \
    -- --no-deps -D warnings
```

```sh
cargo clippy --features test_gpio,esp32c6 \
    --target riscv32imac-unknown-none-elf \
    --profile release-esp32 \
    -- --no-deps -D warnings
```

```sh
cargo clippy --features test_usb,esp32c6 \
    --target riscv32imac-unknown-none-elf \
    --profile release-esp32 \
    -- --no-deps -D warnings
```

```sh
cargo clippy --features test_json,esp32c6 \
    --target riscv32imac-unknown-none-elf \
    --profile release-esp32 \
    -- --no-deps -D warnings
```

All must pass clean.
