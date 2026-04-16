# Phase 6: Documentation

## Goal

Update crate documentation to explain the dual API and when to use each.

## README.md

```markdown
# lpvm-cranelift

Cranelift JIT backend for LPVM.

## Dual API

This crate has two APIs that coexist:

### New Trait API (added M3)

Use this for:
- Backend-agnostic code (generic over `LpvmModule`)
- Multiple instances from one compiled module
- VMContext-based state (fuel, globals)

```rust
use lpvm::LpvmEngine;
use lpvm_cranelift::CraneliftEngine;

let engine = CraneliftEngine::new(options);
let module = engine.compile(&ir, &meta)?;
let mut inst = module.instantiate()?;
let result = inst.call("render", &[LpsValue::F32(1.0)])?;
```

### Old API (legacy, removed M7)

Use this for:
- Hot-path direct calls (DirectCall)
- Current engine code (until M6 migration)
- Simple one-off compilation

```rust
use lpvm_cranelift::{jit, DirectCall};

let module = jit(source, &options)?;
let dc = module.direct_call("render")?;
dc.call_i32_buf(&vmctx, &args, &mut ret_buf);
```

## Features

- `std` — host-only features (native ISA detection)
- `riscv32-emu` — RV32 object compilation + emulator (M4)
- `cranelift-optimizer` — enable Cranelift optimizer
- `cranelift-verifier` — enable Cranelift verifier

## Targets

- Host (x64, aarch64): native JIT via `cranelift-native`
- Embedded RISC-V: hardcoded `riscv32imac` triple
```

## Crate-Level Documentation (lib.rs)

Add module-level docs explaining:
- The dual API situation
- Migration timeline (old API removed in M7)
- Which types implement which traits
- Performance notes (DirectCall vs trait call)

## Inline Documentation

- `CraneliftEngine`: compile LPIR to module
- `CraneliftModule`: immutable compiled code, can instantiate multiple times
- `CraneliftInstance`: per-instance state + callable interface
- `direct_call()`: hot path, use for per-pixel rendering

## Done When

- README.md explains dual API
- lib.rs has module documentation
- All public types have rustdoc
- Examples show both APIs
