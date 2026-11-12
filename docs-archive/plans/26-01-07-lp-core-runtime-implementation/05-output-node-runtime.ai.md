# Phase 5: Output Node Runtime

## Goal

Implement OutputNodeRuntime and OutputNodeConfig, including OutputProvider trait.

## Tasks

1. Create `nodes/output/config.rs`:
   - Move `OutputNode` enum from `nodes/output.rs` to here
   - Rename to `OutputNodeConfig` (or keep as `OutputNode` if that's the pattern)
   - Ensure it uses type-safe IDs where needed
   - Export from `nodes/output/mod.rs`

2. Create `nodes/output/runtime.rs` with:
   - `OutputNodeRuntime` struct:
     - `handle: Option<Box<dyn LedOutput>>`
     - `pixel_count: usize`
     - `bytes_per_pixel: usize`
     - `buffer: Vec<u8>`
     - `status: NodeStatus`
   - Method: `buffer_mut() -> &mut [u8]` (provides mutable access to buffer)
   - Implement `NodeLifecycle` trait:
     - `Config = OutputNodeConfig`
     - `RenderContext = OutputRenderContext`
     - `init()`: Derives `bytes_per_pixel` from config chip type (e.g., "ws2812" = 3), allocates buffer (`pixel_count * bytes_per_pixel`), calls `OutputProvider.create_output()` to get handle
     - `update()`: Reads buffer and calls `handle.write_pixels()` to send to hardware/UI
     - `destroy()`: Cleanup handle if needed

3. Create `traits/output_provider.rs` with:
   - `OutputProvider` trait:
     - `create_output(&self, config: &OutputNodeConfig) -> Result<Box<dyn LedOutput>, Error>`
     - For `GpioStrip`: configures GPIO pin from `config.gpio_pin`, sets up chip driver

4. Update `traits/mod.rs` to export OutputProvider

5. Add tests:
   - Test OutputNodeRuntime::init() allocates buffer correctly
   - Test buffer_mut() returns correct slice
   - Test bytes_per_pixel derivation from chip type
   - Test status tracking
   - Mock OutputProvider for testing

## Success Criteria

- OutputNodeRuntime compiles and implements NodeLifecycle
- OutputProvider trait compiles
- Buffer management works correctly
- All tests pass
- No warnings (except unused code that will be used in later phases)
- Code follows existing style

