//! Shared test support: the authored-shader corpus (from the M3 spike) and
//! the adapter-gated GPU helper.

// Integration-test binaries each compile this module; not all of them use
// every helper.
#![allow(dead_code, reason = "shared across independent test binaries")]

pub mod corpus;
pub mod diff;
pub mod reference;

use lp_gfx_lpvm::TargetLpvmGraphics;
use lp_gfx_wgpu::GpuGraphics;

/// Request a device/queue on the first available adapter, or `None` when the
/// host has no GPU adapter (e.g. CI) — callers skip gracefully.
pub fn test_gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
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

/// Adapter-gated `GpuGraphics` with a CPU compute delegate, or `None`.
pub fn test_graphics() -> Option<GpuGraphics> {
    let (device, queue) = test_gpu()?;
    Some(GpuGraphics::new(
        device,
        queue,
        Box::new(TargetLpvmGraphics::new(lp_shader::ShaderFrontend::Naga)),
    ))
}
