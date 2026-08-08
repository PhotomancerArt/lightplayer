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
    /// Default visual-product probe frame (simulator tier).
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

/// Native control sample format carried by a Studio preview DTO.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiControlSampleFormat {
    U16,
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
    /// Native sample bytes, little-endian for `U16`.
    ///
    /// Shared (`Rc<[u8]>`) so cloning a preview into a view is a refcount bump,
    /// not a deep copy of the payload — the DTO tree is rebuilt often and these
    /// bytes dominate the per-tick cost.
    pub bytes: Rc<[u8]>,
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

/// UI mirror of `lpc_wire::WireCellProjection` — one cell of the 1D→2D
/// projection matrix.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiCellProjection {
    Extrude,
    Radial,
    Angular,
    Mirror,
}

/// UI mirror of `lpc_wire::WireProjectionOrigin` — which precedence arm
/// decided a resolved [`UiCellProjection`] (plan D15 preview captions, e.g.
/// `in 2D · radial (declared)`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiProjectionOrigin {
    Declared,
    ConsumerDefault,
    Forced,
}

/// UI mirror of `lpc_wire::WireConsumerPolicy` — the projection preference
/// a probe requests with, and whether it overrides an authored producer
/// opinion.
///
/// The tile picker's live tiles (P4) are exactly this with `force: true`:
/// "show me what THIS cell would look like", regardless of what the
/// producer declared.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiConsumerPolicy {
    pub default_1d_to_2d: UiCellProjection,
    pub force: bool,
}

impl UiConsumerPolicy {
    /// The defaults-only policy (extrude, never force) — what a caller
    /// that has never heard of spaces effectively sends.
    pub const AUTO: Self = Self {
        default_1d_to_2d: UiCellProjection::Extrude,
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
