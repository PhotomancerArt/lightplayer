# WASM: Host/Guest Memory Management Architecture

## How WASM Mode Differs from Emulator

| Aspect | Emulator (RV32) | WASM |
|--------|-----------------|------|
| **Memory** | Separate `code` and `ram` vectors | Single `wasmtime::Memory` (linear memory) |
| **Builtins** | RISC-V code linked into guest image | WASM functions from `lp_glsl_builtins_wasm.wasm` |
| **VMContext** | Guest allocates, guest pointer | Host allocates, offset into linear memory |
| **Builtin→Memory** | Guest dereferences directly | Builtin accesses `env.memory` import |
| **Host→Memory** | Direct slice into `ram: Vec<u8>` | `memory.data(&store)` / `memory.data_mut(&store)` |
| **Allocator** | Guest `linked_list_allocator` | Host-managed (no guest allocator needed) |

## Current Bug (VMContext)

In `lp-glsl-filetests/src/test_run/wasm_runner.rs:340`:

```rust
// WRONG: Dummy zero passed, builtins crash when dereferencing
wasm_args.push(Val::I32(0)); // Dummy VMContext pointer
```

The builtin `__lp_vm_get_fuel_q32` expects a valid pointer into linear memory but receives `0`, causing a WASM trap when it tries to load from address 0.

## The Fix

Allocate VMContext in **WASM linear memory**, pass the **offset**:

```rust
// CORRECT: Allocate in WASM memory, pass offset
let vmctx_offset = allocate_in_wasm_memory(&mut store, &memory, size_of::<VmContext>(), align_of::<VmContext>())?;
write_vmcontext_to_memory(&memory, &mut store, vmctx_offset, &VmContext::default());
wasm_args.push(Val::I32(vmctx_offset as i32));  // Valid offset in linear memory
```

## WASM Memory Layout

```
WASM Linear Memory (env.memory)
├─ [0x0000..0x0100]       Stack (grows down from high addresses)
├─ [0x0100..0x1000]       Data segment (static globals, shadow stack base)
├─ [0x1000..0x????]       Free space
│   ├─ [vmctx_offset]     ← VMContext allocation (host-managed)
│   ├─ [tex_offset]       ← Texture data (host-managed)
│   └─ [...]              Future: uniforms, globals
└─ [heap_base..end]       Guest heap (if guest uses allocator)
```

## Architecture: Host-Managed WASM Memory

### Key Insight

WASM builtins already import `env.memory` and access it directly. The host doesn't need to call into the guest to allocate—the host can:
1. Reserve space in linear memory at instantiation time
2. Write data directly via `memory.data_mut()`
3. Pass offsets to shaders
4. Builtins access memory through their `env.memory` import

### Implementation

#### 1. Extend `wasm_link.rs` with Memory Reservation

```rust
/// Reserved regions in WASM linear memory
pub struct WasmMemoryLayout {
    pub vmcontext_offset: u32,
    pub vmcontext_size: u32,
    pub heap_start: u32,      // First free byte after reservations
}

/// After instantiating builtins, reserve space for host-managed objects
pub fn reserve_memory_regions(
    store: &mut Store,
    memory: &Memory,
) -> Result<WasmMemoryLayout, GlslError> {
    // Start after data segments (known from module instantiation)
    let mut next_offset = 0x1000u32; // Conservative: start after 4KB
    
    // Reserve VMContext
    let vmctx_size = size_of::<VmContext>() as u32;
    let vmctx_align = align_of::<VmContext>() as u32;
    let vmctx_offset = (next_offset + vmctx_align - 1) & !(vmctx_align - 1);
    next_offset = vmctx_offset + vmctx_size;
    
    // Future: reserve texture descriptor table, uniform buffers, etc.
    
    // Ensure memory is large enough
    let current_pages = memory.size(store);
    let needed_bytes = next_offset;
    let needed_pages = (needed_bytes + 65535) / 65536;
    if needed_pages > current_pages {
        memory.grow(store, needed_pages - current_pages)
            .map_err(|e| GlslError::new(ErrorCode::E0400, format!("memory grow failed: {e}")))?;
    }
    
    // Initialize VMContext
    let vmctx = VmContext::default();
    write_struct_to_memory(memory, store, vmctx_offset, &vmctx)?;
    
    Ok(WasmMemoryLayout {
        vmcontext_offset: vmctx_offset,
        vmcontext_size: vmctx_size,
        heap_start: next_offset,
    })
}

fn write_struct_to_memory<T: Sized>(
    memory: &Memory,
    store: &mut Store,
    offset: u32,
    value: &T,
) -> Result<(), GlslError> {
    let bytes = unsafe {
        core::slice::from_raw_parts(
            value as *const T as *const u8,
            size_of::<T>(),
        )
    };
    let mem_slice = memory.data_mut(store);
    let end = offset as usize + bytes.len();
    if end > mem_slice.len() {
        return Err(GlslError::new(ErrorCode::E0400, "memory write out of bounds"));
    }
    mem_slice[offset as usize..end].copy_from_slice(bytes);
    Ok(())
}
```

#### 2. Store Layout in `WasmExecutable`

```rust
pub struct WasmExecutable {
    store: Store<()>,
    instance: Instance,
    memory: Memory,              // Keep handle for host access
    memory_layout: WasmMemoryLayout, // Know where VMContext lives
    // ... other fields
}

impl WasmExecutable {
    pub fn from_source(source: &str, options: WasmOptions) -> Result<Self, GlslDiagnostics> {
        // ... existing setup ...
        
        let (instance, memory) = wasm_link::instantiate_wasm_module(&engine, &mut store, &wasm_bytes)?;
        let memory = memory.ok_or_else(|| GlslError::new(ErrorCode::E0400, "no memory export"))?;
        
        let memory_layout = wasm_link::reserve_memory_regions(&mut store, &memory)?;
        
        Ok(Self {
            store,
            instance,
            memory,
            memory_layout,
            // ...
        })
    }
}
```

#### 3. Pass VMContext Offset to Shader

In `build_wasm_args()` (replacing the dummy `0`):

```rust
fn build_wasm_args(
    export_info: &WasmExport,
    args: &[GlslValue],
    fm: lp_glsl_naga::FloatMode,
    vmctx_offset: i32,  // NEW: actual offset from memory_layout
) -> Result<Vec<Val>, GlslError> {
    // ... validation ...
    
    let mut wasm_args = Vec::new();
    // Pass VMContext offset (NOT zero!)
    wasm_args.push(Val::I32(vmctx_offset));
    
    for (v, ty) in args.iter().zip(export_info.param_types.iter()) {
        wasm_args.extend(glsl_value_to_wasm_flat(ty, v, fm)?);
    }
    
    Ok(wasm_args)
}
```

#### 4. Builtins Access VMContext

The builtin in `lp_glsl_builtins_wasm` receives the offset and accesses memory:

```rust
// In lp_glsl_builtins_wasm (WASM target)
#[unsafe(no_mangle)]
pub extern "C" fn __lp_vm_get_fuel_q32(vmctx_offset: i32) -> u32 {
    // Access env.memory through raw pointer (WASM linear memory is 0x0-based)
    let ptr = vmctx_offset as *const VmContext;
    unsafe { (*ptr).fuel as u32 }
}
```

The key difference: in WASM, `vmctx_offset` is a valid offset into the module's own linear memory, not a cross-process pointer.

## Texture Memory Strategy

Textures are larger and more dynamic than VMContext. Options:

### Option A: Host-Allocated Per-Texture (Simple)

```rust
impl WasmExecutable {
    pub fn allocate_texture(&mut self, width: u32, height: u32, format: PixelFormat) -> Result<TextureHandle, GlslError> {
        let size = texture_size(width, height, format);
        let offset = self.memory_layout.heap_start;
        let aligned_offset = align_up(offset, 16); // SIMD alignment
        
        // Grow memory if needed
        let end = aligned_offset + size;
        ensure_memory_size(&mut self.store, &self.memory, end)?;
        
        // Write texture data
        // ... caller provides pixel data via memory.data_mut()
        
        self.memory_layout.heap_start = end; // Bump allocator for textures
        
        Ok(TextureHandle { offset: aligned_offset, width, height, format })
    }
}
```

### Option B: Guest Heap (Complex, Rarely Needed)

If textures need to be freed/reallocated dynamically:
- Guest could expose allocator functions (like the emulator plan)
- Host calls into guest to allocate, receives offset
- But this is likely overkill—textures usually live for a frame/scene

### Recommendation

**Option A** (bump allocation for textures) because:
- Textures are usually loaded once per scene
- WASM memory can be reset between test runs
- Simpler than guest allocator calls
- Matches filetest/development use case

## Comparison to Emulator Approach

| Task | Emulator | WASM |
|------|----------|------|
| **Allocate VMContext** | Call `__lp_guest_alloc` via emulator | Reserve in `wasm_link::reserve_memory_regions` |
| **Write VMContext** | Slice into `ram: Vec<u8>` | `memory.data_mut(&store)[offset..]` |
| **Pass to shader** | Guest address (`0x8000_xxxx`) | Offset into linear memory (`0x0000_xxxx`) |
| **Builtin access** | Direct pointer deref | Direct pointer deref (same memory!) |
| **Grow memory** | Not needed (fixed RAM) | `memory.grow(&mut store, pages)` |

## Files to Modify

| File | Change |
|------|--------|
| `lp-glsl-filetests/src/test_run/wasm_link.rs` | Add `WasmMemoryLayout`, `reserve_memory_regions()` |
| `lp-glsl-filetests/src/test_run/wasm_runner.rs` | Store `memory` and `memory_layout` in `WasmExecutable` |
| `lp-glsl-filetests/src/test_run/wasm_runner.rs` | Pass real `vmctx_offset` instead of `0` in `build_wasm_args()` |
| `lp-glsl-builtins-wasm/src/lib.rs` (or vm mod) | Ensure VMContext builtin uses correct offset access |

## Open Questions

1. **Memory size**: Default WASM memory is often 1-2 pages (64KB-128KB). Need to ensure enough space for VMContext + textures.

2. **Memory alignment**: WASM linear memory starts at offset 0. Data segments from the module may occupy low addresses. Need to reserve space after data segments end.

3. **Multiple instantiations**: Each `WasmExecutable` has its own `Store` and `Memory`. VMContext offsets are per-instance, not global.

4. **Fuel/budget tracking**: WASM has built-in fuel metering (already used). VMContext fuel field may be redundant or could be synced periodically.
