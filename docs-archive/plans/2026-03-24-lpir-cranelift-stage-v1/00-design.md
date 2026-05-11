# Stage V1: RV32 object, linking, emulator — design

## Scope of work

Same as `00-notes.md`: RV32 object emission from LPIR, merge with builtins ELF,
run in `lp-riscv-emu`, validate in-crate. No `lps-filetests` wiring (V2).

## Decisions

- **Shared lowering:** One generic path over `cranelift_module::Module` for
  declare/import setup and per-function `define_function`; `JitModule` and
  `ObjectModule` differ only in ISA, builder, and finalize (`finalize_definitions`
  vs `finish`). Confirmed for implementation (no parallel copy of the loop).

## File structure

```
lp-shader/legacy/lpvm-cranelift/
├── build.rs                         # NEW (feature riscv32-emu): embed builtins ELF path
├── Cargo.toml                       # UPDATE: optional riscv32 / object / riscv deps
└── src/
    ├── lib.rs                       # UPDATE: cfg-gated re-exports
    ├── compile.rs                   # UPDATE (optional): emu_from_ir / glue to object+emu
    ├── jit_module.rs                # UPDATE: extract shared define loop
    ├── object_module.rs             # NEW: RV32 ISA, ObjectModule, finish → Vec<u8>
    ├── object_link.rs               # NEW: link object into builtins ELF (lp-riscv-elf)
    ├── emu_run.rs                   # NEW: ElfLoadInfo → emulator, invoke helpers
    ├── builtins.rs                  # UPDATE (if needed): object vs JIT declare parity
    └── emit/                        # UNCHANGED API surface for translation
```

## Conceptual architecture

```
                    IrModule + CompileOptions
                              │
              ┌───────────────┴───────────────┐
              ▼                               ▼
     host ISA + JITModule              riscv32 ISA + ObjectModule
     (existing JitModule)              (NEW: object bytes)
              │                               │
              │                               ▼
              │                    link + verify builtins (lp-riscv-elf)
              │                               │
              │                               ▼
              │                    Riscv32Emulator (lp-riscv-emu)
              │                               │
              └─────────── tests / future V2 filetests ───┘
```

- **Single emitter:** `emit::translate_function` and signatures stay shared;
  only the `Module` implementation and ISA differ.
- **Object path output:** relocatable ELF object bytes from Cranelift
  `ObjectModule::finish`.
- **Link step:** identical semantics to `lps-cranelift`:
  load base builtins executable, relocate/load shader object, verify `__lp_*`
  symbols from `BuiltinId` (or a filtered subset if we narrow declared imports).

## Main components

| Component          | Role                                                                                                                                                               |
|--------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `object_module.rs` | Build `OwnedTargetIsa` for `riscv32` (match old `default_riscv32_flags` + triple), `ObjectBuilder` → `ObjectModule`, run shared define loop, `finish` → `Vec<u8>`. |
| `jit_module.rs`    | Refactor: call shared `define_lpir_in_module`; keep JIT-only finalize + `JitModule` fields.                                                                        |
| `object_link.rs`   | Port logic from `builtins_linker.rs`: merge ELF, return `ElfLoadInfo` or error.                                                                                    |
| `emu_run.rs`       | Configure `Riscv32Emulator`, map symbol → PC, run until halt/timeout; helpers for simple scalar/Q32 returns.                                                       |
| `build.rs`         | When feature enabled, compile-time path to builtins ELF (same contract as old crate).                                                                              |

## Interactions

1. Tests (or future API) call `object_bytes_from_ir(&IrModule, &CompileOptions)`.
2. Linker merges with builtins blob → loaded image + symbol map.
3. Emulator runs entry symbol for a named function (tests pick a simple `@main`
   or first export).

## Dependencies

- Stages I–IV of `lpvm-cranelift` (emitter, builtins resolution, Q32, public
  compile options) are assumed available.
- External: `cranelift-object`, `object`, `lp-riscv-elf`, `lp-riscv-emu`,
  `lp-riscv-inst` (as needed), `cranelift-codegen` feature `riscv32`.
