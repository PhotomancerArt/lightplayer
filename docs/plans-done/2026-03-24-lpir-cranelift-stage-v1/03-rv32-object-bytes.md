## Scope of phase

Implement **`object_module.rs`** (name may vary): create **RISC-V 32** ISA using
the same **triple and flag defaults** as `lp-glsl-cranelift`’s
`Target::riscv32_emulator()` / `default_riscv32_flags`, build **`ObjectModule`**
via `ObjectBuilder::new(isa, b"lpir", default_libcall_names())`, run the shared
`define_lpir_functions` from phase 02, then **`finish`** and return **`Vec<u8>`**
(object ELF).

Expose a public entry point, e.g.:

```rust
#[cfg(feature = "riscv32-emu")]
pub fn object_bytes_from_ir(
    ir: &IrModule,
    options: &CompileOptions,
) -> Result<Vec<u8>, CompilerError>
```

(`object_from_ir` naming is fine if preferred.)

## Code organization reminders

- RV32 flag construction should be **one function** (e.g. `riscv32_isa_flags()`)
  documented as matching the old compiler’s emulator target.
- No emulator or linking in this file — object bytes only.

## Implementation details

- **Q32-only for imports** — same rule as JIT: reject or document F32 + imports.
- **Call convention:** use `module.isa().default_call_conv()` for the RV32 ISA
  (should be `SystemV` on riscv32 for Cranelift).
- **Linkage:** exported user functions as today (`Linkage::Export`).
- If `finish` returns a product type, extract raw ELF bytes per `cranelift-object`
  docs.

## Tests

- **Feature-gated** unit test: minimal LPIR (e.g. `iconst` + `return` or F32 add
  without imports) → `object_bytes_from_ir` → assert bytes start with ELF magic
  `\x7fELF` and non-empty.

## Validate

```bash
cd /Users/yona/dev/photomancer/lp2025/lp-glsl && cargo test -p lpir-cranelift --features riscv32-emu
```

Fix any new warnings in touched files. `cargo +nightly fmt`.
