# Allocation Tracing for Memory Debugging

## Scope of Work

Add allocation tracing infrastructure to the RISC-V emulator (`lp-riscv-emu` /
`lp-riscv-emu-guest`) to capture every heap allocation and deallocation with full
stack traces. This enables offline analysis of memory usage patterns, leak
detection, and fragmentation diagnosis.

The system has three parts:

1. **Guest-side**: A tracking allocator wrapper in `lp-riscv-emu-guest` that
   intercepts `alloc`, `dealloc`, and `realloc`, and makes a syscall for each
   event (passing only type, pointer, size -- minimal guest work).

2. **Host-side**: The emulator host handles the new syscall by walking the
   guest's stack frames (using the existing `unwind_backtrace` infrastructure)
   and writing trace events to an output file. The file includes a header with
   the symbol table (from the ELF) so the trace is self-contained.

3. **Analysis tool**: A converter/analyzer that reads the trace file and can
   output DHAT-compatible JSON (for use with Mozilla's dhat-viewer) and/or
   a timeline format for memory-over-time analysis.

## Current State

### Allocator (Guest)

- `lp-riscv-emu-guest/src/allocator.rs`: Uses `linked_list_allocator::LockedHeap`
  as `#[global_allocator]`. No tracking, no stats.
- Heap is 256KB, defined in `memory.ld`.

### Syscall Infrastructure

- Syscalls 1-8 defined in `lp-riscv-emu-shared/src/syscall.rs`.
- Guest invokes via `ecall` with number in `a7`, args in `a0-a6`.
- Host dispatches in `run_loops.rs::handle_syscall()`.
- The host has full access to guest registers (`self.regs`) and memory
  (`self.memory`).

### Backtrace Support

- `lp-riscv-emu/src/emu/emulator/backtrace.rs`: `unwind_backtrace(pc, regs)`
  walks the guest's frame pointer chain (s0/fp) from the host side.
  Already validated and working for panic backtraces.
- Requires firmware built with frame pointers enabled.
- `lp-riscv-elf/src/elf_loader/backtrace.rs`: `resolve_address()` and
  `format_backtrace()` for symbol resolution from the ELF symbol map.

### ELF Loading

- `ElfLoadInfo` contains `symbol_map: HashMap<String, u32>`, `code_end`, etc.
- Symbol map is already available when loading the ELF.
- The `release-emu` profile is used for fw-emu builds.

### Frame Pointers

- The existing backtrace code requires `-C force-frame-pointers=yes`.
- The `release-emu` profile does NOT currently set `force-frame-pointers`.
  The `BinaryBuildConfig::with_backtrace_support(true)` may handle this --
  needs verification.

### Test Infrastructure

- `fw-tests/tests/scene_render_emu.rs`: Integration test that builds fw-emu,
  loads an ELF, creates an emulator, loads a project, renders frames. This is
  the natural place to add an alloc-trace test.

## Questions

### Q1: Trace File Format

**Context**: We need a format for the trace file. Options discussed:
- Binary format: compact, needs a tool to read
- Text format: human-readable, larger
- JSON Lines: use serde, human-readable, no custom format to maintain

**Answer**: JSON Lines (newline-delimited JSON). First line is a header with
symbol table and metadata. Subsequent lines are events. serde handles all
serialization. Human-readable, greppable, jq-able, and schema evolution is free.

Size (~150 bytes/event × 50k events ≈ 7.5MB) is fine for a debugging tool.
Performance is irrelevant since the emulator is already much slower than hardware.

Header line:
```json
{"type":"header","symbols":[{"addr":4200000,"size":128,"name":"..."},...],"heap_start":...,"heap_size":...}
```

Event lines:
```json
{"t":"A","ptr":...,"sz":...,"ic":...,"frames":[...],"free":...,"largest_free":...}
{"t":"D","ptr":...,"sz":...,"ic":...,"frames":[...],"free":...,"largest_free":...}
{"t":"R","old_ptr":...,"ptr":...,"old_sz":...,"sz":...,"ic":...,"frames":[...],"free":...,"largest_free":...}
```

Symbol table entries include address, size, and name (sizes from ELF `st_size`).
This requires a new function to extract symbol ranges from the ELF, separate
from the existing `build_symbol_map` which is for relocations.

### Q2: Feature Gating

**Context**: The tracking allocator adds overhead. Should it be always-on or
feature-gated?

**Answer**: Guest-side = cargo feature (`alloc-trace` on `lp-riscv-emu-guest`).
When disabled (default), `#[global_allocator]` is plain `LockedHeap` (zero cost).
When enabled, it's `TrackingAllocator<LockedHeap>` that makes a syscall per event.

Host-side = runtime config on `Riscv32Emulator` (e.g. `.with_alloc_trace(path)`).
The emulator stores an `Option<AllocTracer>` that, when present, handles the
new syscall. The host is a normal std program so the runtime check is negligible.

### Q3: Where Does the Trace File Get Written?

**Context**: The host emulator needs to write the trace file somewhere.

**Answer**: Two files in an auto-generated directory:

```
traces/2026-03-08-143052-my-project/
├── meta.json        # Symbol table, heap geometry, run config
└── heap-trace.jsonl # Allocation events (one JSON object per line)
```

Directory name is auto-generated: `traces/YYYY-MM-DD-HHMMSS-<project-name>/`.
`traces/` is added to `.gitignore`. The CLI prints the output path on completion.

`Riscv32Emulator` takes a directory path via `.with_alloc_trace(dir)`. The
`AllocTracer` creates both files. Sane defaults, easy to use.

No in-memory API needed -- analysis happens offline.

### Q4: Timestamp Source

**Context**: Events need timestamps to build a timeline.

**Answer**: Use instruction count (`self.instruction_count`). Deterministic,
already tracked in the emulator, reproducible across runs.

### Q5: Frame Pointer Verification

**Context**: The `release-emu` profile doesn't explicitly set
`force-frame-pointers`. `BinaryBuildConfig::with_backtrace_support(true)` exists
but we need to verify it actually enables frame pointers.

**Answer**: Verified. `with_backtrace_support(true)` adds
`-C force-frame-pointers=yes` to RUSTFLAGS during build. The existing
`scene_render_emu` test already uses this. No changes needed.

### Q6: Fragmentation Tracking

**Context**: OOM can be caused by fragmentation even when total free memory looks
sufficient. Should we track the largest free block?

**Answer**: Include `free` (total free bytes) in each event, passed as a syscall
arg from the guest via `Heap::free()` (available while holding the allocator
lock). Skip `largest_free` for now -- it requires poking at allocator internals.

Fragmentation can be approximated in analysis by replaying the alloc/dealloc
sequence and simulating heap state. Won't be byte-identical to
`linked_list_allocator`'s behavior but close enough. Could even use a
`linked_list_allocator` instance in the analysis tool for faithful replay.

### Q7: Analysis Tool Scope

**Context**: We discussed DHAT JSON as the primary analysis format. How much
tooling should we build in this plan?

**Answer**: Analysis tool is out of scope for this plan. Deliverable is the trace
capture infrastructure + an end-to-end CLI command to exercise it.

### Q8: End-to-End CLI Command

**Context**: We need a way to actually exercise the tracing. A test alone isn't
enough -- we need to be able to run this against real projects.

**Answer**: Add a new `lp-cli emu-trace` subcommand that:
1. Takes a pre-built fw-emu binary path, a project directory, frame count, and
   output trace file path
2. Loads the ELF, creates emulator with alloc tracing enabled
3. Syncs the project to the emulator's filesystem
4. Loads the project, ticks N frames
5. Shuts down, flushes the trace file

Wrap the build + invocation in a `just` recipe (e.g. `just emu-trace <project>`)
that builds fw-emu with `alloc-trace` feature + frame pointers, then runs
`lp-cli emu-trace` with the built binary.

Assume a dev environment with code checkout. No packaging concerns yet.

## Notes

- The "memory allocation of 0 bytes failed" message in the panic log is
  misleading -- it's Rust's default alloc error handler format, not the actual
  requested size.
- The OOM occurs during cranelift lowering (`Lower::lower` → `RawVec::grow_one`)
  on the second shader compilation, after a project reload. First compile
  succeeds with similar free memory, suggesting fragmentation or a leak.
- The emulator is the right place for this work because the allocation patterns
  of cranelift, GLSL compiler, and engine are platform-independent. Only ESP32-
  specific allocations (output channel buffers) differ, and those are small.
