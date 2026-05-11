## Phase 5: Filetest target `rv32lp.q32`

### Scope

Extend `lps-filetests` target system to include the native emulation backend.

- Add `Backend::Rv32lp` in `src/targets/mod.rs`
- Add `Target` to `ALL_TARGETS` with:
  - `backend: Backend::Rv32lp`
  - `float_mode: Q32`
  - `isa: Riscv32`
  - `exec_mode: Emulator`
- Update `display.rs` to format `Rv32lp` as `"rv32lp"`
- Update parse tests

### Code organization

- `targets/mod.rs` — enum and `ALL_TARGETS` array
- `targets/display.rs` — `Display` impl and parsing

### Implementation details

```rust
pub enum Backend {
    Jit,
    Rv32,   // Cranelift-based emu
    Rv32lp, // Native-based emu (new)
    Wasm,
}

// Target name: "rv32lp.q32"
```

Keep `Rv32` (Cranelift) and `Rv32lp` (native) separate for side-by-side comparison during POC.

### Tests

```bash
cargo test -p lps-filetests --lib
```
