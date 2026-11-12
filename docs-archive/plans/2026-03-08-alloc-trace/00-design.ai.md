# Allocation Tracing - Design

## Scope of Work

Add allocation tracing to the RISC-V emulator to capture every heap
allocation/deallocation with full stack traces. Deliver an `lp-cli emu-trace`
command and `just` recipe that runs fw-emu with tracing against a real project,
producing a self-contained trace output directory. Analysis tooling is out of
scope.

## File Structure

```
lp-riscv/lp-riscv-emu-shared/
└── src/
    └── syscall.rs                       # UPDATE: Add SYSCALL_ALLOC_TRACE = 9

lp-riscv/lp-riscv-emu-guest/
├── Cargo.toml                           # UPDATE: Add alloc-trace feature
└── src/
    ├── allocator.rs                     # UPDATE: TrackingAllocator wrapper (feature-gated)
    └── lib.rs                           # UPDATE: (minor, if needed)

lp-riscv/lp-riscv-elf/
└── src/
    └── elf_loader/
        └── symbols.rs                   # UPDATE: Add build_symbol_list() with sizes

lp-riscv/lp-riscv-emu/
├── Cargo.toml                           # UPDATE: Add serde, serde_json deps
└── src/
    ├── alloc_trace.rs                   # NEW: AllocTracer, JSON types, write logic
    └── emu/
        └── emulator/
            ├── state.rs                 # UPDATE: Add Option<AllocTracer> field + builder
            └── run_loops.rs             # UPDATE: Handle SYSCALL_ALLOC_TRACE

lp-core/lp-client/
├── Cargo.toml                           # UPDATE: Add emu feature + riscv deps
└── src/
    ├── lib.rs                           # UPDATE: Conditionally export transport_emu_serial
    └── transport_emu_serial.rs          # MOVED from fw-tests/src/

lp-fw/fw-tests/
├── Cargo.toml                           # UPDATE: Use lp-client emu feature
└── src/
    └── lib.rs                           # UPDATE: Remove transport_emu_serial

lp-cli/
├── Cargo.toml                           # UPDATE: Enable lp-client emu feature
└── src/
    ├── main.rs                          # UPDATE: Add EmuTrace variant
    └── commands/
        ├── mod.rs                       # UPDATE: Add emu_trace module
        └── emu_trace/
            ├── mod.rs                   # NEW: Args struct, re-exports
            └── handler.rs              # NEW: Load ELF, setup emu, sync project, tick, shutdown

justfile                                 # UPDATE: Add emu-trace recipe
.gitignore                               # UPDATE: Add traces/
```

## Conceptual Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  just emu-trace <project-dir> [frames] [output-dir]            │
│  1. cargo build fw-emu --features alloc-trace (+ frame ptrs)   │
│  2. lp-cli emu-trace --bin <path> --project <dir> ...          │
│  3. prints: "Trace written to traces/2026-03-08-143052-myproj/"│
└────────────────────────────────┬────────────────────────────────┘
                                 │
                                 ▼
┌─────────────────────────────────────────────────────────────────┐
│  lp-cli emu-trace                                              │
│  - Load ELF → ElfLoadInfo                                      │
│  - Extract symbol list with sizes (build_symbol_list)          │
│  - Create output dir: traces/YYYY-MM-DD-HHMMSS-<project>/     │
│  - Write meta.json (symbols, heap geometry, run config)        │
│  - Create Riscv32Emulator::new(...)                            │
│      .with_alloc_trace(trace_dir)                              │
│  - SerialEmuClientTransport → LpClient                         │
│  - Sync project, load project, tick N frames, unload, shutdown │
│  - AllocTracer flushes heap-trace.jsonl                        │
│  - Print output path                                           │
└────────────────────────────────┬────────────────────────────────┘
                                 │
          ┌──────────────────────┼──────────────────────┐
          ▼                      ▼                      ▼
┌──────────────────┐  ┌──────────────────┐  ┌─────────────────────┐
│ GUEST (fw-emu)   │  │   HOST (emu)     │  │  Output Directory   │
│                  │  │                  │  │                     │
│ TrackingAllocator│  │ handle_syscall:  │  │ meta.json:          │
│  wraps LockedHeap│  │  ALLOC_TRACE →   │  │  {symbols, heap_*,  │
│                  │  │  unwind_backtrace│  │   run config}       │
│ On alloc/dealloc:│  │  write to jsonl  │  │                     │
│  1. inner.alloc()│  │                  │  │ heap-trace.jsonl:    │
│  2. Heap::free() │  │ AllocTracer:     │  │  {"t":"A",...}      │
│  3. ecall(9,     │  │  BufWriter<File> │  │  {"t":"D",...}      │
│    type, ptr,    │  │  writes events   │  │  {"t":"R",...}      │
│    size, free)   │  │  as JSON lines   │  │  ...                │
└──────────────────┘  └──────────────────┘  └─────────────────────┘
```

## Data Flow (Single Allocation)

1. Guest code calls `alloc(layout)`
2. `TrackingAllocator::alloc()` calls `inner.alloc(layout)` → gets `ptr`
3. Reads `Heap::free()` → `free_bytes` (allocator lock still held)
4. Executes `ecall(SYSCALL_ALLOC_TRACE, type=ALLOC, ptr, size, free_bytes)`
5. Host `handle_syscall` dispatches to `AllocTracer`
6. `AllocTracer` calls `unwind_backtrace(pc, &regs)` → frame addresses
7. Writes JSON line to `heap-trace.jsonl`:
   `{"t":"A","ptr":...,"sz":...,"ic":...,"frames":[...],"free":...}`
8. Sets return register, resumes guest

## Output Format

### meta.json

```json
{
  "version": 1,
  "timestamp": "2026-03-08T14:30:52Z",
  "project": "my-project",
  "frames_requested": 60,
  "heap_start": 2147483648,
  "heap_size": 262144,
  "symbols": [
    { "addr": 4200000, "size": 128, "name": "cranelift_codegen::machinst::lower::Lower::lower" },
    ...
  ]
}
```

### heap-trace.jsonl

```json
{"t":"A","ptr":2147500000,"sz":64,"ic":12345678,"frames":[4200000,4200100,4200200],"free":200000}
{"t":"D","ptr":2147500000,"sz":64,"ic":12345999,"frames":[4200000,4200300],"free":200064}
{"t":"R","old_ptr":2147500000,"ptr":2147501000,"old_sz":64,"sz":128,"ic":12346000,"frames":[4200000,4200400],"free":199936}
```

Event types: `A` = alloc, `D` = dealloc, `R` = realloc.

## Main Components

### TrackingAllocator (guest, feature-gated)

- Wraps `LockedHeap`, implements `GlobalAlloc`
- `alloc()`: calls inner, reads free bytes, makes syscall
- `dealloc()`: makes syscall, calls inner
- `realloc()`: makes syscall with old+new info (or decompose to D+A)
- Syscall args: `a0=type, a1=ptr, a2=size, a3=free_bytes`
  (realloc: `a0=type, a1=old_ptr, a2=new_ptr, a3=old_size, a4=new_size, a5=free`)
- Must be allocation-free (only stack + syscall)

### AllocTracer (host)

- Created via `Riscv32Emulator::with_alloc_trace(dir)`
- Holds `BufWriter<File>` for `heap-trace.jsonl`
- `handle_alloc_event()`: unwind backtrace, serialize JSON, write line
- Flushed on drop or explicit close

### build_symbol_list (lp-riscv-elf)

- New function alongside existing `build_symbol_map`
- Returns `Vec<SymbolInfo>` with `{ addr, size, name }`
- Extracts `st_size` from ELF symbol entries via `object` crate
- Used to write the symbol table in `meta.json`

### emu-trace command (lp-cli)

- Args: `--bin <fw-emu-path>`, `--project <dir>`, `--frames <N>` (default 60),
  `--output <dir>` (default auto-generated under `traces/`)
- Reuses patterns from `scene_render_emu`: ELF load, serial transport, LpClient
- Syncs project files to emulator fs, loads project, ticks frames, unloads
- Prints trace directory path on completion

### just recipe

- `just emu-trace <project-dir>`: builds fw-emu with `alloc-trace` feature +
  frame pointers, then runs `lp-cli emu-trace`
