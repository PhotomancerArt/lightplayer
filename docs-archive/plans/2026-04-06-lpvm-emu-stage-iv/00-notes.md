# M4: lpvm-cranelift RV32 Emulator — Stage IV Plan Notes

## Scope of Work

Enable the RV32 emulation feature in `lpvm-cranelift` behind the `riscv32-emu` feature.
This requires:

1. **Refactoring `lp-riscv-emu`** to separate code, RAM, and execution state for
   Module/Instance separation (the hard part)
2. **Adding LPVM trait implementations** in `lpvm-cranelift` for the emu path:
   - `CraneliftEmuEngine` (LpvmEngine) — compiles LPIR to RV32 object
   - `CraneliftEmuModule` (LpvmModule) — linked ELF image + metadata
   - `CraneliftEmuInstance` (LpvmInstance) — emulator runtime with fresh RAM

## Current State

### `lp-riscv-emu` Architecture (monolithic)

`Riscv32Emulator` owns everything in one struct:

```
Riscv32Emulator {
    regs: [i32; 32],           // per-instance
    pc: u32,                   // per-instance
    memory: Memory {          // fused code+RAM (problem!)
        code: Vec<u8>,         // should be shared (module)
        ram: Vec<u8>,          // per-instance
        code_start, ram_start
    },
    instruction_count: u64,    // per-instance
    traps: Vec<(u32, TrapCode)>, // code-level metadata (should be with code)
    log_buffer, serial_host, time_mode, // per-instance
}
```

**Problem:** `Memory` contains both code and RAM. You can't share code across
instances without cloning. Each `Riscv32Emulator::new(code, ram)` takes ownership
of both.

### `lpvm-cranelift` riscv32-emu Feature (existing)

Current functions work but don't fit the trait model:

- `object_bytes_from_ir()` — compile LPIR to RV32 object (Cranelift ObjectModule)
- `link_object_with_builtins()` — link with builtins library → `ElfLoadInfo { code, ram, symbol_map }`
- `glsl_q32_call_emulated(load, ir, meta, ...)` — one-shot: create emulator, run, discard
- `run_lpir_function_i32()` — convenience wrapper that does compile + link + run

## Questions

### Q1: How to restructure the emulator for Module/Instance separation?

**Context:** We need:
1. **CodeImage** (module-level, immutable, shareable): code bytes + trap map + entry points
2. **InstanceState** (per-instance): registers, PC, instruction count, log buffer, serial, time
3. **RamBuffer** (per-instance): mutable RAM for execution

**Current:** `Riscv32Emulator` has all three fused. `Memory` struct has `code: Vec<u8>` + `ram: Vec<u8>`.

**Suggested approach:**

```rust
// Module-level: immutable, shareable (Arc<CodeImage>)
pub struct CodeImage {
    code: Vec<u8>,
    code_start: u32,
    traps: Vec<(u32, TrapCode)>,
    symbol_map: BTreeMap<String, u32>, // from ELF linking
}

// Per-instance: execution state + RAM
pub struct Riscv32Emulator {
    regs: [i32; 32],
    pc: u32,
    ram: Vec<u8>,
    ram_start: u32,
    code: Arc<CodeImage>,  // reference, not owned
    instruction_count: u64,
    log_buffer, serial_host, time_mode, // ...
}
```

Memory access checks `code` region first (via `CodeImage`), then `ram`.

### Q2: How to preserve backward compatibility with existing consumers?

**Context:** Multiple crates use `Riscv32Emulator::new(code, ram)`:
- `fw-tests` — firmware integration tests
- `lp-riscv-elf` — ELF loading tests
- `lp-riscv-emu-guest-test-app` — guest test binary
- `lp-cli` — memory profiling
- `lp-client` — emulator transports
- `lpvm-cranelift` (riscv32-emu) — `glsl_q32_call_emulated`

**Suggested:** Provide backward-compatible constructors:

```rust
// New API (for LPVM traits)
impl Riscv32Emulator {
    pub fn with_code_image(code: Arc<CodeImage>, ram: Vec<u8>) -> Self { ... }
}

// Old API preserved (convenience that clones)
impl Riscv32Emulator {
    pub fn new(code: Vec<u8>, ram: Vec<u8>) -> Self {
        Self::with_code_image(Arc::new(CodeImage::from_code(code)), ram)
    }
}
```

Existing code continues to work. New trait-based code uses `CodeImage`.

### Q3: How to handle JIT-in-RAM capability?

**Context:** The emulator supports executing code from RAM (not just the code
region). This is used for on-device JIT where compiled code lives in RAM.

**Question:** Does the refactored model still support this?

**Suggested:** Yes. The execution loop checks `code` image first, then falls back
to RAM. For JIT-in-RAM scenarios, the `CodeImage` can be empty/placeholder,
and all execution happens from RAM. The refactored model doesn't change this
behavior — it just separates the concerns.

### Q4: Should `CodeImage` include the symbol map?

**Context:** `ElfLoadInfo` currently has `symbol_map: BTreeMap<String, u32>` for
resolving function names to entry points.

**Suggested:** Yes. `CodeImage` should include the symbol map so that:
- `CraneliftEmuModule` can resolve function names to addresses
- `CraneliftEmuInstance` can call by name

The `link_object_with_builtins()` function currently returns `ElfLoadInfo`;
it should return something convertible to `CodeImage`.

### Q5: How to integrate with LPVM traits in `lpvm-cranelift`?

**Context:** The trait implementations go in `lpvm-cranelift` (not `lp-riscv-emu`).

**Suggested structure:**

In `lpvm-cranelift` behind `riscv32-emu` feature:

```rust
// CraneliftEmuEngine: uses ObjectModule (not JITModule)
pub struct CraneliftEmuEngine { options: CompileOptions }
impl LpvmEngine for CraneliftEmuEngine {
    type Module = CraneliftEmuModule;
    fn compile(&self, ir: &IrModule, meta: &LpsModuleSig) -> Result<Self::Module, ...> {
        let object = object_bytes_from_ir(ir, &self.options)?;
        let linked = link_object_with_builtins(&object)?;
        CraneliftEmuModule::new(linked, meta.clone())
    }
}

// CraneliftEmuModule: holds linked ELF + metadata
pub struct CraneliftEmuModule {
    code_image: Arc<CodeImage>, // from lp-riscv-emu
    metadata: LpsModuleSig,
    ir: IrModule, // needed for signature reconstruction
}
impl LpvmModule for CraneliftEmuModule {
    type Instance = CraneliftEmuInstance;
    fn instantiate(&self) -> Result<Self::Instance, ...> {
        // Create fresh RAM buffer per instance
        let ram = vec![0u8; DEFAULT_RAM_SIZE];
        CraneliftEmuInstance::new(self.code_image.clone(), ram, self.metadata.clone(), ...)
    }
}

// CraneliftEmuInstance: emulator with fresh state
pub struct CraneliftEmuInstance {
    emu: Riscv32Emulator, // from lp-riscv-emu
    metadata: LpsModuleSig,
    ir: IrModule,
}
impl LpvmInstance for CraneliftEmuInstance {
    fn call(&mut self, name: &str, args: &[LpsValue]) -> Result<LpsValue, ...> {
        // Resolve name to address via CodeImage
        // Marshal args to i32
        // Run emulator
        // Decode return
    }
}
```

### Q6: Where does the VMContext fuel handling go?

**Context:** The emulator doesn't currently have fuel counting. The JIT uses
fuel in `VmContextHeader::fuel` (u64 at offset 0).

**Suggested:** The `CraneliftEmuInstance` can:
1. Allocate VMContext header at start of RAM (or separate buffer)
2. Write fuel to offset 0 before each call
3. Check fuel after call (the compiled code should decrement it)

Since the emulator runs the same compiled RV32 code, it will respect the
fuel field in memory. We just need to set it up correctly.

## Design Decisions

1. **Three-layer separation:** `CodeImage` (module), `Riscv32Emulator` (instance),
   `RamBuffer` (owned by instance). This matches the LPVM trait model.

2. **Backward compatibility:** Keep `Riscv32Emulator::new(code, ram)` working
   by creating a `CodeImage` internally.

3. **Symbol map in CodeImage:** Essential for name-based function calls in
the trait API.

4. **Fuel in CraneliftEmuInstance:** Setup/teardown around each call, not
   deep emulator changes.

5. **Trait impls in lpvm-cranelift:** Not in lp-riscv-emu. The emulator is
   a general-purpose tool; the trait wrapper is LPVM-specific.
