//! Output demand root: resolves its control products, renders them into
//! output-owned samples, and exposes the dirty runtime buffer flushed by
//! [`crate::EngineServices`].
//!
//! # Output fragments
//!
//! An output consumes N control products, not one. Each becomes an
//! [`OutputFragment`] — a `(product, offset, len, reversed)` placement — and
//! is rendered into its OWN sub-slice of `control_samples`. With a single
//! producer (every checked-in example) the one fragment covers the whole
//! buffer and the path is byte-identical to the pre-fragment one; that
//! identity is pinned by `tests/output_control_samples_golden.rs`.
//!
//! Placement in this phase is **auto-flow**: fragments follow the resolver's
//! provider order, each starting where the previous one ended (D17v, the
//! map2d "object order is wiring order" rule scaled up to fixtures). A patch
//! file will author offsets later and plug into the same structure.
//!
//! Overlap and gaps are **degraded and reported**, never fatal: contested
//! samples go dark and [`OutputNode::runtime_status`] names the lamp range,
//! a gap warns, and the wire keeps being driven either way. A frame-killing
//! resolve error would take out an entire show over one mis-patched strand.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lpc_model::{
    LpValue, NodeRuntimeStatus, OutputDefView, ProductRef, Revision, SlotData, SlotPath,
    WithRevision,
};

use crate::dataflow::resolver::QueryKey;
use crate::node::{
    DestroyCtx, MemPressureCtx, NodeError, NodeResourceInitContext, NodeRuntime, PressureLevel,
    TickContext, err_ctx,
};
use crate::products::control::{
    ControlLayout, ControlProduct, ControlRenderRequest, ControlRenderTarget, ControlSampleFormat,
};
use crate::resource::{
    RuntimeBuffer, RuntimeBufferId, RuntimeBufferKind, RuntimeBufferMetadata,
    RuntimeChannelSampleFormat,
};

/// The color the `test_pattern` Debug slot paints.
///
/// 25% white on every channel (G2 decision): the slot answers "is this pin
/// wired to that strip?", which needs an unmistakable, graph-independent
/// color — but full white is the maximum-current draw on a long strip, so
/// the pattern stays deliberately dim. If power limits ever become
/// first-class settings, this is the constant a smarter policy replaces.
const TEST_PATTERN_RGB: [u8; 3] = [64, 64, 64];

/// Samples per RGB lamp, the unit both the buffer and the reports speak.
const SAMPLES_PER_LAMP: u32 = 3;

/// One producer's placement inside an output's control sample buffer.
///
/// The offset is in samples, flat: the buffer an output publishes is one
/// sequence, and the wire split (`OutputChannelDef`) reads it that way. A
/// producer's own extent may be multi-row; its samples land here in row
/// order, which is what `ControlExtent::sample_count` already means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutputFragment {
    /// The control product rendered into this range.
    pub product: ControlProduct,
    /// First sample of the range, in the output's buffer.
    pub offset_samples: u32,
    /// Length of the range, in samples.
    pub len_samples: u32,
    /// Render forward, then reverse the range's lamps in place.
    ///
    /// Distinct from `FixtureDef::wire_reversed`, which reverses a producer's
    /// own sampling *within* its product. This one is about PLACEMENT: the
    /// same product, laid into the output buffer end-first, which is what a
    /// strand plugged in at the far end needs.
    pub reversed: bool,
}

impl OutputFragment {
    /// The half-open sample range this fragment covers.
    #[must_use]
    pub const fn end_samples(&self) -> u32 {
        self.offset_samples.saturating_add(self.len_samples)
    }
}

/// What a fragment set covers, and where it disagrees with itself.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FragmentCoverage {
    /// One past the last sample any fragment claims.
    pub total_samples: u32,
    /// Sample ranges claimed by more than one fragment, merged and ordered.
    pub contested: Vec<(u32, u32)>,
    /// Sample ranges below `total_samples` that no fragment claims.
    pub gaps: Vec<(u32, u32)>,
}

/// Output node that owns the materialized control sample buffer.
pub struct OutputNode {
    channel_buffer_id: Option<RuntimeBufferId>,
    control_samples: Vec<u16>,
    def_view: Option<OutputDefView>,
    /// Last frame's fragment-placement complaint, or `None` when the set was
    /// clean. Keep-last-good, like the fixture's `input_error`: a frame that
    /// never got as far as planning fragments leaves the previous report
    /// standing rather than blinking it away.
    fragment_status: Option<NodeRuntimeStatus>,
    /// Interpretation metadata for the frame currently in the buffer,
    /// latched by the render that produced it.
    ///
    /// The published-frame read hands a client the buffer's bytes verbatim;
    /// these two fields are what make those bytes mean something without a
    /// second render. Both survive the test-pattern bypass on purpose — the
    /// pattern repaints an extent the graph already established, so the
    /// layout and the source product are still the frame's truth.
    published_sample_layout: Option<ControlLayout>,
    /// The FIRST fragment's product — the one whose display geometry a
    /// preview card draws. With one producer (the common case) it is simply
    /// "the" source; with several it names the strand at the head of the
    /// wire, and the per-fragment truth lives in `published_sample_layout`.
    published_source_product: Option<ControlProduct>,
}

impl OutputNode {
    #[must_use]
    pub fn new() -> Self {
        Self {
            channel_buffer_id: None,
            control_samples: Vec::new(),
            def_view: None,
            fragment_status: None,
            published_sample_layout: None,
            published_source_product: None,
        }
    }

    pub fn channel_buffer_id(&self) -> Option<RuntimeBufferId> {
        self.channel_buffer_id
    }

    fn def_view(&mut self, ctx: &TickContext<'_>) -> Result<&OutputDefView, NodeError> {
        OutputDefView::get_or_compile(&mut self.def_view, ctx.slot_shapes())
            .map_err(err_ctx("compile output def view"))
    }

    /// Read the `test_pattern` Debug slot from the EFFECTIVE def for this frame.
    ///
    /// Nothing is latched: the override lives in the node's slot data (overlay
    /// on top of the authored base), so the bypass switches on and off purely
    /// by what this read returns.
    ///
    /// "Absent" (an output attached with no project def behind it) reads as
    /// **off** and is logged, never fatal: this slot is a diagnostic, and a
    /// diagnostic must not be able to stop an output pushing pixels. A path
    /// that cannot exist in the `OutputDef` shape is a different thing — a
    /// code bug — and it still fails loudly, out of `def_view`, because
    /// compiling the view is exactly that shape check.
    fn test_pattern_active(&mut self, ctx: &mut TickContext<'_>) -> Result<bool, NodeError> {
        let reader = self.def_view(ctx)?.test_pattern();
        match reader.get(ctx) {
            Ok(active) => Ok(active),
            Err(error) => {
                log::debug!("[output] test_pattern unavailable: {error}");
                Ok(false)
            }
        }
    }

    /// Overwrite every established sample with one color, in place.
    ///
    /// Keeps the LAST-ESTABLISHED sample count: a test pattern never resizes
    /// the channel extent, it only repaints it, so the flush path sees exactly
    /// the buffer shape the real render produced.
    fn fill_solid(&mut self, rgb: [u8; 3]) {
        // 8-bit unorm to 16-bit unorm: 0..=255 maps onto 0..=65535 exactly.
        let rgb16 = [
            u16::from(rgb[0]) * 257,
            u16::from(rgb[1]) * 257,
            u16::from(rgb[2]) * 257,
        ];
        for (index, sample) in self.control_samples.iter_mut().enumerate() {
            *sample = rgb16[index % 3];
        }
    }

    /// Render a planned fragment set into `control_samples` and latch the
    /// frame's interpretation metadata.
    ///
    /// The buffer is sized to the set's extent, every fragment renders into
    /// its own sub-slice, and only then are gaps and contested ranges zeroed —
    /// "degrade AFTER rendering the rest" is what keeps a mis-patched strand
    /// from darkening the strands beside it.
    ///
    /// Separate from [`Self::consume`] so tests can drive placements that
    /// auto-flow cannot yet produce (reversal, overlap, gaps): those arrive
    /// with the patch file, and the engine must already be right when they do.
    fn render_fragments(
        &mut self,
        ctx: &mut TickContext<'_>,
        fragments: &[OutputFragment],
    ) -> Result<(), NodeError> {
        let coverage = fragment_coverage(fragments);
        self.fragment_status = coverage_status(&coverage);

        self.control_samples
            .resize(coverage.total_samples as usize, 0);

        let mut spans = Vec::new();
        for fragment in fragments {
            let start = fragment.offset_samples as usize;
            let end = fragment.end_samples() as usize;
            let Some(target_samples) = self.control_samples.get_mut(start..end) else {
                // Unreachable while the buffer is sized from the same
                // coverage; a fragment that cannot be placed is skipped
                // rather than allowed to panic mid-frame.
                continue;
            };
            let extent = fragment.product.preferred_extent();
            let request = ControlRenderRequest::unorm16(extent);
            let target =
                ControlRenderTarget::new(extent, ControlSampleFormat::Unorm16, target_samples);
            let layout = ctx.render_control(fragment.product, &request, target)?;
            if fragment.reversed
                && let Some(placed) = self.control_samples.get_mut(start..end)
            {
                reverse_lamps(placed);
            }
            for mut span in layout.spans {
                span.start = span.start.saturating_add(fragment.offset_samples);
                spans.push(span);
            }
        }

        for (start, end) in coverage.gaps.iter().chain(coverage.contested.iter()) {
            let range = (*start as usize)..(*end as usize);
            if let Some(samples) = self.control_samples.get_mut(range) {
                samples.fill(0);
            }
        }

        self.published_sample_layout = Some(ControlLayout { spans });
        self.published_source_product = fragments.first().map(|fragment| fragment.product);

        self.publish_channel_buffer(ctx)
    }

    /// Copy `control_samples` into the runtime buffer and mark it dirty for this frame.
    ///
    /// Shared by the render path and the test-pattern bypass so both publish
    /// byte-identical buffer kind, metadata, and revision.
    fn publish_channel_buffer(&self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        let buffer_id = self
            .channel_buffer_id
            .ok_or_else(|| NodeError::msg("output channel buffer not initialized"))?;
        ctx.with_runtime_buffer_mut(buffer_id, ctx.revision(), |buffer| {
            buffer.kind = RuntimeBufferKind::OutputChannels;
            buffer.metadata = RuntimeBufferMetadata::OutputChannels {
                channels: (self.control_samples.len() / 3) as u32,
                sample_format: RuntimeChannelSampleFormat::U16,
            };
            buffer
                .bytes
                .resize(self.control_samples.len().saturating_mul(2), 0);
            for (chunk, sample) in buffer
                .bytes
                .chunks_exact_mut(2)
                .zip(self.control_samples.iter())
            {
                chunk.copy_from_slice(&sample.to_le_bytes());
            }
            Ok(())
        })
    }
}

pub fn output_input_path() -> SlotPath {
    SlotPath::parse("input").expect("output input path")
}

/// The control products behind a resolved output `input`, in fragment order.
///
/// Two shapes arrive here and both are legitimate: a `fragments`-merged
/// receiver gets an index-keyed map (the resolver's provider order, one entry
/// per producer), and a single-binding route still gets a bare leaf. The map
/// is index-keyed and `VecMap` is key-sorted, so iteration IS the order.
fn control_products(data: &SlotData) -> Result<Vec<ControlProduct>, NodeError> {
    match data {
        SlotData::Value(value) => Ok(Vec::from([control_product(value.get())?])),
        SlotData::Map(map) => map
            .entries
            .values()
            .map(|entry| match entry {
                SlotData::Value(value) => control_product(value.get()),
                _ => Err(NodeError::msg(
                    "output input fragment resolved to aggregate data, expected control product",
                )),
            })
            .collect(),
        _ => Err(NodeError::msg(
            "output input resolved to aggregate data, expected control product",
        )),
    }
}

fn control_product(value: &LpValue) -> Result<ControlProduct, NodeError> {
    match value {
        LpValue::Product(ProductRef::Control(product)) => Ok(*product),
        _ => Err(NodeError::msg("output expected control product from input")),
    }
}

/// Lay products end to end in provider order — this phase's whole placement
/// policy (D17v).
///
/// Provider order is the resolver's, and the resolver's is deterministic:
/// binding priority first, then binding ref — which is owner node id, which
/// is the order the loader attached the nodes, which is the order the module
/// lists them (its `nodes` map is key-sorted). So "the second fixture starts
/// where the first one ended" is a property of the project document, not of a
/// hash iteration; `tests/output_fragments.rs` pins it.
fn auto_flow_fragments(products: &[ControlProduct]) -> Vec<OutputFragment> {
    let mut offset = 0u32;
    let mut fragments = Vec::with_capacity(products.len());
    for product in products {
        let len = product.preferred_extent().sample_count();
        fragments.push(OutputFragment {
            product: *product,
            offset_samples: offset,
            len_samples: len,
            reversed: false,
        });
        offset = offset.saturating_add(len);
    }
    fragments
}

/// What a fragment set covers: its extent, its overlaps, and its holes.
///
/// Auto-flow cannot produce either defect — it is a running sum — so this is
/// the check that earns its keep the moment offsets become authored. Ranges
/// come back merged and ordered so a report reads as "34–41", not as a list
/// of pairwise collisions.
fn fragment_coverage(fragments: &[OutputFragment]) -> FragmentCoverage {
    let total_samples = fragments
        .iter()
        .map(OutputFragment::end_samples)
        .max()
        .unwrap_or(0);

    let mut claimed: Vec<(u32, u32)> = fragments
        .iter()
        .filter(|fragment| fragment.len_samples > 0)
        .map(|fragment| (fragment.offset_samples, fragment.end_samples()))
        .collect();
    claimed.sort_unstable();

    let mut contested = Vec::new();
    for (index, (start, end)) in claimed.iter().enumerate() {
        for (other_start, other_end) in claimed.iter().skip(index + 1) {
            let overlap_start = *start.max(other_start);
            let overlap_end = *end.min(other_end);
            if overlap_start < overlap_end {
                contested.push((overlap_start, overlap_end));
            }
        }
    }
    contested.sort_unstable();

    let mut gaps = Vec::new();
    let mut covered_to = 0u32;
    for (start, end) in claimed.iter() {
        if *start > covered_to {
            gaps.push((covered_to, *start));
        }
        covered_to = covered_to.max(*end);
    }

    FragmentCoverage {
        total_samples,
        contested: merge_ranges(contested),
        gaps,
    }
}

/// Coalesce sorted, possibly overlapping ranges into disjoint ones.
fn merge_ranges(ranges: Vec<(u32, u32)>) -> Vec<(u32, u32)> {
    let mut merged: Vec<(u32, u32)> = Vec::with_capacity(ranges.len());
    for (start, end) in ranges {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    merged
}

/// Turn a coverage report into the node status a client shows.
///
/// Contested samples are an `Error` — pixels the project asked two producers
/// for are pixels nobody can trust, and the author has to choose. A gap is a
/// `Warn`: dark lamps in the middle of a wire are usually a patch in progress,
/// not a broken show. Neither ever stops the frame.
fn coverage_status(coverage: &FragmentCoverage) -> Option<NodeRuntimeStatus> {
    if !coverage.contested.is_empty() {
        return Some(NodeRuntimeStatus::Error(format!(
            "lamps {} contested by more than one producer; those lamps are dark",
            lamp_ranges(&coverage.contested)
        )));
    }
    if !coverage.gaps.is_empty() {
        return Some(NodeRuntimeStatus::Warn(format!(
            "lamps {} are driven by no producer and stay dark",
            lamp_ranges(&coverage.gaps)
        )));
    }
    None
}

/// Sample ranges as inclusive lamp ranges — the unit an author counts in.
fn lamp_ranges(ranges: &[(u32, u32)]) -> String {
    let mut text = String::new();
    for (index, (start, end)) in ranges.iter().enumerate() {
        if index > 0 {
            text.push_str(", ");
        }
        let first = start / SAMPLES_PER_LAMP;
        // Inclusive last lamp: a range ending mid-lamp still darkens that lamp.
        let last = end.saturating_sub(1) / SAMPLES_PER_LAMP;
        if first == last {
            text.push_str(&format!("{first}"));
        } else {
            text.push_str(&format!("{first}-{last}"));
        }
    }
    text
}

/// Reverse a placed range's lamps in place, keeping each lamp's channels in
/// their own order.
///
/// A trailing partial lamp (a range whose length is not a multiple of three)
/// stays put: reversing it would move channels between lamps, which is a
/// bigger lie than leaving the odd tail alone.
fn reverse_lamps(samples: &mut [u16]) {
    let lamps = samples.len() / SAMPLES_PER_LAMP as usize;
    for index in 0..lamps / 2 {
        let left = index * SAMPLES_PER_LAMP as usize;
        let right = (lamps - 1 - index) * SAMPLES_PER_LAMP as usize;
        for channel in 0..SAMPLES_PER_LAMP as usize {
            samples.swap(left + channel, right + channel);
        }
    }
}

impl NodeRuntime for OutputNode {
    fn init_resources(&mut self, ctx: &mut NodeResourceInitContext<'_>) -> Result<(), NodeError> {
        if self.channel_buffer_id.is_some() {
            return Ok(());
        }
        let id = ctx.insert_runtime_buffer(WithRevision::new(
            Revision::default(),
            RuntimeBuffer::output_channels_u16(0, Vec::new()),
        ));
        self.channel_buffer_id = Some(id);
        Ok(())
    }

    fn runtime_output_sink_buffer_id(&self) -> Option<RuntimeBufferId> {
        self.channel_buffer_id
    }

    fn runtime_output_sample_layout(&self) -> Option<&ControlLayout> {
        self.published_sample_layout.as_ref()
    }

    fn runtime_output_source_product(&self) -> Option<ControlProduct> {
        self.published_source_product
    }

    fn consume(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        // Bypass, not overwrite: while the Debug slot is on, the graph resolve
        // is skipped entirely, so upstream demand from this root stops. Other
        // outputs are unaffected. The frame the slot goes back to false we fall
        // straight through to the normal resolve below — no black frame.
        //
        // A pattern only ever REPAINTS an extent the graph already established;
        // before the first rendered frame there is nothing to repaint, so that
        // frame renders the graph and establishes it.
        if self.test_pattern_active(ctx)? && !self.control_samples.is_empty() {
            self.fill_solid(TEST_PATTERN_RGB);
            return self.publish_channel_buffer(ctx);
        }

        let prod = ctx
            .resolve(&QueryKey::ConsumedSlot {
                node: ctx.node_id(),
                slot: output_input_path(),
            })
            .map_err(|e| NodeError::msg(format!("resolve output input: {}", e.message)))?;

        let products = control_products(prod.data())?;
        let fragments = auto_flow_fragments(&products);
        self.render_fragments(ctx, &fragments)
    }

    fn runtime_status(&self) -> Option<NodeRuntimeStatus> {
        self.fragment_status.clone()
    }

    fn destroy(&mut self, _ctx: &mut DestroyCtx) -> Result<(), NodeError> {
        Ok(())
    }

    fn handle_memory_pressure(
        &mut self,
        _level: PressureLevel,
        _ctx: &mut MemPressureCtx,
    ) -> Result<(), NodeError> {
        // Nothing droppable here, deliberately. This handler used to clear
        // `control_samples` at `High` (the #303 compile window). Measurement
        // on 2026-08-04 showed the output's own `produce` resizes that buffer
        // back to the control extent EARLIER in the same tick than the shader
        // compile runs (compiles happen at render time), so the drop frees
        // nothing at the compile instant and only re-does the allocation.
        // Removed in M6 P4; see
        // `docs/defects/2026-08-04-compile-window-drops-rebuilt-before-compile.md`
        // and the 2026-08-04 amendment to
        // `docs/adr/2026-08-03-memory-pressure-at-compile-safe-points.md`.
        // The runtime buffer this node publishes into was never touched here
        // anyway — its lifecycle belongs to the sink registration path.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use alloc::string::{String, ToString};
    use alloc::vec;

    use lpc_model::{
        ControlExtent, ControlProduct, ControlSampleLayout, LpValue, NodeId, ProductRef,
        SlotShapeRegistry,
    };

    use crate::dataflow::resolver::{Production, ProductionSource, ResolveError, TickResolver};
    use crate::products::visual::{RenderTextureRequest, TextureRenderProduct, VisualProduct};
    use crate::resource::RuntimeBuffer;

    const BUFFER: RuntimeBufferId = RuntimeBufferId::new(1);
    /// One RGB lamp: the smallest extent that still exercises the triplet layout.
    const ONE_LAMP: ControlExtent = ControlExtent::new(1, 3);
    /// The white the bypass paints, in the u16 unorm units the buffer carries.
    /// The pattern color as the 16-bit unorm samples `fill_solid` publishes,
    /// derived from the const so the tests track a retune automatically.
    const PATTERN16: u16 = TEST_PATTERN_RGB[0] as u16 * 257;

    fn node_id() -> NodeId {
        NodeId::new(1)
    }

    /// Minimal [`TickResolver`] standing in for the engine's session bridge.
    ///
    /// It counts resolves of the output's `input` slot — that is how "the graph
    /// was consulted" is asserted, never inferred — and paints whatever
    /// `graph_color` currently is, so a color change that does NOT reach the
    /// buffer proves the bypass. `test_pattern` stands in for the effective def:
    /// the field the Studio's Debug section writes.
    struct FakeResolver {
        buffer: RuntimeBuffer,
        buffer_frame: Revision,
        /// Resolves of the output's `input` slot: the graph edge the bypass skips.
        input_resolve_calls: u32,
        /// Calls into the render path: the work the bypass skips.
        render_control_calls: u32,
        /// Effective value of the `test_pattern` Debug slot, read every frame.
        test_pattern: bool,
        /// When false the Debug slot has no def behind it and fails to resolve,
        /// as it does for an output attached outside a loaded project.
        test_pattern_resolvable: bool,
        graph_extent: ControlExtent,
        graph_color: [u16; 3],
        /// The products the `input` slot answers with, as a fragment map —
        /// the shape a `merge = "fragments"` route produces. Empty means the
        /// single-leaf answer built from `graph_extent`.
        graph_products: Vec<ControlProduct>,
        /// Paint each sample as `product.output() * 100 + index` instead of
        /// `graph_color`, so a fragment's identity AND its internal order are
        /// both legible in the published buffer.
        paint_by_product: bool,
    }

    impl FakeResolver {
        fn new() -> Self {
            Self {
                buffer: RuntimeBuffer::output_channels_u16(0, Vec::new()),
                buffer_frame: Revision::default(),
                input_resolve_calls: 0,
                render_control_calls: 0,
                test_pattern: false,
                test_pattern_resolvable: true,
                graph_extent: ONE_LAMP,
                graph_color: [1000, 2000, 3000],
                graph_products: Vec::new(),
                paint_by_product: false,
            }
        }

        /// The published buffer decoded back into u16 samples.
        fn published_samples(&self) -> Vec<u16> {
            self.buffer
                .bytes
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect()
        }
    }

    impl TickResolver for FakeResolver {
        fn resolve(&mut self, query: &QueryKey) -> Result<Production, ResolveError> {
            let path = query
                .consumed_slot_path()
                .ok_or_else(|| ResolveError::new(String::from("unexpected query kind")))?;
            match path.to_string().as_str() {
                "test_pattern" if self.test_pattern_resolvable => Ok(Production::leaf(
                    WithRevision::new(Revision::new(1), LpValue::Bool(self.test_pattern)),
                    ProductionSource::Literal,
                )),
                "test_pattern" => Err(ResolveError::new(String::from(
                    "unresolved consumed slot test_pattern",
                ))),
                "input" => {
                    self.input_resolve_calls += 1;
                    if self.graph_products.is_empty() {
                        return Ok(Production::leaf(
                            WithRevision::new(
                                Revision::new(1),
                                LpValue::Product(ProductRef::Control(ControlProduct::new(
                                    node_id(),
                                    0,
                                    self.graph_extent,
                                ))),
                            ),
                            ProductionSource::Literal,
                        ));
                    }
                    let mut entries = lp_collection::VecMap::new();
                    for (index, product) in self.graph_products.iter().enumerate() {
                        entries.insert(
                            lpc_model::SlotMapKey::U32(index as u32),
                            lpc_model::SlotData::Value(WithRevision::new(
                                Revision::new(1),
                                LpValue::Product(ProductRef::Control(*product)),
                            )),
                        );
                    }
                    Ok(Production::new(
                        lpc_model::SlotData::Map(lpc_model::SlotMapDyn::with_revision(
                            Revision::new(1),
                            entries,
                        )),
                        ProductionSource::Merged,
                    ))
                }
                other => Err(ResolveError::new(alloc::format!(
                    "fake resolver has no slot {other}"
                ))),
            }
        }

        fn resolve_static_consumed(
            &mut self,
            node: NodeId,
            path: &'static str,
        ) -> Result<Production, ResolveError> {
            let slot = SlotPath::parse(path)
                .map_err(|e| ResolveError::new(alloc::format!("bad static path: {e}")))?;
            self.resolve(&QueryKey::ConsumedSlot { node, slot })
        }

        fn publish_produced_slot(
            &mut self,
            _node: NodeId,
            _slot: SlotPath,
            _production: Production,
        ) -> Result<(), ResolveError> {
            Ok(())
        }

        fn render_texture(
            &mut self,
            _product: VisualProduct,
            _request: &RenderTextureRequest,
        ) -> Result<TextureRenderProduct, ResolveError> {
            Err(ResolveError::new(String::from(
                "fake resolver renders no textures",
            )))
        }

        fn render_control(
            &mut self,
            product: ControlProduct,
            _request: &ControlRenderRequest,
            target: ControlRenderTarget<'_>,
        ) -> Result<ControlSampleLayout, ResolveError> {
            self.render_control_calls += 1;
            for (index, sample) in target.samples.iter_mut().enumerate() {
                *sample = if self.paint_by_product {
                    (product.output() as u16) * 100 + index as u16
                } else {
                    self.graph_color[index % 3]
                };
            }
            // One span covering the whole fragment, in the fragment's OWN
            // coordinates — the output is what rebases it.
            Ok(ControlSampleLayout {
                spans: vec![lpc_model::ControlSampleSpan {
                    row: 0,
                    start: 0,
                    len: target.samples.len() as u32,
                    encoding: lpc_model::ControlSampleEncoding::Raw,
                }],
            })
        }

        fn runtime_buffer_mut(
            &mut self,
            id: RuntimeBufferId,
            frame: Revision,
        ) -> Result<&mut RuntimeBuffer, ResolveError> {
            if id != BUFFER {
                return Err(ResolveError::new(String::from("unknown runtime buffer")));
            }
            self.buffer_frame = frame;
            Ok(&mut self.buffer)
        }
    }

    /// An output whose buffer id is already assigned (stands in for `init_resources`).
    fn output_node() -> OutputNode {
        let mut node = OutputNode::new();
        node.channel_buffer_id = Some(BUFFER);
        node
    }

    /// Run one `consume` at `frame`, as the engine's tick would.
    fn consume_at(
        node: &mut OutputNode,
        resolver: &mut FakeResolver,
        frame: Revision,
    ) -> Result<(), NodeError> {
        let shapes = SlotShapeRegistry::default();
        let mut ctx = TickContext::new(node_id(), frame, resolver, &shapes);
        node.consume(&mut ctx)
    }

    #[test]
    fn a_graph_frame_establishes_the_channel_extent() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();

        consume_at(&mut node, &mut resolver, Revision::new(1)).expect("graph frame");

        assert_eq!(resolver.published_samples(), vec![1000, 2000, 3000]);
        assert_eq!(resolver.input_resolve_calls, 1);
        assert_eq!(resolver.render_control_calls, 1);
    }

    #[test]
    fn the_test_pattern_bypasses_the_graph_at_the_established_sample_count() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        consume_at(&mut node, &mut resolver, Revision::new(1)).expect("graph frame");

        resolver.test_pattern = true;
        // A graph color change from here on must NOT reach the buffer.
        resolver.graph_color = [4000, 5000, 6000];
        let resolves_before = resolver.input_resolve_calls;
        let renders_before = resolver.render_control_calls;

        consume_at(&mut node, &mut resolver, Revision::new(2)).expect("pattern frame");

        assert_eq!(
            resolver.published_samples(),
            vec![PATTERN16, PATTERN16, PATTERN16],
            "buffer should hold the solid test color as u16 unorm triplets",
        );
        assert_eq!(
            resolver.input_resolve_calls, resolves_before,
            "the graph resolve must be skipped entirely while the pattern is active",
        );
        assert_eq!(
            resolver.render_control_calls, renders_before,
            "the render path must be skipped entirely while the pattern is active",
        );
        assert_eq!(
            resolver.buffer.metadata,
            RuntimeBufferMetadata::OutputChannels {
                channels: 1,
                sample_format: RuntimeChannelSampleFormat::U16,
            },
        );
        assert_eq!(resolver.buffer.kind, RuntimeBufferKind::OutputChannels);
        assert_eq!(
            resolver.buffer_frame,
            Revision::new(2),
            "the bypass must mark the buffer dirty for this frame, like the render path",
        );
    }

    #[test]
    fn the_test_pattern_holds_across_frames_and_repaints_every_one() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        consume_at(&mut node, &mut resolver, Revision::new(1)).expect("graph frame");

        resolver.test_pattern = true;
        for frame in 2..=4i64 {
            consume_at(&mut node, &mut resolver, Revision::new(frame)).expect("pattern frame");
            assert_eq!(
                resolver.buffer_frame,
                Revision::new(frame),
                "every held frame republishes, so the flush path stays fed",
            );
            assert_eq!(
                resolver.published_samples(),
                vec![PATTERN16, PATTERN16, PATTERN16]
            );
        }

        assert_eq!(
            resolver.input_resolve_calls, 1,
            "only the establishing graph frame ever resolved the input",
        );
        assert_eq!(resolver.render_control_calls, 1);
    }

    #[test]
    fn clearing_the_test_pattern_renders_the_graph_again_on_the_same_frame() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        consume_at(&mut node, &mut resolver, Revision::new(1)).expect("graph frame");
        resolver.test_pattern = true;
        consume_at(&mut node, &mut resolver, Revision::new(2)).expect("pattern frame");

        resolver.graph_color = [4000, 5000, 6000];
        resolver.test_pattern = false;
        consume_at(&mut node, &mut resolver, Revision::new(3)).expect("post-clear frame");

        assert_eq!(
            resolver.published_samples(),
            vec![4000, 5000, 6000],
            "no black frame: the graph renders the very frame the override ends",
        );
        assert_eq!(resolver.input_resolve_calls, 2);
    }

    #[test]
    fn the_def_bool_is_read_every_frame_not_latched() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        consume_at(&mut node, &mut resolver, Revision::new(1)).expect("graph frame");

        // on -> off -> on, with nothing but the effective def changing.
        resolver.test_pattern = true;
        consume_at(&mut node, &mut resolver, Revision::new(2)).expect("pattern frame");
        assert_eq!(
            resolver.published_samples(),
            vec![PATTERN16, PATTERN16, PATTERN16]
        );

        resolver.test_pattern = false;
        consume_at(&mut node, &mut resolver, Revision::new(3)).expect("graph frame");
        assert_eq!(resolver.published_samples(), vec![1000, 2000, 3000]);

        resolver.test_pattern = true;
        consume_at(&mut node, &mut resolver, Revision::new(4)).expect("pattern frame");
        assert_eq!(
            resolver.published_samples(),
            vec![PATTERN16, PATTERN16, PATTERN16]
        );

        assert_eq!(
            resolver.input_resolve_calls, 2,
            "exactly the two non-pattern frames consulted the graph",
        );
    }

    #[test]
    fn the_test_pattern_never_resizes_the_channel_extent() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        resolver.graph_extent = ControlExtent::new(2, 3);
        consume_at(&mut node, &mut resolver, Revision::new(1)).expect("graph frame");

        resolver.test_pattern = true;
        consume_at(&mut node, &mut resolver, Revision::new(2)).expect("pattern frame");

        assert_eq!(
            resolver.published_samples(),
            vec![PATTERN16; 6],
            "the pattern repaints the last-established extent, it does not resize it",
        );
        assert_eq!(
            resolver.buffer.metadata,
            RuntimeBufferMetadata::OutputChannels {
                channels: 2,
                sample_format: RuntimeChannelSampleFormat::U16,
            },
        );
    }

    #[test]
    fn the_bypass_publishes_the_same_buffer_shape_as_the_render_path() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        consume_at(&mut node, &mut resolver, Revision::new(1)).expect("graph frame");
        let graph_kind = resolver.buffer.kind.clone();
        let graph_metadata = resolver.buffer.metadata.clone();
        let graph_len = resolver.buffer.bytes.len();

        resolver.test_pattern = true;
        consume_at(&mut node, &mut resolver, Revision::new(2)).expect("pattern frame");

        assert_eq!(resolver.buffer.kind, graph_kind);
        assert_eq!(resolver.buffer.metadata, graph_metadata);
        assert_eq!(resolver.buffer.bytes.len(), graph_len);
    }

    #[test]
    fn the_test_pattern_before_any_frame_falls_through_to_the_graph() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        resolver.test_pattern = true;

        consume_at(&mut node, &mut resolver, Revision::new(1)).expect("first frame");

        assert_eq!(
            resolver.input_resolve_calls, 1,
            "with no established extent there is nothing to repaint, so the graph runs",
        );
        assert_eq!(resolver.published_samples(), vec![1000, 2000, 3000]);
    }

    #[test]
    fn an_unreadable_debug_slot_never_stops_the_output() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        // No project def behind the node — exactly the engine-level harness
        // case, and any future one where an output outlives its def.
        resolver.test_pattern_resolvable = false;

        consume_at(&mut node, &mut resolver, Revision::new(1)).expect("graph frame");
        consume_at(&mut node, &mut resolver, Revision::new(2)).expect("graph frame");

        assert_eq!(
            resolver.published_samples(),
            vec![1000, 2000, 3000],
            "an unreadable diagnostic reads as off; the output keeps pushing pixels",
        );
        assert_eq!(resolver.input_resolve_calls, 2);
    }

    /// The compile-window broadcast drops NOTHING here (M6 P4). The #303
    /// handler cleared `control_samples`; measurement showed the output's own
    /// `produce` resizes it back earlier in the same tick than the compile
    /// runs, so the drop freed nothing at the compile instant. See
    /// `docs/defects/2026-08-04-compile-window-drops-rebuilt-before-compile.md`.
    #[test]
    fn memory_pressure_does_not_drop_the_control_samples() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        consume_at(&mut node, &mut resolver, Revision::new(1)).expect("graph frame");
        let samples_before = node.control_samples.clone();
        assert!(
            !samples_before.is_empty(),
            "an empty baseline would prove nothing"
        );

        for level in [
            PressureLevel::Low,
            PressureLevel::Medium,
            PressureLevel::High,
            PressureLevel::Critical,
        ] {
            let mut ctx = MemPressureCtx::new(node_id(), Revision::new(2));
            node.handle_memory_pressure(level, &mut ctx)
                .expect("handle pressure");
        }

        assert_eq!(
            node.control_samples, samples_before,
            "control_samples must survive a pressure broadcast"
        );
    }

    #[test]
    fn the_output_still_accepts_no_runtime_commands() {
        let mut node = output_node();

        // The Debug slot rides the overlay, not the command channel: nothing
        // here is wire state, and WIRE_PROTO_VERSION is untouched.
        assert!(
            node.handle_command(
                &lpc_wire::WireNodeCommand::PlaylistActivateEntry { entry: 1 },
                0.0,
            )
            .is_err(),
        );
    }

    // ---- Output fragments ---------------------------------------------

    /// Two lamps' worth of samples: the smallest fragment that can be
    /// reversed and still say something.
    const TWO_LAMPS: ControlExtent = ControlExtent::new(1, 6);

    fn product(output: u32, extent: ControlExtent) -> ControlProduct {
        ControlProduct::new(node_id(), output, extent)
    }

    /// Drive a hand-built placement, bypassing auto-flow — the seam the patch
    /// file will author into.
    fn render_fragments_at(
        node: &mut OutputNode,
        resolver: &mut FakeResolver,
        frame: Revision,
        fragments: &[OutputFragment],
    ) -> Result<(), NodeError> {
        let shapes = SlotShapeRegistry::default();
        let mut ctx = TickContext::new(node_id(), frame, resolver, &shapes);
        node.render_fragments(&mut ctx, fragments)
    }

    #[test]
    fn auto_flow_lays_products_end_to_end_in_provider_order() {
        let fragments = auto_flow_fragments(&[
            product(1, TWO_LAMPS),
            product(2, ONE_LAMP),
            product(3, TWO_LAMPS),
        ]);

        assert_eq!(
            fragments
                .iter()
                .map(|f| (
                    f.product.output(),
                    f.offset_samples,
                    f.len_samples,
                    f.reversed
                ))
                .collect::<Vec<_>>(),
            vec![(1, 0, 6, false), (2, 6, 3, false), (3, 9, 6, false)],
            "each fragment starts where the previous one ended, forward",
        );
    }

    /// The order is the *input* order, not a sort of anything the fragments
    /// carry: swap the providers and the offsets swap with them. This is what
    /// makes "the second fixture in the document is the second strand on the
    /// wire" a property of the project text.
    #[test]
    fn auto_flow_offsets_follow_provider_order_not_product_identity() {
        let forward = auto_flow_fragments(&[product(1, TWO_LAMPS), product(2, ONE_LAMP)]);
        let reversed_order = auto_flow_fragments(&[product(2, ONE_LAMP), product(1, TWO_LAMPS)]);

        assert_eq!(forward[0].product.output(), 1);
        assert_eq!(forward[0].offset_samples, 0);
        assert_eq!(reversed_order[0].product.output(), 2);
        assert_eq!(reversed_order[0].offset_samples, 0);
        assert_eq!(reversed_order[1].offset_samples, 3);
    }

    /// The A1 claim at the node's own grain: one producer still renders as one
    /// whole-buffer render, with the same bytes and the same call count.
    #[test]
    fn a_single_fragment_renders_the_whole_buffer_exactly_as_before() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        resolver.graph_extent = TWO_LAMPS;

        consume_at(&mut node, &mut resolver, Revision::new(1)).expect("graph frame");

        assert_eq!(
            resolver.published_samples(),
            vec![1000, 2000, 3000, 1000, 2000, 3000]
        );
        assert_eq!(resolver.render_control_calls, 1);
        assert_eq!(node.runtime_status(), None, "a clean single fragment is Ok");
    }

    #[test]
    fn two_producers_concatenate_into_one_buffer_in_order() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        resolver.paint_by_product = true;
        resolver.graph_products = vec![product(1, TWO_LAMPS), product(2, ONE_LAMP)];

        consume_at(&mut node, &mut resolver, Revision::new(1)).expect("graph frame");

        assert_eq!(
            resolver.published_samples(),
            vec![100, 101, 102, 103, 104, 105, 200, 201, 202],
            "product 1's six samples, then product 2's three",
        );
        assert_eq!(
            resolver.render_control_calls, 2,
            "one render per producer, into disjoint sub-slices",
        );
        assert_eq!(node.runtime_status(), None);
    }

    /// The published layout is the concatenation, rebased: a client reading
    /// span 2 must find it where the samples actually are.
    #[test]
    fn the_published_layout_rebases_every_fragments_spans() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        resolver.graph_products = vec![product(1, TWO_LAMPS), product(2, ONE_LAMP)];

        consume_at(&mut node, &mut resolver, Revision::new(1)).expect("graph frame");

        let layout = node
            .runtime_output_sample_layout()
            .expect("published layout");
        assert_eq!(
            layout
                .spans
                .iter()
                .map(|span| (span.start, span.len))
                .collect::<Vec<_>>(),
            vec![(0, 6), (6, 3)],
        );
        assert_eq!(
            node.runtime_output_source_product().map(|p| p.output()),
            Some(1),
            "the head of the wire is the frame's source product",
        );
    }

    #[test]
    fn a_reversed_fragment_renders_forward_then_flips_its_lamps() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        resolver.paint_by_product = true;

        render_fragments_at(
            &mut node,
            &mut resolver,
            Revision::new(1),
            &[OutputFragment {
                product: product(1, TWO_LAMPS),
                offset_samples: 0,
                len_samples: 6,
                reversed: true,
            }],
        )
        .expect("render");

        assert_eq!(
            resolver.published_samples(),
            vec![103, 104, 105, 100, 101, 102],
            "lamps swap, channels within a lamp do not",
        );
    }

    /// Both directions, side by side in one buffer: the same product placed
    /// forward and reversed must differ only in lamp order.
    #[test]
    fn forward_and_reversed_fragments_coexist_in_one_buffer() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        resolver.paint_by_product = true;

        render_fragments_at(
            &mut node,
            &mut resolver,
            Revision::new(1),
            &[
                OutputFragment {
                    product: product(1, TWO_LAMPS),
                    offset_samples: 0,
                    len_samples: 6,
                    reversed: false,
                },
                OutputFragment {
                    product: product(2, TWO_LAMPS),
                    offset_samples: 6,
                    len_samples: 6,
                    reversed: true,
                },
            ],
        )
        .expect("render");

        assert_eq!(
            resolver.published_samples(),
            vec![100, 101, 102, 103, 104, 105, 203, 204, 205, 200, 201, 202],
        );
        assert_eq!(node.runtime_status(), None);
    }

    #[test]
    fn overlapping_fragments_darken_only_the_contested_lamps_and_report() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        resolver.paint_by_product = true;

        render_fragments_at(
            &mut node,
            &mut resolver,
            Revision::new(1),
            &[
                OutputFragment {
                    product: product(1, ControlExtent::new(1, 9)),
                    offset_samples: 0,
                    len_samples: 9,
                    reversed: false,
                },
                // Starts one lamp before the first one ends.
                OutputFragment {
                    product: product(2, TWO_LAMPS),
                    offset_samples: 6,
                    len_samples: 6,
                    reversed: false,
                },
            ],
        )
        .expect("render");

        assert_eq!(
            resolver.published_samples(),
            vec![100, 101, 102, 103, 104, 105, 0, 0, 0, 203, 204, 205],
            "lamp 2 is claimed twice and goes dark; the lamps around it render",
        );
        let Some(NodeRuntimeStatus::Error(message)) = node.runtime_status() else {
            panic!(
                "a contested output reports an Error, got {:?}",
                node.runtime_status()
            );
        };
        assert!(message.contains("lamps 2"), "{message}");
        assert!(message.contains("contested"), "{message}");
    }

    #[test]
    fn a_gap_between_fragments_stays_dark_and_warns() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        resolver.paint_by_product = true;

        render_fragments_at(
            &mut node,
            &mut resolver,
            Revision::new(1),
            &[
                OutputFragment {
                    product: product(1, ONE_LAMP),
                    offset_samples: 0,
                    len_samples: 3,
                    reversed: false,
                },
                OutputFragment {
                    product: product(2, ONE_LAMP),
                    offset_samples: 9,
                    len_samples: 3,
                    reversed: false,
                },
            ],
        )
        .expect("render");

        assert_eq!(
            resolver.published_samples(),
            vec![100, 101, 102, 0, 0, 0, 0, 0, 0, 200, 201, 202],
        );
        let Some(NodeRuntimeStatus::Warn(message)) = node.runtime_status() else {
            panic!("a gapped output warns, got {:?}", node.runtime_status());
        };
        assert!(message.contains("lamps 1-2"), "{message}");
    }

    /// A gap must not keep last frame's pixels: the samples under it were
    /// written by an earlier placement and have to be cleared every frame.
    #[test]
    fn a_gap_clears_whatever_a_previous_frame_left_under_it() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        resolver.paint_by_product = true;
        let covered = [OutputFragment {
            product: product(1, ControlExtent::new(1, 12)),
            offset_samples: 0,
            len_samples: 12,
            reversed: false,
        }];
        render_fragments_at(&mut node, &mut resolver, Revision::new(1), &covered)
            .expect("full frame");

        render_fragments_at(
            &mut node,
            &mut resolver,
            Revision::new(2),
            &[
                OutputFragment {
                    product: product(1, ONE_LAMP),
                    offset_samples: 0,
                    len_samples: 3,
                    reversed: false,
                },
                OutputFragment {
                    product: product(2, ONE_LAMP),
                    offset_samples: 9,
                    len_samples: 3,
                    reversed: false,
                },
            ],
        )
        .expect("gapped frame");

        assert_eq!(
            resolver.published_samples(),
            vec![100, 101, 102, 0, 0, 0, 0, 0, 0, 200, 201, 202],
        );
    }

    /// Degrade-and-report, never a frame kill: the buffer is still published
    /// and still marked dirty on the frame the overlap exists.
    #[test]
    fn a_contested_output_still_publishes_its_frame() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();

        render_fragments_at(
            &mut node,
            &mut resolver,
            Revision::new(7),
            &[
                OutputFragment {
                    product: product(1, ONE_LAMP),
                    offset_samples: 0,
                    len_samples: 3,
                    reversed: false,
                },
                OutputFragment {
                    product: product(2, ONE_LAMP),
                    offset_samples: 0,
                    len_samples: 3,
                    reversed: false,
                },
            ],
        )
        .expect("a contested placement is not an error");

        assert_eq!(resolver.buffer_frame, Revision::new(7));
        assert_eq!(
            resolver.buffer.metadata,
            RuntimeBufferMetadata::OutputChannels {
                channels: 1,
                sample_format: RuntimeChannelSampleFormat::U16,
            },
        );
    }

    /// The status is per-frame truth once a plan exists: fixing the patch
    /// clears the error on the very next frame.
    #[test]
    fn a_cleared_overlap_clears_the_status() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        let overlapping = [
            OutputFragment {
                product: product(1, ONE_LAMP),
                offset_samples: 0,
                len_samples: 3,
                reversed: false,
            },
            OutputFragment {
                product: product(2, ONE_LAMP),
                offset_samples: 0,
                len_samples: 3,
                reversed: false,
            },
        ];
        render_fragments_at(&mut node, &mut resolver, Revision::new(1), &overlapping)
            .expect("contested frame");
        assert!(node.runtime_status().is_some());

        render_fragments_at(
            &mut node,
            &mut resolver,
            Revision::new(2),
            &auto_flow_fragments(&[product(1, ONE_LAMP), product(2, ONE_LAMP)]),
        )
        .expect("clean frame");

        assert_eq!(node.runtime_status(), None);
    }

    #[test]
    fn coverage_merges_overlaps_and_finds_holes() {
        let coverage = fragment_coverage(&[
            OutputFragment {
                product: product(1, ONE_LAMP),
                offset_samples: 0,
                len_samples: 6,
                reversed: false,
            },
            OutputFragment {
                product: product(2, ONE_LAMP),
                offset_samples: 3,
                len_samples: 6,
                reversed: false,
            },
            OutputFragment {
                product: product(3, ONE_LAMP),
                offset_samples: 6,
                len_samples: 6,
                reversed: false,
            },
            OutputFragment {
                product: product(4, ONE_LAMP),
                offset_samples: 15,
                len_samples: 3,
                reversed: false,
            },
        ]);

        assert_eq!(coverage.total_samples, 18);
        assert_eq!(
            coverage.contested,
            vec![(3, 9)],
            "two separate pairwise overlaps that touch merge into one range",
        );
        assert_eq!(coverage.gaps, vec![(12, 15)]);
    }

    #[test]
    fn auto_flow_coverage_is_always_clean() {
        let coverage = fragment_coverage(&auto_flow_fragments(&[
            product(1, TWO_LAMPS),
            product(2, ONE_LAMP),
            product(3, TWO_LAMPS),
        ]));

        assert!(coverage.contested.is_empty());
        assert!(coverage.gaps.is_empty());
        assert_eq!(coverage.total_samples, 15);
        assert_eq!(coverage_status(&coverage), None);
    }

    #[test]
    fn reverse_lamps_keeps_channel_order_within_each_lamp() {
        let mut samples = vec![1, 2, 3, 4, 5, 6, 7, 8, 9];
        reverse_lamps(&mut samples);
        assert_eq!(samples, vec![7, 8, 9, 4, 5, 6, 1, 2, 3]);

        // A trailing partial lamp is left where it is rather than shredded.
        let mut ragged = vec![1, 2, 3, 4, 5, 6, 7];
        reverse_lamps(&mut ragged);
        assert_eq!(ragged, vec![4, 5, 6, 1, 2, 3, 7]);
    }

    /// A resolved input that is neither a leaf nor a fragment map is a graph
    /// bug, and it says so rather than rendering something arbitrary.
    #[test]
    fn a_non_product_fragment_is_a_node_error() {
        let map =
            lpc_model::SlotData::Map(lpc_model::SlotMapDyn::with_revision(Revision::new(1), {
                let mut entries = lp_collection::VecMap::new();
                entries.insert(
                    lpc_model::SlotMapKey::U32(0),
                    lpc_model::SlotData::Value(WithRevision::new(
                        Revision::new(1),
                        LpValue::F32(1.0),
                    )),
                );
                entries
            }));

        assert!(control_products(&map).is_err());
    }
}
