# Milestone 4: rt_jit - JIT Buffer Compilation

**Goal**: Implement direct JIT buffer output for on-device compilation, bypassing ELF emission and linking.

## Suggested Plan

`lpvm-native-rt-jit-m4`

## Scope

### In Scope

- **JIT buffer emission**: Direct machine code to executable buffer (no ELF)
- **Builtin resolution**: Map `__lp_lpir_fadd_q32` etc to firmware builtin table addresses
- **Runtime linking**: Resolve symbols at compile time using builtin address table
- **Buffer management**: Allocate, write, seal, and jump to JIT code
- **Error handling**: Proper errors for unsupported ops in JIT mode

### Out of Scope

- Full ELF removal (keep for host testing)
- Lazy linking / symbol resolution deferral
- Position-independent code (PIC) optimization
- Multiple JIT code sections

## Key Decisions

1. **Buffer format**: Raw RISC-V machine code with inline builtin addresses
2. **Resolution time**: At compile_shader time, not load time (simpler, matches firmware needs)
3. **Builtin table**: Firmware provides address table; JIT resolves symbols to addresses
4. **Dual path**: Keep ELF for host filetests, JIT buffer for firmware

## Deliverables

| Deliverable | Location | Description |
|-------------|----------|-------------|
| `JitBuffer` | `rt_jit/buffer.rs` | Executable buffer allocation, code write, seal, jump |
| `BuiltinTable` | `rt_jit/builtins.rs` | Map symbol names to function addresses |
| `jit_compile_module` | `rt_jit/compiler.rs` | LPIR → JIT buffer (reuses lower + regalloc + emit) |
| `NativeJitModule` | `rt_jit/module.rs` | LpvmModule impl for JIT output |
| `NativeJitEngine` | `rt_jit/engine.rs` | LpvmEngine impl for JIT compilation |
| Firmware builtin table | `fw-emu/`, `fw-esp32/` | Expose current builtin addresses for resolution |

## Dependencies

- M3: Linear scan producing correct code
- M2: Full lowering (same pipeline, different output)
- Reference: Existing `lpvm-cranelift` JIT path for interface patterns

## Estimated Scope

- **Lines**: ~800-1200
- **Files**: 6-8 new/modified (`rt_jit/` module, firmware builtin tables)
- **Time**: 4-6 days

## Acceptance Criteria

1. `fw-emu` builds with native JIT backend feature
2. Rainbow shader compiles and executes in `fw-emu` via JIT path
3. Builtin calls resolve to correct addresses (psrdnoise, fadd_q32, etc)
4. No ELF linking step in firmware builds (direct code emission)
5. Filetests still pass via ELF path (host testing preserved)
