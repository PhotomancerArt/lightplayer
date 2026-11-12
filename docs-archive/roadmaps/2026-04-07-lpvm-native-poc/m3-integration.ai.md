# Milestone 3: ELF, Integration, Lpvm Implementation

**Goal**: Complete end-to-end pipeline. Fill in Lpvm traits, emit linked ELF, execute in emulator.

## Suggested Plan Name

`lpvm-native-m3`

## Scope

### In Scope

- Minimal ELF emitter: `.text`, `.symtab`, `.strtab`, `.rel.text`
- Complete `NativeEngine::compile()` implementation
- Complete `NativeModule::instantiate()` implementation
- Complete `NativeInstance::call()` implementation
- `rv32lp.q32` target in `lps-filetests` harness
- Link with builtins via `lp-riscv-elf::link_object_with_builtins()`
- Execute `op-add.glsl` in `lp-riscv-emu`

### Explicitly Out of Scope

- No JIT buffer output (M4 or beyond)
- No control flow tests (use only simple arithmetic test)
- No spilling optimization (greedy is sufficient)
- No float mode (Q32 only)

## Key Decisions

### ELF Structure

Minimal but valid relocatable ELF:

```
ELF Header
Program Headers (none for relocatable)
Section Table:
  .text       - Code bytes
  .symtab     - Symbols (undefined: __lp_lpir_fadd_q32)
  .strtab     - Symbol string table
  .rel.text   - Relocations (which bytes need patching)
  .shstrtab   - Section name string table
```

Reference: `cranelift_object` or `goblin` crate for format, but implement minimal version to control size.

### Lpvm Trait Completion

```rust
impl LpvmEngine for NativeEngine {
    fn compile(&self, ir: &IrModule, meta: &LpsModuleSig) -> Result<Arc<dyn LpvmModule>, Error> {
        // 1. Lower all functions
        // 2. Allocate registers (greedy)
        // 3. Emit RV32 bytes + relocs
        // 4. Emit ELF object
        // 5. Link with builtins
        // 6. Return NativeModule with executable bytes
    }
}

impl LpvmModule for NativeModule {
    fn instantiate(&self) -> Result<Box<dyn LpvmInstance>, Error> {
        // Create execution context (vmctx)
        // Return NativeInstance
    }
}

impl LpvmInstance for NativeInstance {
    fn call(&mut self, name: &str, args: &[LpsValue]) -> Result<CallResult, Error> {
        // Set up argument registers/memory
        // Call lp-riscv-emu::run_function(code_ptr, args, vmctx)
        // Return results
    }
}
```

### Filetest Integration

Add to `lps-filetests/src/lib.rs`:

```rust
pub enum TestBackend {
    // ... existing
    Rv32lpQ32,  // New native backend
}

fn run_test(backend: TestBackend, test: &FileTest) -> TestResult {
    match backend {
        // ...
        TestBackend::Rv32lpQ32 => {
            let native = lpvm_native::NativeEngine::new(options);
            let module = native.compile(&ir, &meta)?;
            let mut inst = module.instantiate()?;
            inst.call("add", &test.inputs)?
        }
    }
}
```

## Deliverables

| File                      | Contents                                              |
| ------------------------- | ----------------------------------------------------- |
| `src/output/elf.rs`       | ELF emitter: header, sections, relocations            |
| `src/output/link.rs`      | Interface to `lp-riscv-elf` linking                   |
| `src/lib.rs` updates      | Complete `compile()` implementation                   |
| `src/module.rs` updates   | Complete `instantiate()`, metadata access             |
| `src/instance.rs` updates | Complete `call()`, argument marshalling               |
| `src/isa/rv32/call.rs`    | Builtin call ABI: save ra, args in regs, jal, restore |

### Filetest Changes

| File                       | Change                                       |
| -------------------------- | -------------------------------------------- |
| `lps-filetests/src/lib.rs` | Add `Rv32lpQ32` to `TestBackend` enum        |
| `lps-filetests/Cargo.toml` | Add `lpvm-native` dependency (feature-gated) |

## Dependencies

- M2 complete (RV32 emission)
- `lp-riscv-elf` crate (for linking)
- `lp-riscv-emu` crate (for execution)
- `lps-filetests` crate (for integration)

## Estimated Scope

- ~1000 lines
- 2-3 days
- Complexity: ELF format details, linking interface, execution marshalling

## Validation

**Unit test** (ELF roundtrip):

```bash
# Generate ELF, verify with readelf
riscv64-unknown-elf-readelf -a target/test_output.o
```

**Integration test** (execution):

```bash
# Single test
cargo test -p lps-filetests --test scalar_int_op_add --features native-backend
# Or however we invoke specific filetests
```

**Success criteria**:

```bash
./scripts/filetests.sh scalar/int/op-add.glsl rv32lp.q32
# Produces: PASS or numeric result matching expected
```

Expected behavior: Test compiles, links, executes, returns correct value for `add(1.0, 2.0)` → `3.0` (Q32 representation).
