# JIT: 64-bit Host Pointer Width Issue

## The Problem

In `lpir-cranelift/src/call.rs:48-51`:

```rust
let header = lpvm::VmContextHeader::default();
let vmctx = core::ptr::from_ref(&header).cast::<u8>();
full.push(vmctx as usize as i32);  // 64-bit pointer TRUNCATED to 32-bit!
```

On 64-bit hosts (x86_64, AArch64), this truncates the upper 32 bits of the host pointer. When
JIT-compiled code dereferences this truncated value, it crashes (segfault) or accesses garbage
memory.

### Root Cause

LPIR was designed with a 32-bit assumption for VMContext:

```rust
// In lpir-cranelift/src/emit/mod.rs:110-113
// NOTE: Use I32 for vmctx (not pointer_type) to avoid it being treated as special by
// RISC-V backend with enable_multi_ret_implicit_sret. The vmctx is semantically a pointer
// but we pass it as I32 to prevent ABI confusion.
sig.params.push(AbiParam::new(types::I32));
```

This works for:

- **Emulator**: Guest uses 32-bit addresses (`0x8000_xxxx`)
- **WASM**: Linear memory uses 32-bit offsets
- **JIT on 32-bit targets**: RV32 native, pointers are 32-bit

But fails for:

- **JIT on 64-bit hosts**: Native pointers are 64-bit, truncation is catastrophic

## Prior Art: How Others Solve This

### 1. **Wasmtime (WebAssembly Runtime)**

WASM has 32-bit linear memory indices even on 64-bit hosts. Wasmtime:

- Maps WASM memory to a contiguous 4GB (or less) region in host address space
- 32-bit guest indices are zero-extended to 64-bit host pointers at access time
- Uses a "memory base" register for fast translation

Key insight: Guest never sees host pointers; it uses 32-bit offsets into a reserved region.

### 2. **Cranelift's JIT (`cranelift-jit`)**

Cranelift's JIT module allocates executable memory via `mmap`/`VirtualAlloc`:

- Returns 64-bit pointers on 64-bit hosts
- Uses `PointerType` (I64 on 64-bit, I32 on 32-bit) in IR
- Native code uses native pointer width

Key insight: The JIT IR must use target-appropriate pointer types.

### 3. **v8 / SpiderMonkey (JS Engines)**

Use **pointer compression**:

- All heap objects in a 4GB region
- Store only lower 32 bits in object headers
- Reconstruct 64-bit pointer using a "heap base" register + 32-bit offset

Key insight: 32-bit "handles" map to 64-bit pointers via base+offset.

### 4. **QEMU User Mode**

Runs 32-bit guest code on 64-bit hosts:

- Guest virtual addresses are 32-bit
- Mapped to host addresses via page tables
- mmap ensures guest addresses fit in 32-bit space

Key insight: Control the allocation to ensure guest addresses are 32-bit.

## Proposed Solutions

### Option A: 32-bit VMContext Handle (Wasmtime-style)

Keep VMContext as 32-bit, but make it an **offset/handle** rather than a pointer.

**Architecture:**

```
┌─────────────────────────────────────────────────────────┐
│                   Host 64-bit Address Space                │
│  ┌─────────────────────────────────────────────────────┐  │
│  │         Reserved 4GB Region (or smaller)          │  │
│  │   ┌──────────────┐  ┌────────────────────────────┐  │  │
│  │   │  VMContext   │  │      JIT Code/Heap         │  │  │
│  │   │  at offset 0 │  │                            │  │  │
│  │   └──────────────┘  └────────────────────────────┘  │  │
│  │           ↑ 32-bit offset (zero-extended)           │  │
│  └───────────┼─────────────────────────────────────────┘  │
│              │                                            │
│  Host: 0x0000_7fff_0000_0000 + 0x0000_0000 = 0x0000_7fff_0000_0000
```

**Implementation:**

1. **Reserve a 32-bit-addressable region** in 64-bit host memory:
   ```rust
   // Use mmap to allocate in the lower 4GB of address space
   let base_addr = mmap_32bit_region(size)?;  // Returns 64-bit pointer in low 4GB
   ```

2. **Place VMContext at offset 0** in this region:
   ```rust
   let vmctx_offset = 0u32;  // 32-bit handle
   let vmctx_host_ptr = base_addr + vmctx_offset as usize;  // 64-bit pointer
   // Write VMContext data at host pointer
   ```

3. **Pass 32-bit offset to JIT code**:
   ```rust
   // In call.rs
   full.push(vmctx_offset as i32);  // 0 - valid in 32-bit space
   ```

4. **JIT code reconstructs 64-bit pointer** from offset:
   ```rust
   // In JIT-compiled function prologue
   // base_addr is a constant or global, vmctx_offset is argument
   // Use iadd (pointer_type, base_addr, vmctx_offset) to get real pointer
   ```

**Pros:**

- Keeps LPIR as 32-bit (no IR changes)
- Works across emulator/WASM/JIT
- Matches wasmtime's proven architecture

**Cons:**

- Need to reserve 32-bit-addressable memory on 64-bit hosts
- Requires coordination between host allocation and JIT code generation

### Option B: Add Pointer Type to LPIR (Cranelift-style)

Introduce a target-aware pointer type to LPIR:

**LPIR Changes:**

```rust
// In lpir/src/type
pub enum IrType {
    I32,
    F32,
    Pointer,  // NEW: target-width pointer
}

// VMContext changes from I32 to Pointer
pub struct IrFunction {
    // pub vmctx_vreg: VReg,  // Old: I32
    pub vmctx_vreg: VReg,    // New: Pointer type
    // ...
}
```

**Lowering Changes:**

```rust
// In lpir-cranelift/src/emit/mod.rs
fn ir_type_for_mode(t: IrType, mode: FloatMode, pointer_type: types::Type) -> types::Type {
    match (t, mode) {
        (IrType::I32, _) => types::I32,
        (IrType::F32, FloatMode::F32) => types::F32,
        (IrType::F32, FloatMode::Q32) => types::I32,
        (IrType::Pointer, _) => pointer_type,  // I32 on RV32, I64 on x86_64/AArch64
    }
}

// Signature uses pointer_type for vmctx
sig.params.push(AbiParam::new(pointer_type));
```

**Calling Convention:**

```rust
// In call.rs - different paths for different targets
#[cfg(target_pointer_width = "64")]
{
    // Pass full 64-bit pointer directly
    full.push(vmctx_host_ptr as usize as i64 as i32);  // Or use i64
}

#[cfg(target_pointer_width = "32")]
{
    // Pass 32-bit pointer directly  
    full.push(vmctx_host_ptr as usize as i32);
}
```

**Pros:**

- Clean, target-correct abstraction
- No 32-bit address space limitations
- Matches Cranelift's design

**Cons:**

- Major LPIR change (affects all backends)
- VMContext is the only pointer currently; may be overkill
- Need to handle pointer-width values in Q32 encoding

### Option C: Host-Managed 32-bit Heap (Simpler Variant of A)

Don't change LPIR, but fix the JIT memory allocation.

**Key Insight:** The problem isn't 32-bit vs 64-bit, it's that we're passing a **truncated host
pointer**. If we ensure VMContext lives in the lower 4GB of address space, the truncation is
harmless.

**Implementation:**

1. **On 64-bit hosts, use a custom allocator** that returns 32-bit-addressable memory:
   ```rust
   // In jit_memory.rs
   pub struct JitMemory32 {
       base: *mut u8,      // 64-bit host pointer in low 4GB
       size: usize,
       next_offset: usize,
   }

   impl JitMemory32 {
       pub fn alloc(&mut self, size: usize, align: usize) -> u32 {
           let offset = align_up(self.next_offset, align);
           self.next_offset = offset + size;
           offset as u32  // Return 32-bit offset, valid for truncation
       }
       
       pub fn host_ptr(&self, offset: u32) -> *mut u8 {
           self.base.wrapping_add(offset as usize)
       }
   }
   ```

2. **Allocate VMContext via this allocator**:
   ```rust
   // In call.rs
   let vmctx_offset = jit_memory.alloc(size_of::<VmContext>(), align_of::<VmContext>());
   let vmctx_ptr = jit_memory.host_ptr(vmctx_offset);
   // Write VMContext to host pointer
   unsafe { std::ptr::write(vmctx_ptr as *mut VmContext, VmContext::default()) };
   full.push(vmctx_offset as i32);  // Truncation is safe - it's already < 4GB
   ```

3. **JIT code uses the offset directly** (since it's valid in the 32-bit region):
   ```rust
   // JIT-compiled code receives 32-bit offset
   // It uses base + offset to compute actual address
   // This requires the base address to be known/available in JIT
   ```

**Pros:**

- Minimal changes (mostly to jit_memory.rs)
- No LPIR changes
- Works for VMContext specifically

**Cons:**

- Requires JIT code to know the base address
- Hacky - relies on address space layout
- Doesn't solve arrays/textures in full generality

### Option D: Remove VMContext from JIT (Short-term)

JIT is currently lower priority and tests aren't run often. A pragmatic fix:

**Don't use VMContext in JIT mode** - have builtins use a fallback path:

```rust
// In __lp_vm_get_fuel_q32
#[cfg(jit)]
pub extern "C" fn __lp_vm_get_fuel_q32(_vmctx_word: i32) -> u32 {
    // JIT mode: return default fuel, don't dereference vmctx
    DEFAULT_VMCTX_FUEL as u32
}

#[cfg(not(jit))]
pub extern "C" fn __lp_vm_get_fuel_q32(vmctx_word: i32) -> u32 {
    // Normal mode: dereference vmctx
    let ctx = vmctx_word as usize as *const VmContext;
    unsafe { (*ctx).fuel as u32 }
}
```

**Pros:**

- Immediate fix, unblocks development
- No architectural changes

**Cons:**

- VMContext features don't work in JIT
- Not a real solution

## Native pointer width for JIT (Cranelift-style tradeoff)

Instead of 32-bit handles, widen **only the VMContext parameter** to native pointer width (`i32` on
RV32, `i64` on x86_64/AArch64). That is what Cranelift does (`pointer_type` is target-specific).

### Implementation changes

1. **LPIR type system (moderate)** — Add `IrType::Pointer` (or equivalent); VMContext vreg uses
   pointer width. Every backend must map it; text format / serialization may need updates.

2. **Lowering (simple)** — In `signature_for_ir_func`, use `AbiParam::new(pointer_type)` for
   VMContext instead of `types::I32`. Remove the “I32 to avoid ABI confusion” special case (revisit
   RISC-V `enable_multi_ret_implicit_sret` ordering if needed).

3. **Host call setup (moderate)** — Pass `&header as *const VmContext as usize` (or `isize`). Today
   `invoke_i32_args_returns` takes `&[i32]`, so the first slot cannot hold a full 64-bit pointer
   without either a parallel invoke path or splitting VMContext into two i32 words (undesirable).

4. **Invoke shim (complex)** — Hand-written `extern "C"` shims are all `fn(i32, i32, …)`. For 64-bit
   hosts the first argument becomes `i64`/`usize`, e.g. `extern "C" fn(i64, i32, i32) -> i32`. This
   stacks on existing platform splits (AArch64 asm, StructReturn, multi-ret).

### Tradeoff table

| Aspect                | 32-bit handles (Option A)         | Native pointers (widen VMContext)                                |
|-----------------------|-----------------------------------|------------------------------------------------------------------|
| **LPIR**              | Keep I32 for VMContext            | Add pointer type / widen VMContext only                          |
| **Codegen**           | Base address + zero-extend offset | Use `pointer_type` (Cranelift-native)                            |
| **Invoke**            | Keep `&[i32]`                     | `vmctx: usize` + platform-specific first-arg type                |
| **Allocation**        | Reserve low 4GB / heap base       | Normal stack or heap                                             |
| **Threading**         | Shared heap base (sync)           | No global base; pass pointer per call                            |
| **Arrays / textures** | Handles + indirection             | Natural pointers                                                 |
| **Cross-target**      | Same 32-bit story everywhere      | JIT differs from emulator/WASM (32-bit guest/off vs host native) |

### Invoke is the real cost

`invoke_i32_args_returns` already multiplies variants by arg count, return count, StructReturn, and
AArch64. Adding “first arg is native width” increases combinations but is localized: a plausible API
is `invoke_native_vmctx(code, vmctx: usize, user_args: &[i32], …)` with
`#[cfg(target_pointer_width)]` dispatch.

Cranelift’s `cranelift-jit` does not ship typed invoke shims; it returns raw code pointers. Our
`JitModule::call` layer is where this complexity lives.

### Counter-argument: consistency

| Target     | VMContext meaning           |
|------------|-----------------------------|
| Emulator   | 32-bit guest address        |
| WASM       | 32-bit linear memory offset |
| JIT x86_64 | 64-bit host pointer         |
| JIT RV32   | 32-bit host pointer         |

Builtin bodies may need target-specific paths if they assume “always i32 word.” LPIR portability (
one module, three runtimes) is easier with a single 32-bit convention; per-runtime natural ABI is
easier with native pointers on JIT.

### Verdict on native widening

For **JIT only**, native pointer width is often **less annoying** than 32-bit handles: no `i32`↔
`i64` juggling in a shared heap, no base-register threading issues, and arrays/textures stay
pointer-sized. The price is **LPIR + invoke work**, not a magic free lunch.

`i32`→`i64` conversions at every call are annoying and can invite mistakes; passing one `usize`/
`isize` for VMContext once per invocation is usually clearer.

---

## Recommendation

**Immediate (VMContext PoC):** **Option D** — stub VMContext builtins on JIT if needed so the PoC is
not blocked.

**Long-term — pick one axis:**

- **Cross-runtime consistency (emulator / WASM / JIT share one convention):** Prefer **Option A** (
  32-bit handles / offsets + base where needed). Matches wasmtime-style 32-bit linear indices.

- **JIT-first ergonomics (host speed, real pointers, no global heap base):** Prefer **native pointer
  width for VMContext on JIT** (section above): add LPIR pointer type or widen that parameter only,
  use `pointer_type` in Cranelift, extend invoke for `usize` first argument on 64-bit hosts.

The earlier “default to Option A for long-term” assumed maximum IR portability. If that is not the
goal and JIT complexity should stay local, **native pointers for JIT** is the better default for the
host path.

**Implementation sketch for Option A (unchanged idea):**

```rust
// New module: lpir-cranelift/src/jit_heap.rs

/// 32-bit-addressable heap for JIT on 64-bit hosts.
pub struct JitHeap32 {
    base: *mut u8,        // Host pointer (64-bit)
    size: usize,
    next_offset: AtomicUsize,
}

impl JitHeap32 {
    /// Allocate in lower 4GB of address space.
    pub fn new(size: usize) -> Result<Self, JitHeapError> {
        // Use mmap with MAP_32BIT on Linux, or allocate below 4GB on other platforms
        let base = mmap_32bit(size)?;
        Ok(Self { base, size, next_offset: AtomicUsize::new(0) })
    }
    
    /// Allocate `size` bytes with `align`, return 32-bit offset.
    pub fn alloc(&self, size: usize, align: usize) -> Option<u32> {
        let offset = self.next_offset.fetch_update(
            Ordering::SeqCst,
            Ordering::SeqCst,
            |current| {
                let aligned = (current + align - 1) & !(align - 1);
                let end = aligned + size;
                if end > self.size { return None; }
                Some(end)
            }
        ).ok()?;
        Some(offset as u32)
    }
    
    /// Convert 32-bit offset to host pointer.
    pub fn host_ptr(&self, offset: u32) -> *mut u8 {
        self.base.wrapping_add(offset as usize)
    }
    
    /// Base address for JIT code (to compute pointers from offsets).
    pub fn base_addr(&self) -> u64 {
        self.base as u64
    }
}

// Modified call.rs
impl JitModule {
    pub fn call(&self, name: &str, args: &[GlslQ32]) -> CallResult<GlslReturn<GlslQ32>> {
        // ... setup ...
        
        // Allocate VMContext in 32-bit heap
        let vmctx_offset = self.heap32.alloc(size_of::<VmContext>(), align_of::<VmContext>())
            .ok_or(CallError::OutOfMemory)?;
        let vmctx_ptr = self.heap32.host_ptr(vmctx_offset);
        unsafe { std::ptr::write(vmctx_ptr as *mut VmContext, VmContext::default()) };
        
        // Pass 32-bit offset (truncation is safe - it's already in low 4GB)
        full.push(vmctx_offset as i32);
        
        // Tell JIT code the base address via a global or special register
        // (requires codegen changes)
        
        // ... invoke ...
    }
}
```

## Cross-Reference to Other Notes

| Issue             | Emulator         | WASM                  | JIT                                                           |
|-------------------|------------------|-----------------------|---------------------------------------------------------------|
| **Pointer Type**  | Guest 32-bit     | 32-bit offset         | 32-bit handle *or* native `pointer_type` (see tradeoff above) |
| **VMContext Bug** | Host ptr → guest | Dummy 0 passed        | Host ptr truncated                                            |
| **Fix Strategy**  | Guest allocator  | Linear memory reserve | 32-bit heap + base *or* widen VMContext + invoke shim         |
| **See Notes**     | `rv32-notes.md`  | `wasm-notes.md`       | This file                                                     |

## Open Questions

1. **32-bit address space limit**: 4GB may be restrictive for large textures in JIT mode. We could
   use:
    - Multiple 4GB regions (different base addresses)
    - Sparse allocation (only commit used pages)
    - Handle indirection (texture IDs instead of direct pointers)

2. **Base address communication**: JIT code needs to know the base address to convert offsets →
   pointers. Options:
    - Global variable in JIT data section
    - Extra hidden parameter on every call
    - Special register (like x28) reserved for heap base

3. **Array/texture passing**: If arrays are passed as (ptr, len), we need consistent pointer
   representation. This is why Option A (32-bit handles) is better long-term than Option C (just fix
   VMContext).
