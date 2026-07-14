//! Worker-scoped WebGPU device host.
//!
//! One wgpu device per worker, shared by every GPU-tier runtime the worker
//! hosts. The async adapter/device request lives here at the worker edge
//! (fw-browser is an edge crate — sans-IO ADR); `lp-gfx-wgpu` itself exposes
//! no async and receives the device fully formed.
//!
//! Initialization is explicit: the worker script awaits [`init_device`]
//! (exported as `init_gpu_device`) once at boot and the outcome — available
//! or unavailable with a reason — is recorded for every later runtime
//! creation. Per the fidelity-tiers ADR there is no silent fallback: a
//! runtime that requests the GPU tier while the device is unavailable is
//! created on the CPU tier with the recorded reason attached, surfaced, and
//! queryable.

use std::cell::RefCell;
use std::rc::Rc;

/// The worker's shared WebGPU objects.
pub(crate) struct WorkerGpu {
    pub(crate) instance: wgpu::Instance,
    pub(crate) adapter: wgpu::Adapter,
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
}

enum GpuInit {
    /// Initialization ran and failed; the reason is attached to every
    /// CPU-tier selection that follows.
    Unavailable(String),
    Ready(Rc<WorkerGpu>),
}

thread_local! {
    /// `None` until `init_device` has run (single-threaded worker wasm).
    static GPU: RefCell<Option<GpuInit>> = const { RefCell::new(None) };
    /// Set by the wgpu device-lost callback; checked by every present.
    static DEVICE_LOST: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Request the worker's WebGPU adapter/device (idempotent).
///
/// Returns `Ok(())` when the device is ready, `Err(reason)` when WebGPU is
/// unavailable in this worker (missing `navigator.gpu`, adapter request
/// rejected, device request failed). Either outcome is recorded; runtimes
/// consult it synchronously through [`device`].
pub(crate) async fn init_device() -> Result<(), String> {
    let already = GPU.with(|gpu| {
        gpu.borrow().as_ref().map(|init| match init {
            GpuInit::Ready(_) => Ok(()),
            GpuInit::Unavailable(reason) => Err(reason.clone()),
        })
    });
    if let Some(outcome) = already {
        return outcome;
    }

    let outcome = request_device().await;
    let result = match &outcome {
        Ok(_) => Ok(()),
        Err(reason) => Err(reason.clone()),
    };
    GPU.with(|gpu| {
        *gpu.borrow_mut() = Some(match outcome {
            Ok(worker_gpu) => GpuInit::Ready(Rc::new(worker_gpu)),
            Err(reason) => GpuInit::Unavailable(reason),
        });
    });
    result
}

/// The worker's GPU, or the reason it is unavailable.
///
/// `Err` carries either the recorded init failure or a note that
/// `init_gpu_device` was never awaited (a worker-script bug — surfaced, not
/// papered over).
pub(crate) fn device() -> Result<Rc<WorkerGpu>, String> {
    GPU.with(|gpu| match gpu.borrow().as_ref() {
        Some(GpuInit::Ready(worker_gpu)) => Ok(Rc::clone(worker_gpu)),
        Some(GpuInit::Unavailable(reason)) => Err(reason.clone()),
        None => Err(String::from(
            "worker gpu device was never initialized (init_gpu_device not awaited at boot)",
        )),
    })
}

/// The device-lost reason, if the worker's device has been lost.
pub(crate) fn device_lost() -> Option<String> {
    DEVICE_LOST.with(|lost| lost.borrow().clone())
}

async fn request_device() -> Result<WorkerGpu, String> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::None,
            force_fallback_adapter: false,
            compatible_surface: None,
        })
        .await
        .map_err(|error| format!("webgpu adapter request failed: {error}"))?;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("fw-browser worker gpu"),
            ..Default::default()
        })
        .await
        .map_err(|error| format!("webgpu device request failed: {error}"))?;

    // Device-lost is surfaced, never silently retried: presents fail with
    // the recorded reason and cards enter a visible error state.
    device.set_device_lost_callback(|reason, message| {
        DEVICE_LOST.with(|lost| {
            *lost.borrow_mut() = Some(format!("{reason:?}: {message}"));
        });
    });

    Ok(WorkerGpu {
        instance,
        adapter,
        device,
        queue,
    })
}
