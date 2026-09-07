//! The GPU tier's fault flag: how a spent loop budget reaches the host.
//!
//! [`crate::loop_bound_pass`] gives every module with loops one `@group(0)`
//! `var<storage, read_write> lp_gfx_loop_fault: atomic<u32>` and adds one
//! to it from the invocation that crosses the budget. This module owns the
//! buffer behind that binding and the per-dispatch protocol around it:
//!
//! 1. [`FaultFlag::begin`] before the draw — the flag is cleared in the
//!    command stream, so the count is the dispatch's own;
//! 2. [`FaultFlag::capture`] after the draw — the flag is copied into a
//!    `MAP_READ` staging buffer in the same command buffer;
//! 3. [`FaultFlag::collect`] after the submit — the count is read and a
//!    non-zero one becomes [`GfxError::FuelExhausted`], the same typed
//!    error the LPVM tiers raise from their fuel trap, which the shader
//!    node already routes to a `Fault`.
//!
//! How the count leaves the GPU follows the read-back doctrine
//! ([`crate::read_back`]):
//!
//! - **native** — `collect` maps the staging buffer and waits on the
//!   dispatch's own submission (bounded, like every product read-back),
//!   so the fault is reported for the frame that ran dry.
//! - **wasm32** — the browser cannot block on a map. `begin` harvests the
//!   *previous* dispatch's map if it has landed (the worker's event loop
//!   turns between ticks), `capture` copies only while no map is
//!   outstanding, `collect` issues the map for the frame just drawn and
//!   reports what `begin` harvested: one frame of latency. Harvesting
//!   before the draw rather than after keeps a capture in flight every
//!   frame, so a runaway faults every frame and the engine's 1 s
//!   persistence rule trips instead of seeing a fault every other frame.

use lp_gfx::GfxError;
use lp_shader::{DEFAULT_INVOCATION_FUEL, ShaderFuelTrap, ShaderFuelTrapEntry};

/// Bytes in the flag: one `u32`.
const FLAG_SIZE: u64 = 4;

/// One shader's fault flag: the storage buffer the module binds plus the
/// staging buffer the host reads.
pub(crate) struct FaultFlag {
    /// `@group(0)` slot the loop-bound pass gave the flag.
    binding: u32,
    /// The bound buffer (`STORAGE`), cleared and copied in the command
    /// stream.
    flag: wgpu::Buffer,
    /// `MAP_READ` staging copy the host reads.
    staging: wgpu::Buffer,
    /// Browser one-frame-latency state: `Some` while a `map_async` on
    /// `staging` is outstanding (single-threaded wasm — the lock is never
    /// contended).
    #[cfg(target_arch = "wasm32")]
    pending: Option<std::sync::Arc<std::sync::Mutex<Option<Result<(), wgpu::BufferAsyncError>>>>>,
    /// Browser: whether this dispatch copied into `staging` (so `collect`
    /// should map it).
    #[cfg(target_arch = "wasm32")]
    captured: bool,
    /// Browser: the count `begin` harvested from the previous dispatch.
    #[cfg(target_arch = "wasm32")]
    harvested: u32,
}

impl FaultFlag {
    pub(crate) fn new(device: &wgpu::Device, binding: u32) -> Self {
        let flag = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lp-gfx-wgpu loop fault flag"),
            size: FLAG_SIZE,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lp-gfx-wgpu loop fault staging"),
            size: FLAG_SIZE,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Self {
            binding,
            flag,
            staging,
            #[cfg(target_arch = "wasm32")]
            pending: None,
            #[cfg(target_arch = "wasm32")]
            captured: false,
            #[cfg(target_arch = "wasm32")]
            harvested: 0,
        }
    }

    /// The `@group(0)` slot the module binds the flag to.
    pub(crate) fn binding(&self) -> u32 {
        self.binding
    }

    /// The flag's bind group layout entry: a read-write storage buffer
    /// visible to the fragment stage (where every authored loop runs).
    pub(crate) fn layout_entry(&self) -> wgpu::BindGroupLayoutEntry {
        wgpu::BindGroupLayoutEntry {
            binding: self.binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: false },
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(FLAG_SIZE),
            },
            count: None,
        }
    }

    /// The flag's bind group entry.
    pub(crate) fn bind_group_entry(&self) -> wgpu::BindGroupEntry<'_> {
        wgpu::BindGroupEntry {
            binding: self.binding,
            resource: self.flag.as_entire_binding(),
        }
    }

    /// Before the draw: clear the flag in the command stream (and, on the
    /// browser, harvest the previous dispatch's count if its map landed).
    pub(crate) fn begin(&mut self, encoder: &mut wgpu::CommandEncoder) -> Result<(), GfxError> {
        #[cfg(target_arch = "wasm32")]
        self.harvest()?;
        encoder.clear_buffer(&self.flag, 0, None);
        Ok(())
    }

    /// After the draw: copy the flag into the staging buffer. On the
    /// browser the copy is skipped while a previous map still holds the
    /// staging buffer (a slow frame); the next dispatch captures again.
    pub(crate) fn capture(&mut self, encoder: &mut wgpu::CommandEncoder) {
        #[cfg(target_arch = "wasm32")]
        {
            if self.pending.is_some() {
                self.captured = false;
                return;
            }
            self.captured = true;
        }
        encoder.copy_buffer_to_buffer(&self.flag, 0, &self.staging, 0, FLAG_SIZE);
    }

    /// After the submit: the dispatch's verdict. `Ok` when no invocation
    /// spent its budget, [`GfxError::FuelExhausted`] with the count when
    /// some did. Native waits (bounded by `timeout`) on `submission`; the
    /// browser reports the previous dispatch's count (see the module docs)
    /// and never waits.
    pub(crate) fn collect(
        &mut self,
        device: &wgpu::Device,
        submission: wgpu::SubmissionIndex,
        timeout: Option<core::time::Duration>,
    ) -> Result<(), GfxError> {
        let spent = self.read(device, submission, timeout)?;
        if spent == 0 {
            return Ok(());
        }
        Err(GfxError::FuelExhausted(ShaderFuelTrap {
            entry: ShaderFuelTrapEntry::Invocations { spent },
            budget: DEFAULT_INVOCATION_FUEL,
        }))
    }

    /// Native: map the staging buffer, wait for the dispatch's submission,
    /// read the count. The wait targets this submission specifically
    /// (`submission_index: None` starves under concurrent submitters — see
    /// [`crate::read_back::read_back_f32`]).
    #[cfg(not(target_arch = "wasm32"))]
    fn read(
        &mut self,
        device: &wgpu::Device,
        submission: wgpu::SubmissionIndex,
        timeout: Option<core::time::Duration>,
    ) -> Result<u32, GfxError> {
        let slice = self.staging.slice(..);
        let (map_tx, map_rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = map_tx.send(result);
        });
        device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout,
            })
            .map_err(|e| GfxError::Backend(format!("loop fault flag device poll: {e:?}")))?;
        match map_rx.try_recv() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                return Err(GfxError::Backend(format!(
                    "loop fault flag buffer map: {e:?}"
                )));
            }
            Err(_) => {
                return Err(GfxError::Backend(String::from(
                    "loop fault flag buffer map did not complete after device poll",
                )));
            }
        }
        let spent = read_count(&slice.get_mapped_range());
        self.staging.unmap();
        Ok(spent)
    }

    /// Browser: issue the map for the frame just captured and report what
    /// [`Self::begin`] harvested from the previous one.
    #[cfg(target_arch = "wasm32")]
    fn read(
        &mut self,
        _device: &wgpu::Device,
        _submission: wgpu::SubmissionIndex,
        _timeout: Option<core::time::Duration>,
    ) -> Result<u32, GfxError> {
        use std::sync::{Arc, Mutex};

        if core::mem::take(&mut self.captured) {
            let state = Arc::new(Mutex::new(None));
            let callback_state = Arc::clone(&state);
            self.staging
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    *callback_state.lock().expect("map state (uncontended)") = Some(result);
                });
            self.pending = Some(state);
        }
        Ok(core::mem::take(&mut self.harvested))
    }

    /// Browser: if the outstanding map has landed, take its count and free
    /// the staging buffer for the next capture.
    #[cfg(target_arch = "wasm32")]
    fn harvest(&mut self) -> Result<(), GfxError> {
        let Some(state) = &self.pending else {
            return Ok(());
        };
        let landed = state.lock().expect("map state (uncontended)").take();
        match landed {
            None => Ok(()),
            Some(Err(e)) => {
                self.pending = None;
                Err(GfxError::Backend(format!(
                    "loop fault flag buffer map: {e:?}"
                )))
            }
            Some(Ok(())) => {
                self.harvested = read_count(&self.staging.slice(..).get_mapped_range());
                self.staging.unmap();
                self.pending = None;
                Ok(())
            }
        }
    }
}

fn read_count(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}
