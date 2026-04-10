# FastAlloc v2 Roadmap

## Motivation

The first fastalloc attempt (Stage II M2) failed due to architectural issues: the `OperandSource` trait created an implicit sequential ordering contract between allocator and emitter. This contract was violated by Select32 and MemcpyWords (operand order mismatch) and IConst32 remat (count disagreement). Debugging was painful because the contract was implicit and untestable.

This roadmap implements a clean-slate fastalloc pipeline with:
1. **Debug-first architecture** - Every IR (LPIR, VInst, PhysInst) has textual representation
2. **CLI visibility** - `shader-rv32fa` command with `--trace`, `--show-vinst`, `--show-cfg`
3. **Filetest integration** - Automatic trace output on failure via DEBUG env
4. **PhysInst** - Concrete physical-register instruction stream as allocator output
5. **Functional emitter** - No indirection, just pattern match and encode
6. **Test infrastructure first** - Parsers/formatters before allocator logic

## Key Principle

> Compilers are black boxes. We need to see inside them to build and debug properly.

Every phase must provide visibility into its output before the next phase consumes it.

## Architecture

```
lp-shader/lpvm-native/src/isa/
├── rv32/                    # EXISTING - LinearScan/Greedy pipeline (untouched)
│   ├── abi.rs
│   ├── emit.rs
│   ├── inst.rs
│   ├── lower.rs
│   └── mod.rs
│
└── rv32fa/                  # NEW - FastAlloc pipeline
    ├── abi.rs               # ABI definitions (copy from rv32/)
    ├── inst.rs              # PhysInst enum
    ├── emit.rs              # Functional emitter: PhysInst[] -> bytes
    ├── alloc/               # Allocator modules
    │   ├── mod.rs           # FastAlloc main entry, public API
    │   ├── cfg.rs           # CFG construction and debug display
    │   ├── liveness.rs      # Liveness analysis and debug display
    │   ├── walk.rs          # Backward walk allocator core
    │   ├── spill.rs         # Spill slot management
    │   └── trace.rs         # AllocTrace system
    ├── debug/               # Debug formatting
    │   ├── vinst.rs         # VInst text format + parser
    │   ├── physinst.rs      # PhysInst text format + parser
    │   ├── cfg.rs           # CFG text format
    │   └── liveness.rs      # Liveness text format
    └── mod.rs

lp-cli/src/commands/
├── shader_rv32/             # EXISTING - old allocator command
└── shader_rv32fa.rs         # NEW - fastalloc command
```

## Milestones

| M | Name | Scope | Debug Output |
|---|------|-------|--------------|
| **M0** | VInst Textual IR | VInst parser/formatter | `lp-cli shader-lpir --show-vinst` |
| **M1** | Core Types | PhysInst enum, ABI copy | `lp-cli shader-rv32fa --show-physinst` (parse check) |
| **M2** | CLI & Debug Infra | `shader-rv32fa` command, filetest wiring | `--trace`, `--show-vinst`, `--show-physinst` flags |
| **M3** | Functional Emitter | PhysInst -> bytes | `--emit` to see machine code |
| **M4** | Allocator Shell | CFG, liveness, trace structure | `--show-cfg`, `--show-liveness`, trace with stubbed decisions |
| **M5** | Allocation Core | Backward walk, LRU, spill | Full trace showing real decisions |
| **M6** | Call Clobbers | Caller-save handling | Trace shows spill/reload around calls |
| **M7** | Integration | Wire to emit_function_bytes | End-to-end with full trace on error |
| **M8** | Validation | Filetests, edge cases, cleanup | All filetests pass, trace useful |

## Success Criteria

1. **native-rv32-iadd.glsl** compiles and runs correctly
2. **debug1.glsl** (minimal failing case from v1) compiles and runs correctly  
3. **rainbow_flat.glsl** (simplified rainbow) compiles and runs correctly
4. Every IR stage has debug output: LPIR, VInst, CFG, liveness, PhysInst
5. Trace is useful for understanding allocator decisions
6. Error messages include trace and relevant IR state
7. Old rv32/ pipeline can be safely removed

## Key Decisions

- **Named fields in PhysInst**: Select32 has `cond`, `if_true`, `if_false` - no ordering confusion
- **Always-built trace**: Cheap to construct, formatted on demand, attached to errors
- **CFG for straight-line too**: Even simple functions have a CFG (single block) for consistency
- **Pure functions with Display**: Every analysis pass returns a value that can be printed
- **DEBUG env for filetests**: No `trace: true` directive needed, just `DEBUG=1 cargo test`

## When This Is Done

The `rv32/` directory can be deleted. The `rv32fa/` pipeline becomes the default. LinearScan and Greedy are removed.
