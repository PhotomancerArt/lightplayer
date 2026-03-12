# Phase 2: Host AllocTracer + Symbol List

## Scope

Implement the host-side tracing: `AllocTracer` in `lp-riscv-emu` that handles
the `SYSCALL_ALLOC_TRACE` syscall, walks the guest stack, and writes JSON lines.
Also add `build_symbol_list()` to `lp-riscv-elf` for extracting symbols with
sizes from the ELF.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Add `build_symbol_list()` to lp-riscv-elf

In `lp-riscv/lp-riscv-elf/src/elf_loader/symbols.rs`, add a new function that
returns symbols with sizes (unlike `build_symbol_map` which is for relocations):

```rust
/// Symbol information with address range for trace files.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SymbolInfo {
    pub addr: u32,
    pub size: u32,
    pub name: String,
}

/// Build a list of code symbols with addresses and sizes.
///
/// Used for allocation trace metadata. Sorted by address.
/// Only includes defined symbols in the code (ROM) region.
pub fn build_symbol_list(obj: &object::File, text_base: u64) -> Vec<SymbolInfo> {
    let mut symbols: Vec<SymbolInfo> = obj
        .symbols()
        .filter_map(|sym| {
            let name = sym.name().ok()?;
            if name.is_empty() {
                return None;
            }
            // Only defined symbols
            if sym.section() == SymbolSection::Undefined {
                return None;
            }
            let addr = sym.address();
            // Only code symbols (not RAM)
            if is_ram_address(addr) {
                return None;
            }
            let offset = if addr >= text_base {
                (addr - text_base) as u32
            } else {
                addr as u32
            };
            Some(SymbolInfo {
                addr: offset,
                size: sym.size() as u32,
                name: name.to_string(),
            })
        })
        .collect();
    symbols.sort_by_key(|s| s.addr);
    symbols
}
```

Add `serde` as an optional dependency of `lp-riscv-elf` behind a feature, or
put `SymbolInfo` in a shared location. Since `lp-riscv-elf` already has `std`
feature and is used host-side, adding serde is fine:

```toml
[dependencies]
serde = { version = "1", features = ["derive"], optional = true }

[features]
std = ["serde"]
```

Re-export from `lp-riscv/lp-riscv-elf/src/elf_loader/mod.rs`:

```rust
pub use symbols::{SymbolInfo, build_symbol_list};
```

Update `load_elf` to also call `build_symbol_list` and include the result in
`ElfLoadInfo`:

```rust
pub struct ElfLoadInfo {
    // ... existing fields ...
    pub symbol_list: Vec<SymbolInfo>,
}
```

### 2. Add serde dependencies to lp-riscv-emu

In `lp-riscv/lp-riscv-emu/Cargo.toml`, add under `[dependencies]`:

```toml
serde = { version = "1", features = ["derive"], optional = true }
serde_json = { version = "1", optional = true }
```

Gate behind the `std` feature (alloc tracing only makes sense with file I/O):

```toml
[features]
std = ["env_logger", "log/std", "serde", "serde_json"]
```

### 3. Implement AllocTracer

Create `lp-riscv/lp-riscv-emu/src/alloc_trace.rs`:

**JSON types** (for serde serialization):

```rust
#[derive(Serialize)]
struct TraceMetadata {
    version: u32,
    timestamp: String,
    project: String,
    frames_requested: u32,
    heap_start: u32,
    heap_size: u32,
    symbols: Vec<SymbolInfo>,
}

#[derive(Serialize)]
struct AllocEvent {
    t: &'static str,     // "A", "D", "R"
    ptr: u32,
    sz: u32,
    ic: u64,             // instruction count
    frames: Vec<u32>,
    free: u32,
    // realloc-only fields
    #[serde(skip_serializing_if = "Option::is_none")]
    old_ptr: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    old_sz: Option<u32>,
}
```

**AllocTracer struct:**

```rust
pub struct AllocTracer {
    writer: BufWriter<File>,
    event_count: u64,
}

impl AllocTracer {
    /// Create a new tracer. Writes meta.json and opens heap-trace.jsonl.
    pub fn new(
        trace_dir: &Path,
        metadata: TraceMetadata,
    ) -> Result<Self, std::io::Error> {
        std::fs::create_dir_all(trace_dir)?;

        // Write meta.json
        let meta_path = trace_dir.join("meta.json");
        let meta_file = File::create(&meta_path)?;
        serde_json::to_writer_pretty(BufWriter::new(meta_file), &metadata)?;

        // Open heap-trace.jsonl
        let trace_path = trace_dir.join("heap-trace.jsonl");
        let writer = BufWriter::new(File::create(&trace_path)?);

        Ok(Self {
            writer,
            event_count: 0,
        })
    }

    /// Record an allocation event.
    pub fn record_event(&mut self, event: AllocEvent) {
        // serde_json::to_writer + write newline
        let _ = serde_json::to_writer(&mut self.writer, &event);
        let _ = self.writer.write_all(b"\n");
        self.event_count += 1;
    }

    /// Flush and return event count.
    pub fn finish(&mut self) -> std::io::Result<u64> {
        self.writer.flush()?;
        Ok(self.event_count)
    }
}
```

### 4. Wire into Riscv32Emulator

In `lp-riscv/lp-riscv-emu/src/emu/emulator/state.rs`:

Add field:

```rust
pub struct Riscv32Emulator {
    // ... existing fields ...
    #[cfg(feature = "std")]
    pub(super) alloc_tracer: Option<crate::alloc_trace::AllocTracer>,
}
```

Add builder method:

```rust
#[cfg(feature = "std")]
pub fn with_alloc_trace(
    mut self,
    trace_dir: &std::path::Path,
    metadata: crate::alloc_trace::TraceMetadata,
) -> Result<Self, std::io::Error> {
    self.alloc_tracer = Some(crate::alloc_trace::AllocTracer::new(trace_dir, metadata)?);
    Ok(self)
}
```

### 5. Handle syscall in run_loops.rs

In `handle_syscall`, add a branch for `SYSCALL_ALLOC_TRACE`:

```rust
} else if syscall_info.number == SYSCALL_ALLOC_TRACE {
    #[cfg(feature = "std")]
    if let Some(ref mut tracer) = self.alloc_tracer {
        let event_type = syscall_info.args[0];
        let frames = self.unwind_backtrace(self.pc, &self.regs);

        let event = match event_type {
            0 => { /* ALLOC: build AllocEvent with t="A" */ }
            1 => { /* DEALLOC: build AllocEvent with t="D" */ }
            2 => { /* REALLOC: build AllocEvent with t="R", include old_ptr/old_sz */ }
            _ => { /* unknown, ignore */ }
        };

        tracer.record_event(event);
    }
    self.regs[Gpr::A0.num() as usize] = 0;
    Ok(StepResult::Continue)
}
```

The instruction count is available as `self.instruction_count`.

### 6. Re-export and module registration

In `lp-riscv/lp-riscv-emu/src/emu/mod.rs` or the crate root, expose
`AllocTracer` and `TraceMetadata` publicly (gated behind `std`).

## Validate

```bash
# Check host crate compiles with std
cargo check -p lp-riscv-emu --features std

# Check elf crate compiles
cargo check -p lp-riscv-elf --features std

# Run existing tests
cargo test -p lp-riscv-emu
cargo test -p lp-riscv-elf

# Clippy
cargo clippy -p lp-riscv-emu --features std
cargo clippy -p lp-riscv-elf --features std
```
