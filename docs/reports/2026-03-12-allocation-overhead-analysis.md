# Allocation Overhead Analysis: ESP32 / Emulator

**Date:** 2026-03-12
**Context:** Memory optimization for 320 KB heap; OOM problems on ESP32

---

## 1. Per-Allocation Overhead (linked_list_allocator)

### How linked_list_allocator Works

The `linked_list_allocator` (v0.10) does **not** add metadata to allocated blocks. Metadata (the `Hole` struct) is stored *inside* free blocks; allocated blocks are "pure" user data.

**Overhead comes from layout rounding**, not per-block headers:

```
align_layout(layout):
  size = max(layout.size(), min_size())
  size = align_up(size, align_of::<Hole>())
  → return Layout { size, align }
```

- **min_size()** = `size_of::<Hole>() * 2`
  - On 32-bit (ESP32 / RISC-V): `Hole` = 8 bytes (size + next ptr) → **min_size = 16 bytes**
  - On 64-bit: **min_size = 32 bytes**
- **Alignment**: size is rounded up to multiple of `align_of::<Hole>()` (4 on 32-bit).

### Impact on Small Allocations

| Requested | Actual (32-bit) | Overhead |
|----------:|-----------------:|----------|
| 1 B       | 16 B             | +15 B    |
| 4 B       | 16 B             | +12 B    |
| 8 B       | 16 B             | +8 B     |
| 12 B      | 16 B             | +4 B     |
| 16 B      | 16 B             | 0        |
| 17 B      | 20 B             | +3 B     |
| 24 B      | 24 B             | 0        |

For **143 allocations** averaging ~28 B requested (e.g. lpfn_fns): if many are under 16 B, actual heap usage is higher than the traced `sz`. A rough worst case: 50 tiny allocs × 12 B overhead ≈ **600 B** of untracked overhead from that hotspot alone.

---

## 2. Profiler Gap

### Current Behavior

The heap-summary profiler records `layout.size()` — the **requested** size from `GlobalAlloc::alloc`. The allocator internally rounds via `align_layout`, but the trace does not capture the rounded size.

**Effect:** Reported bytes are a **lower bound** on actual heap usage. Small allocations are undercounted the most.

### Should the Profiler Account for Overhead?

**Yes, for accuracy on constrained targets.** Options:

1. **Replicate align_layout in analysis**  
   In `heap_summary`, when grouping/aggregating, apply the same rounding:
   - 32-bit: `actual_size = max(sz, 16); actual_size = (actual_size + 3) & !3`
   - Use `heap_size` / target from meta to choose 32 vs 64-bit.

2. **Trace actual size in the guest**  
   Patch `TrackingAllocator` to call the allocator, then ask for the layout actually used. `linked_list_allocator::Heap::allocate_first_fit` returns `(NonNull, Layout)` where the `Layout` is the aligned one — but `GlobalAlloc::alloc` does not expose that. Would require a custom allocator wrapper that traces the aligned layout.

3. **Add optional overhead estimate to reports**  
   e.g. "Live: 25,427 bytes (reported) / ~26,200 bytes (est. with allocator overhead)". Implement (1) and show both.

**Recommendation:** Implement (1) in heap-summary for 32-bit targets — deterministic and no guest changes.

---

## 3. lpfn_fns::init_functions Hotspot

### Current Pattern

- **143 allocations**, 4,068 bytes reported
- Uses `Vec<LpfnFn>` built from many `String::from(...)` and `vec![Parameter {...}]`
- Each `LpfnFn` has:
  - `FunctionSignature { name: String, return_type, parameters: Vec<Parameter> }`
  - Each `Parameter { name: String, ty, qualifier }`
- Allocated once at first access, then `Box::leak` for `'static`

### Allocation Breakdown (approximate)

- ~27 LpfnFn entries
- ~27 `String` (function names)
- ~27 `Vec<Parameter>`
- ~70+ `String` (parameter names)
- 1 `Vec<LpfnFn>` + 1 `Box<[LpfnFn]>` (via `into_boxed_slice`)

Total ≈ 143 allocations, dominated by small strings and vectors.

### Should We Optimize?

**Yes.** It is an easy win on a tight heap:

1. **Single init, many small allocs** → high overhead from rounding and fragmentation.
2. **Data is static** → function names and parameter lists are fixed at compile time.
3. **Live for whole process** → these allocations never free, so savings persist.

---

## 4. Optimization Options

### Option A: Static References (Best for ESP32)

Replace heap allocations with `const`/`static` data in ROM (flash):

```rust
struct LpfnFnStatic {
    glsl_sig: FunctionSignatureRef,
    impls: LpfnFnImpl,
}
struct FunctionSignatureRef {
    name: &'static str,
    return_type: Type,
    parameters: &'static [ParameterRef],
}
struct ParameterRef {
    name: &'static str,
    ty: Type,
    qualifier: ParamQualifier,
}

static LPFX_FNS: &[LpfnFnStatic] = &[
    LpfnFnStatic {
        glsl_sig: FunctionSignatureRef {
            name: "lpfn_fbm",
            return_type: Type::Float,
            parameters: &[
                ParameterRef { name: "p", ty: Type::Vec2, qualifier: ParamQualifier::In },
                ParameterRef { name: "octaves", ty: Type::Int, qualifier: ParamQualifier::In },
                ParameterRef { name: "seed", ty: Type::UInt, qualifier: ParamQualifier::In },
            ],
        },
        impls: LpfnFnImpl::Decimal { float_impl: BuiltinId::LpfnFbm2F32, q32_impl: BuiltinId::LpfnFbm2Q32 },
    },
    // ...
];
```

- **Savings:** 0 heap bytes, 0 allocations.
- **Effort:** Generator changes, and either a parallel `LpfnFnRef` type or adapting `LpfnFn` to hold refs. Consumers already use `&str` comparison and iteration; `return_type` is `Type` (Copy).

### Option B: Single Arena Allocation

Allocate one large block and bump within it to build all strings and parameter arrays, then construct the `Vec<LpfnFn>`:

- **Savings:** 143 allocs → 1 alloc; ~2 KB of rounding overhead → ~16 B.
- **Effort:** Custom arena or manual layout in `init_functions`.

### Option C: Batch with `Vec::with_capacity`

Pre-size the main `Vec<LpfnFn>` and inner `Vec`s; strings still allocate individually. Helps a bit but does not remove the main cost.

---

## 5. Summary

| Topic | Finding |
|-------|--------|
| **Allocator overhead** | linked_list_allocator rounds to min 16 B (32-bit); no per-block metadata on allocations. |
| **Profiler** | Tracks requested size only; undercounts heap use for small allocs. Add optional overhead estimate. |
| **lpfn_fns hotspot** | 143 allocs, 4 KB; worth optimizing under memory pressure. |
| **Recommended fix** | Use static refs (Option A) for LPFX registry; add allocator-rounding estimate to heap-summary. |
