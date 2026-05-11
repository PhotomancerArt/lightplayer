# Phase 6: Create HostOutputProvider

## Goal

Implement `OutputProvider` for fw-host that creates `HostLedOutput` instances.

## Tasks

1. Create `fw-host/src/output_provider.rs`:
   - `HostOutputProvider` struct:
     - Manages mapping from output configs to `HostLedOutput` instances
     - Stores `Arc<Mutex<dyn LedOutput>>` instances (or similar)
   - Implement `OutputProvider` trait:
     - `create_output(&self, config: &OutputNode) -> Result<Box<dyn LedOutput>, Error>`
     - For `GpioStrip`: create `HostLedOutput` with appropriate pixel count and bytes per pixel
     - Return boxed trait object

2. Add constructor: `new() -> Self` or similar

3. Update `fw-host/src/main.rs` to include `output_provider` module

## Success Criteria

- `HostOutputProvider` compiles
- Implements `OutputProvider` trait correctly
- Creates `HostLedOutput` instances
- Can be used by `LpApp`
- Code compiles without warnings

