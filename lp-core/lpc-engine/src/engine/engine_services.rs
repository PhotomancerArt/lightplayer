//! Narrow IO/service surface owned by [`super::Engine`].
//!
//! Carries project identity, optional [`OutputProvider`] plumbing, and registered
//! output sinks (fixture-pushed [`crate::resource::RuntimeBuffer`] → flush).

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use hashbrown::HashMap;
use lpc_hardware::OutputError;
use lpc_hardware::{
    ButtonConfig, ButtonInput, HardwareEndpointError, HardwareSystem, RadioConfig, RadioDevice,
};
use lpc_model::nodes::output::{OutputDef, OutputDriverOptionsConfig};
use lpc_model::{HwEndpointSpec, Revision, TreePath};
use lpc_shared::output::{OutputChannelHandle, OutputDriverOptions, OutputFormat, OutputProvider};
use lpc_shared::time::TimeProvider;

use crate::resource::{RuntimeBufferId, RuntimeBufferStore};

/// Per-sink channel state for [`EngineServices`] output flushing.
#[derive(Debug)]
struct OutputSinkBinding {
    endpoint: HwEndpointSpec,
    display_options: Option<OutputDriverOptions>,
    channel_handle: Option<OutputChannelHandle>,
    last_byte_count: Option<u32>,
    /// Hardware generation observed when this sink's last open attempt failed,
    /// or `None` while the sink has an open channel or a retry is due.
    ///
    /// A sink whose endpoint does not exist on the board — a project authored
    /// for four strips loaded onto a one-strip board, say — can never open, and
    /// re-attempting costs a full enumeration of every endpoint the drivers
    /// offer. Parking on the generation means such a sink is asked once per
    /// *hardware change* rather than once per frame, while a pin freed by
    /// another node still lights it on the very next flush.
    parked_at_generation: Option<u64>,
}

/// Failure while flushing one registered output sink.
///
/// A flush attempts every sink, so this always names the endpoint that failed:
/// on a multi-channel board an unnamed error is unactionable, and the frame's
/// other channels were flushed regardless.
#[derive(Debug)]
pub enum OutputFlushError {
    MisalignedPayload {
        endpoint: HwEndpointSpec,
        buffer_id: RuntimeBufferId,
    },
    Provider {
        endpoint: HwEndpointSpec,
        error: OutputError,
    },
}

impl OutputFlushError {
    fn endpoint(&self) -> &HwEndpointSpec {
        match self {
            Self::MisalignedPayload { endpoint, .. } | Self::Provider { endpoint, .. } => endpoint,
        }
    }
}

impl fmt::Display for OutputFlushError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MisalignedPayload {
                endpoint,
                buffer_id,
            } => write!(
                f,
                "output {endpoint} (buffer {buffer_id:?}): payload must be whole u16 RGB triplets (multiple of 6 bytes)",
            ),
            Self::Provider { endpoint, error } => write!(f, "output {endpoint}: {error}"),
        }
    }
}

impl core::error::Error for OutputFlushError {}

/// Project-level IO services and identity owned by the engine.
pub struct EngineServices {
    /// Tree path identifying the project/show root (authored layout anchor).
    project_root: TreePath,
    output_provider: Option<Box<dyn OutputProvider>>,
    time_provider: Option<Rc<dyn TimeProvider>>,
    button_service: Option<Rc<dyn ButtonService>>,
    radio_service: Option<Rc<dyn RadioService>>,
    /// Fixture-written buffers paired with authored output endpoint configuration.
    output_sinks: HashMap<RuntimeBufferId, OutputSinkBinding>,
}

/// Hardware button access used by runtime input nodes.
pub trait ButtonService {
    fn open_button_by_spec(
        &self,
        spec: &HwEndpointSpec,
        config: ButtonConfig,
    ) -> Result<Box<dyn ButtonInput>, HardwareEndpointError>;
}

impl ButtonService for HardwareSystem {
    fn open_button_by_spec(
        &self,
        spec: &HwEndpointSpec,
        config: ButtonConfig,
    ) -> Result<Box<dyn ButtonInput>, HardwareEndpointError> {
        HardwareSystem::open_button_by_spec(self, spec, config)
    }
}

/// Hardware radio access used by runtime input/output bridge nodes.
pub trait RadioService {
    fn open_radio_by_spec(
        &self,
        spec: &HwEndpointSpec,
        config: RadioConfig,
    ) -> Result<Box<dyn RadioDevice>, HardwareEndpointError>;
}

impl RadioService for HardwareSystem {
    fn open_radio_by_spec(
        &self,
        spec: &HwEndpointSpec,
        config: RadioConfig,
    ) -> Result<Box<dyn RadioDevice>, HardwareEndpointError> {
        HardwareSystem::open_radio_by_spec(self, spec, config)
    }
}

impl EngineServices {
    pub fn new(project_root: TreePath) -> Self {
        Self {
            project_root,
            output_provider: None,
            time_provider: None,
            button_service: None,
            radio_service: None,
            output_sinks: HashMap::new(),
        }
    }

    pub fn project_root(&self) -> &TreePath {
        &self.project_root
    }

    /// Replace the optional [`OutputProvider`] used when flushing sinks after each tick.
    pub fn set_output_provider(&mut self, provider: Option<Box<dyn OutputProvider>>) {
        self.close_output_sinks();
        // A different provider is a different world: what the old one refused
        // says nothing about what this one will do, so no sink stays parked
        // across the swap.
        for sink in self.output_sinks.values_mut() {
            sink.parked_at_generation = None;
        }
        self.output_provider = provider;
    }

    pub fn set_time_provider(&mut self, provider: Option<Rc<dyn TimeProvider>>) {
        self.time_provider = provider;
    }

    pub fn time_provider(&self) -> Option<Rc<dyn TimeProvider>> {
        self.time_provider.clone()
    }

    pub fn set_button_service(&mut self, service: Option<Rc<dyn ButtonService>>) {
        self.button_service = service;
    }

    pub fn button_service(&self) -> Option<Rc<dyn ButtonService>> {
        self.button_service.clone()
    }

    pub fn set_radio_service(&mut self, service: Option<Rc<dyn RadioService>>) {
        self.radio_service = service;
    }

    pub fn radio_service(&self) -> Option<Rc<dyn RadioService>> {
        self.radio_service.clone()
    }

    /// Register an output sink: fixture pushes u16 RGB channel bytes into `buffer_id`; flush writes
    /// them through [`OutputProvider`] for `config`'s hardware endpoint.
    ///
    /// Insert the backing [`crate::resource::RuntimeBuffer`] with
    /// [`WithRevision::new`](lpc_model::WithRevision::new)([`Revision::default`], …)
    /// so untouched sinks do not match the post-tick revision until the fixture mutates them.
    pub fn register_output_sink(&mut self, buffer_id: RuntimeBufferId, config: &OutputDef) {
        let endpoint = endpoint_from_output_config(config);
        warn_if_multi_channel(config, &endpoint);
        let display_options = display_options_from_output_config(config);
        if let Some(mut existing) = self.output_sinks.remove(&buffer_id) {
            self.close_output_sink(&mut existing);
        }
        self.output_sinks.insert(
            buffer_id,
            OutputSinkBinding {
                endpoint,
                display_options,
                channel_handle: None,
                last_byte_count: None,
                parked_at_generation: None,
            },
        );
    }

    /// Re-read an output's authored configuration, reopening the channel only
    /// if it actually changed.
    ///
    /// Called for every output on every tick, so the unchanged path — which is
    /// nearly every call — must not allocate. The comparison borrows the
    /// authored endpoint; only a genuine change pays for a copy of it.
    pub fn update_output_sink_config(&mut self, buffer_id: RuntimeBufferId, config: &OutputDef) {
        let display_options = display_options_from_output_config(config);
        let Some(existing) = self.output_sinks.get(&buffer_id) else {
            self.register_output_sink(buffer_id, config);
            return;
        };
        let endpoint_unchanged = match config.primary_endpoint() {
            Some(endpoint) => existing.endpoint == *endpoint,
            // Only an output with no authored channels reaches this arm, so
            // minting the unset spec here never touches the hot path.
            None => existing.endpoint == HwEndpointSpec::default(),
        };
        if endpoint_unchanged && output_options_eq(&existing.display_options, &display_options) {
            return;
        }

        let mut existing = self
            .output_sinks
            .remove(&buffer_id)
            .expect("output sink existed above");
        self.close_output_sink(&mut existing);
        existing.endpoint = endpoint_from_output_config(config);
        warn_if_multi_channel(config, &existing.endpoint);
        existing.display_options = display_options;
        existing.last_byte_count = None;
        // A re-authored endpoint is a fresh question for the hardware, so the
        // sink stops waiting on a generation change it no longer cares about.
        existing.parked_at_generation = None;
        self.output_sinks.insert(buffer_id, existing);
    }

    pub fn unregister_output_sink(&mut self, buffer_id: RuntimeBufferId) {
        if let Some(mut existing) = self.output_sinks.remove(&buffer_id) {
            self.close_output_sink(&mut existing);
        }
    }

    /// Flush sinks whose backing buffer [`WithRevision::revision`] equals `revision`.
    ///
    /// Temporarily removes the boxed [`OutputProvider`] from `self` so sinks can be mutated without
    /// violating borrow rules.
    pub fn flush_dirty_output_sinks(
        &mut self,
        revision: Revision,
        buffers: &RuntimeBufferStore,
    ) -> Result<(), OutputFlushError> {
        let Some(mut boxed) = self.output_provider.take() else {
            return Ok(());
        };
        let result =
            flush_registered_sinks(boxed.as_mut(), revision, buffers, &mut self.output_sinks);
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
                    log::warn!("EngineServices: failed to close output handle {handle:?}: {error}");
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
                log::warn!("EngineServices: failed to close output handle {handle:?}: {error}");
            }
        }
    }
}

impl Drop for EngineServices {
    fn drop(&mut self) {
        self.close_output_sinks();
    }
}

/// The one wire this output drives: its lowest-keyed channel's endpoint.
///
/// An output with no authored channels falls back to the model's unset spec,
/// which no driver resolves — the sink parks instead of silently lighting a
/// default pin. Per-wire fan-out (P2) replaces this whole accessor.
fn endpoint_from_output_config(config: &OutputDef) -> HwEndpointSpec {
    config.primary_endpoint().cloned().unwrap_or_default()
}

/// The engine still drives exactly one wire per output. Authoring more
/// channels is expressible in the model now but not yet honored, so say so
/// when the configuration is read, not once per frame. P2 removes this.
fn warn_if_multi_channel(config: &OutputDef, driven: &HwEndpointSpec) {
    let channels = config.channel_count();
    if channels > 1 {
        log::warn!(
            "EngineServices: output authors {channels} channels; multi-channel output not yet wired, driving channel 0 ({driven}) only"
        );
    }
}

fn display_options_from_output_config(cfg: &OutputDef) -> Option<OutputDriverOptions> {
    cfg.options().map(driver_options_from_cfg)
}

fn driver_options_from_cfg(cfg: &OutputDriverOptionsConfig) -> OutputDriverOptions {
    OutputDriverOptions {
        white_point: *cfg.white_point.value(),
        interpolation_enabled: *cfg.interpolation_enabled.value(),
        dithering_enabled: *cfg.dithering_enabled.value(),
        lut_enabled: *cfg.lut_enabled.value(),
    }
}

fn output_options_eq(
    left: &Option<OutputDriverOptions>,
    right: &Option<OutputDriverOptions>,
) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => {
            left.white_point == right.white_point
                && left.interpolation_enabled == right.interpolation_enabled
                && left.dithering_enabled == right.dithering_enabled
                && left.lut_enabled == right.lut_enabled
        }
        _ => false,
    }
}

/// Flush every dirty sink, then report the first failure.
///
/// Sinks are independent hardware channels and this map has no meaningful
/// order, so a sink that fails to open or write must not cost the frame its
/// other channels: returning early here made one unavailable endpoint look
/// like a whole-project blackout, and which channels survived depended on hash
/// order. Every failure is logged with its endpoint; the first is returned so
/// the tick still reports an error.
fn flush_registered_sinks(
    provider: &mut dyn OutputProvider,
    revision: Revision,
    buffers: &RuntimeBufferStore,
    sinks: &mut HashMap<RuntimeBufferId, OutputSinkBinding>,
) -> Result<(), OutputFlushError> {
    let mut first_error: Option<OutputFlushError> = None;
    let mut failed = 0usize;

    // Read once per flush, not once per sink: the answer is the same for every
    // sink in the frame, and asking is a virtual call through the provider.
    //
    // Read it *before* any open in this flush, too. A successful open bumps the
    // generation, so a value sampled afterwards would park a sink against a
    // number that already reflects this frame's own claims — and a release that
    // raced the open would be missed rather than retried.
    let generation = provider.hardware_generation();

    for (buffer_id, sink) in sinks.iter_mut() {
        let Some(versioned) = buffers.get(*buffer_id) else {
            continue;
        };
        if versioned.changed_at() != revision {
            continue;
        }

        let bytes = versioned.value().bytes.as_slice();
        if bytes.is_empty() {
            continue;
        }

        if sink.parked_at_generation == Some(generation) {
            continue;
        }

        if let Err(error) = flush_one_sink(provider, sink, *buffer_id, bytes, generation) {
            failed += 1;
            log::warn!("EngineServices: {error}");
            first_error.get_or_insert(error);
        }
    }

    match first_error {
        None => Ok(()),
        Some(error) => {
            if failed > 1 {
                log::warn!(
                    "EngineServices: {failed} output sinks failed this frame; reporting {}",
                    error.endpoint()
                );
            }
            Err(error)
        }
    }
}

fn flush_one_sink(
    provider: &mut dyn OutputProvider,
    sink: &mut OutputSinkBinding,
    buffer_id: RuntimeBufferId,
    bytes: &[u8],
    generation: u64,
) -> Result<(), OutputFlushError> {
    if bytes.len() % 6 != 0 {
        return Err(OutputFlushError::MisalignedPayload {
            endpoint: sink.endpoint.clone(),
            buffer_id,
        });
    }

    let u16_payload = decode_bytes_as_u16_le(bytes);
    let led_triplets = u16_payload.len() / 3;
    let byte_count = (led_triplets as u32).saturating_mul(3).max(3);

    ensure_channel_open(provider, sink, byte_count, generation).map_err(|error| {
        OutputFlushError::Provider {
            endpoint: sink.endpoint.clone(),
            error,
        }
    })?;

    let handle = sink
        .channel_handle
        .ok_or_else(|| OutputFlushError::Provider {
            endpoint: sink.endpoint.clone(),
            error: OutputError::InvalidConfig {
                reason: String::from("internal: missing output handle after open"),
            },
        })?;

    provider
        .write(handle, &u16_payload)
        .map_err(|error| OutputFlushError::Provider {
            endpoint: sink.endpoint.clone(),
            error,
        })?;
    sink.last_byte_count = Some(byte_count.max(sink.last_byte_count.unwrap_or(3)));
    Ok(())
}

fn decode_bytes_as_u16_le(bytes: &[u8]) -> Vec<u16> {
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for chunk in bytes.chunks_exact(2) {
        out.push(u16::from_le_bytes([chunk[0], chunk[1]]));
    }
    out
}

/// Open the sink's channel if it has none, parking it on failure.
///
/// `generation` is the provider's hardware generation sampled before any open
/// this flush; a sink that fails records it and is skipped until it changes.
fn ensure_channel_open(
    provider: &dyn OutputProvider,
    sink: &mut OutputSinkBinding,
    byte_count: u32,
    generation: u64,
) -> Result<(), OutputError> {
    if sink.channel_handle.is_some() {
        return Ok(());
    }

    let bc = sink.last_byte_count.unwrap_or(3).max(byte_count).max(3);
    let handle = match provider.open(
        &sink.endpoint,
        bc,
        OutputFormat::Ws2811,
        sink.display_options.clone(),
    ) {
        Ok(handle) => handle,
        Err(error) => {
            sink.parked_at_generation = Some(generation);
            return Err(error);
        }
    };

    if sink.parked_at_generation.take().is_some() {
        log::info!(
            "EngineServices: output {} recovered and is writing again",
            sink.endpoint
        );
    }
    sink.channel_handle = Some(handle);
    sink.last_byte_count = Some(bc);
    Ok(())
}

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;
    use alloc::rc::Rc;
    use alloc::string::ToString;
    use alloc::vec;

    use lpc_hardware::OutputError;
    use lpc_model::nodes::output::{OutputDef, OutputDriverOptionsConfig};
    use lpc_model::{HwEndpointSpec, OptionSlot, Revision, TreePath, WithRevision};
    use lpc_shared::output::{
        MemoryOutputProvider, OutputChannelHandle, OutputDriverOptions, OutputFormat,
        OutputProvider,
    };

    use super::EngineServices;
    use crate::resource::{RuntimeBuffer, RuntimeBufferId, RuntimeBufferStore};

    #[test]
    fn engine_services_drop_closes_open_output_channels() {
        let provider = Rc::new(MemoryOutputProvider::new());
        let mut services = EngineServices::new(TreePath::parse("/p.show").expect("tree path"));
        services.set_output_provider(Some(Box::new(SharedMemoryOutputProvider(Rc::clone(
            &provider,
        )))));

        let mut buffers = RuntimeBufferStore::new();
        let buffer_id = buffers.insert(WithRevision::new(
            Revision::new(1),
            RuntimeBuffer::output_channels_u16(6, vec![0, 1, 0, 2, 0, 3, 0, 4, 0, 5, 0, 6]),
        ));
        let endpoint = endpoint("ws281x:local:D10");
        services.register_output_sink(buffer_id, &OutputDef::new(endpoint.clone()));

        services
            .flush_dirty_output_sinks(Revision::new(1), &buffers)
            .expect("flush opens output channel");
        assert!(provider.is_endpoint_open(&endpoint));
        assert!(provider.is_pin_open(18));

        drop(services);

        assert!(
            !provider.is_endpoint_open(&endpoint),
            "dropping runtime services should release output endpoints"
        );
        assert!(!provider.is_pin_open(18));
    }

    #[test]
    fn output_sink_config_update_reopens_channel_on_next_flush() {
        let provider = Rc::new(MemoryOutputProvider::new());
        let mut services = EngineServices::new(TreePath::parse("/p.show").expect("tree path"));
        services.set_output_provider(Some(Box::new(SharedMemoryOutputProvider(Rc::clone(
            &provider,
        )))));

        let mut buffers = RuntimeBufferStore::new();
        let buffer_id = buffers.insert(WithRevision::new(
            Revision::new(1),
            RuntimeBuffer::output_channels_u16(6, vec![0, 1, 0, 2, 0, 3, 0, 4, 0, 5, 0, 6]),
        ));
        let endpoint = endpoint("ws281x:local:D10");
        services.register_output_sink(buffer_id, &OutputDef::new(endpoint.clone()));
        services
            .flush_dirty_output_sinks(Revision::new(1), &buffers)
            .expect("initial flush");
        let first_handle = provider
            .get_handle_for_endpoint(&endpoint)
            .expect("first handle");

        let mut next = OutputDef::new(endpoint.clone());
        next.options = OptionSlot::some(OutputDriverOptionsConfig {
            white_point: lpc_model::ValueSlot::new([0.5, 1.0, 1.0]),
            ..OutputDriverOptionsConfig::default()
        });
        services.update_output_sink_config(buffer_id, &next);
        services
            .flush_dirty_output_sinks(Revision::new(1), &buffers)
            .expect("flush after config update");

        let second_handle = provider
            .get_handle_for_endpoint(&endpoint)
            .expect("second handle");
        assert_ne!(first_handle, second_handle);
        assert_eq!(provider.open_channel_count(), 1);
    }

    /// Every tick re-reads every output's authored config, so the unchanged
    /// case — which is nearly every one of them — must leave the channel
    /// exactly as it found it. Reopening here would drop a live channel and
    /// re-claim the pin sixty times a second.
    #[test]
    fn re_applying_an_unchanged_config_leaves_the_channel_alone() {
        let provider = Rc::new(CountingOutputProvider::new(MemoryOutputProvider::new()));
        let mut services = EngineServices::new(TreePath::parse("/p.show").expect("tree path"));
        services.set_output_provider(Some(Box::new(SharedCountingProvider(Rc::clone(&provider)))));

        let mut buffers = RuntimeBufferStore::new();
        let buffer_id = output_buffer(&mut buffers, Revision::new(1));
        let config = OutputDef::new(endpoint("ws281x:local:D10"));
        services.register_output_sink(buffer_id, &config);
        services
            .flush_dirty_output_sinks(Revision::new(1), &buffers)
            .expect("initial flush opens the channel");
        let handle = provider
            .inner()
            .get_handle_for_endpoint(&endpoint("ws281x:local:D10"))
            .expect("channel handle");
        let generation = provider.hardware_generation();

        for _ in 0..32 {
            services.update_output_sink_config(buffer_id, &config);
        }

        assert_eq!(
            provider.open_calls(),
            1,
            "no reopen for an unchanged config"
        );
        assert_eq!(
            provider
                .inner()
                .get_handle_for_endpoint(&endpoint("ws281x:local:D10")),
            Some(handle)
        );
        assert_eq!(
            provider.hardware_generation(),
            generation,
            "an unchanged config must not churn the hardware claim"
        );
    }

    #[test]
    fn unregister_output_sink_closes_open_channel() {
        let provider = Rc::new(MemoryOutputProvider::new());
        let mut services = EngineServices::new(TreePath::parse("/p.show").expect("tree path"));
        services.set_output_provider(Some(Box::new(SharedMemoryOutputProvider(Rc::clone(
            &provider,
        )))));

        let mut buffers = RuntimeBufferStore::new();
        let buffer_id = output_buffer(&mut buffers, Revision::new(1));
        let endpoint = endpoint("ws281x:local:D10");
        services.register_output_sink(buffer_id, &OutputDef::new(endpoint.clone()));
        services
            .flush_dirty_output_sinks(Revision::new(1), &buffers)
            .expect("initial flush");
        assert!(provider.is_endpoint_open(&endpoint));

        services.unregister_output_sink(buffer_id);

        assert!(!provider.is_endpoint_open(&endpoint));
    }

    #[test]
    fn engine_services_duplicate_output_pin_reports_hardware_conflict() {
        let provider = Rc::new(MemoryOutputProvider::new());
        let mut services = EngineServices::new(TreePath::parse("/p.show").expect("tree path"));
        services.set_output_provider(Some(Box::new(SharedMemoryOutputProvider(Rc::clone(
            &provider,
        )))));

        let mut buffers = RuntimeBufferStore::new();
        let first = output_buffer(&mut buffers, Revision::new(1));
        let second = output_buffer(&mut buffers, Revision::new(1));
        let endpoint = endpoint("ws281x:local:D10");
        services.register_output_sink(first, &OutputDef::new(endpoint.clone()));
        services.register_output_sink(second, &OutputDef::new(endpoint.clone()));

        let err = services
            .flush_dirty_output_sinks(Revision::new(1), &buffers)
            .expect_err("duplicate endpoint should fail");

        assert!(matches!(
            err,
            super::OutputFlushError::Provider {
                error: OutputError::Hardware { .. },
                ..
            }
        ));
        assert_eq!(provider.open_channel_count(), 1);
        assert!(provider.is_endpoint_open(&endpoint));
        assert!(provider.is_pin_open(18));
    }

    #[test]
    fn engine_services_different_output_pins_contend_for_single_rmt() {
        let provider = Rc::new(MemoryOutputProvider::new());
        let mut services = EngineServices::new(TreePath::parse("/p.show").expect("tree path"));
        services.set_output_provider(Some(Box::new(SharedMemoryOutputProvider(Rc::clone(
            &provider,
        )))));

        let mut buffers = RuntimeBufferStore::new();
        let first = output_buffer(&mut buffers, Revision::new(1));
        let second = output_buffer(&mut buffers, Revision::new(1));
        let first_endpoint = endpoint("ws281x:local:D10");
        let second_endpoint = endpoint("ws281x:local:GPIO19");
        services.register_output_sink(first, &OutputDef::new(first_endpoint.clone()));
        services.register_output_sink(second, &OutputDef::new(second_endpoint.clone()));

        let err = services
            .flush_dirty_output_sinks(Revision::new(1), &buffers)
            .expect_err("single RMT resource should allow only one output");

        assert!(err.to_string().contains("/rmt/ws281x0"));
        assert_eq!(provider.open_channel_count(), 1);
        assert_ne!(
            provider.is_endpoint_open(&first_endpoint),
            provider.is_endpoint_open(&second_endpoint)
        );
        assert_ne!(provider.is_pin_open(18), provider.is_pin_open(19));
    }

    /// One unopenable sink must not cost the frame its other channels.
    ///
    /// `output_sinks` is a `HashMap`, so before flushing was made per-sink the
    /// surviving channels depended on where the bad one landed in hash order —
    /// a four-strip project could go dark except for one strip, and a
    /// different strip on the next run.
    #[test]
    fn a_failing_output_sink_does_not_suppress_the_others() {
        let provider = Rc::new(MemoryOutputProvider::with_hardware_manifest(
            lpc_hardware::default_esp32s3_hardware_manifest(),
        ));
        let mut services = EngineServices::new(TreePath::parse("/p.show").expect("tree path"));
        services.set_output_provider(Some(Box::new(SharedMemoryOutputProvider(Rc::clone(
            &provider,
        )))));

        let mut buffers = RuntimeBufferStore::new();
        // The board has no such pin, so this sink can never open — unlike a
        // contention failure, it stays failed however the map is ordered.
        let unknown = endpoint("ws281x:local:NOT-A-PIN");
        let good = [
            endpoint("ws281x:local:D10"),
            endpoint("ws281x:local:D9"),
            endpoint("ws281x:local:D8"),
        ];
        let bad_buffer = output_buffer(&mut buffers, Revision::new(1));
        services.register_output_sink(bad_buffer, &OutputDef::new(unknown.clone()));
        for spec in &good {
            let buffer_id = output_buffer(&mut buffers, Revision::new(1));
            services.register_output_sink(buffer_id, &OutputDef::new(spec.clone()));
        }

        let err = services
            .flush_dirty_output_sinks(Revision::new(1), &buffers)
            .expect_err("the unknown endpoint must still be reported");

        assert!(
            err.to_string().contains("ws281x:local:NOT-A-PIN"),
            "the flush error must name the sink that failed, got: {err}"
        );
        for spec in &good {
            assert!(
                provider.is_endpoint_open(spec),
                "{spec} was skipped because another sink failed"
            );
        }
        assert_eq!(provider.open_channel_count(), good.len());
    }

    /// An endpoint the board does not have can never open, so the flush seam
    /// must ask about it once and then wait — not once per frame.
    ///
    /// Every attempt costs a full enumeration of every endpoint the drivers
    /// offer, with a status lookup and a formatted spec per endpoint. On a
    /// four-strip project loaded onto a one-strip board that was 45.8% of all
    /// cycles, spent entirely on learning the same "no" 60 times a second.
    #[test]
    fn a_sink_that_cannot_open_is_asked_once_not_once_per_frame() {
        let provider = Rc::new(CountingOutputProvider::new(
            MemoryOutputProvider::with_hardware_manifest(
                lpc_hardware::default_esp32s3_hardware_manifest(),
            ),
        ));
        let mut services = EngineServices::new(TreePath::parse("/p.show").expect("tree path"));
        services.set_output_provider(Some(Box::new(SharedCountingProvider(Rc::clone(&provider)))));

        let mut buffers = RuntimeBufferStore::new();
        let buffer_id = output_buffer(&mut buffers, Revision::new(1));
        services.register_output_sink(
            buffer_id,
            &OutputDef::new(endpoint("ws281x:local:NOT-A-PIN")),
        );

        services
            .flush_dirty_output_sinks(Revision::new(1), &buffers)
            .expect_err("the first frame reports the failure");
        assert_eq!(provider.open_calls(), 1);

        for _ in 0..16 {
            services
                .flush_dirty_output_sinks(Revision::new(1), &buffers)
                .expect("a parked sink does not keep failing the frame");
        }

        assert_eq!(
            provider.open_calls(),
            1,
            "a parked sink must not re-attempt while the hardware is unchanged"
        );
    }

    /// Parking must not cost recovery: when the pin a sink was waiting for is
    /// freed, the very next flush picks it up, with no config change and
    /// nothing to poke.
    #[test]
    fn freeing_the_contended_hardware_lights_the_waiting_sink() {
        let provider = Rc::new(CountingOutputProvider::new(MemoryOutputProvider::new()));
        let mut services = EngineServices::new(TreePath::parse("/p.show").expect("tree path"));
        services.set_output_provider(Some(Box::new(SharedCountingProvider(Rc::clone(&provider)))));

        let mut buffers = RuntimeBufferStore::new();
        let holder = output_buffer(&mut buffers, Revision::new(1));
        let waiter = output_buffer(&mut buffers, Revision::new(1));
        let held = endpoint("ws281x:local:D10");
        let waiting = endpoint("ws281x:local:GPIO19");
        services.register_output_sink(holder, &OutputDef::new(held.clone()));
        services.register_output_sink(waiter, &OutputDef::new(waiting.clone()));

        // The board has one RMT resource, so exactly one of these opens; drive
        // the flush until both have had their attempt.
        let _ = services.flush_dirty_output_sinks(Revision::new(1), &buffers);
        let (open_sink, parked_sink, parked_endpoint) = if provider.inner().is_endpoint_open(&held)
        {
            (holder, waiter, waiting.clone())
        } else {
            (waiter, holder, held.clone())
        };
        let _ = open_sink;
        assert!(!provider.inner().is_endpoint_open(&parked_endpoint));

        // Releasing the winner's claim is a hardware change, so the parked sink
        // gets its retry.
        services.unregister_output_sink(open_sink);
        services
            .flush_dirty_output_sinks(Revision::new(1), &buffers)
            .expect("the freed resource lets the waiting sink open");

        assert!(
            provider.inner().is_endpoint_open(&parked_endpoint),
            "a sink parked on contention must open once the contention clears"
        );
        assert!(services.output_sinks.contains_key(&parked_sink));
    }

    /// Re-authoring the endpoint is a new question, so it must be asked at once
    /// rather than waiting on a hardware change that may never come.
    #[test]
    fn re_authoring_a_parked_sink_retries_without_a_hardware_change() {
        let provider = Rc::new(CountingOutputProvider::new(
            MemoryOutputProvider::with_hardware_manifest(
                lpc_hardware::default_esp32s3_hardware_manifest(),
            ),
        ));
        let mut services = EngineServices::new(TreePath::parse("/p.show").expect("tree path"));
        services.set_output_provider(Some(Box::new(SharedCountingProvider(Rc::clone(&provider)))));

        let mut buffers = RuntimeBufferStore::new();
        let buffer_id = output_buffer(&mut buffers, Revision::new(1));
        services.register_output_sink(
            buffer_id,
            &OutputDef::new(endpoint("ws281x:local:NOT-A-PIN")),
        );
        let _ = services.flush_dirty_output_sinks(Revision::new(1), &buffers);
        let generation_before = provider.hardware_generation();

        let good = endpoint("ws281x:local:D10");
        services.update_output_sink_config(buffer_id, &OutputDef::new(good.clone()));
        services
            .flush_dirty_output_sinks(Revision::new(1), &buffers)
            .expect("the re-authored endpoint opens");

        assert_eq!(
            provider.hardware_generation(),
            generation_before + 1,
            "only the successful claim should have moved the generation"
        );
        assert!(provider.inner().is_endpoint_open(&good));
    }

    /// A new provider is a new world; nothing the old one refused should keep
    /// a sink parked against it.
    #[test]
    fn swapping_the_provider_unparks_sinks() {
        let strict = Rc::new(CountingOutputProvider::new(MemoryOutputProvider::new()));
        let mut services = EngineServices::new(TreePath::parse("/p.show").expect("tree path"));
        services.set_output_provider(Some(Box::new(SharedCountingProvider(Rc::clone(&strict)))));

        let mut buffers = RuntimeBufferStore::new();
        let buffer_id = output_buffer(&mut buffers, Revision::new(1));
        // The strict single-RMT board has no such pin; the permissive provider
        // that replaces it accepts any endpoint.
        let demo = endpoint("ws281x:local:D4");
        services.register_output_sink(buffer_id, &OutputDef::new(demo.clone()));
        let _ = services.flush_dirty_output_sinks(Revision::new(1), &buffers);
        assert_eq!(strict.open_calls(), 1);

        let permissive = Rc::new(CountingOutputProvider::new(
            MemoryOutputProvider::new_permissive(),
        ));
        services.set_output_provider(Some(Box::new(SharedCountingProvider(Rc::clone(
            &permissive,
        )))));
        services
            .flush_dirty_output_sinks(Revision::new(1), &buffers)
            .expect("the replacement provider accepts the endpoint");

        assert_eq!(permissive.open_calls(), 1, "the swap must unpark the sink");
        assert!(permissive.inner().is_endpoint_open(&demo));
    }

    fn endpoint(spec: &'static str) -> HwEndpointSpec {
        HwEndpointSpec::from_static(spec)
    }

    fn output_buffer(store: &mut RuntimeBufferStore, revision: Revision) -> RuntimeBufferId {
        store.insert(WithRevision::new(
            revision,
            RuntimeBuffer::output_channels_u16(3, vec![0, 1, 0, 2, 0, 3]),
        ))
    }

    /// Counts `open` calls, which is how the parking tests tell "asked once"
    /// from "asked every frame".
    struct CountingOutputProvider {
        inner: MemoryOutputProvider,
        open_calls: core::cell::Cell<usize>,
    }

    impl CountingOutputProvider {
        fn new(inner: MemoryOutputProvider) -> Self {
            Self {
                inner,
                open_calls: core::cell::Cell::new(0),
            }
        }

        fn inner(&self) -> &MemoryOutputProvider {
            &self.inner
        }

        fn open_calls(&self) -> usize {
            self.open_calls.get()
        }

        fn hardware_generation(&self) -> u64 {
            self.inner.hardware_generation()
        }
    }

    struct SharedCountingProvider(Rc<CountingOutputProvider>);

    impl OutputProvider for SharedCountingProvider {
        fn open(
            &self,
            endpoint: &HwEndpointSpec,
            byte_count: u32,
            format: OutputFormat,
            options: Option<OutputDriverOptions>,
        ) -> Result<OutputChannelHandle, OutputError> {
            self.0.open_calls.set(self.0.open_calls.get() + 1);
            self.0.inner.open(endpoint, byte_count, format, options)
        }

        fn write(&self, handle: OutputChannelHandle, data: &[u16]) -> Result<(), OutputError> {
            self.0.inner.write(handle, data)
        }

        fn close(&self, handle: OutputChannelHandle) -> Result<(), OutputError> {
            self.0.inner.close(handle)
        }

        fn hardware_generation(&self) -> u64 {
            self.0.inner.hardware_generation()
        }
    }

    struct SharedMemoryOutputProvider(Rc<MemoryOutputProvider>);

    impl OutputProvider for SharedMemoryOutputProvider {
        fn open(
            &self,
            endpoint: &HwEndpointSpec,
            byte_count: u32,
            format: OutputFormat,
            options: Option<OutputDriverOptions>,
        ) -> Result<OutputChannelHandle, OutputError> {
            self.0.open(endpoint, byte_count, format, options)
        }

        fn write(&self, handle: OutputChannelHandle, data: &[u16]) -> Result<(), OutputError> {
            self.0.write(handle, data)
        }

        fn close(&self, handle: OutputChannelHandle) -> Result<(), OutputError> {
            self.0.close(handle)
        }

        fn hardware_generation(&self) -> u64 {
            self.0.hardware_generation()
        }
    }
}
