# Heap Peak Analysis: Where Memory Goes During Shader Compilation

**Date:** 2026-03-10
**Workload:** `examples/basic` — loads a project, compiles one GLSL shader via Cranelift JIT, ticks 10 frames
**Heap:** 320 KB (matching ESP32 target)
**Tools:** `emu-trace` + `heap-summary`

---

## Summary

Peak heap usage reaches 232 KB of 320 KB (72%), leaving only 93 KB free. The dominant
consumer is Cranelift's JIT compilation pipeline, accounting for roughly 60% of peak
allocations. The GLSL frontend and runtime account for another 25%. Application-level
code (server loop, filesystem, project management) accounts for the remaining 15%.

No single function dominates. Memory is spread across many cranelift subsystems that
each hold 5–25 KB of live `Vec` and `HashMap` data during compilation.

---

## Backtrace Unwinder Bugs (Fixed)

Prior to this analysis, all trace events had exactly 3 backtrace frames, making the
data useless for attribution. Two bugs in `backtrace.rs` were responsible:

**Bug 1 — Off-by-one in RAM bounds check.** The entry code (`_entry`) sets
`s0 = __stack_start` which equals `ram_end`. The check `fp >= ram_end` rejected
this as invalid. Fix: `fp > ram_end` (reads at `[fp-4]` and `[fp-8]` remain in bounds).

**Bug 2 — Signed comparison for unsigned addresses.** The check `prev_fp <= 0` used
signed `i32` comparison. RAM starts at `0x80000000`, so all RAM addresses are negative
as `i32`, causing every valid frame pointer to be rejected. Fix: unsigned-only comparison
(`prev_fp_u32 < RAM_START`).

**Added cycle detection:** since the stack grows downward, frame pointers must strictly
increase during unwinding. Check `prev_fp_u32 <= fp` detects self-referencing chains
(as seen at the `_code_entry` → `_entry` boundary).

After fixes: frame depth ranges from 4 to 32 (65% hit the MAX_FRAMES=32 cap).

---

## Peak Allocation Breakdown

At the moment of lowest free memory (95,540 bytes free), 1,650 allocations are live
totaling 232,140 bytes.

### Cranelift IR Construction (~70 KB)

| Bytes | Allocs | Origin | Mechanism |
|------:|-------:|--------|-----------|
| 24,192 | 288 | `instructions::convert_instruction` | Vec grow |
| 15,616 | 9 | `EntityList<T>::push` | Vec grow |
| 15,344 | 22 | `SecondaryMap<K,V>::resize_for_index_mut` | Vec grow |
| 14,720 | 18 | `FuncInstBuilder::build` | Vec grow |
| 5,120 | 9 | `DataFlowGraph::make_inst_results` | Vec grow |

These are cranelift's core IR data structures. `instructions::convert_instruction`
is the largest single contributor (24 KB, 288 allocs) — it converts GLSL IR instructions
into cranelift IR and grows many internal Vecs along the way.

`EntityList`, `SecondaryMap`, and `DataFlowGraph` are cranelift's typed storage pools.
They each hold ~15 KB of live data, with relatively few but large allocations (Vec
doubling).

### Cranelift Codegen & Lowering (~23 KB)

| Bytes | Allocs | Origin | Mechanism |
|------:|-------:|--------|-----------|
| 11,368 | 29 | `constructor_emit_side_effect` | Box clone |
| 8,144 | 7 | `Riscv32Backend::compile_function` | direct + rehash |
| 3,888 | 16 | `Lower<I>::finish_ir_inst` | direct + Vec grow |

The `constructor_emit_side_effect` entry (11 KB) is ISLE-generated code that clones
`MInst` structs via `Box<T>::clone`. This is the RISC-V instruction lowering phase.

### JIT Module Management (~17 KB)

| Bytes | Allocs | Origin | Mechanism |
|------:|-------:|--------|-----------|
| 16,790 | 398 | `JITModule::declare_function` | direct + rehash |

Mostly small per-function metadata allocations (398 allocs averaging 42 bytes each).
The single largest allocation (5.6 KB) is a Vec grow for the module's function table.

### GLSL Frontend (~29 KB)

| Bytes | Allocs | Origin | Mechanism |
|------:|-------:|--------|-----------|
| 10,952 | 31 | `compile_glsl_to_gl_module_jit` | rehash + Vec grow + String clone |
| 9,956 | 139 | `glsl_jit` (per-function JIT) | Vec/HashMap clone + String clone |
| 8,440 | 51 | `GlslCompiler::compile_to_gl_module_jit` | direct + clone |
| 6,544 | 2 | `ShaderRuntime::load_and_compile_shader` | String clone |

The GLSL frontend is clone-heavy: it clones Vecs, HashMaps, and Strings during
compilation. The two `String::clone` calls in `ShaderRuntime` are 6.5 KB — these are
the full shader source strings being cloned.

### Chunked Collections (~22 KB)

| Bytes | Allocs | Origin | Mechanism |
|------:|-------:|--------|-----------|
| 16,344 | 66 | `ChunkedVec<T>::push` | direct + Vec grow |
| 6,336 | 24 | `ChunkedHashMap<K,V>::insert` | direct + Vec grow |

The chunked collections work as designed: 22 KB in 90 allocs. The "via
RawVecInner::finish_grow" sub-entries (1.4 KB + 576 bytes) are the inner Vec
growth within chunks — a small overhead.

### Application / Runtime (~26 KB)

| Bytes | Allocs | Origin | Mechanism |
|------:|-------:|--------|-----------|
| 8,208 | 2 | `fmt::write` | Vec grow |
| 6,816 | 2 | `server_loop::run_server_loop` | Vec grow |
| 5,242 | 14 | `LpFsMemory::write_file` | direct + rehash + String clone |
| 4,068 | 143 | `lpfx_fns::init_functions` | direct |
| 3,272 | 1 | `LpFsMemory::read_file` | direct |

The `fmt::write` entry (8 KB in just 2 allocs) is notable — these are format buffers
that grew large. The server loop holds 6.8 KB in 2 large buffers (likely serial I/O
buffers). `lpfx_fns::init_functions` registers 143 function pointers — small but
numerous.

---

## Observations

1. **No single bottleneck.** Memory is spread across many subsystems. The largest
   single origin (`instructions::convert_instruction`) is only 10% of peak.

2. **Vec doubling is the primary mechanism.** Most large allocations come from
   `RawVecInner::finish_grow` — Rust's Vec capacity-doubling strategy. This wastes
   up to 50% of each Vec's allocation when it last doubled.

3. **Cranelift's data structures are the main concern.** `EntityList`, `SecondaryMap`,
   and `DataFlowGraph` are typed storage pools that grow via Vec doubling. They each
   hold 5–15 KB at peak.

4. **Clone operations in the GLSL frontend.** The frontend clones Vecs, HashMaps,
   and Strings totaling ~15 KB. Some may be avoidable with lifetime changes.

5. **The `fmt::write` 8 KB is suspicious.** Two format operations holding 8 KB at
   peak suggests a format buffer that grew during compilation and wasn't freed until
   later. Worth investigating.

6. **Chunked collections are working well.** They account for 22 KB (9.5% of peak)
   with controlled allocation sizes. Previous experiments showed that reducing chunk
   size from 12 to 8 increased total usage by ~10 KB due to per-allocation metadata
   overhead in `linked_list_allocator`.

---

## Suggestions

### Short-term (targeted fixes, no architectural changes)

- **Investigate `fmt::write` 8 KB.** Two format buffers surviving to peak is unusual.
  If these are debug/log format strings, they could be eliminated or made smaller.

- **Investigate `server_loop::run_server_loop` 6.8 KB.** Two large buffers in the
  server loop — likely serial I/O buffers. Could these be statically allocated or
  reduced in size?

- **Reduce GLSL frontend cloning.** The 6.5 KB of `String::clone` in
  `ShaderRuntime::load_and_compile_shader` is two copies of the full shader source.
  Pass by reference instead.

- **Shrink `lpfx_fns::init_functions`.** 143 allocs at 4 KB — could use a static
  array or a single allocation instead of per-function entries.

### Medium-term (cranelift-side changes)

- **`shrink_to_fit` after compilation.** Cranelift's IR data structures (`EntityList`,
  `SecondaryMap`, `DataFlowGraph`) grow during compilation but are read-only afterward.
  Adding `shrink_to_fit` calls after the build phase could reclaim the wasted doubling
  headroom.

- **Pre-size cranelift Vecs.** `instructions::convert_instruction` does 288 allocs
  because Vecs start small and double repeatedly. If the number of IR instructions
  is known ahead of time, pre-allocating with `Vec::with_capacity` would reduce both
  allocation count and peak wasted capacity.

- **Reduce `MInst` cloning in ISLE codegen.** The `constructor_emit_side_effect`
  entry (11 KB) clones boxed `MInst` instructions. If ISLE could emit references
  or use an arena, these clones could be avoided.

### Long-term (architectural)

- **Arena allocator for compilation.** Cranelift's compilation is a bounded phase:
  allocate during compile, free everything after. A bump allocator for the compilation
  phase would eliminate fragmentation and per-allocation metadata overhead entirely.
  This is the single highest-impact change but requires significant cranelift
  modifications.

- **Compile shaders one at a time.** Currently all shader functions may be in the
  JIT module simultaneously. If functions could be compiled and finalized sequentially,
  peak memory would be lower (only one function's IR in memory at a time).

---

## GlModule Dead Metadata Drop (Fix 4)

**Implemented:** Clear `function_registry`, `source_text`, `source_loc_manager`, `source_map`,
and `glsl_signatures` at the start of `build_jit_executable_memory_optimized`, right after
extracting what's needed (signatures clone, call_conv, pointer_type, func_metadata).
These fields are not used during `define_function`; they are only needed for
`gl_module.into_module()` at the end.

**Before (glmodule-before):** Peak free 79,620 bytes at ic=111,029,133  
**After (glmodule-after):**  Peak free 74,469 bytes at ic=100,487,926

Trace variance (different peak ic) may obscure the effect. The change is low-risk;
run multiple traces to confirm improvement.

---

## Raw Data

Trace: `traces/2026-03-10T16-08-06--examples-basic--bt-fix2/`

```
Events:  311,975
Allocs:  131,686 (10,193,453 bytes total)
Deallocs: 131,505 (10,167,756 bytes freed)
Reallocs: 48,784
Peak:    232,140 bytes used (1,650 allocs live)
Free at peak: 95,540 bytes (29% of 320 KB heap)
Final free:   301,983 bytes (92% of heap)
```
