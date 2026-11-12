# Phase 2: Add frame_id to RenderContext

## Scope of Phase

Add `frame_id` to `RenderContext` so that runtimes can access the current frame ID when updating state fields. This is needed for `StateField::set()` and `StateField::mark_updated()` calls.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Update `lp-engine/src/runtime/contexts.rs`

Add `frame_id` method to `RenderContext` trait:

```rust
/// Context for rendering
pub trait RenderContext {
    /// Get texture (triggers lazy rendering if needed)
    fn get_texture(&mut self, handle: TextureHandle) -> Result<&Texture, Error>;

    /// Get mutable texture (triggers lazy rendering if needed)
    fn get_texture_mut(&mut self, handle: TextureHandle) -> Result<&mut Texture, Error>;

    /// Get current frame time in seconds
    fn get_time(&self) -> f32;

    /// Get output buffer slice
    fn get_output(
        &mut self,
        handle: OutputHandle,
        universe: u32,
        start_ch: u32,
        ch_count: u32,
    ) -> Result<&mut [u8], Error>;

    /// Get output provider
    fn output_provider(&self) -> &dyn OutputProvider;

    /// Get current frame ID
    fn frame_id(&self) -> FrameId;
}
```

### 2. Find all implementations of `RenderContext`

Search for implementations of `RenderContext` and update them to implement `frame_id()`:

```bash
grep -r "impl.*RenderContext" lp-engine/
```

Common implementations might be in:
- `lp-engine/src/project/runtime.rs` (for `ProjectRuntime`)
- Test helpers

### 3. Update implementations

For each implementation, add:

```rust
fn frame_id(&self) -> FrameId {
    // Return the current frame ID from the runtime
    self.frame_id  // or however frame ID is stored
}
```

### 4. Update test helpers

If there are test implementations of `RenderContext`, update them to return a dummy `FrameId`:

```rust
fn frame_id(&self) -> FrameId {
    FrameId::default()  // or FrameId::new(0) for tests
}
```

## Validate

Run the following commands to validate this phase:

```bash
cd lp-core/lp-engine
cargo check
cargo test
```

Fix any warnings or errors before proceeding. All code that implements `RenderContext` must now provide `frame_id()`.
