# M3 — `lpvm-native` integration (link, load, run)

Roadmap: [`docs/roadmaps/2026-04-07-lpvm-native-poc/m3-integration.md`](../../roadmaps/2026-04-07-lpvm-native-poc/m3-integration.md).

## Scope of work

Create `rt_emu` runtime module with `NativeEmuEngine` / `NativeEmuModule` / `NativeEmuInstance` that:
1. Compiles LPIR to RV32 ELF object (reuse M2 emission)
2. Links with builtins via `lpvm_cranelift::link_object_with_builtins`
3. Emulates execution via `lp-riscv-emu`

Expose as `rv32lp.q32` backend in `lps-filetests`.

Out of scope: JIT buffer (M4+), F32, full filetest matrix.

## File structure (final)

```
lp-shader/lpvm-native/
├── Cargo.toml                          # Feature "emu" gates rt_emu + deps
├── src/
│   ├── lib.rs                          # Re-exports, conditionally includes rt_emu
│   ├── error.rs                        # NativeError (shared)
│   ├── native_options.rs               # NativeCompileOptions (shared)
│   ├── lower.rs, regalloc/, isa/       # Shared lowering + emission
│   ├── debug_asm.rs                    # Annotated assembly (host debugging)
│   └── rt_emu/                         # Emulation runtime (feature "emu")
│       ├── mod.rs                      # Re-exports
│       ├── engine.rs                   # NativeEmuEngine (LpvmEngine impl)
│       ├── module.rs                   # NativeEmuModule (LpvmModule impl)
│       └── instance.rs                 # NativeEmuInstance (LpvmInstance impl)
lp-shader/lps-filetests/
├── Cargo.toml                          # lpvm-native with "emu" feature
├── src/targets/mod.rs                  # Backend::Rv32lp, ALL_TARGETS
└── src/test_run/filetest_lpvm.rs       # Native variants in enums
```

## Conceptual architecture

```
  GLSL / LPIR
       │
       ▼
  lower → regalloc → emit ELF .o
       │
       ▼
  link_object_with_builtins (Cranelift)
       │
       ▼
  ElfLoadInfo (code + ram + symbol_map)
       │
       ▼
  NativeEmuModule::instantiate
       │  alloc vmctx in EmuSharedArena
       │  init Riscv32Emulator
       ▼
  NativeEmuInstance::call / call_q32
       │  resolve symbol, set a0=vmctx, a1+=args, run
       ▼
   LpsValueF32 / Vec<i32>
```

## Main components

| Component | Responsibility | Feature |
|-----------|----------------|---------|
| **Core** (`lower.rs`, `regalloc/`, `isa/`) | LPIR → RV32 ELF emission | (none, always) |
| **`rt_emu` module** | Link + emulate runtime | `emu` |
| **`NativeEmuEngine`** | Compile + link, return linked module | `emu` |
| **`NativeEmuModule`** | Hold linked image, metadata, arena ref | `emu` |
| **`NativeEmuInstance`** | Per-instance vmctx + emulator execution | `emu` |
| **Filetests** | `rv32lp` backend targeting `NativeEmuEngine` | `emu` (host only) |

## Phases

See numbered phase files. Key changes from earlier drafts:
- Phase 1: Feature `emu` (not `builtin-link`), separate `rt_emu/` module
- Phase 2: Error variants for link/call/alloc behind `emu` feature
- Phase 3-4: `NativeEmuModule` / `NativeEmuInstance` in `rt_emu/`
- Phase 5-6: `Backend::Rv32lp` in filetests
- Phase 7: Smoke test with `rv32lp.q32`
- Phase 8: Cleanup, validation, AGENTS checks
