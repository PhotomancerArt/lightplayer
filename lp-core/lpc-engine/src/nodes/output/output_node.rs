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

use lpc_model::{
    ControlLamp2d, ControlLayout2d, ControlPathSpan2d, LpValue, NodeRuntimeStatus, OutputDefView,
    ProductRef, Revision, SlotData, SlotPath, WithRevision,
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
            control_samples: Vec::new(),
            def_view: None,
            fragment_status: None,
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
        let mut scratch = ProductScratch::default();
        for fragment in fragments {
            let start = fragment.offset_samples as usize;
            let end = fragment.end_samples() as usize;
            if self.control_samples.get(start..end).is_none() {
                // Unreachable while the buffer is sized from the same
                // coverage; a fragment that cannot be placed is skipped
                // rather than allowed to panic mid-frame.
                continue;
            }
            let extent = fragment.product.preferred_extent();
            let layout = if fragment.covers_whole_product() {
                let target_samples = &mut self.control_samples[start..end];
                let request = ControlRenderRequest::unorm16(extent);
                let target =
                    ControlRenderTarget::new(extent, ControlSampleFormat::Unorm16, target_samples);
                let layout = ctx.render_control(fragment.product, &request, target)?;
                if fragment.reversed {
                    reverse_lamps(&mut self.control_samples[start..end]);
                }
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
                self.control_samples[start..end].copy_from_slice(source);
                if fragment.reversed {
                    reverse_lamps(&mut self.control_samples[start..end]);
                }
                rendered.layout.clone()
            };
            place_spans(&layout, fragment, &mut spans);
        }

        for (start, end) in coverage.gaps.iter().chain(coverage.contested.iter()) {
            let range = (*start as usize)..(*end as usize);
            if let Some(samples) = self.control_samples.get_mut(range) {
                samples.fill(0);
            }
        }

        self.published_sample_layout = Some(ControlLayout { spans });
        if self.published_fragments != fragments {
            self.published_fragments = fragments.to_vec();
            self.placement_revision = ctx.revision();
        }

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
fn plan_fragments(placements: &[FragmentPlacement]) -> Vec<OutputFragment> {
    let mut cursor = 0u32;
    for placement in placements {
        for range in placement.patch.iter().flatten() {
            cursor = cursor.max(lamps_to_samples(range.channel_end()));
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
                        offset_samples: lamps_to_samples(range.channel),
                        len_samples: lamps_to_samples(range.count),
                        reversed: range.reversed,
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
        let start = place_offset(fragment, lamp.sample_start - source_start, SAMPLES_PER_LAMP);
        lamps.push(ControlLamp2d {
            lamp_index: start / SAMPLES_PER_LAMP,
            sample_start: start,
            center: lamp.center,
            radius: lamp.radius,
        });
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
        let start = place_offset(fragment, clipped_start - source_start, len);
        paths.push(ControlPathSpan2d {
            first_lamp: start / SAMPLES_PER_LAMP,
            lamp_count: len / SAMPLES_PER_LAMP,
        });
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
                graph_extent: ONE_LAMP,
                graph_color: [1000, 2000, 3000],
                graph_products: Vec::new(),
                paint_by_product: false,
                patches: Vec::new(),
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

        fn control_patch_placement(&self, product: ControlProduct) -> Option<Vec<PatchedRun>> {
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

    fn run(start: u32, count: u32, channel: u32, reversed: bool) -> PatchedRun {
        PatchedRun {
            start,
            count,
            channel,
            reversed,
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
                channel: 9,
                reversed: false,
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
                },
                OutputFragment {
                    product: product(2, TWO_LAMPS),
                    source_offset_samples: 0,
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
                    source_offset_samples: 0,
                    offset_samples: 0,
                    len_samples: 9,
                    reversed: false,
                },
                // Starts one lamp before the first one ends.
                OutputFragment {
                    product: product(2, TWO_LAMPS),
                    source_offset_samples: 0,
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
                    source_offset_samples: 0,
                    offset_samples: 0,
                    len_samples: 3,
                    reversed: false,
                },
                OutputFragment {
                    product: product(2, ONE_LAMP),
                    source_offset_samples: 0,
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
            source_offset_samples: 0,
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
                    source_offset_samples: 0,
                    offset_samples: 0,
                    len_samples: 3,
                    reversed: false,
                },
                OutputFragment {
                    product: product(2, ONE_LAMP),
                    source_offset_samples: 0,
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
                    source_offset_samples: 0,
                    offset_samples: 0,
                    len_samples: 3,
                    reversed: false,
                },
                OutputFragment {
                    product: product(2, ONE_LAMP),
                    source_offset_samples: 0,
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
                source_offset_samples: 0,
                offset_samples: 0,
                len_samples: 3,
                reversed: false,
            },
            OutputFragment {
                product: product(2, ONE_LAMP),
                source_offset_samples: 0,
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
                source_offset_samples: 0,
                offset_samples: 0,
                len_samples: 6,
                reversed: false,
            },
            OutputFragment {
                product: product(2, ONE_LAMP),
                source_offset_samples: 0,
                offset_samples: 3,
                len_samples: 6,
                reversed: false,
            },
            OutputFragment {
                product: product(3, ONE_LAMP),
                source_offset_samples: 0,
                offset_samples: 6,
                len_samples: 6,
                reversed: false,
            },
            OutputFragment {
                product: product(4, ONE_LAMP),
                source_offset_samples: 0,
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
                },
                OutputFragment {
                    product: product(2, ONE_LAMP),
                    source_offset_samples: 0,
                    offset_samples: 6,
                    len_samples: 3,
                    reversed: false,
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
                },
                OutputFragment {
                    product: body,
                    source_offset_samples: 6,
                    offset_samples: 30,
                    len_samples: 6,
                    reversed: true,
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
                },
                OutputFragment {
                    product: product(1, TWO_LAMPS),
                    source_offset_samples: 60,
                    offset_samples: 3,
                    len_samples: 3,
                    reversed: false,
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
