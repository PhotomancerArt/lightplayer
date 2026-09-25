//! Produced product data for primary node output surfaces.

use std::rc::Rc;

use lpc_model::{
    ControlDisplayLayout, ControlExtent, ControlProduct, ControlSampleLayout, NodeId, ProductRef,
    TimeProduct, VisualProduct,
};

use crate::{
    UiNodeDirtyState, UiProducedBinding, UiSlotAspect, UiSlotAspectKind, UiSlotAspectRow,
    UiSlotShape,
};

/// The family of product a node emits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiProductKind {
    /// No product has been resolved for this output yet.
    Empty,
    /// A visual image, shader result, or other displayable surface.
    Visual,
    /// A control stream, fixture map, or nonvisual device output.
    Control,
    /// A queryable timebase: the handle a clock publishes on `bus:time`.
    ///
    /// A product like the other two, and it wears the same chip — but it has
    /// no *picture*. Everything behind the handle (effective seconds, this
    /// tick's delta, the live phasors) lives in the engine's timebase store,
    /// and the way to look at it is the timebase probe's read-only listing,
    /// not a preview frame. So this kind renders as
    /// [`UiProductPreview::MetadataOnly`] by construction: no probe is ever
    /// requested for it, and no skeleton is ever drawn waiting for one.
    Time,
    /// A product whose presentation is not known by Studio yet.
    Other,
}

/// Whether Studio is actively requesting previews for this product.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiProductTrackingState {
    /// The product has not been watched in this Studio session.
    Untracked,
    /// Studio is actively requesting preview updates for the product.
    Tracking,
    /// Studio has preview data, but this product is not the active watch target.
    Paused,
}

/// Stable frame geometry for preview surfaces.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiProductPreviewFrame {
    /// Preview frame width in logical sample units.
    pub width: u32,
    /// Preview frame height in logical sample units.
    pub height: u32,
}

impl UiProductPreviewFrame {
    /// Default visual-product probe frame (sim tier).
    pub const VISUAL_DEFAULT: Self = Self::new(32, 32);

    /// Visual-product probe frame for real-device lenses: 4× fewer bytes
    /// over the serial wire and 4× fewer per-pixel sRGB encodes on the
    /// ESP32, at a resolution the small preview cards still read fine
    /// (probe-performance plan, runtime-tiered sizing).
    pub const VISUAL_DEVICE: Self = Self::new(16, 16);

    /// Create a preview frame with a nonzero fallback.
    #[must_use]
    pub const fn new(width: u32, height: u32) -> Self {
        Self {
            width: if width == 0 { 1 } else { width },
            height: if height == 0 { 1 } else { height },
        }
    }
}

/// Stable UI-facing identity for a lazy graph product.
///
/// The Studio DTO keeps this separate from rendering state so controllers can
/// request previews and stories can still hand-build product rows.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum UiProductRef {
    /// Renderable visual material produced by a node output.
    Visual { node_id: u32, output: u32 },
    /// Device-control material produced by a node output.
    Control {
        node_id: u32,
        output: u32,
        rows: u32,
        samples_per_row: u32,
    },
    /// Queryable timebase material produced by a node output.
    Time { node_id: u32, output: u32 },
}

impl UiProductRef {
    /// Convert a model product ref into the UI identity used for preview state.
    #[must_use]
    pub fn from_product_ref(product: ProductRef) -> Self {
        match product {
            ProductRef::Visual(product) => Self::from_visual_product(product),
            ProductRef::Control(product) => Self::from_control_product(product),
            ProductRef::Time(product) => Self::from_time_product(product),
        }
    }

    /// Convert a visual product into a UI identity.
    #[must_use]
    pub fn from_visual_product(product: VisualProduct) -> Self {
        Self::Visual {
            node_id: product.node().0,
            output: product.output(),
        }
    }

    /// Convert a control product into a UI identity.
    #[must_use]
    pub fn from_control_product(product: ControlProduct) -> Self {
        let extent = product.preferred_extent();
        Self::Control {
            node_id: product.node().0,
            output: product.output(),
            rows: extent.rows,
            samples_per_row: extent.samples_per_row,
        }
    }

    /// Convert a time product into a UI identity.
    #[must_use]
    pub fn from_time_product(product: TimeProduct) -> Self {
        Self::Time {
            node_id: product.node().0,
            output: product.output(),
        }
    }

    /// The runtime id of the node that produces this product.
    #[must_use]
    pub fn node_id(self) -> u32 {
        match self {
            Self::Visual { node_id, .. }
            | Self::Control { node_id, .. }
            | Self::Time { node_id, .. } => node_id,
        }
    }

    /// Convert this identity back into a visual product when possible.
    #[must_use]
    pub fn visual_product(self) -> Option<VisualProduct> {
        match self {
            Self::Visual { node_id, output } => {
                Some(VisualProduct::new(NodeId::new(node_id), output))
            }
            Self::Control { .. } | Self::Time { .. } => None,
        }
    }

    /// Convert this identity back into a time product when possible.
    #[must_use]
    pub fn time_product(self) -> Option<TimeProduct> {
        match self {
            Self::Time { node_id, output } => Some(TimeProduct::new(NodeId::new(node_id), output)),
            Self::Visual { .. } | Self::Control { .. } => None,
        }
    }

    /// Convert this identity back into a control product when possible.
    #[must_use]
    pub fn control_product(self) -> Option<ControlProduct> {
        match self {
            Self::Control {
                node_id,
                output,
                rows,
                samples_per_row,
            } => Some(ControlProduct::new(
                NodeId::new(node_id),
                output,
                ControlExtent::new(rows, samples_per_row),
            )),
            Self::Visual { .. } | Self::Time { .. } => None,
        }
    }
}

/// Element format of a Studio control preview's samples.
///
/// Live previews arrive at `Srgb8` — the transport precision Studio asks for
/// (`PREVIEW_SAMPLE_FORMAT`): 8 bits, sRGB-encoded, so the codes land where
/// the screen shows them. `U16` (linear) is what a published buffer holds and
/// what a close inspection would ask for; `U8` (linear) is what an output
/// that publishes 8-bit sends verbatim. Consumers read samples through
/// [`UiControlProductPreview::unorm16_sample`], which decodes every format to
/// linear unorm16, so no decode path cares which one arrived.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiControlSampleFormat {
    U8,
    U16,
    Srgb8,
}

impl UiControlSampleFormat {
    /// Bytes one sample occupies in `bytes`.
    #[must_use]
    pub const fn bytes_per_sample(self) -> usize {
        match self {
            Self::U8 | Self::Srgb8 => 1,
            Self::U16 => 2,
        }
    }

    /// The wire's element format, as a preview carries it.
    #[must_use]
    pub const fn from_wire(format: lpc_wire::WireChannelSampleFormat) -> Self {
        match format {
            lpc_wire::WireChannelSampleFormat::U8 => Self::U8,
            lpc_wire::WireChannelSampleFormat::U16 => Self::U16,
            lpc_wire::WireChannelSampleFormat::Srgb8 => Self::Srgb8,
        }
    }
}

/// Data-driven preview for a native control product.
#[derive(Clone, Debug, PartialEq)]
pub struct UiControlProductPreview {
    /// Project revision that produced this sample payload.
    pub revision: i64,
    /// Native control sample extent.
    pub extent: ControlExtent,
    /// Native sample format.
    pub sample_format: UiControlSampleFormat,
    /// How to interpret the native sample buffer.
    pub sample_layout: ControlSampleLayout,
    /// Optional human-facing display layout for the sample data.
    ///
    /// Shared for the same reason as `bytes`: a dome-scale layout is 1500
    /// lamps (~bigger than the sample payload), the layout survives
    /// unchanged across ticks, and the per-tick preview rebuild must not
    /// deep-copy it.
    pub display_layout: Option<Rc<ControlDisplayLayout>>,
    /// Native sample bytes: one per sample at `U8` and `Srgb8`, two
    /// little-endian at `U16`. Read them through [`Self::unorm16_sample`].
    ///
    /// Shared (`Rc<[u8]>`) so cloning a preview into a view is a refcount bump,
    /// not a deep copy of the payload — the DTO tree is rebuilt often and these
    /// bytes dominate the per-tick cost.
    pub bytes: Rc<[u8]>,
}

impl UiControlProductPreview {
    /// Sample `index` as linear unorm16, whatever format it arrived in: a
    /// linear 8-bit level `k` widens to `k · 257`, an sRGB8 code decodes
    /// through the inverse transfer (`lpc_wire::srgb8_to_linear16`, which
    /// re-encodes to the same code), so 255 is full scale either way and
    /// every decode path downstream stays 16-bit. `None` past the buffer.
    #[must_use]
    pub fn unorm16_sample(&self, index: usize) -> Option<u16> {
        match self.sample_format {
            UiControlSampleFormat::U8 => self.bytes.get(index).map(|&v| u16::from(v) * 257),
            UiControlSampleFormat::Srgb8 => self
                .bytes
                .get(index)
                .map(|&code| lpc_wire::srgb8_to_linear16(code)),
            UiControlSampleFormat::U16 => {
                let at = index.checked_mul(2)?;
                let lo = *self.bytes.get(at)?;
                let hi = *self.bytes.get(at + 1)?;
                Some(u16::from_le_bytes([lo, hi]))
            }
        }
    }
}

/// UI mirror of `lpc_wire::WireVisualSpace` — which coordinate space a
/// visual producer renders in, or a preview probe asked for.
///
/// Ordered (1D before 2D) because per-space caches key on
/// `(product, space)` and the stacked preview renders in the same order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum UiVisualSpace {
    OneD,
    TwoD,
}

/// UI mirror of `lpc_wire::WireProjectionShape` — the base coordinate
/// map of a factored projection cell (THE FACTORIZATION, format v9).
/// `ExtrudeX` is today's extrude and the default.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum UiProjectionShape {
    #[default]
    ExtrudeX,
    ExtrudeY,
    Radial,
    Angular,
}

impl UiProjectionShape {
    /// The caption/tile label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::ExtrudeX => "extrude-x",
            Self::ExtrudeY => "extrude-y",
            Self::Radial => "radial",
            Self::Angular => "angular",
        }
    }

    /// The RAW model variant ident — what a shape tile's `EnsurePresent`
    /// appends to the shape enum row's address.
    #[must_use]
    pub const fn variant(self) -> &'static str {
        match self {
            Self::ExtrudeX => "ExtrudeX",
            Self::ExtrudeY => "ExtrudeY",
            Self::Radial => "Radial",
            Self::Angular => "Angular",
        }
    }

    /// Parse a RAW model variant ident; unknown idents read as the
    /// default `ExtrudeX` (the behavior-preserving anchor).
    #[must_use]
    pub fn from_variant(variant: &str) -> Self {
        match variant {
            "ExtrudeY" => Self::ExtrudeY,
            "Radial" => Self::Radial,
            "Angular" => Self::Angular,
            _ => Self::ExtrudeX,
        }
    }
}

/// UI mirror of `lpc_wire::WireCellProjection` — one FACTORED cell of the
/// 1D→2D projection matrix: a base shape plus the two boolean modifiers
/// (mirror folds the strip around the midpoint, flip reverses it — the
/// same uniform chain the engine runs).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UiCellProjection {
    pub shape: UiProjectionShape,
    pub mirror: bool,
    pub flip: bool,
}

impl UiCellProjection {
    /// A plain shape — no mirror, no flip.
    #[must_use]
    pub const fn plain(shape: UiProjectionShape) -> Self {
        Self {
            shape,
            mirror: false,
            flip: false,
        }
    }
}

/// UI mirror of `lpc_wire::WireProjectionOrigin` — which precedence arm
/// decided a resolved [`UiCellProjection`] (plan D15 preview captions, e.g.
/// `in 2D · radial (declared)`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// `ConsumerDefault` died with the v9 factorization: the producer always
/// declares, so a projection is a declaration or a consumer force.
pub enum UiProjectionOrigin {
    Declared,
    Forced,
}

/// UI mirror of `lpc_wire::WireConsumerPolicy` — the projection preference
/// a probe requests with, and whether it overrides an authored producer
/// opinion.
///
/// A forced-policy probe is exactly this with `force: true`: "show me
/// what THIS cell would look like", regardless of what the producer
/// declared. The section's choice tiles do NOT use it — they are
/// schematic drawings of the transform chain (see `ProjectionGlyph`),
/// because nothing web-side can issue an ad-hoc probe today.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiConsumerPolicy {
    pub default_1d_to_2d: UiCellProjection,
    pub force: bool,
}

impl UiConsumerPolicy {
    /// The defaults-only policy (plain extrude-x, never force) — what a
    /// caller that has never heard of spaces effectively sends.
    pub const AUTO: Self = Self {
        default_1d_to_2d: UiCellProjection::plain(UiProjectionShape::ExtrudeX),
        force: false,
    };

    /// The policy a live tile for `projection` probes with: force it, so
    /// the tile shows that cell and not the producer's declared answer.
    #[must_use]
    pub const fn forcing(projection: UiCellProjection) -> Self {
        Self {
            default_1d_to_2d: projection,
            force: true,
        }
    }
}

/// Space metadata a render-product probe answered alongside its preview
/// bytes.
///
/// Cached separately from [`UiProductPreview`] (mirroring how a clock's
/// `UiTimebaseRead` rides beside the preview cache in `ProjectSync` rather
/// than inside it) so a future per-card space request (P3) can read "what
/// did the producer answer" without widening every
/// [`UiProductPreview::VisualSrgb8`] construction site — most of which are
/// hand-built story/test fixtures with no probe behind them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiVisualProductSpace {
    /// The space this probe actually rendered in (the effective request
    /// space).
    pub space: UiVisualSpace,
    /// The 1D→2D cell applied to fill this frame, when one applied.
    pub projection: Option<UiCellProjection>,
    /// Why `projection` was chosen. Present exactly when `projection` is.
    pub origin: Option<UiProjectionOrigin>,
    /// The producer's own native space, independent of what was requested.
    pub primary: UiVisualSpace,
}

/// Small, serializable-enough preview state for a produced product.
///
/// Browser-specific DOM/canvas state belongs in the web crate. This DTO only
/// carries bounded preview bytes and durable error/loading state.
#[derive(Clone, Debug, PartialEq)]
pub enum UiProductPreview {
    /// The product slot has no product value yet.
    Empty,
    /// A probe has been requested or the product is waiting for its first probe.
    Pending,
    /// RGB8 visual preview bytes in row-major order.
    ///
    /// `bytes` is shared (`Rc<[u8]>`) so cloning the preview into a rebuilt view
    /// is a refcount bump rather than a copy of the (often large) RGB8 buffer.
    VisualSrgb8 {
        width: u32,
        height: u32,
        revision: i64,
        bytes: Rc<[u8]>,
    },
    /// Native control samples plus optional display layout.
    ControlNative(UiControlProductPreview),
    /// The product is represented by metadata only in this slice.
    MetadataOnly,
    /// The runtime explicitly does not support this preview.
    Unsupported { reason: String },
    /// The runtime failed while producing this preview.
    Error { message: String },
}

impl UiProductPreview {
    /// Default preview state for a product family.
    #[must_use]
    pub fn for_kind(kind: UiProductKind) -> Self {
        match kind {
            UiProductKind::Empty => Self::Empty,
            UiProductKind::Visual => Self::Pending,
            UiProductKind::Control => Self::Pending,
            // Not `Pending`: a time product has no preview to wait for, so
            // pending would be a spinner that never resolves.
            UiProductKind::Time => Self::MetadataOnly,
            UiProductKind::Other => Self::MetadataOnly,
        }
    }
}

/// One space's preview of a visual product — the unit the D15 preview
/// checkboxes stack.
///
/// A card that checks both spaces gets two of these for one product: the
/// same producer rendered along its strip and rendered into 2D texture
/// space, each with the metadata its caption needs (`native · 1D`,
/// `in 2D · radial (declared)`).
#[derive(Clone, Debug, PartialEq)]
pub struct UiProductSpaceView {
    /// The space this view was probed in.
    pub space: UiVisualSpace,
    /// Preview state for that probe, exactly like
    /// [`UiProducedProduct::preview`].
    pub preview: UiProductPreview,
    /// Frame geometry the probe asked for (a 1D probe is `N × 1`).
    pub frame: UiProductPreviewFrame,
    /// What the producer answered: resolved space, projection, origin,
    /// primary. `None` until a space-tagged result has landed.
    pub meta: Option<UiVisualProductSpace>,
    /// Whether this is the view [`UiProducedProduct::preview`] mirrors —
    /// the card's hero, and the one every space-unaware surface renders.
    pub hero: bool,
}

/// A produced output that deserves primary visual treatment in the node pane.
#[derive(Clone, Debug, PartialEq)]
pub struct UiProducedProduct {
    /// Product slot or friendly output name.
    pub name: String,
    /// Product family for presentation and labeling.
    pub kind: UiProductKind,
    /// Concrete product identity used by controllers to attach preview state.
    pub product: Option<UiProductRef>,
    /// Current preview state for this product — the HERO space's, when the
    /// card previews more than one (see [`Self::spaces`]).
    pub preview: UiProductPreview,
    /// Per-space previews for a visual product, when the card's D15
    /// checkboxes ask for them. **Empty is the ordinary state**: every
    /// space-unaware surface (module heroes, playlist thumbs, story
    /// fixtures) reads [`Self::preview`] and is unaffected. When populated
    /// it always CONTAINS the hero view too, so the stacked renderer can
    /// draw one uniform list.
    pub spaces: Vec<UiProductSpaceView>,
    /// Whether Studio is watching this product now.
    pub tracking: UiProductTrackingState,
    /// The "Show live" action for a preview that is not live: it selects
    /// the product's PRODUCER node, because under a device lens only the
    /// selected node's products stream. `None` while [`Self::tracking`] is
    /// `Tracking`, and wherever no producer node is known.
    pub show_live: Option<crate::UiAction>,
    /// Stable preview frame used even before bytes are available.
    pub frame: UiProductPreviewFrame,
    /// Optional size, shape, or sample-count detail.
    pub detail: Option<String>,
    /// Binding and revision metadata for the product.
    pub binding: UiProducedBinding,
    /// Binding authoring surface when this product is bindable (M4).
    pub authoring: Option<crate::UiBindingAuthoring>,
    /// Edited-state affordance for authored product metadata.
    pub dirty: UiNodeDirtyState,
}

impl UiProducedProduct {
    /// Create a produced product of the requested kind.
    pub fn new(name: impl Into<String>, kind: UiProductKind) -> Self {
        Self {
            name: name.into(),
            kind,
            product: None,
            preview: UiProductPreview::for_kind(kind),
            spaces: Vec::new(),
            tracking: UiProductTrackingState::Untracked,
            show_live: None,
            frame: UiProductPreviewFrame::VISUAL_DEFAULT,
            detail: None,
            binding: UiProducedBinding::none(),
            dirty: UiNodeDirtyState::Clean,
            authoring: None,
        }
    }

    /// Create a visual product.
    pub fn visual(name: impl Into<String>) -> Self {
        Self::new(name, UiProductKind::Visual)
    }

    /// Create an empty product placeholder.
    pub fn empty(name: impl Into<String>) -> Self {
        Self::new(name, UiProductKind::Empty)
    }

    /// Create a control product.
    pub fn control(name: impl Into<String>) -> Self {
        Self::new(name, UiProductKind::Control)
    }

    /// Create a time product (a clock's published timebase handle).
    pub fn time(name: impl Into<String>) -> Self {
        Self::new(name, UiProductKind::Time)
    }

    /// Add size or shape detail.
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Attach concrete product identity.
    #[must_use]
    pub fn with_product(mut self, product: UiProductRef) -> Self {
        self.product = Some(product);
        self
    }

    /// Attach current preview state.
    #[must_use]
    pub fn with_preview(mut self, preview: UiProductPreview) -> Self {
        self.preview = preview;
        self
    }

    /// Attach the current tracking state.
    #[must_use]
    pub fn with_tracking(mut self, tracking: UiProductTrackingState) -> Self {
        self.tracking = tracking;
        self
    }

    /// Attach stable preview frame geometry.
    #[must_use]
    pub fn with_frame(mut self, frame: UiProductPreviewFrame) -> Self {
        self.frame = frame;
        self
    }

    /// Shared detail aspects for produced product popups.
    pub fn visible_aspects(&self) -> Vec<UiSlotAspect> {
        vec![
            produced_product_info_aspect(self),
            self.binding.output_aspect(),
        ]
    }
}

impl UiProductKind {
    /// The kind a resolved product value presents as.
    ///
    /// The single mapping from "what the engine resolved" to "what chip the
    /// UI wears", so a product row, a wiring-drawer value box, and a bound
    /// control's live reading can never disagree about what `bus:time`
    /// carries.
    #[must_use]
    pub fn of_product_ref(product: ProductRef) -> Self {
        match product {
            ProductRef::Visual(_) => Self::Visual,
            ProductRef::Control(_) => Self::Control,
            ProductRef::Time(_) => Self::Time,
        }
    }

    /// Compact label for product detail rows — and the chip text a product
    /// value wears wherever Studio shows one instead of a number.
    pub fn detail_label(self) -> &'static str {
        match self {
            Self::Empty => "Empty product",
            Self::Visual => "Visual product",
            Self::Control => "Control product",
            Self::Time => "Time product",
            Self::Other => "Product",
        }
    }
}

fn produced_product_info_aspect(product: &UiProducedProduct) -> UiSlotAspect {
    let mut shape_row = UiSlotAspectRow::shape(UiSlotShape::Product(
        product.kind.detail_label().to_string(),
    ));
    if let Some(detail) = product.detail.as_ref() {
        shape_row = shape_row.with_detail(detail.clone());
    }

    let mut aspect = UiSlotAspect::new(UiSlotAspectKind::TypeInfo, "Info")
        .with_row(UiSlotAspectRow::new("Name", product.name.clone()))
        .with_row(shape_row);
    if let Some(size) = product_preview_size(&product.preview) {
        aspect = aspect.with_row(UiSlotAspectRow::new("Size", size));
    }
    aspect
}

/// Human-readable extent for a product preview, surfaced in the detail popup so
/// the product face can stay clean.
fn product_preview_size(preview: &UiProductPreview) -> Option<String> {
    match preview {
        UiProductPreview::VisualSrgb8 { width, height, .. } => Some(format!("{width} × {height}")),
        UiProductPreview::ControlNative(preview) => Some(format!(
            "{} × {} samples",
            preview.extent.rows, preview.extent.samples_per_row
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preview(sample_format: UiControlSampleFormat, bytes: &[u8]) -> UiControlProductPreview {
        UiControlProductPreview {
            revision: 1,
            extent: ControlExtent::new(1, 3),
            sample_format,
            sample_layout: ControlSampleLayout::default(),
            display_layout: None,
            bytes: Rc::from(bytes),
        }
    }

    /// `U8` widens by ×257 — 0 and 255 land on the 16-bit full-scale ends —
    /// and `U16` reads little-endian pairs; both stop at the buffer's end.
    #[test]
    fn samples_read_as_unorm16_in_either_format() {
        let narrow = preview(UiControlSampleFormat::U8, &[0, 1, 255]);
        assert_eq!(narrow.unorm16_sample(0), Some(0));
        assert_eq!(narrow.unorm16_sample(1), Some(257));
        assert_eq!(narrow.unorm16_sample(2), Some(u16::MAX));
        assert_eq!(narrow.unorm16_sample(3), None);

        let wide = preview(UiControlSampleFormat::U16, &[0x34, 0x12, 0xff, 0xff, 7]);
        assert_eq!(wide.unorm16_sample(0), Some(0x1234));
        assert_eq!(wide.unorm16_sample(1), Some(u16::MAX));
        assert_eq!(wide.unorm16_sample(2), None, "a half sample is no sample");

        // sRGB8 codes decode through the inverse transfer: code 1 is linear
        // 20/65535 (not 257), and every code re-encodes to itself.
        let display = preview(UiControlSampleFormat::Srgb8, &[0, 1, 255]);
        assert_eq!(display.unorm16_sample(0), Some(0));
        assert_eq!(display.unorm16_sample(1), Some(20));
        assert_eq!(display.unorm16_sample(2), Some(u16::MAX));
        assert_eq!(display.unorm16_sample(3), None);
        let every_code: Vec<u8> = (0..=u8::MAX).collect();
        let every = preview(UiControlSampleFormat::Srgb8, &every_code);
        for code in 0..=u8::MAX {
            let linear = every.unorm16_sample(usize::from(code)).expect("in range");
            assert_eq!(lpc_wire::linear16_to_srgb8(linear), code, "code {code}");
        }
    }
}
