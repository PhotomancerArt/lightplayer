# Stage V1 summary

## Landed

- **`module_lower`:** Shared `lower_lpir_into_module<M: Module>` with `LpirFuncEmitOrder` (`Source` for JIT, `Name` for object).
- **`compile_options.rs`:** `CompileOptions` shared by JIT and object paths.
- **`jit_module`:** Uses shared lowering; behavior unchanged for existing tests.
- **`builtins`:** `declare_module_imports` / `declare_lpir_opcode_builtins` take `&mut impl Module`.
- **Feature `riscv32-emu`:** `cranelift-codegen/riscv32`, `cranelift-object`, `lp-riscv-elf`, `lp-riscv-emu`.
- **`build.rs`:** Embeds `lps-builtins-emu-app` when the feature is enabled (empty bytes + warning if missing).
- **`object_module`:** RV32 imafc triple + flags aligned with `lps-cranelift` emulator target; `object_bytes_from_ir` → ELF object bytes.
- **`object_link`:** `link_object_with_builtins`, `BuiltinId` verification (ported from `builtins_linker` semantics).
- **`emu_run`:** `run_lpir_function_i32` (object → link → emulate); `run_loaded_function_i32` / `run_loaded_function_i32_with_sig`.
- **Public re-exports:** `object_bytes_from_ir`, `run_lpir_function_i32` behind the feature.
- **Tests:** ELF magic (feature on); ignored e2e Q32 `fadd` constants when builtins exe is built.

## Deferred

- Multi-return / struct-return in the emulator helper.
- GLSL-string `emu()` entry (plan scope was IR-only).
- CI guarantee for builtins ELF (developer / `#[ignore]` for now).

## Validate

- `cargo test -p lpir-cranelift`
- `cargo test -p lpir-cranelift --features riscv32-emu`
- `cargo clippy -p lpir-cranelift --all-features -- -D warnings`
