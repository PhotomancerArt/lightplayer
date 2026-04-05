# Plan: VMContext-enabled builtins PoC

## Overview

Add optional VMContext parameter to builtins, creating a proof-of-concept builtin (`__lp_get_fuel`)
that reads from the VMContext. This establishes infrastructure for texture sampling and other
context-aware builtins.

## Design

### Builtin Definition

Builtin GLSL files can declare `vmcontext` as a special parameter type:

```glsl
// In lp-glsl-builtins-emu/builtins/vm/get_fuel.glsl
uint __lp_get_fuel(vmcontext ctx);
```

The `vmcontext` type is:

- Recognized by the codegen as a marker for `needs_vmctx`
- Stripped from the user-visible signature
- Passed implicitly as `&VmContext` to the implementation

### VmContext Structure

```rust
// In lp-glsl-lpvm/src/vmcontext.rs
#[repr(C)]
pub struct VmContext {
    /// Remaining instruction fuel (decremented by interpreter/JIT)
    pub fuel: u64,

    /// Trap handler function pointer (if fuel reaches 0)
    pub trap_handler: u32,

    /// Pointer to metadata describing globals/uniforms layout
    pub metadata: *const GlslType,

    // Globals and uniforms follow contiguously in memory after this header
}

impl VmContext {
    /// Get pointer to globals region (after header)
    pub fn globals_base(&self) -> *mut u8 {
        (self as *const _ as *const u8).add(core::mem::size_of::<VmContext>()) as *mut u8
    }

    /// Placeholder: Read a global value by index
    pub fn get_global(&self, index: usize) -> GlslValue {
        // TODO(Milestone 2): Implement using metadata
        unimplemented!()
    }

    /// Placeholder: Write a global value by index  
    pub fn set_global(&mut self, index: usize, value: GlslValue) {
        // TODO(Milestone 2): Implement using metadata
        unimplemented!()
    }
}
```

### Architecture

```
                    Builtin Definition
                           |
              (GLSL file with vmcontext param)
                           |
                           v
              +---------------------------+
              |  lp-glsl-builtins-gen     |
              |  Parse GLSL, detect       |
              |  vmcontext type, set      |
              |  needs_vmctx flag          |
              +---------------------------+
                           |
              +------------+------------+
              |                         |
              v                         v
    +-------------------+    +-------------------+
    | lp-glsl-builtin-ids|    | Code generation   |
    | (BuiltinSignature   |    | for host & WASM   |
    |  with needs_vmctx)  |    | implementations  |
    +-------------------+    +-------------------+
              |                         |
              v                         v
    +-------------------+    +-------------------+
    | lpir ImportDecl    |    | Rust/C impls      |
    | (needs_vmctx)      |    | receiving &VmCtx  |
    +-------------------+    +-------------------+
              |
              v
    +-------------------+
    | Lowering:         |
    | Add VMCTX_VREG    |
    | to Call args      |
    +-------------------+
              |
              v
    +-------------------+
    | Codegen:           |
    | Include vmctx in   |
    | import signature   |
    +-------------------+
```

## File Structure

```
lp-glsl/
├── lpvm/
│   └── src/
│       └── vmcontext.rs          # VmContext struct, methods, docs
│
├── lp-glsl-builtin-ids/
│   └── src/
│       └── lib.rs                # BuiltinSignature.needs_vmctx flag
│
├── lp-glsl-builtins-emu/
│   ├── builtins/
│   │   └── vm/
│   │       └── get_fuel.glsl     # PoC builtin definition
│   └── src/
│       └── lib.rs                # __lp_get_fuel implementation
│
├── lp-glsl-builtins-wasm/
│   └── src/
│       └── lib.rs                # WASM stub (or impl)
│
├── lp-glsl-builtins-gen/        # Codegen tool
│   └── src/
│       └── (update to detect vmcontext type)
│
├── lp-glsl-naga/
│   └── src/
│       ├── lower_ctx.rs          # Copy needs_vmctx from builtin IDs
│       └── lower_stmt.rs         # Conditionally add VMCTX_VREG
│
├── lp-glsl-filetests/
│   └── filetests/
│       └── vmcontext/
│           └── fuel-read.glsl    # PoC test
│
└── lpir-cranelift/
    └── src/
        └── emit/
            └── mod.rs            # Include vmctx in import signatures
```

## Phases

## Phase 1: Update VmContext with documentation and methods

### Scope

Enhance `VmContext` struct with proper documentation, metadata pointer, and placeholder methods for
global access.

### Implementation Details

**File: `lp-glsl-lpvm/src/vmcontext.rs`**

```rust
//! VmContext is the central runtime structure for shader execution.
//!
//! Memory layout:
//! ```
//! [0..8]   fuel: u64           - Remaining instruction budget
//! [8..12]  trap_handler: u32   - Function pointer for out-of-fuel trap
//! [12..16] metadata: *const GlslType - Describes globals/uniforms layout
//! [16..N]  globals/uniforms    - Contiguous storage (defined by metadata)
//! ```
//!
//! The VmContext is passed to builtins that need runtime access. It provides
//! safe methods for accessing globals/uniforms using the metadata.

#[repr(C)]
pub struct VmContext {
    pub fuel: u64,
    pub trap_handler: u32,
    pub metadata: *const GlslType,
}

impl VmContext {
    /// Size of header (before globals/uniforms region)
    pub const HEADER_SIZE: usize = core::mem::size_of::<VmContext>();
    
    /// Get base pointer to globals storage (read-only for uniforms)
    pub fn globals_base(&self) -> *const u8 {
        (self as *const _ as *const u8).wrapping_add(Self::HEADER_SIZE)
    }
    
    /// Get mutable pointer to globals storage
    pub fn globals_base_mut(&mut self) -> *mut u8 {
        (self as *mut _ as *mut u8).wrapping_add(Self::HEADER_SIZE)
    }
    
    /// Placeholder: Read global by index
    /// TODO(Milestone 2): Wire up with metadata
    pub fn get_global(&self, _index: usize) -> GlslValue {
        unimplemented!("globals access in Milestone 2")
    }
    
    /// Placeholder: Write global by index
    /// TODO(Milestone 2): Wire up with metadata  
    pub fn set_global(&mut self, _index: usize, _value: GlslValue) {
        unimplemented!("globals access in Milestone 2")
    }
    
    /// Placeholder: Read uniform by index
    /// TODO(Milestone 2): Wire up with metadata
    pub fn get_uniform(&self, _index: usize) -> GlslValue {
        unimplemented!("uniforms access in Milestone 2")
    }
}
```

### Validate

```bash
cargo check -p lpvm
cargo test -p lpvm
```

---

## Phase 2: Add needs_vmctx to builtin system

### Scope

Add `needs_vmctx` flag to `BuiltinSignature` and `ImportDecl`, update codegen to detect `vmcontext`
type.

### Implementation Details

**File: `lp-glsl-builtin-ids/src/lib.rs`**

Add to `BuiltinSignature`:

```rust
pub struct BuiltinSignature {
    pub id: BuiltinId,
    pub name: &'static str,
    pub params: &'static [GlslParamKind],
    pub returns: &'static [GlslParamKind],
    pub param_count: u32,
    pub needs_vmctx: bool,  // NEW
}
```

**File: `lp-glsl-builtins-gen`** (codegen tool)

Update parser to:

1. Recognize `vmcontext` as a reserved type name
2. Set `needs_vmctx = true` if any param has type `vmcontext`
3. Exclude `vmcontext` param from `params` array (it's implicit)

**File: `lpir/src/module.rs`**

Add to `ImportDecl`:

```rust
pub struct ImportDecl {
    pub module_name: String,
    pub func_name: String,
    pub param_types: Vec<IrType>,
    pub return_types: Vec<IrType>,
    pub needs_vmctx: bool,  // NEW
}
```

### Validate

```bash
cargo build -p lp-glsl-builtins-gen-app
cargo run -p lp-glsl-builtins-gen-app  # Regenerate builtins
```

---

## Phase 3: Wire through lowering

### Scope

Copy `needs_vmctx` from builtin IDs to ImportDecl, conditionally add VMContext to Call args.

### Implementation Details

**File: `lp-glsl-lp-glsl-naga/src/lower_ctx.rs`**

When building import map from builtin signatures, copy the flag:

```rust
import_decl.needs_vmctx = builtin_sig.needs_vmctx;
```

**File: `lp-glsl-lp-glsl-naga/src/lower_stmt.rs`**

In `lower_user_call()`, check before adding VMContext:

```rust
let mut arg_vs = Vec::new();

// Only add VMContext for:
// 1. User functions (callee >= import_count) - always
// 2. Builtin imports that need it
let is_user_func = callee_ref.0 >= import_count;
let is_vmctx_builtin = if callee_ref.0 < import_count {
    let import_idx = callee_ref.0 as usize;
    ctx.ir.imports[import_idx].needs_vmctx
} else {
    false
};

if is_user_func || is_vmctx_builtin {
    arg_vs.push(VMCTX_VREG);
}

// ... rest of arg processing
```

### Validate

```bash
cargo check -p lp-glsl-naga
cargo test -p lp-glsl-naga
```

---

## Phase 4: Wire through Cranelift codegen

### Scope

Include VMContext in import signatures when `needs_vmctx` is true.

### Implementation Details

**File: `lp-glsl-lpir-cranelift/src/emit/mod.rs`**

In `signature_for_ir_func()`, check if callee is an import with `needs_vmctx`:

```rust
// For imports, check if they need VMContext
let needs_vmctx = if let Some(import_idx) = callee_as_import(func, callee) {
    ctx.ir.imports[import_idx].needs_vmctx
} else {
    // User functions always need VMContext
    true
};

if needs_vmctx {
    sig.params.push(AbiParam::new(types::I32)); // VMContext
}
```

(Note: Need to handle how we know the callee when building signature - may need to pass callee info
or check differently.)

Actually, signature building happens per-function, not per-call. So the function definition itself
needs to carry `needs_vmctx`.

Better approach: `IrFunction` already has `vmctx_vreg` - that's for user functions. For imports, we
check `ImportDecl.needs_vmctx` when generating the import's signature in the module context.

### Validate

```bash
cargo check -p lpir-cranelift
cargo test -p lpir-cranelift
```

---

## Phase 5: Create PoC builtin

### Scope

Create `__lp_get_fuel()` builtin that reads `fuel` from VMContext.

### Implementation Details

**File: `lp-glsl-builtins-emu/builtins/vm/get_fuel.glsl`**

```glsl
// Returns remaining instruction fuel from VMContext
uint __lp_get_fuel(vmcontext ctx);
```

**File: `lp-glsl-builtins-emu/src/lib.rs`**

```rust
use lpvm::vmcontext::VmContext;

/// Get remaining instruction fuel from VMContext
/// 
/// # Safety
/// ctx must be a valid pointer to VmContext
pub unsafe extern "C" fn __lp_get_fuel(ctx: &VmContext) -> u32 {
    ctx.fuel as u32
}
```

**File: `lp-glsl-builtins-wasm/src/lib.rs`**

Stub or actual WASM implementation.

### Validate

```bash
cargo build -p lp-glsl-builtins-emu-app
```

---

## Phase 6: Test file and validation

### Scope

Create test file and verify PoC works.

### Implementation Details

**File: `lp-glsl-filetests/filetests/vmcontext/fuel-read.glsl`**

```glsl
// test run

// PoC: VMContext-enabled builtin reads fuel from runtime
uint test_fuel_read() {
    return __lp_get_fuel();
}

// run: test_fuel_read() > 0
```

### Validate

```bash
# Build all builtins
cargo build -p lp-glsl-builtins-emu-app
cargo build -p lp-glsl-builtins-wasm

# Run PoC test
cargo run -p lp-glsl-filetests-app -- test vmcontext/fuel-read.glsl

# Run full tests to check for regressions
cargo run -p lp-glsl-filetests-app -- test
```

---

## Phase 7: Cleanup & validation

### Scope

Final cleanup, documentation, commit.

### Tasks

1. Review all TODO comments
2. Check for unused imports/functions
3. Verify all tests pass
4. Update any relevant documentation

### Validate

```bash
cargo test -p lpvm -p lp-glsl-naga -p lpir-cranelift -p lp-glsl-filetests
cargo clippy -p lpvm -p lp-glsl-naga -p lpir-cranelift
```

---

## Notes

### Open questions for later milestones:

- How does the metadata pointer get initialized? (By host before shader execution)
- How do we handle the distinction between uniforms (read-only, set at init) vs globals (read-write,
  mutable)?
- Should VmContext methods for globals check permissions at runtime?

### Design decisions documented:

- **Option B** (vmcontext type) selected for builtin signatures
- **`lp-glsl-builtins-emu -> lpvm`** dependency is acceptable
- **Direct field access** for simple fields like `fuel`, methods for complex operations
