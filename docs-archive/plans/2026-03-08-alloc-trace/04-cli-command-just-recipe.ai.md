# Phase 4: lp-cli emu-trace Command + just Recipe

## Scope

Add the `lp-cli emu-trace` subcommand and a `just emu-trace` recipe that wraps
the build + invocation. This is the primary user-facing entry point for
allocation tracing.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Add `emu-trace` subcommand to lp-cli

In `lp-cli/src/main.rs`, add a new variant:

```rust
/// Run emulator with allocation tracing
EmuTrace {
    /// Project directory
    dir: std::path::PathBuf,
    /// Path to pre-built fw-emu binary (with alloc-trace feature)
    #[arg(long)]
    bin: std::path::PathBuf,
    /// Number of frames to render (default: 60)
    #[arg(long, default_value = "60")]
    frames: u32,
    /// Output directory (default: auto-generated under traces/)
    #[arg(long)]
    output: Option<std::path::PathBuf>,
},
```

### 2. Create command module

Create `lp-cli/src/commands/emu_trace/mod.rs`:

```rust
pub mod handler;

pub struct EmuTraceArgs {
    pub dir: std::path::PathBuf,
    pub bin: std::path::PathBuf,
    pub frames: u32,
    pub output: Option<std::path::PathBuf>,
}
```

Create `lp-cli/src/commands/emu_trace/handler.rs`:

The handler should:

1. **Generate output directory** if not specified:
   ```
   traces/YYYY-MM-DD-HHMMSS-<project-name>/
   ```
   Extract project name from the directory name.

2. **Load ELF**:
   ```rust
   let elf_data = std::fs::read(&args.bin)?;
   let load_info = load_elf(&elf_data)?;
   let symbol_list = load_info.symbol_list;
   ```

3. **Build metadata** and create `AllocTracer`:
   ```rust
   let metadata = TraceMetadata {
       version: 1,
       timestamp: chrono_or_manual_timestamp(),
       project: project_name,
       frames_requested: args.frames,
       heap_start: /* from linker symbols */,
       heap_size: /* from linker symbols */,
       symbols: symbol_list,
   };
   ```

   For timestamp, use a simple format. Avoid adding `chrono` as a dep --
   use `std::time::SystemTime` and format manually, or use a lightweight
   approach.

4. **Create emulator** with tracing:
   ```rust
   let mut emulator = Riscv32Emulator::new(load_info.code, load_info.ram)
       .with_alloc_trace(&output_dir, metadata)?
       .with_log_level(LogLevel::Instructions)
       .with_time_mode(TimeMode::Simulated(0))
       .with_allow_unaligned_access(true);
   ```

5. **Set up serial transport and client** (reuse patterns from
   `scene_render_emu`):
   - `SerialEmuClientTransport::new(emulator_arc)`
   - `LpClient::new(Box::new(transport))`

6. **Read project directory** and sync files to emulator fs:
   - Read all files from `args.dir` recursively
   - Use `client.sync_files(...)` or similar to push them

7. **Load project, tick frames**:
   ```rust
   client.load_project(&project_path)?;
   for _ in 0..args.frames {
       // advance time, run emulator, process serial
   }
   client.stop_all_projects()?;
   ```

8. **Flush and report**:
   ```rust
   // Flush tracer
   let event_count = emulator.lock().unwrap().finish_alloc_trace()?;
   println!("Trace written to {}", output_dir.display());
   println!("  {} allocation events recorded", event_count);
   println!("  meta.json: symbol table and run configuration");
   println!("  heap-trace.jsonl: allocation/deallocation events");
   ```

### 3. Wire into main.rs

```rust
Cli::EmuTrace { dir, bin, frames, output } => {
    emu_trace::handler::handle_emu_trace(emu_trace::EmuTraceArgs {
        dir, bin, frames, output,
    })
}
```

Update `lp-cli/src/commands/mod.rs`:

```rust
pub mod emu_trace;
```

### 4. Add `just` recipe

In `justfile`, add:

```just
# Build fw-emu with allocation tracing and run against a project
emu-trace project_dir frames="60":
    cargo build -p fw-emu \
        --target riscv32imac-unknown-none-elf \
        --profile release-emu \
        --features alloc-trace
    RUSTFLAGS="-C force-frame-pointers=yes" cargo build -p fw-emu \
        --target riscv32imac-unknown-none-elf \
        --profile release-emu \
        --features alloc-trace
    cargo run -p lp-cli -- emu-trace \
        {{project_dir}} \
        --bin target/riscv32imac-unknown-none-elf/release-emu/fw-emu \
        --frames {{frames}}
```

Note: The RUSTFLAGS for frame pointers need to be set for the fw-emu build.
Check if the `release-emu` profile can be updated to include
`force-frame-pointers = true` globally (since it's always useful for the emu),
which would simplify the recipe.

### 5. Add `traces/` to .gitignore

Append to `.gitignore`:

```
# Allocation trace output
traces/
```

### 6. Move SerialEmuClientTransport to lp-client

`SerialEmuClientTransport` currently lives in `fw-tests/src/transport_emu_serial.rs`.
Both `fw-tests` and `lp-cli` need it, so move it to `lp-client` (it implements
`ClientTransport`, which is defined there).

Steps:
- Move `fw-tests/src/transport_emu_serial.rs` →
  `lp-core/lp-client/src/transport_emu_serial.rs`
- Gate behind a feature (e.g. `emu`) in `lp-client/Cargo.toml` since it pulls
  in `lp-riscv-emu` and `lp-riscv-elf` dependencies:
  ```toml
  [features]
  emu = ["lp-riscv-emu", "lp-riscv-elf"]

  [dependencies]
  lp-riscv-emu = { path = "../../lp-riscv/lp-riscv-emu", features = ["std"], optional = true }
  lp-riscv-elf = { path = "../../lp-riscv/lp-riscv-elf", features = ["std"], optional = true }
  ```
- Re-export from `lp-client` lib: `#[cfg(feature = "emu")] pub mod transport_emu_serial;`
- Update `fw-tests/Cargo.toml` to use `lp-client` with `emu` feature instead of
  its own transport module
- Update `fw-tests/src/lib.rs` and `scene_render_emu.rs` imports accordingly
- Update `lp-cli/Cargo.toml` to enable `lp-client`'s `emu` feature:
  ```toml
  lp-client = { path = "../lp-core/lp-client", features = ["ws", "serial", "emu"] }
  ```
- Verify `fw-tests` and `lp-cli` both compile and the existing
  `test_scene_render_fw_emu` test still passes

### 7. Other dependencies

`lp-cli` already depends on `lp-riscv-emu` (with std) and `lp-riscv-elf`.
No other new dependencies needed beyond the `emu` feature on `lp-client`.

## Validate

```bash
# Build lp-cli
cargo build -p lp-cli

# Run the full flow with a test project
just emu-trace path/to/test-project 10

# Verify output
ls traces/
cat traces/*/meta.json | python3 -m json.tool
head -5 traces/*/heap-trace.jsonl

# Run existing tests
cargo test -p lp-cli
```
