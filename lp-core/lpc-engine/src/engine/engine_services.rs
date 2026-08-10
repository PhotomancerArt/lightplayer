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
    WS281X_MAX_LEDS_PER_CHANNEL, ws281x_capped_byte_count,
};
use lpc_model::nodes::output::{OutputDef, OutputDriverOptionsConfig};
use lpc_model::{HwEndpointSpec, NodeId, Revision, TreePath};
use lpc_shared::output::{OutputChannelHandle, OutputDriverOptions, OutputFormat, OutputProvider};
use lpc_shared::time::TimeProvider;

use crate::resource::{RuntimeBufferId, RuntimeBufferStore};

/// Every wire one output node drives from its single control buffer.
///
/// The node renders one flat buffer and the wires take slices of it in
/// ascending channel-key order, so the set — not the wire — owns everything
/// authored per node: the display options (shared by every wire, Q3) and the
/// node identity every log and error carries.
#[derive(Debug)]
struct OutputSinkSet {
    node: NodeId,
    display_options: Option<OutputDriverOptions>,
    /// Wires in ascending channel-key order.
    ///
    /// Empty means "this output drives nothing": either it authors no
    /// channels, or its channels could not be sliced (see
    /// [`remainder_offender`]). An empty set is silent from then on — the
    /// refusal is logged once, when the configuration is read.
    wires: Vec<OutputWire>,
}

/// One authored channel of an output node: a slice of the node's control
/// buffer, pushed to one hardware endpoint.
///
/// The slice lives here rather than in the endpoint spec on purpose: an
/// endpoint names a wire and nothing else, so two wires are two distinct
/// specs, and where each wire's pixels come from is the engine's business.
#[derive(Debug)]
struct OutputWire {
    /// Authored channel key. Identifies the wire in logs and in the per-tick
    /// diff, which matches wires by key so an edit to one leaves the others —
    /// including their parking — untouched.
    channel: u32,
    endpoint: HwEndpointSpec,
    /// First sample of the node's buffer this wire carries.
    start_samples: u32,
    /// Samples this wire carries, or `None` for "whatever is left".
    ///
    /// Only the last wire may take the remainder, so a single-channel output
    /// with no authored count drives the whole buffer — exactly what every
    /// output did before channels existed.
    len_samples: Option<u32>,
    channel_handle: Option<OutputChannelHandle>,
    last_byte_count: Option<u32>,
    /// Hardware generation observed when this wire's last open attempt failed,
    /// or `None` while the wire has an open channel or a retry is due.
    ///
    /// A wire whose endpoint does not exist on the board — a project authored
    /// for four strips loaded onto a one-strip board, say — can never open, and
    /// re-attempting costs a full enumeration of every endpoint the drivers
    /// offer. Parking on the generation means such a wire is asked once per
    /// *hardware change* rather than once per frame, while a pin freed by
    /// another node still lights it on the very next flush.
    parked_at_generation: Option<u64>,
    /// Buffer sample count the last buffer-underrun warning was issued for.
    ///
    /// An overflowing slice keeps overflowing every frame, so the warning is
    /// rate-limited on the extent that caused it: loud once per shape, not
    /// sixty times a second (the same bargain the LED cap makes).
    truncated_at_samples: Option<u32>,
    /// Pre-cap sample count the last shared-cap warning was issued for.
    ///
    /// Distinct from `truncated_at_samples`: that one fires when the buffer
    /// holds fewer samples than authored, this one fires when the wire wants
    /// more samples than [`WS281X_MAX_LEDS_PER_CHANNEL`] ever grants, on host,
    /// emulator, or device alike. Rate-limited the same way — loud once per
    /// shape, not once per frame.
    capped_at_samples: Option<u32>,
}

impl OutputWire {
    fn new(planned: PlannedWire<'_>) -> Self {
        Self {
            channel: planned.channel,
            endpoint: planned.endpoint.clone(),
            start_samples: planned.start_samples,
            len_samples: planned.len_samples,
            channel_handle: None,
            last_byte_count: None,
            parked_at_generation: None,
            truncated_at_samples: None,
            capped_at_samples: None,
        }
    }

    fn matches(&self, planned: PlannedWire<'_>) -> bool {
        self.channel == planned.channel
            && self.endpoint == *planned.endpoint
            && self.start_samples == planned.start_samples
            && self.len_samples == planned.len_samples
    }
}

/// One wire as the authored `channels` map describes it, before any runtime
/// extent is known.
#[derive(Clone, Copy)]
struct PlannedWire<'a> {
    channel: u32,
    endpoint: &'a HwEndpointSpec,
    start_samples: u32,
    len_samples: Option<u32>,
}

/// Failure while flushing one wire of a registered output sink.
///
/// A flush attempts every wire of every sink, so this always names the node and
/// the channel that failed: on a multi-channel board an unnamed error is
/// unactionable, and the frame's other wires were flushed regardless.
#[derive(Debug)]
pub enum OutputFlushError {
    MisalignedPayload {
        node: NodeId,
        buffer_id: RuntimeBufferId,
    },
    Provider {
        node: NodeId,
        channel: u32,
        endpoint: HwEndpointSpec,
        error: OutputError,
    },
    /// The end-of-frame [`OutputProvider::flush`] barrier failed — a
    /// transmission begun by a `write` this frame did not complete. Carries no
    /// node: the barrier is frame-wide, and the provider's own log names the
    /// channel.
    Flush { error: OutputError },
}

impl fmt::Display for OutputFlushError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MisalignedPayload { node, buffer_id } => write!(
                f,
                "output node {node} (buffer {buffer_id:?}): payload must be whole u16 RGB triplets (multiple of 6 bytes)",
            ),
            Self::Provider {
                node,
                channel,
                endpoint,
                error,
            } => write!(
                f,
                "output node {node} channel {channel} {endpoint}: {error}"
            ),
            Self::Flush { error } => write!(f, "output flush barrier: {error}"),
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
    /// Fixture-written buffers paired with the wires their output node drives.
    output_sinks: HashMap<RuntimeBufferId, OutputSinkSet>,
    /// Scratch the flush decodes each node buffer into, once per frame.
    ///
    /// Kept across frames so the decode of a buffer several wires share costs
    /// no allocation at all in the steady state — it used to be a fresh `Vec`
    /// per sink per frame.
    flush_samples: Vec<u16>,
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
            flush_samples: Vec::new(),
        }
    }

    pub fn project_root(&self) -> &TreePath {
        &self.project_root
    }

    /// Replace the optional [`OutputProvider`] used when flushing sinks after each tick.
    pub fn set_output_provider(&mut self, provider: Option<Box<dyn OutputProvider>>) {
        self.close_output_sinks();
        // A different provider is a different world: what the old one refused
        // says nothing about what this one will do, so no wire stays parked
        // across the swap.
        for sink in self.output_sinks.values_mut() {
            for wire in &mut sink.wires {
                wire.parked_at_generation = None;
            }
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

    /// Register an output sink: fixture pushes u16 RGB channel bytes into `buffer_id`; flush slices
    /// them across `config`'s channels and writes each slice through [`OutputProvider`].
    ///
    /// Insert the backing [`crate::resource::RuntimeBuffer`] with
    /// [`WithRevision::new`](lpc_model::WithRevision::new)([`Revision::default`], …)
    /// so untouched sinks do not match the post-tick revision until the fixture mutates them.
    pub fn register_output_sink(
        &mut self,
        buffer_id: RuntimeBufferId,
        node: NodeId,
        config: &OutputDef,
    ) {
        if let Some(mut existing) = self.output_sinks.remove(&buffer_id) {
            self.close_output_sink(&mut existing);
        }
        let mut set = OutputSinkSet {
            node,
            display_options: display_options_from_output_config(config),
            wires: Vec::new(),
        };
        self.reconcile_wires(&mut set, config, false);
        self.output_sinks.insert(buffer_id, set);
    }

    /// Re-read an output's authored configuration, reopening only the wires
    /// that actually changed.
    ///
    /// Called for every output on every tick, so the unchanged path — which is
    /// nearly every call — must not allocate. The comparison walks the authored
    /// channels in key order against the wires already registered, borrowing
    /// each endpoint; only a genuine change pays for a copy of one.
    pub fn update_output_sink_config(
        &mut self,
        buffer_id: RuntimeBufferId,
        node: NodeId,
        config: &OutputDef,
    ) {
        let display_options = display_options_from_output_config(config);
        let Some(existing) = self.output_sinks.get(&buffer_id) else {
            self.register_output_sink(buffer_id, node, config);
            return;
        };
        // The display pipeline is built at open time, so a change to it
        // reopens every wire; the wires themselves are compared one by one.
        let options_unchanged = output_options_eq(&existing.display_options, &display_options);
        if options_unchanged && existing.node == node && wires_match_config(&existing.wires, config)
        {
            return;
        }

        let mut set = self
            .output_sinks
            .remove(&buffer_id)
            .expect("output sink existed above");
        set.node = node;
        set.display_options = display_options;
        self.reconcile_wires(&mut set, config, !options_unchanged);
        self.output_sinks.insert(buffer_id, set);
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
        let result = flush_registered_sinks(
            boxed.as_mut(),
            revision,
            buffers,
            &mut self.output_sinks,
            &mut self.flush_samples,
        );
        self.output_provider = Some(boxed);
        result
    }

    /// Rebuild a sink set's wires from its authored channels, keeping every
    /// wire the edit did not touch exactly as it was.
    ///
    /// Only the changed path runs this, so it may allocate. Matching by channel
    /// key is what keeps parking per wire: re-authoring channel 2 must not send
    /// channel 0 back to the hardware, and must not un-park channel 1 into
    /// another hopeless open attempt.
    fn reconcile_wires(&self, set: &mut OutputSinkSet, config: &OutputDef, force_reopen: bool) {
        let mut previous = core::mem::take(&mut set.wires);

        if let Some(offender) = remainder_offender(config) {
            log::warn!(
                "EngineServices: output node {} channel {offender} omits `count` but is not its \
                 last channel; only the highest-keyed channel may take the remainder. This output \
                 drives no wires until it is re-authored.",
                set.node
            );
            for wire in &mut previous {
                self.close_output_wire(wire);
            }
            return;
        }

        for (key, port) in config.ports.entries.iter() {
            match port.count() {
                Some(0) => {
                    log::warn!(
                        "EngineServices: output node {} port {key} ({}) authors a lamp count \
                         of 0; it drives nothing and is skipped",
                        set.node,
                        port.endpoint(),
                    );
                }
                // Authored-count validation, at registration rather than at the
                // first flush: an explicit count above the shared cap is wrong
                // the moment it is authored, whether or not a buffer has
                // arrived yet to prove it at runtime. `wire_slice` enforces the
                // actual grant every frame (it also covers the remainder
                // channel, whose size is buffer-driven and unknowable here);
                // this is the config-time half of the same warning.
                Some(count) if count > WS281X_MAX_LEDS_PER_CHANNEL as u32 => {
                    log::warn!(
                        "EngineServices: output node {} port {key} ({}) authors {count} \
                         lamps, above the shared {WS281X_MAX_LEDS_PER_CHANNEL}-lamp per-wire \
                         cap; it registers capped and drives only the first \
                         {WS281X_MAX_LEDS_PER_CHANNEL} lamps",
                        set.node,
                        port.endpoint(),
                    );
                }
                _ => {}
            }
        }

        let mut wires = Vec::with_capacity(config.port_count());
        for planned in planned_wires(config) {
            let existing = previous
                .iter()
                .position(|wire| wire.channel == planned.channel);
            match existing {
                Some(index) => {
                    let mut wire = previous.remove(index);
                    if force_reopen || !wire.matches(planned) {
                        // A re-sliced or re-pinned wire is a fresh question for
                        // the hardware: the open channel is the wrong size or
                        // the wrong pin, and whatever the old one was waiting
                        // for no longer applies.
                        self.close_output_wire(&mut wire);
                        wire.endpoint = planned.endpoint.clone();
                        wire.start_samples = planned.start_samples;
                        wire.len_samples = planned.len_samples;
                        wire.last_byte_count = None;
                        wire.parked_at_generation = None;
                        wire.truncated_at_samples = None;
                        wire.capped_at_samples = None;
                    }
                    wires.push(wire);
                }
                None => wires.push(OutputWire::new(planned)),
            }
        }

        for wire in &mut previous {
            self.close_output_wire(wire);
        }
        set.wires = wires;
    }

    fn close_output_sinks(&mut self) {
        let Some(provider) = self.output_provider.as_deref() else {
            return;
        };

        for sink in self.output_sinks.values_mut() {
            for wire in &mut sink.wires {
                close_output_wire_with(provider, wire);
            }
        }
    }

    fn close_output_sink(&self, sink: &mut OutputSinkSet) {
        for wire in &mut sink.wires {
            self.close_output_wire(wire);
        }
    }

    fn close_output_wire(&self, wire: &mut OutputWire) {
        let Some(provider) = self.output_provider.as_deref() else {
            // No provider ever opened it; drop the stale handle so a later
            // provider does not inherit one it never issued.
            wire.channel_handle = None;
            return;
        };
        close_output_wire_with(provider, wire);
    }
}

fn close_output_wire_with(provider: &dyn OutputProvider, wire: &mut OutputWire) {
    if let Some(handle) = wire.channel_handle.take() {
        if let Err(error) = provider.close(handle) {
            log::warn!("EngineServices: failed to close output handle {handle:?}: {error}");
        }
    }
}

impl Drop for EngineServices {
    fn drop(&mut self) {
        self.close_output_sinks();
    }
}

/// Walk `channels` in ascending key order, deriving each wire's slice of the
/// node's control buffer.
///
/// A channel's `count` is in lamps and a lamp is three samples; slices start
/// where the previous one ended. A channel with no count takes the remainder,
/// which only the last one may do — [`remainder_offender`] is the check, and
/// this iterator assumes it has passed. Channels authoring zero lamps drive
/// nothing and are dropped here (the warning is issued once, on the changed
/// path, not by this iterator — it runs per tick).
///
/// Allocation-free by construction: endpoints are borrowed and the running
/// offset lives in the iterator's own state, so the per-tick diff drives it
/// directly.
fn planned_wires(config: &OutputDef) -> impl Iterator<Item = PlannedWire<'_>> {
    let mut start = 0u32;
    config
        .ports
        .entries
        .iter()
        .filter_map(move |(key, channel)| match channel.count() {
            Some(0) => None,
            Some(count) => {
                let len = count.saturating_mul(3);
                let wire = PlannedWire {
                    channel: *key,
                    endpoint: channel.endpoint(),
                    start_samples: start,
                    len_samples: Some(len),
                };
                start = start.saturating_add(len);
                Some(wire)
            }
            None => Some(PlannedWire {
                channel: *key,
                endpoint: channel.endpoint(),
                start_samples: start,
                len_samples: None,
            }),
        })
}

/// The key of the first channel that takes the remainder without being the
/// last one, if any — the one authoring mistake that makes the slices
/// undefined.
///
/// Two channels with no count cannot both be "the rest of the buffer", and a
/// count-less channel in the middle leaves everything after it without a
/// start. Rather than guess, the whole output is refused: a wrong strip lit is
/// worse than a dark one, and the log names the channel to fix.
fn remainder_offender(config: &OutputDef) -> Option<u32> {
    let mut remainder: Option<u32> = None;
    for wire in planned_wires(config) {
        if let Some(previous) = remainder {
            return Some(previous);
        }
        if wire.len_samples.is_none() {
            remainder = Some(wire.channel);
        }
    }
    None
}

/// Do these wires already say exactly what the authored channels say?
///
/// The per-tick hot path: walks both sequences in key order without allocating
/// or cloning, and answers "nothing to do" for the overwhelming majority of
/// calls.
fn wires_match_config(wires: &[OutputWire], config: &OutputDef) -> bool {
    if remainder_offender(config).is_some() {
        // A refused output holds no wires, and stays refused silently.
        return wires.is_empty();
    }
    let mut planned = planned_wires(config);
    for wire in wires {
        match planned.next() {
            Some(next) if wire.matches(next) => {}
            _ => return false,
        }
    }
    planned.next().is_none()
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

/// Flush every wire of every dirty sink, then report the first failure.
///
/// Wires are independent hardware channels and this map has no meaningful
/// order, so a wire that fails to open or write must not cost the frame its
/// siblings: returning early here made one unavailable endpoint look like a
/// whole-project blackout, and which channels survived depended on hash order.
/// Every failure is logged with its node and channel; the first is returned so
/// the tick still reports an error.
///
/// Each node's buffer is decoded **once**, into `samples`, and every wire of
/// that node takes a sub-slice of it — the decode used to run per sink, which
/// with N wires per node would have been N copies of the same buffer per frame.
fn flush_registered_sinks(
    provider: &mut dyn OutputProvider,
    revision: Revision,
    buffers: &RuntimeBufferStore,
    sinks: &mut HashMap<RuntimeBufferId, OutputSinkSet>,
    samples: &mut Vec<u16>,
) -> Result<(), OutputFlushError> {
    let mut first_error: Option<OutputFlushError> = None;
    let mut failed = 0usize;

    // Read once per flush, not once per wire: the answer is the same for every
    // wire in the frame, and asking is a virtual call through the provider.
    //
    // Read it *before* any open in this flush, too. A successful open bumps the
    // generation, so a value sampled afterwards would park a wire against a
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

        // Nothing to drive, or every wire is waiting on a hardware change:
        // skip before paying for the decode.
        if sink
            .wires
            .iter()
            .all(|wire| wire.parked_at_generation == Some(generation))
        {
            continue;
        }

        if bytes.len() % 6 != 0 {
            let error = OutputFlushError::MisalignedPayload {
                node: sink.node,
                buffer_id: *buffer_id,
            };
            failed += 1;
            log::warn!("EngineServices: {error}");
            first_error.get_or_insert(error);
            continue;
        }

        decode_bytes_as_u16_le_into(bytes, samples);

        for wire in sink.wires.iter_mut() {
            if wire.parked_at_generation == Some(generation) {
                continue;
            }
            if let Err(error) = flush_one_wire(
                provider,
                sink.node,
                wire,
                sink.display_options.as_ref(),
                samples,
                generation,
            ) {
                failed += 1;
                log::warn!("EngineServices: {error}");
                first_error.get_or_insert(error);
            }
        }
    }

    // The frame-wide barrier: a provider whose `write` starts transmissions
    // without waiting (so wires transmit concurrently) completes them all
    // here, before the engine goes back to rendering. Synchronous providers
    // default this to a no-op.
    if let Err(error) = provider.flush() {
        failed += 1;
        let error = OutputFlushError::Flush { error };
        log::warn!("EngineServices: {error}");
        first_error.get_or_insert(error);
    }

    match first_error {
        None => Ok(()),
        Some(error) => {
            if failed > 1 {
                log::warn!(
                    "EngineServices: {failed} output wires failed this frame; reporting {error}"
                );
            }
            Err(error)
        }
    }
}

fn flush_one_wire(
    provider: &mut dyn OutputProvider,
    node: NodeId,
    wire: &mut OutputWire,
    display_options: Option<&OutputDriverOptions>,
    samples: &[u16],
    generation: u64,
) -> Result<(), OutputFlushError> {
    let Some(slice) = wire_slice(node, wire, samples) else {
        return Ok(());
    };
    let byte_count = ((slice.len() / 3) * 3) as u32;

    // Every provider refuses a write shorter than the channel it opened
    // (`DataLengthMismatch`), so a wire whose slice shrank — a re-authored
    // count, a smaller upstream extent — must give the channel back and take a
    // right-sized one. Growth needs no reopen: providers resize upward on write.
    if let Some(last) = wire.last_byte_count
        && byte_count < last
        && wire.channel_handle.is_some()
    {
        close_output_wire_with(provider, wire);
        wire.last_byte_count = None;
    }

    ensure_channel_open(provider, wire, display_options, byte_count, generation).map_err(
        |error| OutputFlushError::Provider {
            node,
            channel: wire.channel,
            endpoint: wire.endpoint.clone(),
            error,
        },
    )?;

    let handle = wire
        .channel_handle
        .ok_or_else(|| OutputFlushError::Provider {
            node,
            channel: wire.channel,
            endpoint: wire.endpoint.clone(),
            error: OutputError::InvalidConfig {
                reason: String::from("internal: missing output handle after open"),
            },
        })?;

    provider
        .write(handle, slice)
        .map_err(|error| OutputFlushError::Provider {
            node,
            channel: wire.channel,
            endpoint: wire.endpoint.clone(),
            error,
        })?;
    wire.last_byte_count = Some(byte_count.max(wire.last_byte_count.unwrap_or(3)));
    Ok(())
}

/// The samples this wire carries out of the node's decoded buffer, or `None`
/// when the buffer holds nothing for it this frame.
///
/// The authored slice is what the project asked for; the buffer is what the
/// graph produced, and it is upstream-driven, so the two disagree routinely
/// (a shader resized, a fixture lost a path, the counts simply add up to more
/// lamps than exist). Overflow is clamped rather than fatal — the wires that
/// do have pixels must still light — but it is said out loud, once per extent.
///
/// A second, independent clamp applies after: no wire may carry more than
/// [`WS281X_MAX_LEDS_PER_CHANNEL`] lamps, the shared bound every
/// [`lpc_shared::output::OutputProvider`] is granted through this one seam.
/// Applying it here — rather than leaving it to each provider — is what makes
/// host, emulator, and device agree on the same byte count for the same
/// authored channel; a provider is free to keep its own copy as defense in
/// depth, but this is the one that decides.
fn wire_slice<'a>(node: NodeId, wire: &mut OutputWire, samples: &'a [u16]) -> Option<&'a [u16]> {
    let available = samples.len() as u32;
    let wanted_end = match wire.len_samples {
        Some(len) => wire.start_samples.saturating_add(len),
        None => available,
    };
    let start = wire.start_samples.min(available);
    let end = wanted_end.min(available);

    if end < wanted_end {
        if wire.truncated_at_samples != Some(available) {
            log::warn!(
                "EngineServices: output node {node} channel {} ({}) wants samples \
                 {}..{wanted_end} but the control buffer holds {available}; \
                 driving {} sample(s) of it",
                wire.channel,
                wire.endpoint,
                wire.start_samples,
                end.saturating_sub(start),
            );
            wire.truncated_at_samples = Some(available);
        }
    } else {
        wire.truncated_at_samples = None;
    }

    let requested = ((end.saturating_sub(start)) / 3) * 3;
    // Samples and WS281x protocol bytes are the same count here — one 16-bit
    // sample renders to one 8-bit protocol byte — so the byte-count helper
    // applies to this sample count unchanged.
    let (len, capped) = ws281x_capped_byte_count(requested);
    if capped {
        if wire.capped_at_samples != Some(requested) {
            log::warn!(
                "EngineServices: output node {node} channel {} ({}) wants {} lamp(s) but the \
                 shared per-channel cap is {WS281X_MAX_LEDS_PER_CHANNEL} lamps; granting {} \
                 lamp(s) of it",
                wire.channel,
                wire.endpoint,
                requested / 3,
                len / 3,
            );
            wire.capped_at_samples = Some(requested);
        }
    } else {
        wire.capped_at_samples = None;
    }

    if len == 0 {
        return None;
    }
    let start = start as usize;
    Some(&samples[start..start + len as usize])
}

fn decode_bytes_as_u16_le_into(bytes: &[u8], out: &mut Vec<u16>) {
    out.clear();
    out.reserve(bytes.len() / 2);
    out.extend(
        bytes
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]])),
    );
}

/// Open the wire's channel if it has none, parking it on failure.
///
/// `generation` is the provider's hardware generation sampled before any open
/// this flush; a wire that fails records it and is skipped until it changes.
fn ensure_channel_open(
    provider: &dyn OutputProvider,
    wire: &mut OutputWire,
    display_options: Option<&OutputDriverOptions>,
    byte_count: u32,
    generation: u64,
) -> Result<(), OutputError> {
    if wire.channel_handle.is_some() {
        return Ok(());
    }

    let bc = wire.last_byte_count.unwrap_or(3).max(byte_count).max(3);
    let handle = match provider.open(
        &wire.endpoint,
        bc,
        OutputFormat::Ws2811,
        display_options.cloned(),
    ) {
        Ok(handle) => handle,
        Err(error) => {
            wire.parked_at_generation = Some(generation);
            return Err(error);
        }
    };

    if wire.parked_at_generation.take().is_some() {
        log::info!(
            "EngineServices: output {} recovered and is writing again",
            wire.endpoint
        );
    }
    wire.channel_handle = Some(handle);
    wire.last_byte_count = Some(bc);
    Ok(())
}

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;
    use alloc::rc::Rc;
    use alloc::string::ToString;
    use alloc::vec;
    use alloc::vec::Vec;

    use lpc_hardware::OutputError;
    use lpc_model::nodes::output::{OutputDef, OutputDriverOptionsConfig, OutputPortDef};
    use lpc_model::{HwEndpointSpec, NodeId, OptionSlot, Revision, TreePath, WithRevision};
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
        services.register_output_sink(buffer_id, node(1), &OutputDef::new(endpoint.clone()));

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
        services.register_output_sink(buffer_id, node(1), &OutputDef::new(endpoint.clone()));
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
        services.update_output_sink_config(buffer_id, node(1), &next);
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
        services.register_output_sink(buffer_id, node(1), &config);
        services
            .flush_dirty_output_sinks(Revision::new(1), &buffers)
            .expect("initial flush opens the channel");
        let handle = provider
            .inner()
            .get_handle_for_endpoint(&endpoint("ws281x:local:D10"))
            .expect("channel handle");
        let generation = provider.hardware_generation();

        for _ in 0..32 {
            services.update_output_sink_config(buffer_id, node(1), &config);
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

    /// Every flush must end with the provider's frame-wide barrier — the call
    /// that completes transmissions a concurrent provider's `write` started
    /// without waiting for. This pins the *engine* half of the contract; the
    /// provider half lives in `fw-esp32-common`. It exists because the
    /// barrier once vanished silently inside a delegating wrapper that did
    /// not forward the defaulted method, and nothing on the host noticed.
    #[test]
    fn every_flush_ends_with_the_provider_barrier() {
        let provider = Rc::new(CountingOutputProvider::new(MemoryOutputProvider::new()));
        let mut services = EngineServices::new(TreePath::parse("/p.show").expect("tree path"));
        services.set_output_provider(Some(Box::new(SharedCountingProvider(Rc::clone(&provider)))));

        let mut buffers = RuntimeBufferStore::new();
        let buffer_id = output_buffer(&mut buffers, Revision::new(1));
        services.register_output_sink(
            buffer_id,
            node(1),
            &OutputDef::new(endpoint("ws281x:local:D10")),
        );

        services
            .flush_dirty_output_sinks(Revision::new(1), &buffers)
            .expect("flush");
        assert_eq!(provider.flush_calls(), 1, "one barrier per flush");

        // A flush with nothing dirty still runs the barrier: whether anything
        // is in flight is the provider's knowledge, not the engine's.
        services
            .flush_dirty_output_sinks(Revision::new(2), &buffers)
            .expect("clean flush");
        assert_eq!(provider.flush_calls(), 2);
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
        services.register_output_sink(buffer_id, node(1), &OutputDef::new(endpoint.clone()));
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
        services.register_output_sink(first, node(1), &OutputDef::new(endpoint.clone()));
        services.register_output_sink(second, node(2), &OutputDef::new(endpoint.clone()));

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
        services.register_output_sink(first, node(1), &OutputDef::new(first_endpoint.clone()));
        services.register_output_sink(second, node(2), &OutputDef::new(second_endpoint.clone()));

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
        services.register_output_sink(bad_buffer, node(1), &OutputDef::new(unknown.clone()));
        for spec in &good {
            let buffer_id = output_buffer(&mut buffers, Revision::new(1));
            services.register_output_sink(buffer_id, node(9), &OutputDef::new(spec.clone()));
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
            node(1),
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
        services.register_output_sink(holder, node(1), &OutputDef::new(held.clone()));
        services.register_output_sink(waiter, node(2), &OutputDef::new(waiting.clone()));

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
            node(1),
            &OutputDef::new(endpoint("ws281x:local:NOT-A-PIN")),
        );
        let _ = services.flush_dirty_output_sinks(Revision::new(1), &buffers);
        let generation_before = provider.hardware_generation();

        let good = endpoint("ws281x:local:D10");
        services.update_output_sink_config(buffer_id, node(1), &OutputDef::new(good.clone()));
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
        services.register_output_sink(buffer_id, node(1), &OutputDef::new(demo.clone()));
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

    // ---- one node, many wires -------------------------------------------

    /// The whole point of channels: one node, one buffer, N wires, each
    /// carrying exactly its own lamps and nobody else's.
    #[test]
    fn one_node_slices_its_buffer_across_its_wires() {
        let provider = Rc::new(MemoryOutputProvider::with_hardware_manifest(
            lpc_hardware::default_esp32s3_hardware_manifest(),
        ));
        let mut services = EngineServices::new(TreePath::parse("/p.show").expect("tree path"));
        services.set_output_provider(Some(Box::new(SharedMemoryOutputProvider(Rc::clone(
            &provider,
        )))));

        let mut buffers = RuntimeBufferStore::new();
        // Six lamps: 2 + 3 authored, and the last channel takes what is left.
        let buffer_id = ramp_buffer(&mut buffers, 6, Revision::new(1));
        services.register_output_sink(buffer_id, node(1), &triple_channel_output());

        services
            .flush_dirty_output_sinks(Revision::new(1), &buffers)
            .expect("every wire opens and writes");

        assert_eq!(provider.open_channel_count(), 3);
        assert_eq!(wire_data(&provider, "ws281x:local:D10"), ramp(0, 6));
        assert_eq!(wire_data(&provider, "ws281x:local:D9"), ramp(6, 9));
        assert_eq!(
            wire_data(&provider, "ws281x:local:D8"),
            ramp(15, 3),
            "the highest-keyed channel omits its count, so it takes the remainder"
        );
    }

    /// Two channels cannot both be "the rest of the buffer", and a count-less
    /// channel in the middle leaves everything after it without a start. The
    /// output is refused whole rather than lighting a guess — and it must not
    /// panic, because this is authored data.
    #[test]
    fn an_output_with_two_remainder_channels_drives_nothing() {
        let provider = Rc::new(MemoryOutputProvider::new_permissive());
        let mut services = EngineServices::new(TreePath::parse("/p.show").expect("tree path"));
        services.set_output_provider(Some(Box::new(SharedMemoryOutputProvider(Rc::clone(
            &provider,
        )))));

        let mut buffers = RuntimeBufferStore::new();
        let buffer_id = ramp_buffer(&mut buffers, 6, Revision::new(1));
        let config = OutputDef::with_ports([
            (0, OutputPortDef::new(endpoint("ws281x:local:D10"))),
            (1, OutputPortDef::new(endpoint("ws281x:local:D9"))),
        ]);
        services.register_output_sink(buffer_id, node(1), &config);

        services
            .flush_dirty_output_sinks(Revision::new(1), &buffers)
            .expect("a refused output is not a frame failure");

        assert_eq!(
            provider.open_channel_count(),
            0,
            "an output whose slices are undefined must not light a wire"
        );

        // And once the authoring is fixed, the very next tick drives it.
        services.update_output_sink_config(buffer_id, node(1), &triple_channel_output());
        services
            .flush_dirty_output_sinks(Revision::new(1), &buffers)
            .expect("the corrected output flushes");
        assert_eq!(provider.open_channel_count(), 3);
    }

    /// Counts are authored; the buffer is whatever the graph produced. When
    /// they disagree the wires that do have pixels must still light, so the
    /// overflow is clamped — loudly — rather than fatal.
    #[test]
    fn slices_past_the_end_of_the_buffer_are_truncated_not_fatal() {
        let provider = Rc::new(MemoryOutputProvider::with_hardware_manifest(
            lpc_hardware::default_esp32s3_hardware_manifest(),
        ));
        let mut services = EngineServices::new(TreePath::parse("/p.show").expect("tree path"));
        services.set_output_provider(Some(Box::new(SharedMemoryOutputProvider(Rc::clone(
            &provider,
        )))));

        let mut buffers = RuntimeBufferStore::new();
        // Three lamps for a project that authored 2 + 3 + 4.
        let buffer_id = ramp_buffer(&mut buffers, 3, Revision::new(1));
        let config = OutputDef::with_ports([
            (
                0,
                OutputPortDef::with_count(endpoint("ws281x:local:D10"), 2),
            ),
            (1, OutputPortDef::with_count(endpoint("ws281x:local:D9"), 3)),
            (2, OutputPortDef::with_count(endpoint("ws281x:local:D8"), 4)),
        ]);
        services.register_output_sink(buffer_id, node(1), &config);

        services
            .flush_dirty_output_sinks(Revision::new(1), &buffers)
            .expect("a short buffer must not fail the frame");

        assert_eq!(wire_data(&provider, "ws281x:local:D10"), ramp(0, 6));
        assert_eq!(
            wire_data(&provider, "ws281x:local:D9"),
            ramp(6, 3),
            "the straddling wire carries the lamps that exist"
        );
        assert!(
            !provider.is_endpoint_open(&endpoint("ws281x:local:D8")),
            "a wire with no samples at all opens nothing"
        );
    }

    /// The other direction, and the one output fragments create: a buffer
    /// LONGER than the authored counts add up to, because a second producer
    /// was merged onto the same output. The wire split is a second coordinate
    /// system over the same buffer and it is authored, not derived — so
    /// fully-counted wires carry exactly what they authored and the extra
    /// samples simply go nowhere, silently. A tail nobody claimed is not a
    /// defect to warn about; the OUTPUT is where an unclaimed range is
    /// reported (`OutputNode::runtime_status`).
    #[test]
    fn a_fragment_grown_buffer_leaves_fully_counted_wires_alone() {
        let provider = Rc::new(MemoryOutputProvider::with_hardware_manifest(
            lpc_hardware::default_esp32s3_hardware_manifest(),
        ));
        let mut services = EngineServices::new(TreePath::parse("/p.show").expect("tree path"));
        services.set_output_provider(Some(Box::new(SharedMemoryOutputProvider(Rc::clone(
            &provider,
        )))));

        let mut buffers = RuntimeBufferStore::new();
        // Eight lamps for a project that authored 2 + 3: a second fixture's
        // fragment grew the buffer past what the wires ask for.
        let buffer_id = ramp_buffer(&mut buffers, 8, Revision::new(1));
        let config = OutputDef::with_ports([
            (
                0,
                OutputPortDef::with_count(endpoint("ws281x:local:D10"), 2),
            ),
            (1, OutputPortDef::with_count(endpoint("ws281x:local:D9"), 3)),
        ]);
        services.register_output_sink(buffer_id, node(1), &config);

        services
            .flush_dirty_output_sinks(Revision::new(1), &buffers)
            .expect("a long buffer must not fail the frame");

        assert_eq!(provider.open_channel_count(), 2);
        assert_eq!(wire_data(&provider, "ws281x:local:D10"), ramp(0, 6));
        assert_eq!(
            wire_data(&provider, "ws281x:local:D9"),
            ramp(6, 9),
            "each wire carries exactly its authored count; the tail is unclaimed"
        );
    }

    /// …and a remainder channel GROWS with the buffer, which is what makes
    /// "add a second strand to this output" work with no re-authoring at all:
    /// the last wire simply carries more lamps.
    #[test]
    fn a_fragment_grown_buffer_extends_the_remainder_wire() {
        let provider = Rc::new(MemoryOutputProvider::with_hardware_manifest(
            lpc_hardware::default_esp32s3_hardware_manifest(),
        ));
        let mut services = EngineServices::new(TreePath::parse("/p.show").expect("tree path"));
        services.set_output_provider(Some(Box::new(SharedMemoryOutputProvider(Rc::clone(
            &provider,
        )))));

        let mut buffers = RuntimeBufferStore::new();
        let four_lamps = ramp_buffer(&mut buffers, 4, Revision::new(1));
        let config = OutputDef::new(endpoint("ws281x:local:D10"));
        services.register_output_sink(four_lamps, node(1), &config);
        services
            .flush_dirty_output_sinks(Revision::new(1), &buffers)
            .expect("first flush");
        assert_eq!(wire_data(&provider, "ws281x:local:D10"), ramp(0, 12));

        // A second producer merges in; the buffer doubles under the same
        // authored output.
        let mut buffers = RuntimeBufferStore::new();
        let eight_lamps = ramp_buffer(&mut buffers, 8, Revision::new(2));
        services.unregister_output_sink(four_lamps);
        services.register_output_sink(eight_lamps, node(1), &config);
        services
            .flush_dirty_output_sinks(Revision::new(2), &buffers)
            .expect("second flush");

        assert_eq!(
            wire_data(&provider, "ws281x:local:D10"),
            ramp(0, 24),
            "the remainder wire takes the whole grown buffer"
        );
    }

    /// Every provider refuses a write shorter than the channel it opened, so a
    /// wire whose slice shrank must give the channel back and take a
    /// right-sized one. Without the reopen this fails every frame, forever.
    #[test]
    fn a_shrunken_slice_reopens_the_channel() {
        let provider = Rc::new(MemoryOutputProvider::with_hardware_manifest(
            lpc_hardware::default_esp32s3_hardware_manifest(),
        ));
        let mut services = EngineServices::new(TreePath::parse("/p.show").expect("tree path"));
        services.set_output_provider(Some(Box::new(SharedMemoryOutputProvider(Rc::clone(
            &provider,
        )))));

        let mut buffers = RuntimeBufferStore::new();
        let buffer_id = ramp_buffer(&mut buffers, 6, Revision::new(1));
        let wide = OutputDef::with_ports([
            (
                0,
                OutputPortDef::with_count(endpoint("ws281x:local:D10"), 4),
            ),
            (2, OutputPortDef::new(endpoint("ws281x:local:D9"))),
        ]);
        services.register_output_sink(buffer_id, node(1), &wide);
        services
            .flush_dirty_output_sinks(Revision::new(1), &buffers)
            .expect("initial flush");
        let first = provider
            .get_handle_for_endpoint(&endpoint("ws281x:local:D10"))
            .expect("first handle");
        assert_eq!(wire_data(&provider, "ws281x:local:D10"), ramp(0, 12));

        let narrow = OutputDef::with_ports([
            (
                0,
                OutputPortDef::with_count(endpoint("ws281x:local:D10"), 2),
            ),
            (2, OutputPortDef::new(endpoint("ws281x:local:D9"))),
        ]);
        services.update_output_sink_config(buffer_id, node(1), &narrow);
        services
            .flush_dirty_output_sinks(Revision::new(1), &buffers)
            .expect("the shrunken wire reopens instead of failing the write");

        let second = provider
            .get_handle_for_endpoint(&endpoint("ws281x:local:D10"))
            .expect("second handle");
        assert_ne!(first, second, "a narrower slice must reopen the channel");
        assert_eq!(wire_data(&provider, "ws281x:local:D10"), ramp(0, 6));
        assert_eq!(
            wire_data(&provider, "ws281x:local:D9"),
            ramp(6, 12),
            "the remainder wire grew to cover what channel 0 gave up"
        );
    }

    /// Parking is per wire, exactly as it was per sink: a channel the board
    /// cannot give must not take its siblings down, and must not re-ask every
    /// frame either.
    #[test]
    fn one_unopenable_wire_neither_blocks_nor_re_asks() {
        let provider = Rc::new(CountingOutputProvider::new(
            MemoryOutputProvider::with_hardware_manifest(
                lpc_hardware::default_esp32s3_hardware_manifest(),
            ),
        ));
        let mut services = EngineServices::new(TreePath::parse("/p.show").expect("tree path"));
        services.set_output_provider(Some(Box::new(SharedCountingProvider(Rc::clone(&provider)))));

        let mut buffers = RuntimeBufferStore::new();
        let buffer_id = ramp_buffer(&mut buffers, 4, Revision::new(1));
        let config = OutputDef::with_ports([
            (
                0,
                OutputPortDef::with_count(endpoint("ws281x:local:NOT-A-PIN"), 2),
            ),
            (1, OutputPortDef::new(endpoint("ws281x:local:D10"))),
        ]);
        services.register_output_sink(buffer_id, node(1), &config);

        let err = services
            .flush_dirty_output_sinks(Revision::new(1), &buffers)
            .expect_err("the missing pin is still reported");
        assert!(
            err.to_string().contains("node 1 channel 0"),
            "the error must name the node and the channel, got: {err}"
        );
        assert_eq!(
            wire_data(provider.inner(), "ws281x:local:D10"),
            ramp(6, 6),
            "the sibling wire flushed its own slice regardless"
        );

        // The sibling's successful claim bumped the hardware generation, so the
        // parked wire is owed exactly one retry (the retry ADR's recovery
        // path); after that nothing has changed and it stays quiet.
        let _ = services.flush_dirty_output_sinks(Revision::new(1), &buffers);
        let opens = provider.open_calls();
        for _ in 0..8 {
            services
                .flush_dirty_output_sinks(Revision::new(1), &buffers)
                .expect("a parked wire does not keep failing the frame");
        }
        assert_eq!(
            provider.open_calls(),
            opens,
            "neither wire re-attempts while the hardware is unchanged"
        );
    }

    /// Re-authoring one channel asks the hardware about that channel only. The
    /// sink used to park as a unit, so editing any field un-parked every wire
    /// and paid for a full enumeration per hopeless endpoint all over again.
    #[test]
    fn editing_one_wire_leaves_its_siblings_parked() {
        let provider = Rc::new(CountingOutputProvider::new(
            MemoryOutputProvider::with_hardware_manifest(
                lpc_hardware::default_esp32s3_hardware_manifest(),
            ),
        ));
        let mut services = EngineServices::new(TreePath::parse("/p.show").expect("tree path"));
        services.set_output_provider(Some(Box::new(SharedCountingProvider(Rc::clone(&provider)))));

        let mut buffers = RuntimeBufferStore::new();
        let buffer_id = ramp_buffer(&mut buffers, 4, Revision::new(1));
        let missing = |zero: &str, one: &str| {
            OutputDef::with_ports([
                (
                    0,
                    OutputPortDef::with_count(HwEndpointSpec::parse(zero).expect("spec"), 2),
                ),
                (
                    1,
                    OutputPortDef::with_count(HwEndpointSpec::parse(one).expect("spec"), 2),
                ),
            ])
        };
        services.register_output_sink(
            buffer_id,
            node(1),
            &missing("ws281x:local:NOT-A-PIN", "ws281x:local:ALSO-MISSING"),
        );
        let _ = services.flush_dirty_output_sinks(Revision::new(1), &buffers);
        assert_eq!(provider.open_calls(), 2, "both wires had their one attempt");

        // Fix channel 1 only.
        services.update_output_sink_config(
            buffer_id,
            node(1),
            &missing("ws281x:local:NOT-A-PIN", "ws281x:local:D10"),
        );
        let _ = services.flush_dirty_output_sinks(Revision::new(1), &buffers);

        assert_eq!(
            provider.open_calls(),
            3,
            "only the re-authored wire asked again; channel 0 is still parked"
        );
        assert!(
            provider
                .inner()
                .is_endpoint_open(&endpoint("ws281x:local:D10"))
        );
    }

    // ---- shared per-channel cap (P3) --------------------------------------

    /// The headline parity claim: a wire that wants more than the shared
    /// 1024-lamp cap is truncated to exactly `1024*3` bytes, on the plain
    /// memory provider — no protocol-specific behavior involved, just the
    /// engine seam every provider goes through.
    #[test]
    fn a_1500_lamp_channel_truncates_to_the_shared_cap_on_the_memory_provider() {
        let provider = Rc::new(MemoryOutputProvider::new());
        let mut services = EngineServices::new(TreePath::parse("/p.show").expect("tree path"));
        services.set_output_provider(Some(Box::new(SharedMemoryOutputProvider(Rc::clone(
            &provider,
        )))));

        let mut buffers = RuntimeBufferStore::new();
        let buffer_id = ramp_buffer(&mut buffers, 1500, Revision::new(1));
        let endpoint = endpoint("ws281x:local:D10");
        services.register_output_sink(buffer_id, node(1), &OutputDef::new(endpoint.clone()));

        services
            .flush_dirty_output_sinks(Revision::new(1), &buffers)
            .expect("a capped wire still opens and writes");

        assert_eq!(
            wire_data(&provider, "ws281x:local:D10"),
            ramp(0, super::WS281X_MAX_LEDS_PER_CHANNEL as u32 * 3),
            "only the first 1024 lamps reach the memory provider"
        );
    }

    /// The same parity claim again, but through a provider that applies no
    /// cap of its own — not even `MemoryOutputProvider`'s length bookkeeping,
    /// just a bare recorder of what it was asked to open and write.
    ///
    /// This stands in for `fw-emu`'s `SyscallOutputProvider`, the other
    /// non-capping provider N1 found (`fw-emu/src/output.rs:52`):
    /// `SyscallOutputProvider` cannot be unit-tested on this host toolchain
    /// today (`cargo test -p fw-emu` fails to link — the guest crate
    /// `lp-riscv-emu-guest` defines its own `panic_impl`, which collides with
    /// `std`'s under a host test binary; `fw-emu` is not in this phase's
    /// validation package list for the same reason). What this test proves
    /// instead: the engine seam hands *any* non-capping provider the exact
    /// same capped byte count, so the guarantee does not depend on what the
    /// provider does with it.
    #[test]
    fn a_1500_lamp_channel_truncates_identically_through_a_non_capping_provider() {
        let provider = Rc::new(RecordingOutputProvider::new());
        let mut services = EngineServices::new(TreePath::parse("/p.show").expect("tree path"));
        services.set_output_provider(Some(Box::new(SharedRecordingProvider(Rc::clone(
            &provider,
        )))));

        let mut buffers = RuntimeBufferStore::new();
        let buffer_id = ramp_buffer(&mut buffers, 1500, Revision::new(1));
        let endpoint = endpoint("ws281x:local:D10");
        services.register_output_sink(buffer_id, node(1), &OutputDef::new(endpoint));

        services
            .flush_dirty_output_sinks(Revision::new(1), &buffers)
            .expect("a capped wire still opens and writes");

        let cap_bytes = super::WS281X_MAX_LEDS_PER_CHANNEL as u32 * 3;
        assert_eq!(
            provider.opens(),
            vec![cap_bytes],
            "the provider must be asked to open exactly the capped byte count, never 1500*3"
        );
        assert_eq!(
            provider.writes(),
            vec![cap_bytes as usize],
            "the provider must be handed exactly the capped sample count on write"
        );
    }

    /// 375 and 1024 lamps both sit at or under the cap, so neither is
    /// truncated — the DOD's explicit "uncapped everywhere" boundary case.
    #[test]
    fn channels_at_or_under_the_cap_open_uncapped() {
        for lamps in [375u32, super::WS281X_MAX_LEDS_PER_CHANNEL as u32] {
            let provider = Rc::new(MemoryOutputProvider::new());
            let mut services = EngineServices::new(TreePath::parse("/p.show").expect("tree path"));
            services.set_output_provider(Some(Box::new(SharedMemoryOutputProvider(Rc::clone(
                &provider,
            )))));

            let mut buffers = RuntimeBufferStore::new();
            let buffer_id = ramp_buffer(&mut buffers, lamps, Revision::new(1));
            let endpoint = endpoint("ws281x:local:D10");
            services.register_output_sink(buffer_id, node(1), &OutputDef::new(endpoint.clone()));

            services
                .flush_dirty_output_sinks(Revision::new(1), &buffers)
                .expect("flush");

            assert_eq!(
                wire_data(&provider, "ws281x:local:D10"),
                ramp(0, lamps * 3),
                "a {lamps}-lamp channel must not be truncated"
            );
        }
    }

    /// A dome-shaped project: five wires, 300 lamps each, none anywhere near
    /// the 1024 cap. All five must open and carry exactly their own slice,
    /// uncapped, with the offsets undisturbed by capping logic that never
    /// fires.
    #[test]
    fn a_five_channel_dome_shaped_project_opens_every_wire_uncapped() {
        let provider = Rc::new(MemoryOutputProvider::new_permissive());
        let mut services = EngineServices::new(TreePath::parse("/p.show").expect("tree path"));
        services.set_output_provider(Some(Box::new(SharedMemoryOutputProvider(Rc::clone(
            &provider,
        )))));

        let mut buffers = RuntimeBufferStore::new();
        let buffer_id = ramp_buffer(&mut buffers, 1500, Revision::new(1));
        let config = OutputDef::with_ports((0..5).map(|i| {
            (
                i,
                OutputPortDef::with_count(
                    HwEndpointSpec::parse(&alloc::format!("ws281x:local:D{i}")).expect("spec"),
                    300,
                ),
            )
        }));
        services.register_output_sink(buffer_id, node(1), &config);

        services
            .flush_dirty_output_sinks(Revision::new(1), &buffers)
            .expect("every strand opens and writes");

        assert_eq!(provider.open_channel_count(), 5);
        for i in 0..5u32 {
            let spec = HwEndpointSpec::parse(&alloc::format!("ws281x:local:D{i}")).expect("spec");
            let handle = provider
                .get_handle_for_endpoint(&spec)
                .unwrap_or_else(|| panic!("channel {i} never opened"));
            assert_eq!(
                provider.get_data(handle),
                Some(ramp(i * 900, 900)),
                "channel {i} must carry exactly its own 300 lamps, uncapped"
            );
        }
    }

    /// A channel authoring more than the cap must not corrupt where its
    /// siblings start: the cap only trims what reaches the provider, the
    /// buffer offsets downstream channels are computed from stay the full
    /// authored count.
    #[test]
    fn a_capped_channel_does_not_shift_its_siblings_offset() {
        let provider = Rc::new(MemoryOutputProvider::new_permissive());
        let mut services = EngineServices::new(TreePath::parse("/p.show").expect("tree path"));
        services.set_output_provider(Some(Box::new(SharedMemoryOutputProvider(Rc::clone(
            &provider,
        )))));

        let mut buffers = RuntimeBufferStore::new();
        // Channel 0 authors 1500 lamps (over the cap); channel 1 takes the
        // remainder — 3 lamps — starting where channel 0's *authored* extent
        // ends, at sample 4500, not where its *capped* write ended.
        let buffer_id = ramp_buffer(&mut buffers, 1503, Revision::new(1));
        let config = OutputDef::with_ports([
            (
                0,
                OutputPortDef::with_count(endpoint("ws281x:local:D0"), 1500),
            ),
            (1, OutputPortDef::new(endpoint("ws281x:local:D1"))),
        ]);
        services.register_output_sink(buffer_id, node(1), &config);

        services
            .flush_dirty_output_sinks(Revision::new(1), &buffers)
            .expect("both wires open and write");

        assert_eq!(
            wire_data(&provider, "ws281x:local:D0"),
            ramp(0, super::WS281X_MAX_LEDS_PER_CHANNEL as u32 * 3),
            "channel 0 is capped to its first 1024 lamps"
        );
        assert_eq!(
            wire_data(&provider, "ws281x:local:D1"),
            ramp(1500 * 3, 9),
            "channel 1 still starts after channel 0's full 1500-lamp authored extent"
        );
    }

    fn endpoint(spec: &'static str) -> HwEndpointSpec {
        HwEndpointSpec::from_static(spec)
    }

    fn node(id: u32) -> NodeId {
        NodeId::new(id)
    }

    /// Three wires over one buffer: 2 lamps, 3 lamps, and the remainder.
    fn triple_channel_output() -> OutputDef {
        OutputDef::with_ports([
            (
                0,
                OutputPortDef::with_count(endpoint("ws281x:local:D10"), 2),
            ),
            (1, OutputPortDef::with_count(endpoint("ws281x:local:D9"), 3)),
            (2, OutputPortDef::new(endpoint("ws281x:local:D8"))),
        ])
    }

    /// A node buffer of `lamps` RGB lamps whose samples count up from 1, so a
    /// wire's slice is identifiable by value alone.
    fn ramp_buffer(
        store: &mut RuntimeBufferStore,
        lamps: u32,
        revision: Revision,
    ) -> RuntimeBufferId {
        let mut bytes = Vec::new();
        for sample in ramp(0, lamps * 3) {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        store.insert(WithRevision::new(
            revision,
            RuntimeBuffer::output_channels_u16(lamps, bytes),
        ))
    }

    /// The `len` samples of [`ramp_buffer`] starting at sample `start`.
    fn ramp(start: u32, len: u32) -> Vec<u16> {
        (start..start + len).map(|i| (i + 1) as u16).collect()
    }

    fn wire_data(provider: &MemoryOutputProvider, spec: &'static str) -> Vec<u16> {
        let handle = provider
            .get_handle_for_endpoint(&endpoint(spec))
            .unwrap_or_else(|| panic!("{spec} never opened"));
        provider
            .get_data(handle)
            .unwrap_or_else(|| panic!("{spec} opened but wrote nothing"))
    }

    fn output_buffer(store: &mut RuntimeBufferStore, revision: Revision) -> RuntimeBufferId {
        store.insert(WithRevision::new(
            revision,
            RuntimeBuffer::output_channels_u16(3, vec![0, 1, 0, 2, 0, 3]),
        ))
    }

    /// Counts `open` calls, which is how the parking tests tell "asked once"
    /// from "asked every frame" — and `flush` calls, which is how the barrier
    /// test tells "the engine's end-of-frame barrier reached the provider"
    /// from "a delegating wrapper swallowed it into the trait default" (the
    /// exact silent failure `lpa-server`'s `SharedOutputProvider` shipped).
    struct CountingOutputProvider {
        inner: MemoryOutputProvider,
        open_calls: core::cell::Cell<usize>,
        flush_calls: core::cell::Cell<usize>,
    }

    impl CountingOutputProvider {
        fn new(inner: MemoryOutputProvider) -> Self {
            Self {
                inner,
                open_calls: core::cell::Cell::new(0),
                flush_calls: core::cell::Cell::new(0),
            }
        }

        fn inner(&self) -> &MemoryOutputProvider {
            &self.inner
        }

        fn open_calls(&self) -> usize {
            self.open_calls.get()
        }

        fn flush_calls(&self) -> usize {
            self.flush_calls.get()
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

        fn flush(&self) -> Result<(), OutputError> {
            self.0.flush_calls.set(self.0.flush_calls.get() + 1);
            OutputProvider::flush(&self.0.inner)
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

    /// Deliberately non-capping [`OutputProvider`]: it records what it is
    /// asked to open and write and applies no bound of its own, unlike
    /// `MemoryOutputProvider` (which at least validates length on write) or
    /// `Esp32OutputProvider` (which keeps its own backstop). See
    /// `a_1500_lamp_channel_truncates_identically_through_a_non_capping_provider`
    /// for why this stands in for `fw-emu`'s `SyscallOutputProvider`.
    struct RecordingOutputProvider {
        opens: core::cell::RefCell<Vec<u32>>,
        writes: core::cell::RefCell<Vec<usize>>,
        next_handle: core::cell::Cell<i32>,
    }

    impl RecordingOutputProvider {
        fn new() -> Self {
            Self {
                opens: core::cell::RefCell::new(Vec::new()),
                writes: core::cell::RefCell::new(Vec::new()),
                next_handle: core::cell::Cell::new(0),
            }
        }

        fn opens(&self) -> Vec<u32> {
            self.opens.borrow().clone()
        }

        fn writes(&self) -> Vec<usize> {
            self.writes.borrow().clone()
        }
    }

    struct SharedRecordingProvider(Rc<RecordingOutputProvider>);

    impl OutputProvider for SharedRecordingProvider {
        fn open(
            &self,
            _endpoint: &HwEndpointSpec,
            byte_count: u32,
            format: OutputFormat,
            _options: Option<OutputDriverOptions>,
        ) -> Result<OutputChannelHandle, OutputError> {
            assert_eq!(format, OutputFormat::Ws2811);
            self.0.opens.borrow_mut().push(byte_count);
            let handle = self.0.next_handle.get();
            self.0.next_handle.set(handle + 1);
            Ok(OutputChannelHandle::new(handle))
        }

        fn write(&self, _handle: OutputChannelHandle, data: &[u16]) -> Result<(), OutputError> {
            self.0.writes.borrow_mut().push(data.len());
            Ok(())
        }

        fn close(&self, _handle: OutputChannelHandle) -> Result<(), OutputError> {
            Ok(())
        }

        fn hardware_generation(&self) -> u64 {
            0
        }
    }
}
