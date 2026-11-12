# M4: `lpvm-cranelift` RV32 Emulator

## Goal

Enable the RV32 emulation path in `lpvm-cranelift` for cross-testing and CI.
This adds object compilation, ELF linking, and emulator execution to the same
crate behind the `riscv32-emu` feature.

**Context:** The `lpir-cranelift` crate currently has `riscv32-emu` feature code
that uses `cranelift-object` to compile to RV32 machine code, links with a
builtins library, and runs in an emulator. This code is useful for:
1. Testing RISC-V code generation on host (CI)
2. Filetests that need RV32 semantics
3. Debugging embedded JIT issues by emulating the same code

This milestone is about ensuring this code works with the new trait API and
is properly integrated.

## Scope

The `riscv32-emu` feature adds:
- `ObjectModule` compilation (instead of `JITModule`)
- ELF linking with builtins
- `LpvmInstance` implementation that runs in emulator

**Architecture:** Same crate (`lpvm-cranelift`), feature-gated.

### Feature structure

```toml
[features]
default = ["std"]
std = ["cranelift-native", ...]
riscv32-emu = ["cranelift-object", "dep:object", "dep:rv32-emu", ...]
```

### Implementation

**New trait impls (behind `riscv32-emu` feature):**
- `CraneliftEmuEngine` — implements `LpvmEngine`, uses `ObjectModule`
- `CraneliftEmuModule` — implements `LpvmModule`, holds object bytes + metadata
- `CraneliftEmuInstance` — implements `LpvmInstance`, runs in emulator

**Existing code to adapt:**
- `object_bytes_from_ir()` — compile LPIR to object file
- `link_object_with_builtins()` — link with builtins → ELF
- `glsl_q32_call_emulated()` / `run_lpir_function_i32()` — emulator runner

### Trait mapping

| LPVM trait     | RV32 emu implementation                | Notes                                      |
|----------------|------------------------------------------|--------------------------------------------|
| `LpvmEngine`   | Cranelift object compiler (RV32 triple)  | Creates `ObjectModule`, emits RV32 code    |
| `LpvmModule`   | Linked ELF + metadata                    | Contains executable image, symbols         |
| `LpvmInstance` | Emulator runtime                         | Loads ELF, runs in rv32-emu                |

## Unit Tests

- Compile LPIR to RV32 object code
- Link with builtins to create ELF
- Instantiate and call via emulator
- Verify output matches JIT execution (same LPIR)

## What NOT To Do

- Do NOT make this the default — JIT is the product (M3)
- Do NOT require this for embedded builds
- Do NOT move the code to a separate crate (keep in `lpvm-cranelift`)

## Validation

```bash
# Host build with emu feature
cargo check -p lpvm-cranelift --features riscv32-emu

# Tests (host only)
cargo test -p lpvm-cranelift --features riscv32-emu
```

## Done When

- `riscv32-emu` feature compiles
- `LpvmEngine`/`LpvmModule`/`LpvmInstance` work via emulator
- Filetests can use emulator path via feature flag
- Unit tests pass
