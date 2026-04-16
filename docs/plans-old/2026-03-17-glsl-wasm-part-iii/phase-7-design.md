# Phase 7 Design: LPFX functions + out parameters

Plan reference: `2026-03-17-glsl-wasm-part-iii.md` Phase 7

## Goals

1. Implement WASM linear memory + bump allocator for out-param slots
2. Implement out/inout parameters (caller allocates, passes pointer, callee writes, caller reads)
3. Implement LPFX imports (lpfn_worley, lpfn_fbm, lpfn_psrdnoise)
4. Handle LPFX vector returns
5. Validate LPFX tests

---

## 1. WASM linear memory

**Memory section:** `(memory 1)` — one page (64KiB). For Phase 7, we only need a few slots for out params.

**Bump allocator:** Reserve a few bytes at start. Each out-param slot: 4 bytes (i32) or 12 bytes (vec3). Simple bump: next_offset += size. For out params we need at most a few vectors (e.g. gradient in psrdnoise = vec3).

**Export memory:** If host needs to read it, we might export. For out params, the *caller* allocates. So the caller is our generated code. We need memory to exist. Declare (memory 1), optionally export. The generated module uses it for out-param slots.

---

## 2. Out parameter ABI

**Caller side:**
1. Allocate slot (bump: get next free offset, advance)
2. Push pointer (offset as i32) as extra argument
3. Call function
4. Read back via i32.load
5. For vec3: load 3 consecutive i32s

**Callee side (import/builtin):** Receives pointer as last param. Writes via i32.store. For vec3 gradient: store 3 times at offset, offset+4, offset+8.

**Our case:** LPFX functions are *imports*. The host implements them. So the host receives the pointer. The host writes to that pointer in the shared memory. After the call returns, our generated code reads from the pointer.

**Memory ownership:** The WASM module declares memory. We instantiate it. The host's import function receives a pointer into that memory. The host must write via wasmtime's memory API. When we call an import, we pass the pointer. The import's Rust implementation uses `caller.get_export("memory")` or similar to get the Memory, then `memory.write(ptr, bytes)`.

---

## 3. lpfn_psrdnoise gradient out param

**Signature (conceptually):** `psrdnoise(vec3 p, vec3 period, out vec3 gradient) -> float`

**Flattened for WASM:** Params: p.x, p.y, p.z, period.x, period.y, period.z, gradient_ptr. Results: (result i32) for the float return.

**Caller:** Allocate 12 bytes. Push p components, period components, ptr. Call. Load 3 i32s from ptr. Build vec3 result.

---

## 4. LPFX as imports

Same pattern as Phase 6 builtins. Add LPFX to the import set. Map GLSL name "lpfn_psrdnoise" (or whatever) to import name "__lpfn_psrdnoise3_q32" etc. Provide host function that calls lps-builtins.

**Host for psrdnoise with out:** The Rust impl has signature `(p_x, p_y, p_z, period_x, period_y, period_z, gradient_ptr: i32) -> i32`. It computes result and gradient. It needs to write gradient to memory. It receives a `Store<T>` or `Caller` to access memory. `caller.get_export("memory")` and write at gradient_ptr.

---

## 5. Vector returns from imports

**Multi-value return from import:** WASM supports func imports with multiple results. So `(import "lpfn" "..." (func (param ...) (result i32) (result i32) (result i32)))`. Host returns 3 values. Caller gets 3 on stack.

**Alternative:** Return via memory (sret). Pass a pointer as first param, function writes result there, returns void or status. Cranelift might use sret. Check LPFX builtin ABI in Cranelift.

---

## 6. Bump allocator details

**Static allocation:** At compile time, we know the max out-param size per call. For a function that calls psrdnoise once, we need 12 bytes. Could reserve a fixed region (e.g. 256 bytes) at offset 0. First call uses 0..11, next uses 12..23, etc. But calls can be nested or in loops. We need to free. Simple: don't free. 256 bytes is enough for many calls. Or: allocate at function entry, free at exit. That requires stack-like allocator. Simpler: one static region, allocate at each call site, never free. If we have at most K simultaneous out-param slots (analysis), reserve K*12. For rainbow we likely have 1.

**Simplicity:** Reserve 64 bytes. Bump from 0. No free. Overflow = error (or grow). For single psrdnoise call: use 0..11.

---

## File change summary

| File | Changes |
|------|---------|
| `codegen/mod.rs` | Add memory section; bump allocator state |
| `codegen/expr/function.rs` | emit_lpfn_call: allocate slot, emit ptr, call, load results |
| `codegen/memory.rs` | New: reserve, allocate slot, emit instructions |
| `exec/*` | Host LPFX imports that write to memory |
| Import section | Add LPFX imports with correct signatures |

---

## Validation

- LPFX filetests
- rainbow.shader if it uses psrdnoise
