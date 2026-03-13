# Node Panic Recovery

# Design

## Scope of Work

Wrap all `NodeRuntime::render()` call sites in `catch_unwind` so that panics in
node execution (shader JIT, fixtures, outputs) are caught and reported as
`NodeStatus::Error` instead of crashing the firmware. Include panic message,
file/line, and raw frame PCs (hex) in the error payload for off-device
symbolication.

## File Structure

```
lp-core/lp-shared/
└── src/
    ├── backtrace.rs                # NEW: PanicPayload + capture_frames() dispatch
    └── lib.rs                      # UPDATE: add pub mod backtrace

lp-core/lp-engine/
├── Cargo.toml                      # UPDATE: oom-recovery → panic-recovery
└── src/
    ├── project/
    │   └── runtime.rs              # UPDATE: wrap 3 render sites in catch_unwind
    └── nodes/
        └── shader/
            └── runtime.rs          # UPDATE: oom-recovery → panic-recovery

lp-core/lp-server/
└── Cargo.toml                      # UPDATE: oom-recovery → panic-recovery

lp-fw/fw-esp32/
├── Cargo.toml                      # UPDATE: oom-recovery → panic-recovery
└── src/
    └── main.rs                     # UPDATE: panic handler builds PanicPayload

lp-fw/fw-emu/
└── Cargo.toml                      # UPDATE: oom-recovery → panic-recovery

lp-riscv/lp-riscv-emu-guest/
├── Cargo.toml                      # UPDATE: add lp-shared dep
└── src/
    └── panic.rs                    # UPDATE: panic handler builds PanicPayload
```

## Conceptual Architecture

```
  Node panic occurs (e.g. shader JIT execution)
       │
       ▼
  Platform panic handler (fw-esp32 or emu-guest)
       │
       ├─ Formats message + file + line from PanicInfo
       ├─ Calls capture_frames() → arch-specific FP walker
       ├─ Packs into PanicPayload (owned, survives unwind)
       └─ Calls begin_panic(Box::new(payload))
       │
       ▼
  catch_unwind in ProjectRuntime (lp-engine)
       │
       ├─ Downcasts Box<dyn Any> → PanicPayload
       ├─ Formats: "panic: msg (at file:line) [0x..., 0x...]"
       └─ Sets NodeStatus::Error(formatted_string)
       │
       ▼
  Node skipped on subsequent frames (existing behavior)
  Error surfaced to client via project API (existing behavior)
```

## Main Components

- **`PanicPayload`** (`lp-shared::backtrace`): Owned struct carrying panic
  message, file, line, and raw frame PCs. Platform-independent.
- **`capture_frames()`** (`lp-shared::backtrace`): Platform-independent API
  that dispatches to arch-specific FP walkers. Returns number of frames
  captured into a caller-provided `[u32]` buffer.
- **`catch_node_panic()`** (`lp-engine::project::runtime`): Helper that wraps
  a render closure in `catch_unwind`, extracts `PanicPayload`, and converts
  to `Result<(), Error>`. Used at all three render call sites.
- **Panic handlers** (fw-esp32, emu-guest): Updated to build `PanicPayload`
  with message + file + line + frames, then pass to `begin_panic`.

# Phases

## Phase 1: PanicPayload + capture_frames in lp-shared

### Scope

Add `lp-shared::backtrace` module with:
- `PanicPayload` struct
- `capture_frames()` with riscv32 implementation and fallback stubs
- `PanicPayload::format_error()` method for producing the NodeStatus string

### Code Organization Reminders

- `backtrace.rs` should have the public types and dispatch function at the top,
  arch-specific implementations at the bottom.
- The riscv32 FP walker uses unsafe inline asm — keep it isolated in its own
  function.

### Implementation Details

New file `lp-core/lp-shared/src/backtrace.rs`:

```rust
use alloc::format;
use alloc::string::String;

pub const MAX_FRAMES: usize = 16;

/// Panic payload that survives unwinding.
///
/// Built by platform panic handlers, caught by catch_unwind in the engine.
pub struct PanicPayload {
    pub message: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub frames: [u32; MAX_FRAMES],
    pub frame_count: usize,
}

impl PanicPayload {
    pub fn new(message: String, file: Option<String>, line: Option<u32>) -> Self {
        let mut payload = Self {
            message,
            file,
            line,
            frames: [0; MAX_FRAMES],
            frame_count: 0,
        };
        payload.frame_count = capture_frames(&mut payload.frames);
        payload
    }

    /// Format as error string for NodeStatus::Error.
    ///
    /// Format: "panic: <msg> (at <file>:<line>) [0x00001234, 0x00005678, ...]"
    pub fn format_error(&self) -> String {
        let mut s = format!("panic: {}", self.message);
        if let Some(ref file) = self.file {
            if let Some(line) = self.line {
                s.push_str(&format!(" (at {file}:{line})"));
            } else {
                s.push_str(&format!(" (at {file})"));
            }
        }
        if self.frame_count > 0 {
            s.push_str(" [");
            for i in 0..self.frame_count {
                if i > 0 {
                    s.push_str(", ");
                }
                s.push_str(&format!("0x{:08x}", self.frames[i]));
            }
            s.push(']');
        }
        s
    }
}

/// Capture stack frame return addresses into `buf`.
///
/// Returns the number of frames written. Platform-specific: uses frame pointer
/// walking on supported architectures, returns 0 on unsupported platforms.
pub fn capture_frames(buf: &mut [u32]) -> usize {
    capture_frames_arch(buf)
}

#[cfg(target_arch = "riscv32")]
fn capture_frames_arch(buf: &mut [u32]) -> usize {
    if buf.is_empty() {
        return 0;
    }
    let mut count = 0;
    let fp: u32;
    unsafe { core::arch::asm!("mv {}, s0", out(reg) fp) };

    let mut fp = fp;
    while count < buf.len() {
        if fp == 0 || fp % 4 != 0 {
            break;
        }
        let ra = unsafe { ((fp - 4) as *const u32).read() };
        let prev_fp = unsafe { ((fp - 8) as *const u32).read() };
        if ra == 0 {
            break;
        }
        buf[count] = ra;
        count += 1;
        if prev_fp == 0 || prev_fp <= fp {
            break;
        }
        fp = prev_fp;
    }
    count
}

#[cfg(target_arch = "wasm32")]
fn capture_frames_arch(_buf: &mut [u32]) -> usize {
    0
}

#[cfg(not(any(target_arch = "riscv32", target_arch = "wasm32")))]
fn capture_frames_arch(_buf: &mut [u32]) -> usize {
    0
}
```

Add `pub mod backtrace;` to `lp-core/lp-shared/src/lib.rs`.

### Validate

```
cargo test --package lp-shared
cargo check --package lp-engine
just check
```

## Phase 2: Rename oom-recovery → panic-recovery

### Scope

Mechanical rename of the feature flag across all Cargo.toml files and
`#[cfg(feature = "...")]` attributes.

### Code Organization Reminders

- Pure rename, no logic changes.

### Implementation Details

Files to change:

**`lp-core/lp-engine/Cargo.toml`**: `oom-recovery` → `panic-recovery`
```toml
panic-recovery = ["dep:unwinding"]
```

**`lp-core/lp-engine/src/nodes/shader/runtime.rs`**: 4 occurrences
```
#[cfg(feature = "oom-recovery")]  →  #[cfg(feature = "panic-recovery")]
#[cfg(not(feature = "oom-recovery"))]  →  #[cfg(not(feature = "panic-recovery"))]
```

**`lp-core/lp-server/Cargo.toml`**:
```toml
panic-recovery = ["lp-engine/panic-recovery"]
```

**`lp-fw/fw-esp32/Cargo.toml`**:
```toml
lp-server = { ..., features = ["panic-recovery"], ... }
```

**`lp-fw/fw-emu/Cargo.toml`**:
```toml
lp-server = { ..., features = ["panic-recovery"] }
```

Also update the comment in `justfile` line 8:
```
# fw-esp32 uses release-esp32 (panic=unwind, nightly) for panic recovery
```

And the `test_oom` feature description in `fw-esp32/Cargo.toml` line 30:
```toml
test_oom = []   # Allocate until OOM, verify catch_unwind recovers (for panic recovery validation)
```

### Validate

```
cargo check --package lp-engine --features panic-recovery
cargo check --package lp-server --features panic-recovery
just check
```

## Phase 3: Wrap render call sites in catch_unwind

### Scope

Add `catch_node_panic()` helper in `project/runtime.rs` and wrap all three
render call sites. Gate behind `panic-recovery` feature. Without the feature,
behavior is unchanged.

### Code Organization Reminders

- `catch_node_panic` helper goes near the bottom of `runtime.rs` (utility fn).
- Keep the render loop structure clean — the catch_unwind wrapping should be
  minimal and readable.

### Implementation Details

Add to `lp-core/lp-engine/src/project/runtime.rs`:

Imports (at top, feature-gated):
```rust
#[cfg(feature = "panic-recovery")]
use core::panic::AssertUnwindSafe;
#[cfg(feature = "panic-recovery")]
use unwinding::panic::catch_unwind;
```

Add `lp-shared` import for `PanicPayload`:
```rust
#[cfg(feature = "panic-recovery")]
use lp_shared::backtrace::PanicPayload;
```

Helper function (at bottom of file):
```rust
/// Wrap a render call in catch_unwind, converting panics to Error.
///
/// If the closure panics and panic-recovery is enabled, catches the panic
/// and returns an Err with the formatted panic info. Without panic-recovery,
/// this is a direct call (panics propagate normally).
#[cfg(feature = "panic-recovery")]
fn catch_node_panic(
    f: impl FnOnce() -> Result<(), Error>,
) -> Result<(), Error> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(result) => result,
        Err(payload) => {
            let msg = if let Some(p) = payload.downcast_ref::<PanicPayload>() {
                p.format_error()
            } else {
                alloc::string::String::from("panic: unknown (no payload)")
            };
            Err(Error::new(msg))
        }
    }
}

#[cfg(not(feature = "panic-recovery"))]
fn catch_node_panic(
    f: impl FnOnce() -> Result<(), Error>,
) -> Result<(), Error> {
    f()
}
```

Then wrap the three render calls. Each of the three blocks that currently does:
```rust
unsafe { (*runtime_ptr).render(&mut ctx) }
```
becomes:
```rust
catch_node_panic(|| unsafe { (*runtime_ptr).render(&mut ctx) })
```

The wrapping goes around the entire block that produces `render_result`, so
the existing error-handling code (setting `NodeStatus::Error`) picks up panic
errors naturally.

For the `ensure_texture_rendered` shader render path (line ~1611), same
pattern — wrap the inner render call.

### Validate

```
cargo test --package lp-engine
just test-app
just check
```

## Phase 4: Update panic handlers to build PanicPayload

### Scope

Update both panic handlers to construct `PanicPayload` with message, file,
line, and frame PCs, then pass it to `begin_panic`.

### Code Organization Reminders

- Keep panic handlers concise — delegate formatting to `PanicPayload::new()`.
- The `lp-riscv-emu-guest` panic handler has two variants (with/without
  unwinding). Only the `unwinding` variant needs `PanicPayload`.

### Implementation Details

**`lp-fw/fw-esp32/src/main.rs`** — update the `#[panic_handler]`:

```rust
#[panic_handler]
fn panic_handler(info: &PanicInfo) -> ! {
    esp_println::println!("\n\n====================== PANIC ======================");
    esp_println::println!("{info}");
    esp_println::println!();

    let message = {
        use core::fmt::Write;
        let mut buf = alloc::string::String::new();
        let _ = write!(buf, "{}", info.message());
        if buf.is_empty() {
            alloc::string::String::from("panic occurred (no message)")
        } else {
            buf
        }
    };

    let (file, line) = if let Some(loc) = info.location() {
        (Some(alloc::string::String::from(loc.file())), Some(loc.line()))
    } else {
        (None, None)
    };

    let payload = lp_shared::backtrace::PanicPayload::new(message, file, line);
    let code = unwinding::panic::begin_panic(alloc::boxed::Box::new(payload));

    esp_println::println!("unwinding failed: code={}", code.0);
    loop {}
}
```

Note: `lp-shared` is already an optional dependency of `fw-esp32` (behind
`server` feature). The panic handler runs unconditionally, so we need
`lp-shared` as a non-optional dependency. Move it from optional to required,
or gate the PanicPayload construction behind `server` feature with a fallback
ZST payload when `server` is not enabled.

Simpler approach: keep `lp-shared` optional, gate the PanicPayload usage:

```rust
#[cfg(feature = "server")]
let payload: alloc::boxed::Box<dyn core::any::Any> = {
    // ... build PanicPayload as above ...
    alloc::boxed::Box::new(payload)
};
#[cfg(not(feature = "server"))]
let payload: alloc::boxed::Box<dyn core::any::Any> = {
    struct Dummy;
    alloc::boxed::Box::new(Dummy)
};
let code = unwinding::panic::begin_panic(payload);
```

**`lp-riscv/lp-riscv-emu-guest/src/panic.rs`** — update the unwinding variant:

Add `lp-shared` as a dependency of `lp-riscv-emu-guest` (default-features =
false), gated behind the `unwinding` feature:

In `lp-riscv-emu-guest/Cargo.toml`:
```toml
lp-shared = { path = "../../lp-core/lp-shared", optional = true, default-features = false }

[features]
unwinding = ["dep:unwinding", "dep:lp-shared"]
```

Update the unwinding panic handler:
```rust
#[cfg(feature = "unwinding")]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    extern crate alloc;
    use core::fmt::Write;

    let message = {
        let mut buf = alloc::string::String::new();
        let _ = write!(buf, "{}", info.message());
        if buf.is_empty() {
            alloc::string::String::from("panic occurred (no message)")
        } else {
            buf
        }
    };
    let (file, line) = if let Some(loc) = info.location() {
        (Some(alloc::string::String::from(loc.file())), Some(loc.line()))
    } else {
        (None, None)
    };

    let payload = lp_shared::backtrace::PanicPayload::new(message, file, line);
    let _code = unwinding::panic::begin_panic(alloc::boxed::Box::new(payload));

    // begin_panic returned — no catch_unwind on stack. Fall back to host report.
    report_panic_to_host(info);
}
```

### Validate

```
cargo check --package fw-esp32
cargo check --package lp-riscv-emu-guest --features unwinding
cargo check --target riscv32imac-unknown-none-elf --package fw-esp32
just check
```

Also run the existing unwind test if available:
```
cargo test --package fw-emu unwind
```

## Phase 5: Cleanup & validation

### Cleanup & validation

- Grep the git diff for TODO, debug prints, temporary code. Remove them.
- Run `cargo +nightly fmt` on all changed files.
- Fix all warnings.

Validation:
```
just check
just test-app
cargo check --target riscv32imac-unknown-none-elf --package fw-esp32
cargo check --target riscv32imac-unknown-none-elf --package fw-emu
```

### Plan cleanup

Move remaining notes to `# Notes` at bottom. Move plan file to
`docs/plans-done/`.

### Commit

```
feat(engine): catch panics in node render and report as errors

- Add PanicPayload and capture_frames() to lp-shared::backtrace
- Rename oom-recovery feature to panic-recovery
- Wrap all render call sites in catch_unwind (fixture, output, shader)
- Update fw-esp32 and emu-guest panic handlers to build PanicPayload
  with message, file, line, and raw frame PCs for off-device symbolication
```

# Notes

## Questions (answered)

### Q1: Feature gating
Rename `oom-recovery` to `panic-recovery`.

### Q2: Frame pointer walking location
In the panic handler (before `begin_panic`), since stack is gone after unwind.

### Q3: Panic payload struct
`String` for message/file, `[u32; 16]` for frames. In `lp-shared`.

### Q4: Shared types location
`lp-shared::backtrace` — dep of both engine and firmware.

### Q5: Frame pointer walking impl
New on-device FP walker in `lp-shared::backtrace`. Dispatch via
`capture_frames()` → arch-specific `capture_frames_arch()`.
- `riscv32`: inline asm FP walk
- `wasm32`: stub (0 frames)
- fallback: stub (0 frames)

### Q6: Texture invalidation
Not needed — minor issue during dev, existing flow skips `state_ver` update.

### Q7: Error message format
`"panic: <msg> (at <file>:<line>) [0x..., 0x...]"` in `NodeStatus::Error`.
