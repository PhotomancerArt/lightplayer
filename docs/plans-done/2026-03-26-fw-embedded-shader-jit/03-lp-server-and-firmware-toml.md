# Phase 3: `lp-server`, `fw-emu`, `fw-esp32` — dependency wiring

## Scope of phase

Ensure **no `libstd`** firmware builds pull in the **full compiler stack** through normal dependency
features — **without** a separate “enable shader” feature on **`lp-server`** unless forwarding is
strictly necessary. **`fw-emu`** and **`fw-esp32`** default **`server`** images include *
*`lp-engine`** with **`glsl`/JIT**.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

1. **`lp-server/Cargo.toml`**
    - Verify **`lp-engine`** dependency enables whatever **`lpvm-cranelift`** features Phase 1–2
      require when **`default-features = false`**.
    - Add **forwarding** features only if **`fw-*`** cannot set **`lp-engine`** features
      transitively (Cargo limitation); prefer **single source of truth** on **`lp-engine`**.

2. **`fw-emu/Cargo.toml`**
    - Adjust **`lp-server`** line so **`panic-recovery`** (and any required *
      *`lp-engine`/`lpvm-cranelift`** features) match Phase 2.
    - No “compiler on” knob unless required by Cargo.

3. **`fw-esp32/Cargo.toml`**
    - Same as **`fw-emu`** for optional **`lp-server`** / **`server`** feature set.
    - Default **`server`** build must type-check with **GLSL + JIT** in the graph.

4. **`fw-core`**
    - Only touch if it re-exports or constrains **`lp-server`** features.

## Tests to write

- None beyond **compile checks** in this phase.

## Validate

```bash
cargo +nightly fmt -p lp-server -p fw-emu -p fw-esp32 -p fw-core
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

Use **`release-esp32`** / **`release-emu`** profiles as in `justfile` if they affect features. Fix
new warnings.
