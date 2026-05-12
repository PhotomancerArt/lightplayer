# M4: rt_jit - JIT Buffer Compilation - Design

## Scope

Implement direct JIT buffer output for on-device compilation on RISC-V targets, bypassing ELF emission and linking. Enables fw-emu and fw-esp32 to compile and execute GLSL shaders directly.

### In Scope

- `rt_jit` module (RISC-V only): JIT buffer compilation
- `BuiltinTable`: Map symbol names to function addresses (inline match, no codegen)
- `JitEmitContext`: Emit machine code + record relocations
- `JitBuffer`: Executable buffer allocation and management
- Firmware integration: Populate `BuiltinTable` at startup

## Proposed Implementation Phases

| Phase | Focus | Deliverable | File |
|-------|-------|-------------|------|
| 1 | Create `rt_jit` module and `JitBuffer` | Module structure, executable memory | `01-phase-rt-jit-module.md` |
| 2 | Create `BuiltinTable` | Symbol resolution (inline match, no codegen) | `02-phase-builtin-table.md` |
| 3 | Create `JitEmitContext` | Code emission + relocations | `03-phase-jit-emit.md` |
| 4 | Create `NativeJitEngine/Module/Instance` | Lpvm trait implementations | `04-phase-engine-module.md` |
| 5 | Firmware integration | fw-esp32 + fw-emu initialization | `05-phase-firmware-integration.md` |
| 6 | Testing and cleanup | Filetests, binary size, docs | `06-phase-testing-cleanup.md` |

### Out of Scope

- ELF removal (keep for host testing)
- Lazy linking / deferred resolution
- PIC optimization
- Multiple JIT code sections

## File Structure

```
lp-shader/lpvm-native/src/
├── lib.rs                          # UPDATE: Add rt_jit module (riscv32 gated)
├── rt_jit/                         # NEW: JIT compilation module (riscv32 only)
│   ├── mod.rs                      # NEW: Module exports
│   ├── buffer.rs                   # NEW: JitBuffer - executable memory
│   ├── builtins.rs                 # NEW: BuiltinTable - symbol → address
│   ├── compiler.rs                 # NEW: LPIR → JIT buffer
│   ├── module.rs                   # NEW: NativeJitModule (LpvmModule impl)
│   ├── engine.rs                   # NEW: NativeJitEngine (LpvmEngine impl)
│   └── instance.rs                 # NEW: NativeJitInstance (LpvmInstance impl)
├── rt_emu/                         # EXISTING: Emulator path (host)
│   └── ...
└── isa/rv32/
    └── emit.rs                     # UPDATE: Add emit_function_to_jit()

fw-esp32/src/
└── main.rs                         # UPDATE: Populate BuiltinTable at startup

fw-emu/src/
└── main.rs                         # UPDATE: Populate BuiltinTable at startup
```

## Conceptual Architecture

### JIT Pipeline (RISC-V Firmware)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              FIRMWARE STARTUP                                 │
│  ┌────────────────────────────────────────────────────────────────────────┐  │
│  │ Build BuiltinTable                                                     │  │
│  │   for bid in BuiltinId::all() {                                        │  │
│  │       table.register(bid.name(), builtin_address(bid));                │  │
│  │   }                                                                    │  │
│  └────────────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────────┘
                                       │
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                              SHADER COMPILATION                               │
│                                                                               │
│  ┌─────────────┐   ┌─────────────┐   ┌─────────────┐   ┌─────────────────┐  │
│  │   LPIR      │ → │   Lower     │ → │  RegAlloc   │ → │  Emit VInsts    │  │
│  │  Module     │   │   to VInst  │   │  (Linear)   │   │  + Relocations  │  │
│  └─────────────┘   └─────────────┘   └─────────────┘   └─────────────────┘  │
│                                                               │               │
│                                                               ▼               │
│  ┌────────────────────────────────────────────────────────────────────────┐  │
│  │ Finalize: Resolve Relocations                                        │  │
│  │   for reloc in relocs {                                                │  │
│  │       addr = builtin_table.lookup(reloc.symbol);                      │  │
│  │       patch_auipc_jalr(reloc.offset, addr);                            │  │
│  │   }                                                                    │  │
│  └────────────────────────────────────────────────────────────────────────┘  │
│                                       │                                       │
│                                       ▼                                       │
│  ┌────────────────────────────────────────────────────────────────────────┐  │
│  │ JitBuffer: Executable Memory                                           │  │
│  │   ptr: *mut u8 (allocated via alloc::alloc)                             │  │
│  │   code: [u8; N] (patched machine code)                                 │  │
│  └────────────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────────┘
                                       │
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                              SHADER EXECUTION                                 │
│                                                                               │
│  ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────────────┐   │
│  │  Set up args    │ →  │  call entry_ptr │ →  │  Builtin executes       │   │
│  │  (vmctx + args) │    │  (direct call)  │    │  (e.g., __lp_lpir_fadd) │   │
│  └─────────────────┘    └─────────────────┘    └─────────────────────────┘   │
│                                                             │                 │
│                                                             ▼                 │
│                                                  ┌─────────────────┐         │
│                                                  │  Return result  │         │
│                                                  │  (a0/a1 or sret)│         │
│                                                  └─────────────────┘         │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Comparison: JIT vs ELF Paths

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          SHARED CODE (95%)                                    │
│  ┌─────────────┐   ┌─────────────┐   ┌─────────────┐   ┌─────────────────┐  │
│  │   LPIR      │ → │   Lower     │ → │  RegAlloc   │ → │  VInst Emission │  │
│  │  Module     │   │   to VInst  │   │  (Linear)   │   │  (same emitter) │  │
│  └─────────────┘   └─────────────┘   └─────────────┘   └─────────────────┘  │
└─────────────────────────────────────────────────────────────────────────────┘
                          │                           │
          ┌───────────────┘                           └───────────────┐
          ▼                                                           ▼
┌─────────────────────────────────────┐    ┌─────────────────────────────────────┐
│           ELF PATH (Host)            │    │           JIT PATH (RV32)          │
│                                      │    │                                      │
│  ┌───────────────────────────────┐  │    │  ┌───────────────────────────────┐  │
│  │ Wrap in ELF                   │  │    │  │ Allocate JitBuffer            │  │
│  │   object crate                │  │    │  │   alloc::alloc                │  │
│  └───────────────────────────────┘  │    │  └───────────────────────────────┘  │
│                  │                   │    │                  │                   │
│  ┌───────────────▼───────────────┐  │    │  ┌───────────────▼───────────────┐  │
│  │ Link with builtins            │  │    │  │ Resolve relocations           │  │
│  │   lp-riscv-elf loader         │  │    │  │   inline patching             │  │
│  └───────────────────────────────┘  │    │  └───────────────────────────────┘  │
│                  │                   │    │                  │                   │
│  ┌───────────────▼───────────────┐  │    │  ┌───────────────▼───────────────┐  │
│  │ Load into emulator            │  │    │  │ Direct execution              │  │
│  │   lp-riscv-emu                │  │    │  │   call ptr()                  │  │
│  └───────────────────────────────┘  │    │  └───────────────────────────────┘  │
└─────────────────────────────────────┘    └─────────────────────────────────────┘
```

## Main Components

### 1. BuiltinTable (`rt_jit/builtins.rs`)

Maps symbol names to function addresses. Populated once at firmware startup by iterating over `BuiltinId::all()`.

```rust
pub struct BuiltinTable {
    symbols: BTreeMap<&'static str, usize>,
}

impl BuiltinTable {
    pub fn new() -> Self;
    
    /// Populate table by iterating BuiltinId::all()
    /// Uses inline match for address lookup (no codegen needed)
    pub fn populate(&mut self) {
        for bid in BuiltinId::all() {
            if let Some(addr) = builtin_address(bid) {
                self.symbols.insert(bid.name(), addr);
            }
        }
    }
    
    pub fn lookup(&self, name: &str) -> Option<usize>;
}

/// Get address of a builtin by ID (inline match, same pattern as cranelift)
fn builtin_address(bid: BuiltinId) -> Option<usize> {
    use lps_builtins::builtins::*;
    match bid {
        BuiltinId::LpLpirFaddQ32 => Some(lpir::fadd_q32::__lp_lpir_fadd_q32 as usize),
        BuiltinId::LpLpirFmulQ32 => Some(lpir::fmul_q32::__lp_lpir_fmul_q32 as usize),
        // ... all builtins (inline match, no codegen)
        _ => None,
    }
}
```

### 2. JitEmitContext (`rt_jit/compiler.rs`)

Emits machine code and records relocations (following cranelift pattern).

```rust
pub struct JitEmitContext<'a> {
    code: Vec<u8>,
    relocs: Vec<NativeReloc>,
    builtin_table: &'a BuiltinTable,
}

impl<'a> JitEmitContext<'a> {
    pub fn new(builtin_table: &'a BuiltinTable) -> Self;
    
    /// Emit one function to JIT buffer
    pub fn emit_function(&mut self, func: &IrFunction, ...) -> Result<(), NativeError>;
    
    /// Finalize: resolve relocations and return executable buffer
    pub fn finalize(self) -> Result<JitBuffer, NativeError>;
}
```

### 3. JitBuffer (`rt_jit/buffer.rs`)

Executable memory buffer.

```rust
pub struct JitBuffer {
    ptr: *mut u8,
    len: usize,
    capacity: usize,
}

impl JitBuffer {
    /// Get entry point for a function
    /// 
    /// # Safety
    /// Caller must ensure offset is valid and buffer contains valid RISC-V code
    pub unsafe fn entry_ptr(&self, offset: usize) -> *const u8;
}

impl Drop for JitBuffer {
    fn drop(&mut self) {
        // Deallocate via alloc::dealloc
    }
}
```

### 4. NativeJitEngine (`rt_jit/engine.rs`)

`LpvmEngine` implementation for JIT compilation.

```rust
pub struct NativeJitEngine {
    builtin_table: BuiltinTable,
    options: NativeCompileOptions,
}

impl LpvmEngine for NativeJitEngine {
    type Module = NativeJitModule;
    type Error = NativeError;
    
    fn compile(&self, ir: &IrModule, meta: &LpsModuleSig) -> Result<Self::Module, Self::Error>;
    fn memory(&self) -> &dyn LpvmMemory;
}
```

### 5. NativeJitModule/Instance (`rt_jit/module.rs`, `rt_jit/instance.rs`)

`LpvmModule` and `LpvmInstance` implementations.

```rust
pub struct NativeJitModule {
    ir: IrModule,
    meta: LpsModuleSig,
    buffer: JitBuffer,
    entry_offsets: BTreeMap<String, usize>, // function name → offset in buffer
    options: NativeCompileOptions,
}

impl LpvmModule for NativeJitModule {
    type Instance = NativeJitInstance;
    // ...
}
```

## How Components Interact

1. **Firmware Startup**
   - Call `ensure_builtins_referenced()` to prevent dead code elimination
   - Build `BuiltinTable` by iterating `BuiltinId::all()`
   - Create `NativeJitEngine` with the table

2. **Compile Shader**
   - `NativeJitEngine::compile()` called with LPIR module
   - Creates `JitEmitContext` with reference to `BuiltinTable`
   - For each function: emit VInsts, record relocations
   - `finalize()`: Resolve relocations by looking up addresses, patch code
   - Return `NativeJitModule` with `JitBuffer` and entry offsets

3. **Execute Shader**
   - `NativeJitModule::instantiate()` creates `NativeJitInstance`
   - `NativeJitInstance::call_q32()`:
     - Look up function entry offset
     - Set up vmctx + args on stack
     - `unsafe { call(entry_ptr, args...) }`
     - Return results

## Key Design Decisions

1. **Cranelift-style finalize pattern**: Emit code with relocations, then patch in place
2. **BuiltinTable at startup**: One-time construction, O(log n) lookups during compile
3. **RISC-V target gating**: `rt_jit` only compiles on `#[cfg(target_arch = "riscv32")]`
4. **Simple heap allocation**: `alloc::alloc` for JIT buffers (debug fragmentation if needed)
5. **Inline builtin_address()**: No codegen needed - just match on BuiltinId and cast function pointers

## Deliverables

| Deliverable | Location | Description |
|-------------|----------|-------------|
| `rt_jit/` module | `lpvm-native/src/rt_jit/` | JIT compilation infrastructure |
| `BuiltinTable` | `rt_jit/builtins.rs` | Symbol → address mapping (populated at startup) |
| `JitBuffer` | `rt_jit/buffer.rs` | Executable memory management |
| `NativeJitEngine` | `rt_jit/engine.rs` | `LpvmEngine` implementation |
| `builtin_address()` | `rt_jit/builtins.rs` | Inline match for builtin addresses |
| Firmware integration | `fw-*/src/main.rs` | Table population at startup |

## Dependencies

- M3: Linear scan regalloc ✅ (in place)
- M2: Full lowering ✅ (in place)
- Cranelift JIT pattern analysis ✅ (completed)

## Validation Plan

1. **Host**: ELF filetests continue to pass (same lowering/regalloc)
2. **fw-emu**: JIT compiles and executes simple shaders
3. **fw-esp32**: Real hardware validation
4. If needed: Create `EmuJit` using `lp-riscv-emu-guest` for more direct testing
