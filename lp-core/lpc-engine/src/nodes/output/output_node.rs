//! Output demand root: resolves its control products, renders them into
//! output-owned samples, and exposes the dirty runtime buffer flushed by
//! [`crate::EngineServices`].
//!
//! # Output fragments
//!
//! An output consumes N control products, not one. Each becomes one or more
//! [`OutputFragment`]s — a `(product, source offset, offset, len, reversed)`
//! placement — rendered into its OWN sub-slice of `control_samples`. With a
//! single unpatched producer (every checked-in example) the one fragment
//! covers the whole buffer and the path is byte-identical to the pre-fragment
//! one; that identity is pinned by
//! `tests/output_control_samples_golden.rs`.
//!
//! Placement default is **auto-flow**: fragments follow the resolver's
//! provider order, each starting where the previous one ended (D17v, the
//! map2d "object order is wiring order" rule scaled up to fixtures). A
//! producer that authored a **patch** replaces its own auto-flow placement
//! with the runs that patch resolved to — several ranges of its lamps, at
//! authored wire offsets, forward or reversed — while everything unpatched
//! keeps flowing, after every anchor. See [`plan_fragments`].
//!
//! Overlap and gaps are **degraded and reported**, never fatal: contested
//! samples go dark and [`OutputNode::runtime_status`] names the lamp range,
//! a gap warns, and the wire keeps being driven either way. A frame-killing
//! resolve error would take out an entire show over one mis-patched strand.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lpc_model::nodes::output::chase;
use lpc_model::{
    ColorOrder, ControlLamp2d, ControlLayout2d, ControlPathSpan2d, LpValue, NodeRuntimeStatus,
    OutputDefView, ProductRef, Revision, SlotData, SlotPath, WithRevision,
};

use crate::dataflow::resolver::QueryKey;
use crate::node::{
    DestroyCtx, MemPressureCtx, NodeError, NodeResourceInitContext, NodeRuntime, PatchedRun,
    PressureLevel, TickContext, err_ctx,
};
use crate::products::control::{
    ControlHint, ControlLayout, ControlProduct, ControlRenderRequest, ControlRenderTarget,
    ControlSampleFormat, ControlSpan,
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

/// Full period of the `highlight` breath, in seconds.
///
/// The highlight FADES — a raised cosine between a dim floor and a bright
/// crest — rather than blinking: a hard on/off square reads as distracting
/// flashing on the piece (G1 feedback), where lp2014's field-proven
/// selection breathed (`BaseOutputDevice.applyFading`: `64 + timeCos(750ms)
/// × 192`, never fully dark). 750 ms keeps it findable at a glance without
/// reading as show content.
const HIGHLIGHT_BREATH_SECONDS: f32 = 0.75;

/// The breath's floor and crest, 16-bit unorm per channel (white).
///
/// Contrast on ANY background comes from the dim-the-rest pass (see
/// [`OutputNode::paint_highlight`]): everything else drops to 25%, so full
/// white content dims to 16383 — the crest (96 → 24672) rises clearly above
/// it and the floor (16 → 4112) dips clearly below mid content, and the
/// MOTION of the breath does the rest. Peak current stays bounded: the
/// crest is 37.5% white on the selection alone while the rest of the strip
/// sits at quarter power — less total draw than the old half-duty blink.
const HIGHLIGHT_FLOOR_16: u16 = 16 * 257;
const HIGHLIGHT_CREST_16: u16 = 96 * 257;

/// The breath's level at `time_seconds`: a raised cosine from
/// [`HIGHLIGHT_FLOOR_16`] (at phase 0) to [`HIGHLIGHT_CREST_16`] (half
/// period), never zero — a selection must never read as dead lamps.
fn highlight_level_16(time_seconds: f32) -> u16 {
    let phase =
        0.5 - 0.5 * libm::cosf(core::f32::consts::TAU * time_seconds / HIGHLIGHT_BREATH_SECONDS);
    let span = f32::from(HIGHLIGHT_CREST_16 - HIGHLIGHT_FLOOR_16);
    HIGHLIGHT_FLOOR_16 + libm::roundf(span * phase.clamp(0.0, 1.0)) as u16
}

// The chase's numbers — head/tail hues, head sizing, sweep period, body
// window — live in `lpc_model::nodes::output::chase`, NOT here. The studio
// controller paints the same language for objects that have no wire yet
// (an unmapped selection publishes no bytes to carry a chase), and two
// copies of these constants would let the panel drift from the wall. This
// module owns only the WIRE side: parsing the `chase:` microformat and
// laying the language onto an output's sample buffer.

/// Pack an RGB triple into the channel order a run is stored in.
///
/// Producers render already-ordered samples (`fixture_node::ordered_rgb_u16`)
/// and declare the order in their span encoding; a diagnostic painting a
/// SPECIFIC color has to speak the same order or blue comes out green.
fn ordered_rgb_16(color_order: ColorOrder, [r, g, b]: [u16; 3]) -> [u16; 3] {
    match color_order {
        ColorOrder::Rgb => [r, g, b],
        ColorOrder::Grb => [g, r, b],
        ColorOrder::Rbg => [r, b, g],
        ColorOrder::Gbr => [g, b, r],
        ColorOrder::Brg => [b, r, g],
        ColorOrder::Bgr => [b, g, r],
    }
}

/// Samples per RGB lamp, the unit both the buffer and the reports speak.
const SAMPLES_PER_LAMP: u32 = 3;

/// One producer's placement inside an output's control sample buffer.
///
/// The offset is in samples, flat: the buffer an output publishes is one
/// sequence, and the wire split (`OutputPortDef`) reads it that way. A
/// producer's own extent may be multi-row; its samples land here in row
/// order, which is what `ControlExtent::sample_count` already means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutputFragment {
    /// The control product rendered into this range.
    pub product: ControlProduct,
    /// First sample OF THE PRODUCT this range takes.
    ///
    /// Zero for every auto-flowed producer — a fixture with no patch
    /// contributes its whole product as one run. A patched fixture splits
    /// into several runs of its own lamps, and this is which run.
    pub source_offset_samples: u32,
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
    /// Rotate the placed lamps within the fragment's window, in SAMPLES
    /// (always a whole number of lamps). Applied after `reversed` — the
    /// kernel's canonical order (`lpc_mapping::patched_wire_lamp`): source
    /// lamp `j` lands at window slot `(j' + k) mod N`. Rotation permutes
    /// *within* the window, so coverage/collision math is untouched.
    pub rotation_samples: u32,
}

impl OutputFragment {
    /// The half-open sample range this fragment covers.
    #[must_use]
    pub const fn end_samples(&self) -> u32 {
        self.offset_samples.saturating_add(self.len_samples)
    }

    /// Does this fragment take the producer's whole product?
    ///
    /// The yes case renders straight into the output buffer, which is the
    /// path every unpatched project takes and the one whose bytes the golden
    /// oracle pins. A partial run cannot: rendering materializes a whole
    /// product, so a slice of one has to come out of a scratch buffer.
    #[must_use]
    fn covers_whole_product(&self) -> bool {
        self.source_offset_samples == 0
            && self.len_samples == self.product.preferred_extent().sample_count()
    }
}

/// One producer as the placement planner sees it: the product plus the patch
/// it resolved to, if it has one.
#[derive(Clone, Debug, PartialEq)]
pub struct FragmentPlacement {
    pub product: ControlProduct,
    /// The producer's resolved patch, in LAMPS. `None` — the ordinary case —
    /// means "auto-flow me".
    pub patch: Option<Vec<PatchedRun>>,
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
    /// Sample count of the frame last published to the channel buffer — the
    /// "established extent" a test pattern repaints. The samples themselves
    /// live in the runtime buffer (their one home, 6 B/lamp); the node takes
    /// that storage for the frame it renders and hands it back at publish.
    established_samples: usize,
    /// The frame the channel buffer was last published at, so a render that
    /// fails midway can hand the storage back without marking the buffer
    /// changed — the wire keeps its last good frame.
    published_at: Revision,
    def_view: Option<OutputDefView>,
    /// Last frame's fragment-placement complaint, or `None` when the set was
    /// clean. Keep-last-good, like the fixture's `input_error`: a frame that
    /// never got as far as planning fragments leaves the previous report
    /// standing rather than blinking it away.
    fragment_status: Option<NodeRuntimeStatus>,
    /// A name collision with a live sibling output (D39): duplicate names
    /// make `at.output` ambiguous, so the engine keeps routing by exact
    /// match and BOTH outputs wear this error until one is renamed.
    identity_status: Option<NodeRuntimeStatus>,
    /// Interpretation metadata for the frame currently in the buffer,
    /// latched by the render that produced it.
    ///
    /// The published-frame read hands a client the buffer's bytes verbatim;
    /// these two fields are what make those bytes mean something without a
    /// second render. Both survive the test-pattern bypass on purpose — the
    /// pattern repaints an extent the graph already established, so the
    /// layout and the placement set are still the frame's truth.
    published_sample_layout: Option<ControlLayout>,
    /// The placement set the frame in the buffer was rendered from.
    ///
    /// This is what makes the frame's GEOMETRY recoverable. A producer's
    /// display layout is stated in its own lamp numbering, which stops being
    /// the wire's numbering the moment a second producer joins or a patch
    /// moves a run: only the fragment that placed it knows where those lamps
    /// ended up. The published-frame read rebases each producer's layout
    /// through its fragments (see [`merge_fragment_display_layouts`]), so
    /// EVERY fixture's lamps appear, at their own wire positions, wearing
    /// their own colors.
    published_fragments: Vec<OutputFragment>,
    /// Revision stamped the last time [`Self::published_fragments`] CHANGED.
    ///
    /// The display-layout read is revision-gated, and a producer's own layout
    /// revision only moves when its mapping or render extent does — a patch
    /// edit moves neither. Without this the wire could be re-cut underneath a
    /// client that would never be told to re-fetch the geometry. Follows the
    /// fixture's `FixtureDisplayLayoutKey` idiom: stamp on change, not per
    /// frame.
    placement_revision: Revision,
}

impl OutputNode {
    #[must_use]
    pub fn new() -> Self {
        Self {
            channel_buffer_id: None,
            established_samples: 0,
            published_at: Revision::default(),
            def_view: None,
            fragment_status: None,
            identity_status: None,
            published_sample_layout: None,
            published_fragments: Vec::new(),
            placement_revision: Revision::default(),
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

    /// The authored `name` slot, read through the effective def like
    /// `test_pattern`. "Absent" (unset option, no def behind a bare
    /// runtime output) quietly reads as `None` — an unnamed output is the
    /// ordinary single-output state, not a fault.
    fn authored_name(&mut self, ctx: &mut TickContext<'_>) -> Option<String> {
        match ctx.resolve_static_consumed("name.some") {
            Ok(production) => production
                .value_leaf()
                .and_then(|leaf| {
                    <lpc_model::OutputName as lpc_model::FromLpValue>::from_lp_value(leaf.value())
                        .ok()
                })
                .map(|name| String::from(name.as_str())),
            Err(error) => {
                log::debug!("[output] name unavailable: {}", error.message);
                None
            }
        }
    }

    /// Read the `highlight` Debug slot's value for this frame.
    ///
    /// Same tolerance contract as [`Self::test_pattern_active`]: an
    /// unresolvable slot reads as "no highlight" and is logged, never fatal.
    /// The microformat parse is equally forgiving — see [`parse_highlight`].
    fn highlight_value(&mut self, ctx: &mut TickContext<'_>) -> Result<Highlight, NodeError> {
        let reader = self.def_view(ctx)?.highlight();
        match reader.get::<_, String>(ctx) {
            Ok(text) => Ok(parse_highlight(&text)),
            Err(error) => {
                log::debug!("[output] highlight unavailable: {error}");
                Ok(Highlight::default())
            }
        }
    }

    /// Paint the highlight over whatever frame is in the buffer, in the mode
    /// the slot named.
    ///
    /// Both modes are overlays, not bypasses, and both dim the rest of the
    /// wire first. Chase falls back to the breath when the output's sample
    /// layout cannot say which channel is which (A2) — a mis-decoded chase
    /// would name the wrong end of the strand, which is worse than the
    /// direction-free language.
    fn paint_highlight(&self, samples: &mut [u16], highlight: &Highlight, time_seconds: f32) {
        match highlight {
            Highlight::Breath(spans) => Self::paint_breath(samples, spans, time_seconds),
            Highlight::Chase(spans) => {
                if !self.paint_chase(samples, spans, time_seconds) {
                    Self::paint_breath(samples, &breath_spans(spans), time_seconds);
                }
            }
        }
    }

    /// Paint the breathing-white highlight over the named spans.
    ///
    /// An overlay, not a bypass, in two moves (lp2014's selection language):
    /// first EVERY established sample dims to a quarter — the background
    /// recedes so the selection never has to shout — then the named lamps
    /// take the breathing white ([`highlight_level_16`]). Lamps past the
    /// established extent are clipped, not an error: a selection can outlive
    /// a shrinking wire by a frame, and a diagnostic never kills output.
    fn paint_breath(samples: &mut [u16], spans: &[(u32, u32)], time_seconds: f32) {
        if spans.is_empty() {
            return;
        }
        for sample in samples.iter_mut() {
            *sample >>= 2;
        }
        let level = highlight_level_16(time_seconds);
        for (start, count) in spans {
            let first = lamps_to_samples(*start) as usize;
            let last = lamps_to_samples(start.saturating_add(*count)) as usize;
            let end = last.min(samples.len());
            if first >= end {
                continue;
            }
            for sample in samples[first..end].iter_mut() {
                *sample = level;
            }
        }
    }

    /// Paint the chase over the named spans, in OBJECT order.
    ///
    /// Same dim-the-rest first move as the breath, then every named lamp
    /// takes its object-order color from the shared language
    /// ([`chase::lamp_rgb_16`]): blue head, red tail, and a white dot
    /// sweeping head-to-tail between them. A reversed span's
    /// wire indices are walked backward, so object order stays continuous
    /// across a strand plugged in at the far end — which is exactly the
    /// mis-wiring the language exists to show.
    ///
    /// Returns `false` and paints NOTHING when the output's declared sample
    /// layout does not resolve every named in-extent lamp to an RGB run
    /// (A2): the caller then falls back to the breath.
    fn paint_chase(&self, samples: &mut [u16], spans: &[ChaseSpan], time_seconds: f32) -> bool {
        let total = chase_lamp_count(spans);
        if total == 0 {
            return false;
        }
        let Some(layout) = self.published_sample_layout.as_ref() else {
            return false;
        };
        let established = samples.len();

        // Resolve first, paint second: a half-decoded chase is a lie about
        // the wire, so one unresolvable lamp sends the whole output to the
        // breath rather than painting part of the answer.
        let mut lookup = ChannelOrders::new(layout);
        for wire in chase_wire_lamps(spans) {
            let first = lamps_to_samples(wire) as usize;
            if first.saturating_add(SAMPLES_PER_LAMP as usize) > established {
                continue;
            }
            if lookup.order_at(first as u32).is_none() {
                return false;
            }
        }

        for sample in samples.iter_mut() {
            *sample >>= 2;
        }
        let phase = chase::phase_at(time_seconds);
        let mut lookup = ChannelOrders::new(layout);
        for (ordinal, wire) in chase_wire_lamps(spans).enumerate() {
            let first = lamps_to_samples(wire) as usize;
            let end = first.saturating_add(SAMPLES_PER_LAMP as usize);
            if end > established {
                continue;
            }
            let rgb = chase::lamp_rgb_16(ordinal as u32, total, phase);
            let Some(order) = lookup.order_at(first as u32) else {
                continue;
            };
            let ordered = ordered_rgb_16(order, rgb);
            samples[first..end].copy_from_slice(&ordered);
        }
        true
    }

    /// Overwrite every established sample with one color, in place.
    ///
    /// Keeps the LAST-ESTABLISHED sample count: a test pattern never resizes
    /// the channel extent, it only repaints it, so the flush path sees exactly
    /// the buffer shape the real render produced.
    fn fill_solid(samples: &mut [u16], rgb: [u8; 3]) {
        // 8-bit unorm to 16-bit unorm: 0..=255 maps onto 0..=65535 exactly.
        let rgb16 = [
            u16::from(rgb[0]) * 257,
            u16::from(rgb[1]) * 257,
            u16::from(rgb[2]) * 257,
        ];
        for (index, sample) in samples.iter_mut().enumerate() {
            *sample = rgb16[index % 3];
        }
    }

    /// Take the channel buffer's sample storage for this frame.
    ///
    /// The runtime buffer is the samples' one home; rendering into a
    /// node-owned copy and publishing it would be a second 6 B/lamp buffer
    /// and a copy per frame. Taking marks the buffer changed for this frame
    /// — every caller either publishes in the same tick or restores through
    /// [`Self::restore_channel_samples`].
    fn take_channel_samples(&self, ctx: &mut TickContext<'_>) -> Result<Vec<u16>, NodeError> {
        let buffer_id = self
            .channel_buffer_id
            .ok_or_else(|| NodeError::msg("output channel buffer not initialized"))?;
        let mut taken = Vec::new();
        ctx.with_runtime_buffer_mut(buffer_id, ctx.revision(), |buffer| {
            // A buffer that is not (yet) a sample buffer yields nothing; the
            // render sizes the empty Vec and the publish stores it as samples.
            taken = buffer.take_samples16().unwrap_or_default();
            Ok(())
        })?;
        Ok(taken)
    }

    /// Hand the storage back after a render that failed, marked at the frame
    /// it was last published: the flush sees no change and the wire keeps its
    /// last good frame rather than a half-rendered one.
    fn restore_channel_samples(&self, ctx: &mut TickContext<'_>, samples: Vec<u16>) {
        let Some(buffer_id) = self.channel_buffer_id else {
            return;
        };
        let _ = ctx.with_runtime_buffer_mut(buffer_id, self.published_at, |buffer| {
            buffer.set_samples16(samples);
            Ok(())
        });
    }

    /// Take, render, publish — the whole frame for a planned fragment set.
    ///
    /// Separate from [`Self::consume`] so tests can drive placements that
    /// auto-flow cannot yet produce (reversal, overlap, gaps): those arrive
    /// with the patch file, and the engine must already be right when they do.
    #[cfg(test)]
    fn render_fragments(
        &mut self,
        ctx: &mut TickContext<'_>,
        fragments: &[OutputFragment],
    ) -> Result<(), NodeError> {
        let mut samples = self.take_channel_samples(ctx)?;
        if let Err(error) = self.render_fragments_into(ctx, fragments, &mut samples) {
            self.restore_channel_samples(ctx, samples);
            return Err(error);
        }
        self.publish_channel_buffer(ctx, samples)
    }

    /// Render a planned fragment set into `samples` and latch the frame's
    /// interpretation metadata.
    ///
    /// The buffer is sized to the set's extent, every fragment renders into
    /// its own sub-slice, and only then are gaps and contested ranges zeroed —
    /// "degrade AFTER rendering the rest" is what keeps a mis-patched strand
    /// from darkening the strands beside it.
    ///
    /// The samples are the channel buffer's own storage, taken for the frame
    /// by [`Self::take_channel_samples`].
    fn render_fragments_into(
        &mut self,
        ctx: &mut TickContext<'_>,
        fragments: &[OutputFragment],
        samples: &mut Vec<u16>,
    ) -> Result<(), NodeError> {
        let coverage = fragment_coverage(fragments);
        self.fragment_status = coverage_status(&coverage);

        samples.resize(coverage.total_samples as usize, 0);

        let mut spans = Vec::new();
        let mut scratch = ProductScratch::default();
        for fragment in fragments {
            let start = fragment.offset_samples as usize;
            let end = fragment.end_samples() as usize;
            if samples.get(start..end).is_none() {
                // Unreachable while the buffer is sized from the same
                // coverage; a fragment that cannot be placed is skipped
                // rather than allowed to panic mid-frame.
                continue;
            }
            let extent = fragment.product.preferred_extent();
            let layout = if fragment.covers_whole_product() {
                let target_samples = &mut samples[start..end];
                let request = ControlRenderRequest::unorm16(extent);
                let target =
                    ControlRenderTarget::new(extent, ControlSampleFormat::Unorm16, target_samples);
                let layout = ctx.render_control(fragment.product, &request, target)?;
                if fragment.reversed {
                    reverse_lamps(&mut samples[start..end]);
                }
                rotate_lamps(&mut samples[start..end], fragment.rotation_samples);
                layout
            } else {
                // A partial run: the producer renders whole (once per frame,
                // however many runs it was cut into), and each run is copied
                // out of that. The copy is the price of a patched fixture and
                // nobody else pays it.
                let rendered = scratch.render(ctx, fragment.product)?;
                let source_start = fragment.source_offset_samples as usize;
                let source_end = source_start.saturating_add(fragment.len_samples as usize);
                let Some(source) = rendered.samples.get(source_start..source_end) else {
                    // A run past the end of its own product — the patch was
                    // resolved against a different lamp count than the one
                    // that just rendered. Leave those lamps dark rather than
                    // shifting the rest of the wire to cover it up.
                    continue;
                };
                samples[start..end].copy_from_slice(source);
                if fragment.reversed {
                    reverse_lamps(&mut samples[start..end]);
                }
                rotate_lamps(&mut samples[start..end], fragment.rotation_samples);
                rendered.layout.clone()
            };
            place_spans(&layout, fragment, &mut spans);
        }

        for (start, end) in coverage.gaps.iter().chain(coverage.contested.iter()) {
            let range = (*start as usize)..(*end as usize);
            if let Some(samples) = samples.get_mut(range) {
                samples.fill(0);
            }
        }

        self.published_sample_layout = Some(ControlLayout { spans });
        if self.published_fragments != fragments {
            self.published_fragments = fragments.to_vec();
            self.placement_revision = ctx.revision();
        }
        Ok(())
    }

    /// Hand the rendered samples back to the runtime buffer and mark it dirty
    /// for this frame. No copy: the Vec is the buffer's own storage.
    ///
    /// Shared by the render path and the test-pattern bypass so both publish
    /// identical buffer kind, metadata, and revision.
    fn publish_channel_buffer(
        &mut self,
        ctx: &mut TickContext<'_>,
        samples: Vec<u16>,
    ) -> Result<(), NodeError> {
        let buffer_id = self
            .channel_buffer_id
            .ok_or_else(|| NodeError::msg("output channel buffer not initialized"))?;
        let revision = ctx.revision();
        self.established_samples = samples.len();
        self.published_at = revision;
        let channels = (samples.len() / 3) as u32;
        ctx.with_runtime_buffer_mut(buffer_id, revision, |buffer| {
            buffer.kind = RuntimeBufferKind::OutputChannels;
            buffer.metadata = RuntimeBufferMetadata::OutputChannels {
                channels,
                sample_format: RuntimeChannelSampleFormat::U16,
            };
            buffer.set_samples16(samples);
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

/// The whole placement policy: anchors land where they say, everyone else
/// auto-flows after every anchor (D17v, D5).
///
/// **Auto-flow** is the unpatched rule and the base case: each producer starts
/// where the previous one ended, in provider order — which is the resolver's,
/// and the resolver's is deterministic (binding priority first, then binding
/// ref, which is owner node id, which is the order the loader attached the
/// nodes, which is the order the module lists them; its `nodes` map is
/// key-sorted). "The second fixture starts where the first one ended" is a
/// property of the project document, not of a hash iteration;
/// `tests/output_fragments.rs` pins it.
///
/// **Partial patching is first-class** — patching one fixture of five must not
/// require patching the other four — so unpatched producers keep the
/// running-sum rule among themselves and simply start past the highest
/// anchored end.
/// A producer whose run list is present but EMPTY contributes no fragment
/// and no gap: its lamps are patched onto some other output (D40's
/// zero-runs-here case), which is not the same thing as `None` — auto-flow
/// me. The engine's placement query documents the distinction.
fn plan_fragments(placements: &[FragmentPlacement]) -> Vec<OutputFragment> {
    let mut cursor = 0u32;
    for placement in placements {
        for range in placement.patch.iter().flatten() {
            cursor = cursor.max(lamps_to_samples(range.lamp_end()));
        }
    }

    let mut fragments = Vec::with_capacity(placements.len());
    for placement in placements {
        match &placement.patch {
            Some(ranges) => {
                for range in ranges {
                    fragments.push(OutputFragment {
                        product: placement.product,
                        source_offset_samples: lamps_to_samples(range.start),
                        offset_samples: lamps_to_samples(range.lamp),
                        len_samples: lamps_to_samples(range.count),
                        reversed: range.reversed,
                        rotation_samples: lamps_to_samples(range.offset),
                    });
                }
            }
            None => {
                let len = placement.product.preferred_extent().sample_count();
                fragments.push(OutputFragment {
                    product: placement.product,
                    source_offset_samples: 0,
                    offset_samples: cursor,
                    len_samples: len,
                    reversed: false,
                    rotation_samples: 0,
                });
                cursor = cursor.saturating_add(len);
            }
        }
    }
    fragments
}

/// Patch documents count in lamps; buffers count in samples.
const fn lamps_to_samples(lamps: u32) -> u32 {
    lamps.saturating_mul(SAMPLES_PER_LAMP)
}

/// What the `highlight` Debug slot asked this output to paint.
///
/// Two light languages, one slot (microformat v2 — see [`parse_highlight`]):
/// the wire-side selection BREATHES (white, direction-free) and a
/// fixture-side one CHASES (blue head, red tail, white dot in object order).
/// Which one a selection deserves is the client's call; the engine only
/// honors what the string names.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Highlight {
    /// v1, the bare span list: breathing white over unordered wire spans.
    Breath(Vec<(u32, u32)>),
    /// v2, `chase:`: ordered, direction-carrying spans in OBJECT order.
    Chase(Vec<ChaseSpan>),
}

impl Default for Highlight {
    fn default() -> Self {
        Self::Breath(Vec::new())
    }
}

impl Highlight {
    /// Nothing to paint — the byte-identity path every unpulsed project takes.
    fn is_empty(&self) -> bool {
        match self {
            Self::Breath(spans) => spans.is_empty(),
            Self::Chase(spans) => spans.is_empty(),
        }
    }
}

/// One run of an object's lamps on the wire, carrying which way it runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ChaseSpan {
    /// Lowest wire lamp of the run.
    start: u32,
    /// Lamps in the run, always at least one.
    count: u32,
    /// The run's wire indices descend as object order advances — a strand
    /// plugged in at the far end. Written as a descending range (`59-0`).
    reversed: bool,
}

/// Most lamps a single chase will walk before it is treated as garbage.
///
/// The chase costs work per lamp (the breath is slice math), so a fat-fingered
/// `0-4294967295` must not become a four-billion-iteration frame. Far above
/// any real output's lamp count, far below a stall.
const CHASE_MAX_LAMPS: u32 = 65_536;

/// Parse the `highlight` Debug slot's microformat (v2).
///
/// Grammar, in the output's flat wire numbering — the same numbering
/// `at.lamp` anchors and the status reports speak:
///
/// ```text
/// value   := [ "chase:" ] list         ; prefix is case-insensitive
/// list    := segment { "," segment }
/// segment := lamp | lamp "-" lamp      ; inclusive on both ends
/// ```
///
/// - **No prefix** (`"0-29,45,90-119"`) is the v1 breath: unordered wire
///   spans, inverted ranges skipped. Every value ever written keeps its
///   meaning.
/// - **`chase:`** (`"chase:60-119,59-0"`) lists the spans in OBJECT order —
///   the first segment holds object lamp 0 — and a DESCENDING range means
///   that run is walked backward on the wire.
/// - Whitespace around segments and around the prefix is tolerated; a
///   segment that does not parse is SKIPPED; an unknown prefix (anything
///   else before a `:`) reads as EMPTY. The slot is a diagnostic written by
///   tooling and hand-driven from the Debug section, so half a highlight
///   beats a dead one — and a value the engine does not understand paints
///   nothing rather than guessing.
fn parse_highlight(text: &str) -> Highlight {
    let text = text.trim();
    let Some((prefix, rest)) = text.split_once(':') else {
        return Highlight::Breath(parse_highlight_lamps(text));
    };
    if !prefix.trim().eq_ignore_ascii_case("chase") {
        return Highlight::default();
    }
    Highlight::Chase(parse_chase_spans(rest))
}

/// Parse the v1 bare span list into `(start, count)` lamp spans.
///
/// Order carries no meaning here and an inverted range is skipped — the
/// breath lights a set of lamps, not a path.
fn parse_highlight_lamps(text: &str) -> Vec<(u32, u32)> {
    let mut spans = Vec::new();
    for segment in text.split(',') {
        let Some((first, last)) = parse_span_segment(segment) else {
            continue;
        };
        if last < first {
            continue;
        }
        spans.push((first, (last - first).saturating_add(1)));
    }
    spans
}

/// Parse the `chase:` span list, keeping listed order and direction.
fn parse_chase_spans(text: &str) -> Vec<ChaseSpan> {
    let mut spans = Vec::new();
    for segment in text.split(',') {
        let Some((first, last)) = parse_span_segment(segment) else {
            continue;
        };
        let (start, end, reversed) = if last < first {
            (last, first, true)
        } else {
            (first, last, false)
        };
        spans.push(ChaseSpan {
            start,
            count: (end - start).saturating_add(1),
            reversed,
        });
    }
    spans
}

/// One `lamp` or `lamp-lamp` segment as its written endpoints, or `None`
/// when it is not a segment at all.
fn parse_span_segment(segment: &str) -> Option<(u32, u32)> {
    let segment = segment.trim();
    if segment.is_empty() {
        return None;
    }
    let (first, last) = match segment.split_once('-') {
        Some((first, last)) => (first.trim().parse::<u32>(), last.trim().parse::<u32>()),
        None => (segment.parse::<u32>(), segment.parse::<u32>()),
    };
    match (first, last) {
        (Ok(first), Ok(last)) => Some((first, last)),
        _ => None,
    }
}

/// Total lamps a chase names, saturating: the `n` its head/tail sizing and
/// sweep position are measured in, clipping to the wire notwithstanding.
fn chase_lamp_count(spans: &[ChaseSpan]) -> u32 {
    let total = spans
        .iter()
        .fold(0u32, |total, span| total.saturating_add(span.count));
    if total > CHASE_MAX_LAMPS { 0 } else { total }
}

/// The chase's wire lamps in OBJECT order: spans as listed, and a reversed
/// span's indices walked from its top down, so the object's own numbering
/// stays continuous across a re-plugged strand.
fn chase_wire_lamps(spans: &[ChaseSpan]) -> impl Iterator<Item = u32> + '_ {
    spans.iter().flat_map(|span| {
        (0..span.count).map(move |step| {
            if span.reversed {
                span.start.saturating_add(span.count - 1 - step)
            } else {
                span.start.saturating_add(step)
            }
        })
    })
}

/// The same lamps as breath spans — the A2 fallback's shape, where direction
/// is dropped because the wire cannot say which channel is blue.
fn breath_spans(spans: &[ChaseSpan]) -> Vec<(u32, u32)> {
    spans.iter().map(|span| (span.start, span.count)).collect()
}

/// Which channel order each sample of a published buffer is stored in.
///
/// A cursor, not a map: the layout's runs and a chase's lamps both walk the
/// buffer roughly in order, so remembering the last run that answered keeps
/// the lookup O(1) on the common path without allocating. Rows are ignored —
/// an output's published buffer is one flat sequence and its producers all
/// place into row 0.
struct ChannelOrders<'a> {
    spans: &'a [ControlSpan],
    cursor: usize,
}

impl<'a> ChannelOrders<'a> {
    fn new(layout: &'a ControlLayout) -> Self {
        Self {
            spans: &layout.spans,
            cursor: 0,
        }
    }

    /// The channel order of the RGB run covering `sample`, or `None` when no
    /// run does (a gap, or a `Raw` run with no color interpretation).
    fn order_at(&mut self, sample: u32) -> Option<ColorOrder> {
        let count = self.spans.len();
        for step in 0..count {
            let index = (self.cursor + step) % count;
            let span = &self.spans[index];
            if sample < span.start || sample >= span.start.saturating_add(span.len) {
                continue;
            }
            match &span.encoding {
                ControlHint::RgbPixels { color_order, .. } => {
                    self.cursor = index;
                    return Some(*color_order);
                }
                ControlHint::Raw => continue,
            }
        }
        None
    }
}

/// One product rendered whole, for the fragments that only want part of it.
struct RenderedProduct {
    product: ControlProduct,
    samples: Vec<u16>,
    layout: ControlLayout,
}

/// Per-frame cache of whole-product renders.
///
/// A patched fixture is cut into several runs, and every one of them wants the
/// same rendered product. Rendering it once per frame rather than once per run
/// is the difference between a patch costing a copy and a patch costing a
/// whole extra shader sample pass per range.
#[derive(Default)]
struct ProductScratch {
    rendered: Vec<RenderedProduct>,
}

impl ProductScratch {
    fn render(
        &mut self,
        ctx: &mut TickContext<'_>,
        product: ControlProduct,
    ) -> Result<&RenderedProduct, NodeError> {
        if let Some(index) = self
            .rendered
            .iter()
            .position(|rendered| rendered.product == product)
        {
            return Ok(&self.rendered[index]);
        }
        let extent = product.preferred_extent();
        let mut samples = alloc::vec![0u16; extent.sample_count() as usize];
        let request = ControlRenderRequest::unorm16(extent);
        let target = ControlRenderTarget::new(extent, ControlSampleFormat::Unorm16, &mut samples);
        let layout = ctx.render_control(product, &request, target)?;
        self.rendered.push(RenderedProduct {
            product,
            samples,
            layout,
        });
        Ok(self.rendered.last().expect("just pushed"))
    }
}

/// Rebase one fragment's share of its producer's layout into the output
/// buffer's coordinates, appending to `out` in fragment order.
///
/// A whole-product fragment keeps its producer's spans verbatim, offset — the
/// unpatched path, unchanged. A partial one clips each span to the run it
/// took and, when the run is reversed, mirrors it inside the run: the lamps
/// are laid down end-first, so the span that led the product trails on the
/// wire. A clipped RGB span is still RGB — a run of lamps in the same channel
/// order — with a lamp count to match its new length.
fn place_spans(layout: &ControlLayout, fragment: &OutputFragment, out: &mut Vec<ControlSpan>) {
    let source_start = fragment.source_offset_samples;
    let source_end = source_start.saturating_add(fragment.len_samples);
    for span in &layout.spans {
        let span_end = span.start.saturating_add(span.len);
        let clipped_start = span.start.max(source_start);
        let clipped_end = span_end.min(source_end);
        if clipped_start >= clipped_end {
            continue;
        }
        let relative = clipped_start - source_start;
        let len = clipped_end - clipped_start;
        let start = place_offset(fragment, relative, len);
        // A rotated span can wrap the fragment's window edge and come out
        // as two runs; each piece keeps the span's encoding, resized.
        for (start, len) in rotated_pieces(fragment, start, len) {
            let encoding = if len == span.len {
                span.encoding.clone()
            } else {
                match &span.encoding {
                    ControlHint::RgbPixels { color_order, .. } => ControlHint::RgbPixels {
                        count: len / SAMPLES_PER_LAMP,
                        color_order: *color_order,
                    },
                    other => other.clone(),
                }
            };
            out.push(ControlSpan {
                row: span.row,
                start,
                len,
                encoding,
            });
        }
    }
}

/// Apply a fragment's rotation to one placed run, splitting it where it
/// wraps the window edge. `start` is absolute (buffer coordinates, already
/// through [`place_offset`]); pieces come back absolute too. The unrotated
/// case is the identity, one piece.
fn rotated_pieces(
    fragment: &OutputFragment,
    start: u32,
    len: u32,
) -> impl Iterator<Item = (u32, u32)> {
    let window = fragment.len_samples.max(1);
    let rotation = fragment.rotation_samples % window;
    let base = fragment.offset_samples;
    let mut pieces = [(0u32, 0u32); 2];
    let count;
    if rotation == 0 || len == 0 {
        pieces[0] = (start, len);
        count = 1;
    } else {
        let relative = start.saturating_sub(base);
        let rotated = (relative + rotation) % window;
        if rotated + len <= window {
            pieces[0] = (base + rotated, len);
            count = 1;
        } else {
            let head = window - rotated;
            pieces[0] = (base + rotated, head);
            pieces[1] = (base, len - head);
            count = 2;
        }
    }
    pieces.into_iter().take(count)
}

/// Map one sample offset INSIDE a fragment's source run onto the output
/// buffer, honouring the fragment's reversal.
///
/// `relative` counts from the run's first sample; `len` is the run's length in
/// samples. A reversed run is laid down end-first, so the mirror is taken over
/// whole `size`-sample groups (a lamp stays a lamp — reversal reorders lamps,
/// it never reorders a lamp's channels, which is exactly what
/// [`reverse_lamps`] does to the samples).
const fn place_offset(fragment: &OutputFragment, relative: u32, size: u32) -> u32 {
    if fragment.reversed {
        fragment.offset_samples.saturating_add(
            fragment
                .len_samples
                .saturating_sub(relative.saturating_add(size)),
        )
    } else {
        fragment.offset_samples.saturating_add(relative)
    }
}

/// Rebase one fragment's share of its producer's DISPLAY layout into the
/// output buffer's coordinates, appending to `lamps` / `paths`.
///
/// The display twin of [`place_spans`], and it exists for the same reason: a
/// producer states geometry in its OWN lamp numbering (`sample_start =
/// channel * 3`), and the wire's numbering is the fragment's. A lamp outside
/// this fragment's source run belongs to another run and is skipped here — it
/// gets rebased when that run is placed.
fn place_display_lamps(
    layout: &ControlLayout2d,
    fragment: &OutputFragment,
    lamps: &mut Vec<ControlLamp2d>,
    paths: &mut Vec<ControlPathSpan2d>,
) {
    let source_start = fragment.source_offset_samples;
    let source_end = source_start.saturating_add(fragment.len_samples);

    for lamp in &layout.lamps {
        let lamp_end = lamp.sample_start.saturating_add(SAMPLES_PER_LAMP);
        // Whole lamps only: a lamp straddling the edge of a run has no honest
        // wire position, and a patch cuts on lamp boundaries by construction.
        if lamp.sample_start < source_start || lamp_end > source_end {
            continue;
        }
        let placed = place_offset(fragment, lamp.sample_start - source_start, SAMPLES_PER_LAMP);
        // A lamp is one rotation piece by construction (rotation moves whole
        // lamps), so the iterator yields exactly one placement.
        for (start, _) in rotated_pieces(fragment, placed, SAMPLES_PER_LAMP) {
            lamps.push(ControlLamp2d {
                lamp_index: start / SAMPLES_PER_LAMP,
                sample_start: start,
                center: lamp.center,
                radius: lamp.radius,
            });
        }
    }

    for path in &layout.paths {
        let path_start = path.first_lamp.saturating_mul(SAMPLES_PER_LAMP);
        let path_end = path_start.saturating_add(path.lamp_count.saturating_mul(SAMPLES_PER_LAMP));
        let clipped_start = path_start.max(source_start);
        let clipped_end = path_end.min(source_end);
        if clipped_start >= clipped_end {
            continue;
        }
        let len = clipped_end - clipped_start;
        let placed = place_offset(fragment, clipped_start - source_start, len);
        // A rotated path span can wrap the window edge — two runs then.
        for (start, len) in rotated_pieces(fragment, placed, len) {
            paths.push(ControlPathSpan2d {
                first_lamp: start / SAMPLES_PER_LAMP,
                lamp_count: len / SAMPLES_PER_LAMP,
            });
        }
    }
}

/// The display layout of a whole OUTPUT: every producer's lamps, rebased
/// through the fragments that placed them, in wire order.
///
/// A client draws a published frame by reading each lamp's `sample_start` out
/// of the frame's bytes ([`lpc_wire::OutputFrameEntry`]), so those offsets
/// have to be the WIRE's, not the producer's. One unpatched producer makes
/// the two coincide, which is why single-fixture projects looked right while
/// the peach — two fixtures, one of them cut in half and plugged in backwards
/// around the other — painted the body's far half with the leaf's colors and
/// drew no leaf at all.
///
/// `revision` is the caller's fold of every input layout's revision with the
/// output's own placement revision: geometry here changes when a mapping
/// changes OR when the patch moves a run, and the `IfChanged` gate has to see
/// both.
///
/// Extent hints are the componentwise max: the merged layout is a composite of
/// producers that may render at different sizes, and lamp centers are already
/// normalized, so the hints only tell a viewer what aspect to reserve.
#[must_use]
pub fn merge_fragment_display_layouts(
    placed: &[(OutputFragment, ControlLayout2d)],
    revision: Revision,
) -> ControlLayout2d {
    let mut lamps = Vec::new();
    let mut paths = Vec::new();
    let mut width_hint = 0;
    let mut height_hint = 0;
    for (fragment, layout) in placed {
        width_hint = width_hint.max(layout.width_hint);
        height_hint = height_hint.max(layout.height_hint);
        place_display_lamps(layout, fragment, &mut lamps, &mut paths);
    }
    // Wire order, so a consumer can scan lamps and bytes together. Contested
    // samples can leave two lamps on one offset — both are drawn, dark,
    // which is the honest picture of two strands claiming one channel.
    lamps.sort_by_key(|lamp| lamp.sample_start);
    paths.sort_by_key(|path| path.first_lamp);
    ControlLayout2d::new(revision, width_hint, height_hint, lamps).with_paths(paths)
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

/// Rotate a placed range's lamps right by `rotation_samples`, in place —
/// the render-side application of the kernel's `(j' + k) mod N`: after the
/// forward (and possibly reversed) layout put oriented lamp `j'` at window
/// slot `j'`, a right-rotation by `k` lamps moves it to `(j' + k) mod N`.
fn rotate_lamps(samples: &mut [u16], rotation_samples: u32) {
    let len = samples.len();
    if len == 0 {
        return;
    }
    let rotation = (rotation_samples as usize) % len;
    if rotation != 0 {
        samples.rotate_right(rotation);
    }
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

    fn runtime_output_fragments(&self) -> &[OutputFragment] {
        &self.published_fragments
    }

    fn runtime_output_placement_revision(&self) -> Revision {
        self.placement_revision
    }

    fn consume(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        // Identity first, resolve second: the fixtures this resolve ticks
        // read the registered-name set for their dangling-entry checks, so
        // this output's name must be on the books before they run.
        let name = self.authored_name(ctx);
        let collision = ctx.register_output_identity(name);
        self.identity_status = collision.map(|name| {
            NodeRuntimeStatus::Error(format!(
                "two outputs are named {name:?}; patch entries naming it are ambiguous — \
                 rename one"
            ))
        });

        // Bypass, not overwrite: while the Debug slot is on, the graph resolve
        // is skipped entirely, so upstream demand from this root stops. Other
        // outputs are unaffected. The frame the slot goes back to false we fall
        // straight through to the normal resolve below — no black frame.
        //
        // A pattern only ever REPAINTS an extent the graph already established;
        // before the first rendered frame there is nothing to repaint, so that
        // frame renders the graph and establishes it.
        let highlight = self.highlight_value(ctx)?;
        if self.test_pattern_active(ctx)? && self.established_samples > 0 {
            let mut samples = self.take_channel_samples(ctx)?;
            Self::fill_solid(&mut samples, TEST_PATTERN_RGB);
            self.paint_highlight(&mut samples, &highlight, ctx.time_seconds());
            return self.publish_channel_buffer(ctx, samples);
        }

        let prod = ctx
            .resolve(&QueryKey::ConsumedSlot {
                node: ctx.node_id(),
                slot: output_input_path(),
            })
            .map_err(|e| NodeError::msg(format!("resolve output input: {}", e.message)))?;

        let products = control_products(prod.data())?;
        // Each producer's patch, asked for at the moment it is placed. A
        // fixture resolves its patch in its own `produce` — which the input
        // resolve above just ran — so this reads THIS frame's answer, and an
        // edited patch document moves the wire on the next tick with no cache
        // to invalidate.
        let placements: Vec<FragmentPlacement> = products
            .into_iter()
            .map(|product| FragmentPlacement {
                patch: ctx.control_patch_placement(product),
                product,
            })
            .collect();
        let fragments = plan_fragments(&placements);
        let mut samples = self.take_channel_samples(ctx)?;
        if let Err(error) = self.render_fragments_into(ctx, &fragments, &mut samples) {
            self.restore_channel_samples(ctx, samples);
            return Err(error);
        }
        // The highlight overlay repaints AFTER the real render, so the frame
        // underneath stays the graph's and un-highlighted lamps are untouched.
        // With no highlight this is a no-op — the byte-identical path every
        // unpulsed project takes.
        if !highlight.is_empty() {
            self.paint_highlight(&mut samples, &highlight, ctx.time_seconds());
        }
        self.publish_channel_buffer(ctx, samples)
    }

    fn runtime_status(&self) -> Option<NodeRuntimeStatus> {
        self.identity_status
            .clone()
            .or_else(|| self.fragment_status.clone())
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
        /// Effective value of the `highlight` Debug slot, read every frame.
        highlight: String,
        /// The `highlight` twin of `test_pattern_resolvable`.
        highlight_resolvable: bool,
        /// Channel order the rendered span declares, or `None` for a `Raw`
        /// span with no color interpretation — the layout the chase needs
        /// and the one it must refuse (A2).
        sample_color_order: Option<ColorOrder>,
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
        /// Resolved patches, per product — what the engine's host answers
        /// from the producing node.
        patches: Vec<(ControlProduct, Vec<PatchedRun>)>,
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
                highlight: String::new(),
                highlight_resolvable: true,
                sample_color_order: None,
                graph_extent: ONE_LAMP,
                graph_color: [1000, 2000, 3000],
                graph_products: Vec::new(),
                paint_by_product: false,
                patches: Vec::new(),
            }
        }

        /// The published samples.
        fn published_samples(&self) -> Vec<u16> {
            self.buffer
                .samples16()
                .map(<[u16]>::to_vec)
                .unwrap_or_default()
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
                "highlight" if self.highlight_resolvable => Ok(Production::leaf(
                    WithRevision::new(Revision::new(1), LpValue::String(self.highlight.clone())),
                    ProductionSource::Literal,
                )),
                "highlight" => Err(ResolveError::new(String::from(
                    "unresolved consumed slot highlight",
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
            let len = target.samples.len() as u32;
            Ok(ControlSampleLayout {
                spans: vec![lpc_model::ControlSampleSpan {
                    row: 0,
                    start: 0,
                    len,
                    encoding: match self.sample_color_order {
                        Some(color_order) => lpc_model::ControlSampleEncoding::RgbPixels {
                            count: len / 3,
                            color_order,
                        },
                        None => lpc_model::ControlSampleEncoding::Raw,
                    },
                }],
            })
        }

        fn control_patch_placement(
            &self,
            product: ControlProduct,
            _consumer: NodeId,
        ) -> Option<Vec<PatchedRun>> {
            self.patches
                .iter()
                .find(|(patched, _)| *patched == product)
                .map(|(_, runs)| runs.clone())
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
        consume_at_time(node, resolver, frame, 0.0)
    }

    /// [`consume_at`] with an explicit frame time — what the highlight's
    /// blink phase is derived from.
    fn consume_at_time(
        node: &mut OutputNode,
        resolver: &mut FakeResolver,
        frame: Revision,
        time_seconds: f32,
    ) -> Result<(), NodeError> {
        let shapes = SlotShapeRegistry::default();
        let mut ctx = TickContext::with_render_services(
            node_id(),
            frame,
            resolver,
            &shapes,
            None,
            None,
            time_seconds,
        );
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
        let graph_len = resolver.buffer.byte_len();

        resolver.test_pattern = true;
        consume_at(&mut node, &mut resolver, Revision::new(2)).expect("pattern frame");

        assert_eq!(resolver.buffer.kind, graph_kind);
        assert_eq!(resolver.buffer.metadata, graph_metadata);
        assert_eq!(resolver.buffer.byte_len(), graph_len);
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
    /// handler cleared the control samples; measurement showed the output's
    /// own `produce` resizes them back earlier in the same tick than the
    /// compile runs, so the drop freed nothing at the compile instant. See
    /// `docs/defects/2026-08-04-compile-window-drops-rebuilt-before-compile.md`.
    /// The samples live in the channel buffer now; the established extent
    /// the node remembers must survive too.
    #[test]
    fn memory_pressure_does_not_drop_the_control_samples() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        consume_at(&mut node, &mut resolver, Revision::new(1)).expect("graph frame");
        let samples_before = resolver.published_samples();
        let established_before = node.established_samples;
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
            resolver.published_samples(),
            samples_before,
            "the published samples must survive a pressure broadcast"
        );
        assert_eq!(node.established_samples, established_before);
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

    // ---- Highlight pulse ----------------------------------------------

    #[test]
    fn parse_highlight_lamps_reads_the_microformat() {
        assert_eq!(
            parse_highlight_lamps("0-29, 45,90-119"),
            vec![(0, 30), (45, 1), (90, 30)]
        );
        assert_eq!(parse_highlight_lamps(""), vec![]);
        assert_eq!(
            parse_highlight_lamps("zork, 7, 9-3, -2, 4-4"),
            vec![(7, 1), (4, 1)],
            "junk and inverted segments are skipped, never fatal",
        );
    }

    #[test]
    fn the_highlight_paints_its_lamps_and_dims_the_rest() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        resolver.graph_extent = TWO_LAMPS;
        resolver.highlight = String::from("1");

        consume_at(&mut node, &mut resolver, Revision::new(1)).expect("graph frame");

        let floor = HIGHLIGHT_FLOOR_16;
        assert_eq!(
            resolver.published_samples(),
            vec![250, 500, 750, floor, floor, floor],
            "lamp 0 keeps the graph's colors at quarter power (the background \
             recedes); lamp 1 wears the breath's floor at phase 0",
        );
        assert_eq!(
            resolver.input_resolve_calls, 1,
            "the highlight is an overlay, not a bypass: the graph still renders",
        );
    }

    #[test]
    fn the_highlight_breathes_and_never_goes_dark() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        resolver.graph_extent = TWO_LAMPS;
        resolver.highlight = String::from("1");

        // Half period: the crest (libm cos of an f32 π rounds within one
        // step of exact, so pin through the level fn's own value).
        let crest_time = HIGHLIGHT_BREATH_SECONDS / 2.0;
        consume_at_time(&mut node, &mut resolver, Revision::new(1), crest_time)
            .expect("graph frame");
        let crest = resolver.published_samples()[3];
        assert_eq!(crest, highlight_level_16(crest_time));
        assert!(
            crest >= HIGHLIGHT_CREST_16 - 1,
            "half period reaches the crest ({crest})"
        );

        // A hard-blink regression would spend half the cycle at zero; the
        // breath NEVER goes dark — a selection must never read as dead lamps.
        for step in 0..12 {
            let time = HIGHLIGHT_BREATH_SECONDS * (step as f32) / 12.0;
            assert!(
                highlight_level_16(time) >= HIGHLIGHT_FLOOR_16,
                "breath floor holds at t={time}"
            );
        }
    }

    #[test]
    fn the_highlight_composes_with_the_test_pattern() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        resolver.graph_extent = TWO_LAMPS;
        consume_at(&mut node, &mut resolver, Revision::new(1)).expect("graph frame");

        resolver.test_pattern = true;
        resolver.highlight = String::from("0");
        consume_at_time(&mut node, &mut resolver, Revision::new(2), 0.0).expect("pattern frame");

        let floor = HIGHLIGHT_FLOOR_16;
        let dimmed_pattern = PATTERN16 >> 2;
        assert_eq!(
            resolver.published_samples(),
            vec![
                floor,
                floor,
                floor,
                dimmed_pattern,
                dimmed_pattern,
                dimmed_pattern
            ],
            "the breath overlays the pattern too (pattern dimmed beneath it), \
             so a selection stays findable mid-wiring-test",
        );
    }

    #[test]
    fn highlight_lamps_past_the_established_extent_are_clipped() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        resolver.graph_extent = TWO_LAMPS;
        resolver.highlight = String::from("1-500");

        consume_at(&mut node, &mut resolver, Revision::new(1)).expect("graph frame");

        let floor = HIGHLIGHT_FLOOR_16;
        assert_eq!(
            resolver.published_samples(),
            vec![250, 500, 750, floor, floor, floor],
            "the buffer keeps its extent; lamps the wire does not have are ignored",
        );
    }

    #[test]
    fn an_unreadable_highlight_never_stops_the_output() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        resolver.highlight_resolvable = false;

        consume_at(&mut node, &mut resolver, Revision::new(1)).expect("graph frame");

        assert_eq!(
            resolver.published_samples(),
            vec![1000, 2000, 3000],
            "an unreadable diagnostic reads as no highlight; the output keeps pushing pixels",
        );
    }

    #[test]
    fn clearing_the_highlight_restores_the_graph_frame() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        resolver.highlight = String::from("0");
        consume_at(&mut node, &mut resolver, Revision::new(1)).expect("pulsed frame");

        resolver.highlight = String::new();
        consume_at(&mut node, &mut resolver, Revision::new(2)).expect("clear frame");

        assert_eq!(
            resolver.published_samples(),
            vec![1000, 2000, 3000],
            "the frame the slot clears, the graph's own colors are back — nothing latched",
        );
    }

    // ---- Highlight chase ----------------------------------------------

    #[test]
    fn parse_highlight_reads_both_languages() {
        assert_eq!(
            parse_highlight("0-29, 45,90-119"),
            Highlight::Breath(vec![(0, 30), (45, 1), (90, 30)]),
            "a bare list is still the v1 breath, unchanged",
        );
        assert_eq!(
            parse_highlight("chase:60-119,59-0"),
            Highlight::Chase(vec![
                ChaseSpan {
                    start: 60,
                    count: 60,
                    reversed: false,
                },
                ChaseSpan {
                    start: 0,
                    count: 60,
                    reversed: true,
                },
            ]),
            "chase spans keep their listed OBJECT order, and a descending \
             range is the reversed run",
        );
        assert_eq!(
            parse_highlight(" CHASE: 7 , zork , 4-4 "),
            Highlight::Chase(vec![
                ChaseSpan {
                    start: 7,
                    count: 1,
                    reversed: false,
                },
                ChaseSpan {
                    start: 4,
                    count: 1,
                    reversed: false,
                },
            ]),
            "the prefix is case-insensitive and padding-tolerant; junk \
             segments are skipped, never fatal",
        );
        assert_eq!(
            parse_highlight("wobble:0-3"),
            Highlight::Breath(Vec::new()),
            "an unknown prefix paints nothing rather than guessing a language",
        );
        assert!(parse_highlight("").is_empty());
        assert!(parse_highlight("chase:").is_empty());
    }

    /// The wire paints the SHARED language, sample for sample — not a
    /// second copy of it (Q9). The studio controller paints the same
    /// `chase::lamp_rgb_16` for objects that have no wire yet, so this
    /// assertion is what keeps the panel, the sprites and the wall from
    /// drifting apart. Head sizing and the body window themselves are
    /// pinned in `lpc_model::nodes::output::chase`.
    #[test]
    fn the_chase_paints_the_shared_light_language_lamp_for_lamp() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        resolver.graph_extent = lamps(24);
        resolver.sample_color_order = Some(ColorOrder::Rgb);
        resolver.highlight = String::from("chase:0-23");

        let seconds = 0.7;
        consume_at_time(&mut node, &mut resolver, Revision::new(1), seconds).expect("graph frame");

        let phase = chase::phase_at(seconds);
        let expected: Vec<u16> = (0..24)
            .flat_map(|ordinal| chase::lamp_rgb_16(ordinal, 24, phase))
            .collect();
        assert_eq!(resolver.published_samples(), expected);
    }

    #[test]
    fn the_chase_paints_blue_head_red_tail_and_dims_the_rest() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        resolver.graph_extent = lamps(6);
        resolver.sample_color_order = Some(ColorOrder::Rgb);
        resolver.highlight = String::from("chase:0-3");

        consume_at_time(&mut node, &mut resolver, Revision::new(1), 0.0).expect("graph frame");

        let floor = chase::BODY_FLOOR_16;
        assert_eq!(
            resolver.published_samples(),
            vec![
                // object lamp 0: the blue head (n = 4, so one lamp each end)
                0, 0, 65535, // body, at the floor: the dot sits on the head end at t=0
                floor, floor, floor, floor, floor, floor,
                // object lamp 3: the red tail
                65535, 0, 0, // unnamed lamps keep the graph frame at quarter power
                250, 500, 750, 250, 500, 750,
            ],
        );
    }

    #[test]
    fn a_reversed_chase_span_walks_its_wire_lamps_backward() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        resolver.graph_extent = lamps(4);
        resolver.sample_color_order = Some(ColorOrder::Rgb);
        resolver.highlight = String::from("chase:3-0");

        consume_at_time(&mut node, &mut resolver, Revision::new(1), 0.0).expect("graph frame");

        let floor = chase::BODY_FLOOR_16;
        assert_eq!(
            resolver.published_samples(),
            vec![
                65535, 0, 0, // wire lamp 0 is the OBJECT's last: red
                floor, floor, floor, floor, floor, floor, 0, 0,
                65535, // wire lamp 3 is object lamp 0: blue
            ],
            "a strand plugged in at the far end runs its head at the high \
             wire index — the mis-wiring the language exists to show",
        );
    }

    #[test]
    fn the_chase_dot_sweeps_the_object_in_order() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        resolver.graph_extent = lamps(11);
        resolver.sample_color_order = Some(ColorOrder::Rgb);
        resolver.highlight = String::from("chase:0-10");

        // Half a period: the dot sits at the object's midpoint (u = 0.5),
        // which for 11 lamps is object lamp 5 exactly.
        consume_at_time(
            &mut node,
            &mut resolver,
            Revision::new(1),
            chase::SWEEP_SECONDS / 2.0,
        )
        .expect("mid-sweep frame");
        let samples = resolver.published_samples();
        assert_eq!(
            &samples[15..18],
            &[chase::BODY_CREST_16; 3],
            "the dot is full white where it stands",
        );
        assert!(
            samples[12] < chase::BODY_CREST_16 && samples[12] > chase::BODY_FLOOR_16,
            "and falls off around it ({})",
            samples[12],
        );

        // At phase 0 the dot has run back to the head end, so the midpoint
        // is dark again — the body is dark-with-a-runner, not a wash.
        consume_at_time(&mut node, &mut resolver, Revision::new(2), 0.0).expect("start frame");
        assert_eq!(
            &resolver.published_samples()[15..18],
            &[chase::BODY_FLOOR_16; 3],
        );
    }

    #[test]
    fn the_chase_paints_in_the_outputs_declared_channel_order() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        resolver.graph_extent = lamps(2);
        resolver.sample_color_order = Some(ColorOrder::Grb);
        resolver.highlight = String::from("chase:0-1");

        consume_at_time(&mut node, &mut resolver, Revision::new(1), 0.0).expect("graph frame");

        assert_eq!(
            resolver.published_samples(),
            vec![0, 0, 65535, 0, 65535, 0],
            "on a GRB run the red tail is the MIDDLE channel — a chase that \
             ignored the layout would light the wrong end green",
        );
    }

    #[test]
    fn a_chase_without_an_rgb_layout_falls_back_to_the_breath() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        resolver.graph_extent = TWO_LAMPS;
        // The default fake publishes `Raw` spans: samples with no color
        // interpretation, the A2 case.
        resolver.highlight = String::from("chase:0-1");

        consume_at_time(&mut node, &mut resolver, Revision::new(1), 0.0).expect("graph frame");

        let floor = HIGHLIGHT_FLOOR_16;
        assert_eq!(
            resolver.published_samples(),
            vec![floor, floor, floor, floor, floor, floor],
            "the whole output breathes rather than painting a chase whose \
             blue might come out green",
        );
    }

    #[test]
    fn an_absurd_chase_span_breathes_instead_of_walking_four_billion_lamps() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        resolver.graph_extent = TWO_LAMPS;
        resolver.sample_color_order = Some(ColorOrder::Rgb);
        resolver.highlight = String::from("chase:0-4294967295");

        consume_at_time(&mut node, &mut resolver, Revision::new(1), 0.0).expect("graph frame");

        let floor = HIGHLIGHT_FLOOR_16;
        assert_eq!(
            resolver.published_samples(),
            vec![floor, floor, floor, floor, floor, floor],
            "a hand-typed absurdity costs a breath, not a frame",
        );
    }

    #[test]
    fn a_chase_naming_no_lamps_publishes_the_graph_frame_byte_for_byte() {
        let mut chased = output_node();
        let mut chased_resolver = FakeResolver::new();
        chased_resolver.sample_color_order = Some(ColorOrder::Rgb);
        chased_resolver.highlight = String::from("chase:");
        consume_at(&mut chased, &mut chased_resolver, Revision::new(1)).expect("chase frame");

        let mut plain = output_node();
        let mut plain_resolver = FakeResolver::new();
        plain_resolver.sample_color_order = Some(ColorOrder::Rgb);
        consume_at(&mut plain, &mut plain_resolver, Revision::new(1)).expect("plain frame");

        assert_eq!(
            chased_resolver.buffer.data, plain_resolver.buffer.data,
            "an empty chase is an empty highlight: the render path publishes \
             exactly the bytes it always did",
        );
        assert_eq!(chased_resolver.published_samples(), vec![1000, 2000, 3000]);
    }

    // ---- Output fragments ---------------------------------------------

    /// Two lamps' worth of samples: the smallest fragment that can be
    /// reversed and still say something.
    const TWO_LAMPS: ControlExtent = ControlExtent::new(1, 6);

    /// An extent of `count` RGB lamps in one row — the shape a chase's
    /// object order is read against.
    const fn lamps(count: u32) -> ControlExtent {
        ControlExtent::new(1, count * SAMPLES_PER_LAMP)
    }

    fn product(output: u32, extent: ControlExtent) -> ControlProduct {
        ControlProduct::new(node_id(), output, extent)
    }

    /// The unpatched placement of a producer list: what [`plan_fragments`]
    /// does when nobody authored a patch, which is the base case every
    /// checked-in project takes.
    fn auto_flow_fragments(products: &[ControlProduct]) -> Vec<OutputFragment> {
        let placements: Vec<FragmentPlacement> = products
            .iter()
            .map(|product| FragmentPlacement {
                product: *product,
                patch: None,
            })
            .collect();
        plan_fragments(&placements)
    }

    /// A patched producer, for the planner tests.
    fn patched(product: ControlProduct, runs: &[PatchedRun]) -> FragmentPlacement {
        FragmentPlacement {
            product,
            patch: Some(runs.to_vec()),
        }
    }

    /// The display-rebase side of rotation: a placed run splits at the
    /// window's wrap point, and lamp positions permute by the same mod
    /// math the sample copy uses.
    #[test]
    fn rotated_pieces_split_at_the_window_wrap() {
        let fragment = OutputFragment {
            product: product(1, ControlExtent::new(1, 15)),
            source_offset_samples: 0,
            offset_samples: 30,
            len_samples: 15, // five lamps
            reversed: false,
            rotation_samples: 6, // two lamps
        };
        // A run of the first three lamps (samples 30..39) rotated right two
        // lamps: slots 2..5 — one piece, no wrap.
        assert_eq!(
            rotated_pieces(&fragment, 30, 9).collect::<Vec<_>>(),
            vec![(36, 9)]
        );
        // A run of the LAST three lamps (samples 36..45) rotated right two:
        // slots (2+2)%5=4 then wrapping to 0..2 — two pieces.
        assert_eq!(
            rotated_pieces(&fragment, 36, 9).collect::<Vec<_>>(),
            vec![(42, 3), (30, 6)]
        );
        // No rotation: the identity, one piece.
        let plain = OutputFragment {
            rotation_samples: 0,
            ..fragment
        };
        assert_eq!(
            rotated_pieces(&plain, 36, 9).collect::<Vec<_>>(),
            vec![(36, 9)]
        );
    }

    fn run(start: u32, count: u32, lamp: u32, reversed: bool) -> PatchedRun {
        PatchedRun {
            start,
            count,
            lamp,
            reversed,
            offset: 0,
            output: None,
        }
    }

    /// Drive a hand-built placement, bypassing the planner — the seam a
    /// resolved patch authors into.
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
            node.runtime_output_fragments()
                .iter()
                .map(|fragment| (fragment.product.output(), fragment.offset_samples))
                .collect::<Vec<_>>(),
            vec![(1, 0), (2, 6)],
            "the placement set is latched with the frame, in wire order",
        );
    }

    /// The placement revision is a CHANGE stamp, not a frame counter: a
    /// client gating geometry on it must not be told to re-fetch every tick,
    /// and must be told the moment the wire is re-cut.
    #[test]
    fn the_placement_revision_moves_only_when_the_placement_does() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        resolver.graph_products = vec![product(1, TWO_LAMPS), product(2, ONE_LAMP)];

        consume_at(&mut node, &mut resolver, Revision::new(1)).expect("first frame");
        let first = node.runtime_output_placement_revision();
        consume_at(&mut node, &mut resolver, Revision::new(2)).expect("second frame");
        assert_eq!(
            node.runtime_output_placement_revision(),
            first,
            "an unchanged wire keeps its stamp",
        );

        // The patch moves the second producer: same products, new placement.
        resolver.patches = vec![(
            product(2, ONE_LAMP),
            vec![PatchedRun {
                start: 0,
                count: 1,
                lamp: 9,
                reversed: false,
                offset: 0,
                output: None,
            }],
        )];
        consume_at(&mut node, &mut resolver, Revision::new(3)).expect("repatched frame");
        assert_eq!(
            node.runtime_output_placement_revision(),
            Revision::new(3),
            "re-cutting the wire stamps the frame that did it",
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
                source_offset_samples: 0,
                offset_samples: 0,
                len_samples: 6,
                reversed: true,
                rotation_samples: 0,
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
                    source_offset_samples: 0,
                    offset_samples: 0,
                    len_samples: 6,
                    reversed: false,
                    rotation_samples: 0,
                },
                OutputFragment {
                    product: product(2, TWO_LAMPS),
                    source_offset_samples: 0,
                    offset_samples: 6,
                    len_samples: 6,
                    reversed: true,
                    rotation_samples: 0,
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
                    source_offset_samples: 0,
                    offset_samples: 0,
                    len_samples: 9,
                    reversed: false,
                    rotation_samples: 0,
                },
                // Starts one lamp before the first one ends.
                OutputFragment {
                    product: product(2, TWO_LAMPS),
                    source_offset_samples: 0,
                    offset_samples: 6,
                    len_samples: 6,
                    reversed: false,
                    rotation_samples: 0,
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
                    source_offset_samples: 0,
                    offset_samples: 0,
                    len_samples: 3,
                    reversed: false,
                    rotation_samples: 0,
                },
                OutputFragment {
                    product: product(2, ONE_LAMP),
                    source_offset_samples: 0,
                    offset_samples: 9,
                    len_samples: 3,
                    reversed: false,
                    rotation_samples: 0,
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
            source_offset_samples: 0,
            offset_samples: 0,
            len_samples: 12,
            reversed: false,
            rotation_samples: 0,
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
                    source_offset_samples: 0,
                    offset_samples: 0,
                    len_samples: 3,
                    reversed: false,
                    rotation_samples: 0,
                },
                OutputFragment {
                    product: product(2, ONE_LAMP),
                    source_offset_samples: 0,
                    offset_samples: 9,
                    len_samples: 3,
                    reversed: false,
                    rotation_samples: 0,
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
                    source_offset_samples: 0,
                    offset_samples: 0,
                    len_samples: 3,
                    reversed: false,
                    rotation_samples: 0,
                },
                OutputFragment {
                    product: product(2, ONE_LAMP),
                    source_offset_samples: 0,
                    offset_samples: 0,
                    len_samples: 3,
                    reversed: false,
                    rotation_samples: 0,
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
                source_offset_samples: 0,
                offset_samples: 0,
                len_samples: 3,
                reversed: false,
                rotation_samples: 0,
            },
            OutputFragment {
                product: product(2, ONE_LAMP),
                source_offset_samples: 0,
                offset_samples: 0,
                len_samples: 3,
                reversed: false,
                rotation_samples: 0,
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
                source_offset_samples: 0,
                offset_samples: 0,
                len_samples: 6,
                reversed: false,
                rotation_samples: 0,
            },
            OutputFragment {
                product: product(2, ONE_LAMP),
                source_offset_samples: 0,
                offset_samples: 3,
                len_samples: 6,
                reversed: false,
                rotation_samples: 0,
            },
            OutputFragment {
                product: product(3, ONE_LAMP),
                source_offset_samples: 0,
                offset_samples: 6,
                len_samples: 6,
                reversed: false,
                rotation_samples: 0,
            },
            OutputFragment {
                product: product(4, ONE_LAMP),
                source_offset_samples: 0,
                offset_samples: 15,
                len_samples: 3,
                reversed: false,
                rotation_samples: 0,
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

    // --- Patched placement -------------------------------------------------

    /// The equivalence the whole design leans on: with nobody patched, the
    /// planner IS auto-flow. Every checked-in project is this case.
    #[test]
    fn an_unpatched_plan_is_exactly_auto_flow() {
        let products = [product(1, TWO_LAMPS), product(2, ONE_LAMP)];
        let placements: Vec<FragmentPlacement> = products
            .iter()
            .map(|product| FragmentPlacement {
                product: *product,
                patch: None,
            })
            .collect();

        assert_eq!(
            plan_fragments(&placements),
            vec![
                OutputFragment {
                    product: product(1, TWO_LAMPS),
                    source_offset_samples: 0,
                    offset_samples: 0,
                    len_samples: 6,
                    reversed: false,
                    rotation_samples: 0,
                },
                OutputFragment {
                    product: product(2, ONE_LAMP),
                    source_offset_samples: 0,
                    offset_samples: 6,
                    len_samples: 3,
                    reversed: false,
                    rotation_samples: 0,
                },
            ]
        );
    }

    /// A patched producer contributes one fragment per resolved run, at the
    /// wire offsets the patch authored, in lamps-to-samples.
    #[test]
    fn a_patched_producer_contributes_one_fragment_per_run() {
        let body = product(1, ControlExtent::new(1, 12));

        let fragments =
            plan_fragments(&[patched(body, &[run(0, 2, 0, false), run(2, 2, 10, true)])]);

        assert_eq!(
            fragments,
            vec![
                OutputFragment {
                    product: body,
                    source_offset_samples: 0,
                    offset_samples: 0,
                    len_samples: 6,
                    reversed: false,
                    rotation_samples: 0,
                },
                OutputFragment {
                    product: body,
                    source_offset_samples: 6,
                    offset_samples: 30,
                    len_samples: 6,
                    reversed: true,
                    rotation_samples: 0,
                },
            ]
        );
    }

    /// Partial patching (D5): the unpatched fixture is not required to declare
    /// anything, and it lands past every anchor rather than under one.
    #[test]
    fn unpatched_producers_flow_after_the_highest_anchored_end() {
        let body = product(1, ControlExtent::new(1, 12));
        let leaf = product(2, ONE_LAMP);

        let fragments = plan_fragments(&[
            patched(body, &[run(0, 4, 8, false)]),
            FragmentPlacement {
                product: leaf,
                patch: None,
            },
        ]);

        assert_eq!(fragments[1].offset_samples, 36, "past lamp 12, not at 0");
        assert!(fragment_coverage(&fragments).contested.is_empty());
    }

    /// The patched fixture's own runs are what the buffer gets — the lamps in
    /// the run, from the run's place in the product, reversed when asked.
    #[test]
    fn a_patched_product_places_its_own_sub_runs() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        resolver.paint_by_product = true;
        let body = product(1, ControlExtent::new(1, 12));

        render_fragments_at(
            &mut node,
            &mut resolver,
            Revision::new(1),
            &plan_fragments(&[patched(body, &[run(2, 2, 0, false), run(0, 2, 2, true)])]),
        )
        .expect("render");

        assert_eq!(
            resolver.published_samples(),
            vec![106, 107, 108, 109, 110, 111, 103, 104, 105, 100, 101, 102],
            "lamps 2-3 lead the wire; lamps 0-1 follow, end-first",
        );
        assert_eq!(node.runtime_status(), None, "the runs tile the wire");
    }

    /// One render per producer per frame, however many runs it was cut into —
    /// a patch costs a copy, not an extra sample pass per range.
    #[test]
    fn a_producer_cut_into_several_runs_still_renders_once() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        resolver.paint_by_product = true;
        let body = product(1, ControlExtent::new(1, 12));

        render_fragments_at(
            &mut node,
            &mut resolver,
            Revision::new(1),
            &plan_fragments(&[patched(
                body,
                &[
                    run(0, 1, 0, false),
                    run(1, 1, 1, false),
                    run(2, 2, 2, false),
                ],
            )]),
        )
        .expect("render");

        assert_eq!(resolver.render_control_calls, 1);
    }

    /// A partial run's spans are clipped to the lamps it took and rebased onto
    /// the wire; a reversed run mirrors them inside itself, because the span
    /// that led the product trails on the wire.
    #[test]
    fn a_partial_runs_spans_are_clipped_and_a_reversed_ones_mirrored() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        let body = product(1, ControlExtent::new(1, 12));

        render_fragments_at(
            &mut node,
            &mut resolver,
            Revision::new(1),
            &plan_fragments(&[patched(body, &[run(0, 1, 0, false), run(1, 3, 1, true)])]),
        )
        .expect("render");

        // The fake publishes ONE span per product covering all 12 samples;
        // each run clips it to its own share.
        let layout = node
            .runtime_output_sample_layout()
            .expect("published layout");
        assert_eq!(
            layout
                .spans
                .iter()
                .map(|span| (span.start, span.len))
                .collect::<Vec<_>>(),
            vec![(0, 3), (3, 9)],
        );
    }

    /// A run that reaches past its own product — a patch resolved against a
    /// lamp count the fixture no longer has — leaves those lamps dark instead
    /// of sliding the rest of the wire over to cover it.
    #[test]
    fn a_run_past_the_end_of_its_product_leaves_its_lamps_dark() {
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
                    source_offset_samples: 0,
                    offset_samples: 0,
                    len_samples: 3,
                    reversed: false,
                    rotation_samples: 0,
                },
                OutputFragment {
                    product: product(1, TWO_LAMPS),
                    source_offset_samples: 60,
                    offset_samples: 3,
                    len_samples: 3,
                    reversed: false,
                    rotation_samples: 0,
                },
            ],
        )
        .expect("render");

        assert_eq!(
            resolver.published_samples(),
            vec![100, 101, 102, 0, 0, 0],
            "the placeable run still lights",
        );
    }

    /// The patch reaches the render through `consume`, not only through a
    /// hand-built fragment list: this is the path a real frame takes.
    #[test]
    fn consume_places_the_producers_patch() {
        let mut node = output_node();
        let mut resolver = FakeResolver::new();
        resolver.paint_by_product = true;
        let body = product(1, ControlExtent::new(1, 6));
        resolver.graph_products = vec![body];
        resolver.patches = vec![(body, vec![run(1, 1, 0, false), run(0, 1, 1, false)])];

        consume_at(&mut node, &mut resolver, Revision::new(1)).expect("graph frame");

        assert_eq!(
            resolver.published_samples(),
            vec![103, 104, 105, 100, 101, 102],
            "the patch swapped the two lamps on the wire",
        );
    }
}
