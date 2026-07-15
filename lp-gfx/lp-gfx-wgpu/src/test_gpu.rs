//! Adapter-gated wgpu device helper for unit tests.
//!
//! Returns `None` when the host has no GPU adapter (e.g. CI) so tests skip
//! gracefully instead of failing. Integration tests keep their own copy in
//! `tests/util/mod.rs` (dev-dependency scope).

/// Request a device/queue on the first available adapter, or `None`.
pub(crate) fn test_gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
    }))
    .ok()?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("lp-gfx-wgpu tests"),
        ..Default::default()
    }))
    .ok()?;
    Some((device, queue))
}
