# M4: rt_jit - JIT Buffer Compilation - Notes

## Scope of Work

Implement direct JIT buffer output for on-device compilation, bypassing ELF emission and linking. This enables fw-emu and fw-esp32 to compile and execute GLSL shaders directly without the ELF linking step.

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

## Current State of Codebase

### Existing Architecture (rt_emu)

The current `rt_emu` module in `lpvm-native` implements the LpvmEngine trait for host-side emulation:

```
LPIR → emit_module_elf() → ELF bytes
  ↓
link_object_with_builtins() → ElfLoadInfo (code + ram + symbol_map)
  ↓
Riscv32Emulator → execute
```

Key files:
- `lp-shader/lpvm-native/src/rt_emu/engine.rs` - `NativeEmuEngine` implements `LpvmEngine`
- `lp-shader/lpvm-native/src/rt_emu/module.rs` - `NativeEmuModule` implements `LpvmModule`
- `lp-shader/lpvm-native/src/rt_emu/instance.rs` - `NativeEmuInstance` implements `LpvmInstance`
- `lp-shader/lpvm-native/src/isa/rv32/emit.rs` - `emit_module_elf()` produces ELF

### Cranelift JIT Pattern (Our Inspiration)

Cranelift's JIT backend (`cranelift-jit`) demonstrates the pattern we should follow:

**Phase 1: Code Emission with Relocations**
```rust
// 1. Emit code with placeholder instructions (auipc+jalr for RISC-V calls)
// 2. Record each relocation in ModuleReloc { offset, kind, name, addend }
// 3. Store in CompiledBlob { ptr, size, relocs }
```

**Phase 2: Symbol Registration**
```rust
// JITBuilder holds:
// - symbols: HashMap<String, *const u8>  // Pre-registered symbols
// - lookup_symbols: Vec<Box<dyn Fn(&str) -> Option<*const u8>>>  // Dynamic lookup
```

**Phase 3: Finalize + Relocate**
```rust
// finalize_definitions():
// 1. For each function in functions_to_finalize:
//    - Call perform_relocations(|name| get_address(name))
//    - Lookup symbol address (from compiled functions, symbols table, or lookup fn)
//    - Patch instruction at offset with resolved address
// 2. Mark memory executable
```

**RISC-V Call Relocation Example:**
```rust
// For Reloc::RiscvCallPlt (auipc+jalr pair):
let base = get_address(name);
let pcrel = (base as isize) - (at as isize);
let hi20 = pcrel.wrapping_add(0x800) & 0xFFFFF000;
let lo12 = pcrel.wrapping_sub(hi20) & 0xFFF;
// Patch auipc: inst = (inst & 0xFFF) | hi20
// Patch jalr: inst = (inst & 0xFFFFF) | (lo12 << 20)
```

### Emission Pipeline (Current vs Proposed)

**Current (ELF path):**
1. Lower LPIR ops to VInst via `lower_ops()`
2. Register allocation via `LinearScan` or `GreedyAlloc`
3. Emit machine code via `EmitContext::emit_vinst()`
4. Collect relocations in `NativeReloc` structures
5. **Wrap in ELF via `emit_module_elf()`**
6. **Link with builtins executable**
7. Load into emulator

**Proposed (JIT path):**
1. Lower LPIR ops to VInst via `lower_ops()`
2. Register allocation via `LinearScan` or `GreedyAlloc`
3. Emit machine code + record relocations
4. **Resolve relocations by looking up builtin addresses**
5. **Patch code in place**
6. Jump to code directly

Key difference: Steps 5-7 become a single "finalize" step that patches relocations inline. No ELF, no external linking.

### Firmware Builtin Exposure

The `lps-builtins` crate provides `builtin_refs.rs` (auto-generated) that references all builtin functions. The firmware (`fw-esp32`, `fw-emu`) links this crate and calls `ensure_builtins_referenced()` to prevent dead code elimination.

### Key Decisions (Following Cranelift Pattern)

**Q1: How should the firmware provide builtin addresses to the JIT compiler?**

Following cranelift's pattern:
```rust
// Option B (Runtime table + lookup function) - RECOMMENDED
pub struct BuiltinTable {
    symbols: BTreeMap<&'static str, usize>,  // name -> address
}

impl BuiltinTable {
    pub fn lookup(&self, name: &str) -> Option<usize> {
        self.symbols.get(name).copied()
    }
}

// Engine holds the table
pub struct NativeJitEngine {
    builtin_table: BuiltinTable,
    options: NativeCompileOptions,
}
```

This mirrors `JITBuilder::symbols` + `JITBuilder::lookup_symbols`. The firmware constructs this table at startup by taking addresses of all `__lp_*` functions.

**Q2: How should JIT buffer allocation work on bare metal?**

Following cranelift's `JITMemoryProvider` pattern:
```rust
pub trait JitMemoryProvider {
    fn allocate_executable(&mut self, size: usize, align: usize) -> *mut u8;
    fn finalize(&mut self);  // Make executable (no-op on ESP32)
}

// For ESP32: Simple heap allocation (DRAM is executable by default on ESP32-C6)
struct HeapMemoryProvider;
impl JitMemoryProvider for HeapMemoryProvider {
    fn allocate_executable(&mut self, size: usize, align: usize) -> *mut u8 {
        alloc::alloc(alloc::Layout::from_size_align(size, align).unwrap())
    }
    fn finalize(&mut self) {}  // ESP32 doesn't need explicit permission changes
}
```

**Q3: Should we keep the ELF path for firmware at all?**

**Option A: Dual path** - Keep both:
- ELF path for host filetests (with emulator)
- JIT path for firmware (direct execution)

The JIT path is the primary product. ELF remains for testing/validation.

**Q4: How to handle executable memory permissions on ESP32-C6?**

ESP32-C6 DRAM (0x3FC0_0000) IS executable by default. No special handling needed - just allocate from heap.

**Q5: What is the interface between firmware and lpvm-native for builtin table?**

Following cranelift's `JITBuilder::symbols`:
```rust
// Firmware constructs table at startup
let mut table = BuiltinTable::new();
table.register("__lp_lpir_fadd_q32", __lp_lpir_fadd_q32 as usize);
table.register("__lp_lpir_fmul_q32", __lp_lpir_fmul_q32 as usize);
// ... all builtins

// Pass to engine
let engine = NativeJitEngine::new(table, options);
```

## Questions for User Confirmation

### Q1: Confirm cranelift-style JIT approach

**Context:** Based on cranelift's JIT implementation, I propose we follow their pattern:

1. **Emit code** with placeholder instructions (auipc+jalr for RISC-V calls) + record relocations
2. **Register symbols** in a `BuiltinTable` (name -> address mappings for all `__lp_*` functions)
3. **Finalize** by patching relocations: look up addresses, patch instructions in place
4. **Execute** directly from the buffer

This means:
- No ELF generation for firmware builds
- No separate linking step
- Direct code emission with inline patching
- `BuiltinTable` constructed at firmware startup

**Do you confirm this approach?** Any concerns about the "patch in place" strategy vs alternatives?

### Q2: How to construct BuiltinTable in firmware?

**Context:** The firmware needs to populate `BuiltinTable` with addresses of all builtin functions.

Options:
- **A: Manual registration** - Firmware code explicitly calls `table.register("__lp_lpir_fadd_q32", __lp_lpir_fadd_q32 as usize)` for each builtin
- **B: Generated code** - Build script generates a function that registers all builtins
- **C: Linker script symbols** - Use linker-defined symbols at known addresses

**Suggested: Option A** for now (explicit, clear, ~100 lines of init code). We can automate later.

**Which approach do you prefer?**

### Q3: Memory allocation strategy for JIT buffers

**Context:** Following cranelift's pattern, we need a `JitMemoryProvider` trait.

For **fw-emu** (host): Use `alloc::alloc` (heap is fine)
For **fw-esp32** (ESP32-C6): Use `alloc::alloc` (DRAM is executable by default)

**Any concerns about heap fragmentation** from repeatedly allocating/freeing JIT buffers? Should we consider a simple bump allocator or pool?

### Q4: Testing strategy

**Context:** We need to validate the JIT path works correctly.

Proposed strategy:
1. **Host filetests**: Continue using ELF + emulator (already works, tests same lowering/regalloc)
2. **fw-emu integration tests**: Add tests that use JIT path (compile → direct call, no emulation)
3. **fw-esp32**: Real hardware validation (manual for now)

The JIT and ELF paths share the same lowering + regalloc, so filetests validate most of the pipeline. JIT-specific issues would be in:
- Relocation resolution
- Memory allocation
- Direct function calls

**Does this testing strategy cover your concerns?**

### Q5: Feature flag structure

**Context:** We need to support both ELF and JIT paths conditionally.

Proposed feature flags for `lpvm-native`:
- `elf` (default on host): Enables ELF emission, depends on `object` crate
- `rt_jit` (default on firmware): Enables JIT buffer compilation
- `emu` (current): Enables emulator-based execution (requires `std` currently)

The firmware (`fw-esp32`, `fw-emu`) would use:
```toml
[dependencies]
lpvm-native = { path = "../../lp-shader/lpvm-native", default-features = false, features = ["rt_jit"] }
```

**Does this feature flag structure work for your build requirements?**

## Design Notes (Post-Cranelift Analysis)

### The Cranelift Pattern

Cranelift separates **compilation** from **finalization**:

```rust
// 1. Compilation: emit code + record relocations
let mut module = JITModule::new(builder);
module.define_function(func_id, ctx)?;  // emits code, records relocations

// 2. Finalization: resolve relocations + make executable
module.finalize_definitions()?;  // patches code, marks memory executable

// 3. Get pointer and call
let ptr = module.get_finalized_function(func_id);
```

We'll follow the same pattern:
```rust
// 1. Compilation: emit code + record relocations
let mut ctx = JitEmitContext::new(&builtin_table);
emit_function_to_jit(&mut ctx, func, ir, ...)?;  // emits code, records NativeRelocs

// 2. Finalization: resolve relocations
let buffer = ctx.finalize()?;  // patches auipc+jalr pairs with actual addresses

// 3. Get pointer and call
let ptr = buffer.entry_ptr();
```

### Memory Safety Considerations

JIT code execution is inherently unsafe:
- Code pointer must be properly aligned (4-byte for RISC-V)
- Buffer must be in executable memory (DRAM on ESP32-C6)
- No bounds checking on JIT code once jumped to

The `JitBuffer` type encapsulates these safety requirements:
```rust
pub struct JitBuffer {
    ptr: *mut u8,
    len: usize,
    capacity: usize,
}

impl JitBuffer {
    /// Get entry point for a function at given offset
    /// 
    /// # Safety
    /// Caller must ensure:
    /// - offset is valid (within buffer bounds, aligned)
    /// - buffer contains valid RISC-V code
    /// - function signature matches expected ABI
    pub unsafe fn entry_ptr(&self, offset: usize) -> *const u8;
}
```

### Performance Considerations

- JIT compilation happens once per shader
- Buffer allocation happens once per compilation
- Relocation resolution is O(n_relocs × log n_symbols) - negligible for shaders
- Direct function calls to builtins (no indirection through symbol table at runtime)

### Binary Size Impact

Positive impact on firmware:
- Remove `object` crate dependency (~50KB)
- Remove ELF emission code (~10KB)
- Remove linker/loader code from firmware path
- Net savings: ~60KB+ of flash

### Relocation Types for RISC-V

From cranelift's implementation, we need:
- `RiscvCallPlt`: auipc+jalr pair for calls (our current emit already does this)
- `RiscvPCRelHi20`: high 20 bits for auipc
- `RiscvPCRelLo12I`: low 12 bits for jalr/load/store

Our emit already generates auipc+jalr with placeholders. We just need to patch them during finalize.

### Dependencies

- M3: Linear scan producing correct code ✅ (already in place)
- M2: Full lowering (same pipeline, different output) ✅ (already in place)
- Cranelift JIT pattern analysis ✅ (just completed)
