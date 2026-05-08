//! Narrow runtime service surface for core [`super::CoreProjectRuntime`].
//!
//! Carries project identity, optional [`OutputProvider`] plumbing, and registered
//! output sinks (fixture-pushed [`crate::runtime_buffer::RuntimeBuffer`] → flush).

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use hashbrown::HashMap;
use lpc_model::{FrameId, TreePath};
use lpc_shared::error::OutputError;
use lpc_shared::output::{OutputChannelHandle, OutputDriverOptions, OutputFormat, OutputProvider};
use lpc_model::nodes::output::{OutputDef, OutputDriverOptionsConfig};

use crate::runtime_buffer::{RuntimeBufferId, RuntimeBufferStore};

/// Per-sink channel state for [`RuntimeServices`] output flushing.
#[derive(Debug)]
struct OutputSinkBinding {
    pin: u32,
    display_options: Option<OutputDriverOptions>,
    channel_handle: Option<OutputChannelHandle>,
    last_byte_count: Option<u32>,
}

/// Failure while flushing registered output sinks.
#[derive(Debug)]
pub enum OutputFlushError {
    MisalignedPayload { buffer_id: RuntimeBufferId },
    Provider(OutputError),
}

impl fmt::Display for OutputFlushError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MisalignedPayload { buffer_id } => write!(
                f,
                "output buffer {buffer_id:?}: payload must be whole u16 RGB triplets (multiple of 6 bytes)",
            ),
            Self::Provider(e) => write!(f, "{e}"),
        }
    }
}

impl core::error::Error for OutputFlushError {}

/// Project-level services and identity, separate from the core engine spine.
pub struct RuntimeServices {
    /// Tree path identifying the project/show root (authored layout anchor).
    project_root: TreePath,
    output_provider: Option<Box<dyn OutputProvider>>,
    /// Fixture-written buffers paired with GPIO output configuration.
    output_sinks: HashMap<RuntimeBufferId, OutputSinkBinding>,
}

impl RuntimeServices {
    pub fn new(project_root: TreePath) -> Self {
        Self {
            project_root,
            output_provider: None,
            output_sinks: HashMap::new(),
        }
    }

    pub fn project_root(&self) -> &TreePath {
        &self.project_root
    }

    /// Replace the optional [`OutputProvider`] used when flushing sinks after each tick.
    pub fn set_output_provider(&mut self, provider: Option<Box<dyn OutputProvider>>) {
        self.close_output_sinks();
        self.output_provider = provider;
    }

    /// Register an output sink: fixture pushes u16 RGB channel bytes into `buffer_id`; flush writes
    /// them through [`OutputProvider`] for `config`'s GPIO pin.
    ///
    /// Insert the backing [`crate::runtime_buffer::RuntimeBuffer`] with
    /// [`Versioned::new`](lpc_model::Versioned::new)([`FrameId::default`](FrameId::default), …)
    /// so untouched sinks do not match the post-tick frame id until the fixture mutates them.
    pub fn register_output_sink(&mut self, buffer_id: RuntimeBufferId, config: &OutputDef) {
        let pin = pin_from_output_config(config);
        let display_options = display_options_from_output_config(config);
        if let Some(mut existing) = self.output_sinks.remove(&buffer_id) {
            self.close_output_sink(&mut existing);
        }
        self.output_sinks.insert(
            buffer_id,
            OutputSinkBinding {
                pin,
                display_options,
                channel_handle: None,
                last_byte_count: None,
            },
        );
    }

    /// Flush sinks whose backing buffer [`Versioned::changed_frame`] equals `frame_id`.
    ///
    /// Temporarily removes the boxed [`OutputProvider`] from `self` so sinks can be mutated without
    /// violating borrow rules.
    pub fn flush_dirty_output_sinks(
        &mut self,
        frame_id: FrameId,
        buffers: &RuntimeBufferStore,
    ) -> Result<(), OutputFlushError> {
        let Some(mut boxed) = self.output_provider.take() else {
            return Ok(());
        };
        let result =
            flush_registered_sinks(boxed.as_mut(), frame_id, buffers, &mut self.output_sinks);
        self.output_provider = Some(boxed);
        result
    }

    fn close_output_sinks(&mut self) {
        let Some(provider) = self.output_provider.as_deref() else {
            return;
        };

        for sink in self.output_sinks.values_mut() {
            if let Some(handle) = sink.channel_handle.take() {
                if let Err(error) = provider.close(handle) {
                    log::warn!(
                        "RuntimeServices: failed to close output handle {handle:?}: {error}"
                    );
                }
            }
        }
    }

    fn close_output_sink(&self, sink: &mut OutputSinkBinding) {
        let Some(provider) = self.output_provider.as_deref() else {
            return;
        };
        if let Some(handle) = sink.channel_handle.take() {
            if let Err(error) = provider.close(handle) {
                log::warn!("RuntimeServices: failed to close output handle {handle:?}: {error}");
            }
        }
    }
}

impl Drop for RuntimeServices {
    fn drop(&mut self) {
        self.close_output_sinks();
    }
}

fn pin_from_output_config(config: &OutputDef) -> u32 {
    config.pin()
}

fn display_options_from_output_config(cfg: &OutputDef) -> Option<OutputDriverOptions> {
    cfg.options().map(driver_options_from_cfg)
}

fn driver_options_from_cfg(cfg: &OutputDriverOptionsConfig) -> OutputDriverOptions {
    OutputDriverOptions {
        lum_power: *cfg.lum_power.value(),
        white_point: *cfg.white_point.value(),
        brightness: (*cfg.brightness.value()).clamp(0.0, 1.0),
        interpolation_enabled: *cfg.interpolation_enabled.value(),
        dithering_enabled: *cfg.dithering_enabled.value(),
        lut_enabled: *cfg.lut_enabled.value(),
    }
}

fn flush_registered_sinks(
    provider: &mut dyn OutputProvider,
    frame_id: FrameId,
    buffers: &RuntimeBufferStore,
    sinks: &mut HashMap<RuntimeBufferId, OutputSinkBinding>,
) -> Result<(), OutputFlushError> {
    for (buffer_id, sink) in sinks.iter_mut() {
        let Some(versioned) = buffers.get(*buffer_id) else {
            continue;
        };
        if versioned.changed_frame() != frame_id {
            continue;
        }

        let bytes = versioned.value().bytes.as_slice();
        if bytes.is_empty() {
            continue;
        }

        if bytes.len() % 6 != 0 {
            return Err(OutputFlushError::MisalignedPayload {
                buffer_id: *buffer_id,
            });
        }

        let u16_payload = decode_bytes_as_u16_le(bytes);
        let led_triplets = u16_payload.len() / 3;
        let byte_count = (led_triplets as u32).saturating_mul(3).max(3);

        ensure_channel_open(provider, sink, byte_count)?;

        let handle = sink.channel_handle.ok_or_else(|| {
            OutputFlushError::Provider(OutputError::InvalidConfig {
                reason: String::from("internal: missing output handle after open"),
            })
        })?;

        provider
            .write(handle, &u16_payload)
            .map_err(OutputFlushError::Provider)?;
        sink.last_byte_count = Some(byte_count.max(sink.last_byte_count.unwrap_or(3)));
    }
    Ok(())
}

fn decode_bytes_as_u16_le(bytes: &[u8]) -> Vec<u16> {
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for chunk in bytes.chunks_exact(2) {
        out.push(u16::from_le_bytes([chunk[0], chunk[1]]));
    }
    out
}

fn ensure_channel_open(
    provider: &dyn OutputProvider,
    sink: &mut OutputSinkBinding,
    byte_count: u32,
) -> Result<(), OutputFlushError> {
    if sink.channel_handle.is_some() {
        return Ok(());
    }

    let bc = sink.last_byte_count.unwrap_or(3).max(byte_count).max(3);
    let handle = provider
        .open(
            sink.pin,
            bc,
            OutputFormat::Ws2811,
            sink.display_options.clone(),
        )
        .map_err(OutputFlushError::Provider)?;
    sink.channel_handle = Some(handle);
    sink.last_byte_count = Some(bc);
    Ok(())
}

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;
    use alloc::rc::Rc;
    use alloc::vec;

    use lpc_model::{FrameId, TreePath, Versioned};
    use lpc_shared::error::OutputError;
    use lpc_shared::output::{
        MemoryOutputProvider, OutputChannelHandle, OutputDriverOptions, OutputFormat,
        OutputProvider,
    };
    use lpc_model::nodes::output::OutputDef;

    use super::RuntimeServices;
    use crate::runtime_buffer::{RuntimeBuffer, RuntimeBufferStore};

    #[test]
    fn runtime_services_drop_closes_open_output_channels() {
        let provider = Rc::new(MemoryOutputProvider::new());
        let mut services = RuntimeServices::new(TreePath::parse("/p.show").expect("tree path"));
        services.set_output_provider(Some(Box::new(SharedMemoryOutputProvider(Rc::clone(
            &provider,
        )))));

        let mut buffers = RuntimeBufferStore::new();
        let buffer_id = buffers.insert(Versioned::new(
            FrameId::new(1),
            RuntimeBuffer::output_channels_u16(6, vec![0, 1, 0, 2, 0, 3, 0, 4, 0, 5, 0, 6]),
        ));
        services.register_output_sink(buffer_id, &OutputDef::new(4));

        services
            .flush_dirty_output_sinks(FrameId::new(1), &buffers)
            .expect("flush opens output channel");
        assert!(provider.is_pin_open(4));

        drop(services);

        assert!(
            !provider.is_pin_open(4),
            "dropping runtime services should release output pins"
        );
    }

    struct SharedMemoryOutputProvider(Rc<MemoryOutputProvider>);

    impl OutputProvider for SharedMemoryOutputProvider {
        fn open(
            &self,
            pin: u32,
            byte_count: u32,
            format: OutputFormat,
            options: Option<OutputDriverOptions>,
        ) -> Result<OutputChannelHandle, OutputError> {
            self.0.open(pin, byte_count, format, options)
        }

        fn write(&self, handle: OutputChannelHandle, data: &[u16]) -> Result<(), OutputError> {
            self.0.write(handle, data)
        }

        fn close(&self, handle: OutputChannelHandle) -> Result<(), OutputError> {
            self.0.close(handle)
        }
    }
}
