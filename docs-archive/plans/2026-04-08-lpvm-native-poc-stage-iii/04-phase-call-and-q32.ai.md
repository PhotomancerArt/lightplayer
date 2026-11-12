## Phase 4: `NativeEmuInstance::call` and `call_q32`

### Scope

Implement `LpvmInstance` for `NativeEmuInstance` in `rt_emu/instance.rs`:

- `call(name, &[LpsValueF32])` → validate Q32 mode, flatten args, execute, decode return
- `call_q32(name, &[i32])` → flat i32 args, execute, return words
- `debug_state()` → `last_debug` clone (populated on emulator failure)

Private helpers:
- `refresh_vmctx_header()` — reset fuel/trap before each call
- `invoke_flat(name, &[i32])` → core emulation logic:
  - Resolve symbol from `ElfLoadInfo.symbol_map`
  - Build full arg list (a0=vmctx, a1+=args)
  - Create `Riscv32Emulator` with linked code/ram
  - Execute via `call_function` or `call_function_with_struct_return`
  - Map `DataValue` results to `i32` words

### Code organization

- `rt_emu/instance.rs` — instance struct + `LpvmInstance` impl + helpers
- All emulation logic isolated to this file

### Implementation details

Match `lpvm_emu::EmuInstance` patterns:
- Parameter validation (out/inout not supported yet)
- Arity checking against both `LpsModuleSig` and `IrFunction`
- Use `flat_q32_words_from_f32_args`, `decode_q32_return`, `q32_to_lps_value_f32`

### Tests

```bash
cargo check -p lpvm-native --features emu
cargo test -p lpvm-native --features emu --lib
```
