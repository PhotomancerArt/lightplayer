# Phase 5: Wire Graphics Through LpServer

## Scope

Add `Rc<dyn LpGraphics>` to `LpServer` and pass it through to `ProjectRuntime`.

## Files

```
lp-core/lp-server/src/
├── lib.rs                        # UPDATE: add graphics field, pass to ProjectRuntime
```

## LpServer Changes

### Add graphics field

```rust
use alloc::rc::Rc;
use lp_engine::gfx::LpGraphics;

pub struct LpServer {
    // ... existing fields
    project_runtime: ProjectRuntime,
    graphics: Rc<dyn LpGraphics>,     // NEW: stored for potential restart/reload
}
```

### Constructor changes

```rust
impl LpServer {
    /// Create a new LpServer with injected graphics backend.
    ///
    /// The graphics backend is passed through to ProjectRuntime and
    /// all shader nodes. This allows the firmware to choose the
    /// backend at startup (Cranelift for native, Wasm for browser, etc).
    pub fn new<F>(
        fs_factory: F,
        graphics: Rc<dyn LpGraphics>,     // NEW parameter
    ) -> Self
    where
        F: FnOnce() -> Box<dyn VirtualFileSystem> + 'static,
    {
        let fs = fs_factory();
        let mut project_runtime = ProjectRuntime::new(graphics.clone()); // Pass through
        project_runtime.set_file_watcher(fs.watch_arc());

        Self {
            fs,
            transport: None,
            project_runtime,
            graphics,                       // Store for restarts
        }
    }
}
```

### Server state methods

No changes needed to `get_runtime()`, `get_runtime_mut()`, `get_fs()` — they return `ProjectRuntime` which now contains graphics.

## Call Site Impact

`LpServer::new()` signature changes. All firmware crates need updating:
- `fw-esp32/src/main.rs`
- `fw-emu/src/main.rs`
- Tests in `lp-server/src/` (if any direct instantiation)

## Validate

```bash
cargo check -p lp-server --lib 2>&1 | head -20
```

Expected: Errors in LpServer::new() call sites (fw-esp32, fw-emu, tests).
