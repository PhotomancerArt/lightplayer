//! Project probe helpers.

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use lp_collection::VecMap;
use lpc_model::{ChannelName, Kind, NodeId};
use lpc_registry::ProjectRegistry;
use lpc_wire::{
    BindingGraphProbeRequest, BindingGraphProbeResult, ControlDisplayLayoutProbeResult,
    ControlDisplayLayoutRead, ControlProductProbeRequest, ControlProductProbeResult,
    OutputFrameEntry, OutputFrameProbeRequest, OutputFrameProbeResult, RenderProductProbeRequest,
    RenderProductProbeResult, TimebaseProbeRequest, TimebaseProbeResult, WireBindingDirection,
    WireBindingEndpoint, WireBindingGraph, WireBindingOrigin, WireBusChannel, WireBusChannelValue,
    WireCellProjection, WireChannelSampleFormat, WireConsumerPolicy, WireEffectiveBinding,
    WirePhasorOrigin, WirePhasorRow, WireProjectionOrigin, WireVisualSpace,
};
use lps_shared::TextureStorageFormat;

use crate::dataflow::binding::{
    BindingEntry, BindingPriority, BindingRef, BindingSource, BindingTarget,
};
use crate::node::NodeEntryState;
use crate::products::control::{
    ControlLayout, ControlProduct, ControlRenderRequest, ControlRenderTarget,
};
use crate::products::visual::RenderTextureRequest;
use crate::resource::{RuntimeBufferId, RuntimeBufferMetadata, RuntimeChannelSampleFormat};

use super::Engine;
use crate::products::visual::{
    CellProjection, ConsumerPolicy, ProductSpaceInfo, ProjectionOrigin, VisualSpace,
    resolve_1d_to_2d_with_origin,
};

/// One output node found by the published-frame tree walk, snapshotted so the
/// layout pass can take `&mut Engine` without holding a tree borrow.
struct PublishedOutputCandidate {
    node: NodeId,
    buffer_id: RuntimeBufferId,
    sample_layout: Option<ControlLayout>,
    source_product: Option<ControlProduct>,
}

impl Engine {
    pub(super) fn read_project_render_product_probe(
        &mut self,
        registry: &ProjectRegistry,
        request: RenderProductProbeRequest,
    ) -> RenderProductProbeResult {
        let space = request.space.map_or(VisualSpace::TwoD, engine_visual_space);
        let policy = request
            .policy
            .map_or(ConsumerPolicy::AUTO, engine_consumer_policy);
        let texture_request = RenderTextureRequest {
            width: request.width,
            height: request.height,
            format: TextureStorageFormat::Rgba16Unorm,
            time_seconds: self.frame_time().total_ms as f32 / 1000.0,
            space,
            policy,
        };
        let revision = self.revision();
        let product = request.product;
        // Best-effort: a producer that cannot answer the space query still
        // renders (or fails) through the normal path below; `two_d()` is
        // the same "no opinion" fallback an undeclared shader answers with.
        let space_info = self
            .visual_product_space(registry, product)
            .unwrap_or_else(|_| ProductSpaceInfo::two_d());
        let (projection, origin) = projection_and_origin(space_info, space, policy);
        let primary = wire_visual_space(space_info.primary);
        match self.render_texture_product(registry, product, &texture_request) {
            Ok(texture) => {
                let Some(bytes) = texture.try_raw_bytes() else {
                    // GPU tier: the product stayed GPU-resident (no readback
                    // in the browser). Structured answer, not an error — the
                    // runtime is healthy, bytes are simply not available on
                    // this tier (fidelity-tiers ADR).
                    return RenderProductProbeResult::GpuResident {
                        product,
                        revision,
                        width: texture.width(),
                        height: texture.height(),
                        space: wire_visual_space(space),
                        projection,
                        origin,
                        primary,
                    };
                };
                let bytes = match request.format {
                    lpc_wire::WireTextureFormat::Rgba16 => bytes.to_vec(),
                    lpc_wire::WireTextureFormat::Srgb8 => rgba16_linear_to_srgb8(bytes),
                };
                RenderProductProbeResult::Texture {
                    product,
                    revision,
                    width: texture.width(),
                    height: texture.height(),
                    format: request.format,
                    bytes,
                    space: wire_visual_space(space),
                    projection,
                    origin,
                    primary,
                }
            }
            Err(error) => RenderProductProbeResult::Error {
                product,
                message: format!("{error}"),
            },
        }
    }

    /// Snapshot the effective binding graph and bus channel summary.
    ///
    /// Derives from the runtime binding index (the bus stays virtual):
    /// every registered binding — authored and default, including bindings
    /// on implicit runtime consumed slots with no def field — plus every
    /// referenced channel with providers/consumers as indices into the
    /// binding list. See docs/adr/2026-07-06-binding-graph-probe.md.
    ///
    /// Public (unlike the sibling probes) so host-level tests can assert
    /// the effective graph directly — the binding index is load-time
    /// materialized state with no other read surface.
    pub fn read_project_binding_graph_probe(
        &mut self,
        registry: &ProjectRegistry,
        request: BindingGraphProbeRequest,
    ) -> BindingGraphProbeResult {
        let revision = self.revision();

        let mut bindings = Vec::new();
        let mut wire_index: VecMap<BindingRef, u32> = VecMap::new();
        let mut scoped_rows = Vec::new();
        for (binding_ref, entry) in self.tree().bindings_with_refs() {
            wire_index.insert(binding_ref, bindings.len() as u32);
            bindings.push(wire_effective_binding(entry));
            scoped_rows.push((entry.owner, entry.clone()));
        }
        // Endpoint scopes need tree lookups, which borrow-conflict with the
        // iteration above — annotate in a second pass. The same pass stamps
        // the declared `panel = "show"` hint on Default-origin consuming
        // bindings (the additive override on the derived membership rule) —
        // authored bindings are public already, so the hint is only read
        // where it can change the answer.
        for (binding, (owner, entry)) in bindings.iter_mut().zip(scoped_rows.iter()) {
            if let lpc_wire::WireBindingEndpoint::Bus { scope, .. } = &mut binding.endpoint {
                *scope = match binding.direction {
                    lpc_wire::WireBindingDirection::Publishes => {
                        self.tree().node_scope(*owner).map(wire_scope_ref)
                    }
                    lpc_wire::WireBindingDirection::Consumes => {
                        self.tree().bus_read_scope(*owner).map(wire_scope_ref)
                    }
                };
            }
            if binding.origin == WireBindingOrigin::Default
                && binding.direction == lpc_wire::WireBindingDirection::Consumes
                && let Some(slot) = &binding.slot
            {
                binding.panel_show = self
                    .tree()
                    .get(binding.node)
                    .and_then(|entry| entry.def_location.as_ref())
                    .and_then(|location| {
                        super::engine::authored_def_slot_panel_hint(
                            registry,
                            self.slot_shapes(),
                            location,
                            slot,
                        )
                    })
                    .is_some();
            }
            let _ = entry;
        }

        // Channels list PER SCOPE (wire 8): same-named channels in
        // different scopes are distinct entries, and display strings are a
        // client concern. Sink scopes list too — a playlist entry's panel
        // control reads its live/engaged state off the entry's own
        // (scope, channel) row (panel.md §3, the per-entry held value) —
        // but presentation surfaces (the bus listing, binding pickers)
        // are expected to filter `scope.is_sink()` rows out: R2 keeps sink
        // channels invisible to enclosing READERS, which is a resolution
        // property, not a wire omission. Non-sink scopes list first so
        // name-keyed readers keep finding the enclosing row. A scope-less
        // tree (test harness without assigned scopes) falls back to the
        // flat listing.
        let scopes: Vec<Option<crate::node::ScopeRef>> = {
            let mut all: Vec<_> = self.tree().scopes();
            all.sort_by_key(|scope| scope.is_sink());
            if all.is_empty() {
                alloc::vec![None]
            } else {
                all.into_iter().map(Some).collect()
            }
        };
        let root_scope = self.tree().node_scope(self.tree().root());

        let mut channels = Vec::new();
        for scope in scopes {
            let scope_channels: Vec<(ChannelName, Kind)> = match scope {
                Some(scope) => self.tree().scope_channels(scope),
                None => self
                    .tree()
                    .bus_channels()
                    .map(|(name, kind)| (name.clone(), kind))
                    .collect(),
            };
            for (name, kind) in scope_channels {
                // An engaged panel writer surfaces as a provider row with
                // Panel origin — synthesized here (it lives in the side
                // store, not in any node's binding set) so the UI can
                // render engaged state from the same graph it already
                // reads.
                let panel_row = scope.and_then(|scope| {
                    self.panel_writers()
                        .get(scope, &name)
                        .map(|writer| WireEffectiveBinding {
                            owner: scope.owner(),
                            node: scope.owner(),
                            slot: None,
                            direction: WireBindingDirection::Publishes,
                            endpoint: WireBindingEndpoint::Literal {
                                value: writer.value.clone(),
                            },
                            origin: WireBindingOrigin::Panel,
                            panel_show: false,
                            priority: BindingPriority::panel().as_i32(),
                            kind,
                        })
                });
                let panel_index = panel_row.map(|row| {
                    let index = bindings.len() as u32;
                    bindings.push(row);
                    index
                });
                let mut providers = match scope {
                    Some(scope) => self.tree().providers_for_bus_in_scope(scope, &name),
                    None => self.tree().providers_for_bus(&name),
                };
                // Highest priority first; stable by binding ref within a
                // priority.
                providers.sort_by_key(|(binding_ref, entry)| {
                    (core::cmp::Reverse(entry.priority), *binding_ref)
                });
                let providers: Vec<u32> = panel_index
                    .into_iter()
                    .chain(
                        providers
                            .iter()
                            .filter_map(|(binding_ref, _)| wire_index.get(binding_ref).copied()),
                    )
                    .collect();
                let consumers = match scope {
                    Some(scope) => self.tree().consumers_for_bus_in_scope(scope, &name),
                    None => self.tree().consumers_for_bus(&name),
                }
                .iter()
                .filter_map(|(binding_ref, _)| wire_index.get(binding_ref).copied())
                .collect();

                let value = (request.include_values
                    && self.bus_probe_value_is_sink_demand_free(scope, &name))
                .then(
                    || match self.resolve_bus_channel_value(registry, scope, &name) {
                        Ok(production) => WireBusChannelValue {
                            revision,
                            value: production.value_leaf().map(|leaf| leaf.value().clone()),
                            error: None,
                        },
                        Err(error) => WireBusChannelValue {
                            revision,
                            value: None,
                            error: Some(format!("{error:?}")),
                        },
                    },
                );

                // Root-scope role, decided engine-side ONCE: the primary
                // visual is the root scope's listing of the vocabulary
                // channel the root module's mirror reads. The name
                // comparison lives only here, next to the vocabulary.
                let primary_visual = name.0 == lpc_model::PRIMARY_VISUAL_CHANNEL
                    && (scope == root_scope || root_scope.is_none());
                channels.push(WireBusChannel {
                    scope: scope.map(wire_scope_ref),
                    name: name.0.clone(),
                    kind: Some(kind),
                    providers,
                    consumers,
                    value,
                    primary_visual,
                });
            }
        }

        BindingGraphProbeResult::Graph(WireBindingGraph {
            revision,
            bindings,
            channels,
        })
    }

    /// Whether resolving `(scope, channel)`'s value from a probe stays free
    /// of sink-scope demand — the R2 no-demand property ("a probe pull must
    /// never render an inactive playlist entry"), kept at the value-request
    /// seam now that sink scopes list their channel rows.
    ///
    /// Mirrors the resolution walk (panel overlay included): the value is
    /// resolvable when the WINNING provider level is either a panel writer
    /// (a literal — no demand), a non-sink level (probe demand on enclosing
    /// producers is today's accepted behavior), all-literal sink providers,
    /// or nothing at all (a provider-less channel resolves without ticking
    /// anything). Only a sink level winning with a producer-backed source
    /// is refused — that value would have rendered the entry.
    fn bus_probe_value_is_sink_demand_free(
        &self,
        scope: Option<crate::node::ScopeRef>,
        channel: &ChannelName,
    ) -> bool {
        let Some(mut scope) = scope else {
            return true;
        };
        loop {
            if self.panel_writers().get(scope, channel).is_some() {
                return true;
            }
            let candidates = self.tree().providers_for_bus_in_scope(scope, channel);
            if !candidates.is_empty() {
                return !scope.is_sink()
                    || candidates
                        .iter()
                        .all(|(_, entry)| matches!(entry.source, BindingSource::Literal(_)));
            }
            match self.tree().parent_scope(scope) {
                Some(parent) => scope = parent,
                None => return true,
            }
        }
    }

    pub(super) fn read_project_control_product_probe(
        &mut self,
        registry: &ProjectRegistry,
        request: ControlProductProbeRequest,
    ) -> ControlProductProbeResult {
        let product = request.product;
        let extent = product.preferred_extent();
        let WireChannelSampleFormat::U16 = request.sample_format else {
            return ControlProductProbeResult::Unsupported {
                product,
                reason: format!(
                    "control product preview sample format {:?} is not supported",
                    request.sample_format
                ),
            };
        };
        let sample_count = extent.sample_count() as usize;
        let mut samples = vec![0u16; sample_count];
        let render_request = ControlRenderRequest::unorm16(extent);
        let target = ControlRenderTarget::new(
            extent,
            crate::products::control::ControlSampleFormat::Unorm16,
            samples.as_mut_slice(),
        );
        let revision = self.revision();
        match self.render_control_product_probe(
            registry,
            product,
            &render_request,
            target,
            request.display_layout,
        ) {
            Ok((sample_layout, display_layout)) => ControlProductProbeResult::Preview {
                product,
                revision,
                extent,
                sample_format: request.sample_format,
                sample_layout,
                display_layout,
                bytes: control_samples_u16_to_bytes(&samples),
            },
            Err(error) => ControlProductProbeResult::Error {
                product,
                message: format!("{error}"),
            },
        }
    }

    /// Read the frames every output node has ALREADY published.
    ///
    /// The cheap counterpart of
    /// [`read_project_control_product_probe`](Self::read_project_control_product_probe):
    /// that one re-renders the product inside the request path, which is
    /// exactly what a live card feed must not do. This walks the tree for
    /// nodes owning a sink buffer, clones each buffer's published bytes, and
    /// pairs them with the sample layout the publishing tick latched. The
    /// only non-trivial work is the optional display layout, which producers
    /// answer from cached mapping state (no resolve, no render).
    ///
    /// Cost is O(published bytes) plus O(lamps) per requested layout. The
    /// bytes clone is deliberate and accepted — it is one more copy of a
    /// buffer that already exists, where a render call would be a whole
    /// frame's work.
    pub fn read_project_output_frame_probe(
        &mut self,
        registry: &ProjectRegistry,
        request: OutputFrameProbeRequest,
    ) -> OutputFrameProbeResult {
        // Snapshot the candidates first: the display-layout pass below needs
        // `&mut self`, and the tree walk borrows it immutably.
        let candidates: Vec<PublishedOutputCandidate> = self
            .tree()
            .entries()
            .filter_map(|entry| {
                let NodeEntryState::Alive(node) = entry.state.value() else {
                    return None;
                };
                let buffer_id = node.runtime_output_sink_buffer_id()?;
                Some(PublishedOutputCandidate {
                    node: entry.id,
                    buffer_id,
                    sample_layout: node.runtime_output_sample_layout().cloned(),
                    source_product: node.runtime_output_source_product(),
                })
            })
            .collect();

        let mut outputs = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            let Some(buffer) = self.runtime_buffers().get(candidate.buffer_id) else {
                continue;
            };
            let RuntimeBufferMetadata::OutputChannels {
                channels,
                sample_format,
            } = buffer.value().metadata
            else {
                // A sink buffer that is not carrying output channels has not
                // published a frame yet (or is not one): skip it rather than
                // invent a shape for it.
                continue;
            };
            let revision = buffer.changed_at();
            let bytes = buffer.value().bytes.clone();

            let display_layout = if let ControlDisplayLayoutRead::None = request.display_layout {
                ControlDisplayLayoutProbeResult::Omitted
            } else if let Some(product) = candidate.source_product {
                self.control_display_layout_probe(registry, product, request.display_layout)
                    .unwrap_or_else(|error| ControlDisplayLayoutProbeResult::Unsupported {
                        reason: format!("{error}"),
                    })
            } else {
                ControlDisplayLayoutProbeResult::Unsupported {
                    reason: String::from(
                        "output has not published a frame yet, so its source product is unknown",
                    ),
                }
            };

            outputs.push(OutputFrameEntry {
                node: candidate.node,
                revision,
                channels,
                sample_format: match sample_format {
                    RuntimeChannelSampleFormat::U8 => WireChannelSampleFormat::U8,
                    RuntimeChannelSampleFormat::U16 => WireChannelSampleFormat::U16,
                },
                sample_layout: candidate.sample_layout.unwrap_or_default(),
                display_layout,
                bytes,
            });
        }

        OutputFrameProbeResult::Frame { outputs }
    }

    /// List the live phasors riding one clock's timebase (parent D10).
    ///
    /// A pure read of the timebase side store: the rows already exist
    /// because consumers queried them, and this walk neither materializes a
    /// phasor nor refreshes any despawn clock — a Studio card watching the
    /// listing cannot keep a dead integrator alive, and closing the card
    /// cannot kill a live one.
    ///
    /// `Unknown` (not an error) when the product names a clock with no
    /// published timebase: a card asking about a node that just left the
    /// tree, or one that has not produced yet, is not a fault.
    pub fn read_project_timebase_probe(
        &mut self,
        request: TimebaseProbeRequest,
    ) -> TimebaseProbeResult {
        let product = request.product;
        let revision = self.revision();
        let Some(entry) = self.timebases().entry(product.node()) else {
            return TimebaseProbeResult::Unknown { product };
        };
        let phasors = entry
            .phasors()
            .map(|(key, state)| WirePhasorRow {
                origin: wire_phasor_origin(key),
                phase: state.phase,
                cycle: state.cycle,
                period_seconds: state.period_seconds,
                readings: state
                    .readings()
                    .iter()
                    .map(|reading| lpc_wire::WirePhasorReading {
                        node: reading.node.0,
                        slot: alloc::string::ToString::to_string(&reading.slot),
                        waveform: reading.waveform,
                        phase_offset: reading.phase_offset,
                    })
                    .collect(),
            })
            .collect();
        TimebaseProbeResult::Timebase {
            product,
            revision,
            seconds: entry.effective_seconds,
            delta_seconds: entry.delta_seconds,
            phasors,
        }
    }
}

/// Project a phasor integrator's identity onto the wire.
///
/// The vocabulary shift is deliberate: the store's `Private`/`Shared` names
/// the *sharing* consequence, while the row names the *source* a reader can
/// go look at — a node's slot, or a channel. The consequence follows from
/// the source and does not need saying twice.
fn wire_phasor_origin(key: &crate::dataflow::timebase::PhasorKey) -> WirePhasorOrigin {
    match key {
        crate::dataflow::timebase::PhasorKey::Private { node, slot } => WirePhasorOrigin::Node {
            node: node.0,
            slot: alloc::string::ToString::to_string(slot),
        },
        crate::dataflow::timebase::PhasorKey::Shared { scope, channel } => {
            WirePhasorOrigin::Channel {
                scope: wire_scope_ref(*scope),
                channel: alloc::string::ToString::to_string(&channel.0),
            }
        }
    }
}

fn control_samples_u16_to_bytes(samples: &[u16]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

fn rgba16_linear_to_srgb8(bytes: &[u8]) -> alloc::vec::Vec<u8> {
    let mut out = alloc::vec::Vec::with_capacity(bytes.len() / 8 * 3);
    for px in bytes.chunks_exact(8) {
        out.push(linear_unorm16_to_srgb8(u16::from_le_bytes([px[0], px[1]])));
        out.push(linear_unorm16_to_srgb8(u16::from_le_bytes([px[2], px[3]])));
        out.push(linear_unorm16_to_srgb8(u16::from_le_bytes([px[4], px[5]])));
    }
    out
}

/// Encode one linear unorm16 sample as sRGB8 via the generated lookup
/// table — an index instead of a per-sample `libm::powf`, which dominated
/// on-device probe cost (thousands of calls per frame). Within 1 u8 LSB of
/// the exact float transfer (see [`srgb8_lut`](super::srgb8_lut) and the
/// exhaustive test below).
fn linear_unorm16_to_srgb8(value: u16) -> u8 {
    super::srgb8_lut::LINEAR16_TO_SRGB8[(value >> 4) as usize]
}

/// The render-product probe's `(projection, origin)` pair: `Some` exactly
/// when the request actually crossed a 1D→2D projection — a 2D request
/// against a 1D-primary producer. Every other combination (native space, or
/// the fixed 2D→1D centre scanline, which carries no [`CellProjection`]
/// choice) answers `(None, None)`.
fn projection_and_origin(
    space_info: ProductSpaceInfo,
    space: VisualSpace,
    policy: ConsumerPolicy,
) -> (Option<WireCellProjection>, Option<WireProjectionOrigin>) {
    if space_info.primary == VisualSpace::OneD && space == VisualSpace::TwoD {
        let (cell, origin) = resolve_1d_to_2d_with_origin(space_info, policy);
        (
            Some(wire_cell_projection(cell)),
            Some(wire_projection_origin(origin)),
        )
    } else {
        (None, None)
    }
}

fn wire_visual_space(space: VisualSpace) -> WireVisualSpace {
    match space {
        VisualSpace::OneD => WireVisualSpace::OneD,
        VisualSpace::TwoD => WireVisualSpace::TwoD,
    }
}

fn engine_visual_space(space: WireVisualSpace) -> VisualSpace {
    match space {
        WireVisualSpace::OneD => VisualSpace::OneD,
        WireVisualSpace::TwoD => VisualSpace::TwoD,
    }
}

fn wire_cell_projection(cell: CellProjection) -> WireCellProjection {
    match cell {
        CellProjection::Extrude => WireCellProjection::Extrude,
        CellProjection::Radial => WireCellProjection::Radial,
        CellProjection::Angular => WireCellProjection::Angular,
        CellProjection::Mirror => WireCellProjection::Mirror,
    }
}

fn engine_cell_projection(cell: WireCellProjection) -> CellProjection {
    match cell {
        WireCellProjection::Extrude => CellProjection::Extrude,
        WireCellProjection::Radial => CellProjection::Radial,
        WireCellProjection::Angular => CellProjection::Angular,
        WireCellProjection::Mirror => CellProjection::Mirror,
    }
}

fn wire_projection_origin(origin: ProjectionOrigin) -> WireProjectionOrigin {
    match origin {
        ProjectionOrigin::Declared => WireProjectionOrigin::Declared,
        ProjectionOrigin::ConsumerDefault => WireProjectionOrigin::ConsumerDefault,
        ProjectionOrigin::Forced => WireProjectionOrigin::Forced,
    }
}

fn engine_consumer_policy(policy: WireConsumerPolicy) -> ConsumerPolicy {
    ConsumerPolicy {
        default_1d_to_2d: engine_cell_projection(policy.default_1d_to_2d),
        force: policy.force,
    }
}

/// Project one runtime binding entry onto the wire.
///
/// The anchor is the binding's local slot: the consumed target when one
/// exists, otherwise the produced source feeding a bus channel. A binding
/// with no local slot (literal or bus-to-bus bridge publishing to a
/// channel) anchors to its owner with no slot path.
/// Project an engine scope into its wire form.
fn wire_scope_ref(scope: crate::node::ScopeRef) -> lpc_wire::WireScopeRef {
    match scope {
        crate::node::ScopeRef::Module { owner } => lpc_wire::WireScopeRef::Module { owner },
        crate::node::ScopeRef::Sink { owner, entry } => {
            lpc_wire::WireScopeRef::Sink { owner, entry }
        }
    }
}

fn wire_effective_binding(entry: &BindingEntry) -> WireEffectiveBinding {
    let origin = wire_binding_origin(entry.priority);
    let (node, slot, direction, endpoint) = match (&entry.source, &entry.target) {
        (source, BindingTarget::ConsumedSlot { node, slot }) => (
            *node,
            Some(slot.clone()),
            WireBindingDirection::Consumes,
            wire_endpoint_from_source(source),
        ),
        (BindingSource::ProducedSlot { node, slot }, BindingTarget::BusChannel(channel)) => (
            *node,
            Some(slot.clone()),
            WireBindingDirection::Publishes,
            WireBindingEndpoint::Bus {
                scope: None,
                channel: channel.0.clone(),
            },
        ),
        (
            BindingSource::Literal(_) | BindingSource::BusChannel(_),
            BindingTarget::BusChannel(channel),
        ) => (
            entry.owner,
            None,
            WireBindingDirection::Publishes,
            WireBindingEndpoint::Bus {
                scope: None,
                channel: channel.0.clone(),
            },
        ),
    };
    WireEffectiveBinding {
        owner: entry.owner,
        node,
        slot,
        direction,
        endpoint,
        origin,
        priority: entry.priority.as_i32(),
        kind: entry.kind,
        panel_show: false,
    }
}

/// Origin derives from priority until declarative default policy lands:
/// defaults register at fallback priority (roadmap M5, ADR).
fn wire_binding_origin(priority: BindingPriority) -> WireBindingOrigin {
    if priority == BindingPriority::default_fallback() {
        WireBindingOrigin::Default
    } else if priority == BindingPriority::panel() {
        WireBindingOrigin::Panel
    } else {
        WireBindingOrigin::Authored
    }
}

fn wire_endpoint_from_source(source: &BindingSource) -> WireBindingEndpoint {
    match source {
        BindingSource::Literal(value) => WireBindingEndpoint::Literal {
            value: value.clone(),
        },
        BindingSource::ProducedSlot { node, slot } => WireBindingEndpoint::NodeSlot {
            node: *node,
            slot: slot.clone(),
        },
        BindingSource::BusChannel(channel) => WireBindingEndpoint::Bus {
            scope: None,
            channel: channel.0.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use lpc_model::LpValue;

    use super::*;
    use crate::engine::test_support::{EngineTestBuilder, bus, output, produced_slot};

    /// The exact float transfer the LUT replaces (the pre-LUT
    /// implementation, kept as the test reference).
    fn linear_unorm16_to_srgb8_reference(value: u16) -> u8 {
        let linear = value as f32 / u16::MAX as f32;
        let srgb = if linear <= 0.003_130_8 {
            linear * 12.92
        } else {
            1.055 * libm::powf(linear, 1.0 / 2.4) - 0.055
        };
        (srgb.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
    }

    #[test]
    fn srgb8_lut_matches_float_reference_within_one_lsb() {
        for value in 0..=u16::MAX {
            let lut = linear_unorm16_to_srgb8(value);
            let reference = linear_unorm16_to_srgb8_reference(value);
            let error = (i16::from(lut) - i16::from(reference)).abs();
            assert!(
                error <= 1,
                "lut diverges at {value}: lut={lut} reference={reference}"
            );
        }
    }

    #[test]
    fn binding_graph_probe_reports_bindings_channels_and_values() {
        let mut h = EngineTestBuilder::new()
            .shader("writer", output("outputs[0]", 0.5))
            .bind_bus("video", produced_slot("writer", "outputs[0]"))
            .fixture("reader")
            .bind_demand_input("reader", bus("video"))
            .demand_root("reader")
            .build();
        h.tick(10).expect("tick");

        let result = h.engine.read_project_binding_graph_probe(
            &h.registry,
            BindingGraphProbeRequest {
                include_values: true,
            },
        );

        let BindingGraphProbeResult::Graph(graph) = result else {
            panic!("expected graph result");
        };
        assert_eq!(graph.bindings.len(), 2);
        assert_eq!(graph.channels.len(), 1);

        let channel = &graph.channels[0];
        assert_eq!(channel.name, "video");
        assert!(channel.kind.is_some());
        assert_eq!(channel.providers.len(), 1);
        assert_eq!(channel.consumers.len(), 1);

        let provider = &graph.bindings[channel.providers[0] as usize];
        assert_eq!(provider.node, h.node("writer"));
        assert_eq!(provider.direction, WireBindingDirection::Publishes);
        assert_eq!(provider.origin, WireBindingOrigin::Authored);
        assert!(matches!(
            &provider.endpoint,
            WireBindingEndpoint::Bus { channel, .. } if channel == "video"
        ));
        assert_eq!(
            provider.slot.as_ref(),
            Some(&lpc_model::SlotPath::parse("outputs[0]").expect("slot path"))
        );

        let consumer = &graph.bindings[channel.consumers[0] as usize];
        assert_eq!(consumer.node, h.node("reader"));
        assert_eq!(consumer.direction, WireBindingDirection::Consumes);
        assert!(matches!(
            &consumer.endpoint,
            WireBindingEndpoint::Bus { channel, .. } if channel == "video"
        ));

        let value = channel.value.as_ref().expect("value requested");
        assert_eq!(value.error, None);
        assert_eq!(value.value, Some(LpValue::F32(0.5)));
    }

    #[test]
    fn binding_graph_probe_orders_providers_by_priority_and_derives_origin() {
        let mut h = EngineTestBuilder::new()
            .shader("fallback", output("outputs[0]", 0.1))
            .shader("main", output("outputs[0]", 0.9))
            .bind_bus_with_priority(
                "video",
                produced_slot("fallback", "outputs[0]"),
                BindingPriority::default_fallback().as_i32(),
            )
            .expect("bind fallback")
            .bind_bus_with_priority("video", produced_slot("main", "outputs[0]"), 0)
            .expect("bind main")
            .build();
        h.tick(10).expect("tick");

        let result = h.engine.read_project_binding_graph_probe(
            &h.registry,
            BindingGraphProbeRequest {
                include_values: false,
            },
        );

        let BindingGraphProbeResult::Graph(graph) = result else {
            panic!("expected graph result");
        };
        let channel = &graph.channels[0];
        assert_eq!(channel.providers.len(), 2);
        assert!(channel.value.is_none());

        let first = &graph.bindings[channel.providers[0] as usize];
        let second = &graph.bindings[channel.providers[1] as usize];
        assert_eq!(first.node, h.node("main"));
        assert_eq!(first.origin, WireBindingOrigin::Authored);
        assert_eq!(second.node, h.node("fallback"));
        assert_eq!(second.origin, WireBindingOrigin::Default);
        assert_eq!(
            second.priority,
            BindingPriority::default_fallback().as_i32()
        );
    }
}
