# M3: lpvm-cranelift JIT — Stage III Implementation Plan

## Goal

Add LPVM trait implementations to the existing Cranelift crate. Rename
`lpir-cranelift` → `lpvm-cranelift` and create `CraneliftEngine`,
`CraneliftModule`, `CraneliftInstance` alongside the existing API.

## Architecture Decision

**Dual API approach:** Both old and new APIs coexist in the same crate.

- **Old API** (`JitModule`, `DirectCall`, `jit()`) stays until M7 (engine migration)
- **New trait API** (`CraneliftEngine`, `CraneliftModule`, `CraneliftInstance`) added now
- Cleanup happens in M7 when `lp-engine` migrates to traits

## Why Not Separate Crate?

Both JIT and RV32 emu paths need the same Cranelift lowering code. Creating
separate crates would require either:
1. Duplicating the lowering logic
2. Awkward crate dependencies

Better to have one `lpvm-cranelift` crate with:
- JIT path always available (M3)
- RV32 emu path behind `riscv32-emu` feature (M4)

## Files

| File | Purpose |
|------|---------|
| `00-notes.md` | Architecture notes and decisions |
| `01-design.md` | High-level design document |
| `02-phase-1-rename.md` | Rename crate, update workspace |
| `03-phase-2-engine.md` | Add `CraneliftEngine` (LpvmEngine) |
| `04-phase-3-module.md` | Add `CraneliftModule` (LpvmModule) |
| `05-phase-4-instance.md` | Add `CraneliftInstance` (LpvmInstance) |
| `06-phase-5-tests.md` | Integration and validation |
| `07-phase-6-docs.md` | Documentation updates |

## Phases

1. **Phase 1:** Rename `lpir-cranelift` → `lpvm-cranelift`, move to `lp-shader/`
2. **Phase 2:** Add `CraneliftEngine` implementing `LpvmEngine`
3. **Phase 3:** Add `CraneliftModule` implementing `LpvmModule` + `direct_call()`
4. **Phase 4:** Add `CraneliftInstance` implementing `LpvmInstance`
5. **Phase 5:** Integration tests, validate both APIs work
6. **Phase 6:** Documentation explaining dual API

## Key Design Points

### No `LpvmMemory` Trait

JIT memory is internal to `CraneliftInstance` as a `Vec<u8>`. WASM is the
special case with external memory; other backends manage memory internally.

### `DirectCall` Hot Path

Beyond the trait interface, `CraneliftModule` has `direct_call()` method
returning `DirectCall` for zero-overhead calls. The engine render loop uses
this; tests use the ergonomic trait interface.

### ISA Selection

```rust
#[cfg(not(target_arch = "riscv32"))]
// Use cranelift-native for host ISA detection

#[cfg(target_arch = "riscv32")]
// Hardcoded riscv32imac triple for embedded
```

## Validation

```bash
# Host
cargo check -p lpvm-cranelift
cargo test -p lpvm-cranelift

# Embedded JIT (the product)
cargo check -p lpvm-cranelift --target riscv32imac-unknown-none-elf

# Firmware (still uses old API)
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
```

## Timeline

- **M3 (now):** JIT trait implementation
- **M4:** RV32 emu feature in same crate
- **M5:** Migrate filetests to trait API
- **M6:** Migrate `lp-engine` to trait API
- **M7:** Remove old API

See `docs/roadmaps/2026-04-04-lpvm/` for full roadmap.
