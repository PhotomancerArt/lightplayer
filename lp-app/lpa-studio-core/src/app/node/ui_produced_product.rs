//! Produced product data for primary node output surfaces.

use lpc_model::{ControlProduct, NodeId, ProductRef, VisualProduct};

use crate::{UiNodeDirtyState, UiProducedBinding, UiSlotAspect, UiSlotAspectKind, UiSlotAspectRow};

/// The family of product a node emits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiProductKind {
    /// No product has been resolved for this output yet.
    Empty,
    /// A visual image, shader result, or other displayable surface.
    Visual,
    /// A control stream, fixture map, or nonvisual device output.
    Control,
    /// A product whose presentation is not known by Studio yet.
    Other,
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
}

impl UiProductRef {
    /// Convert a model product ref into the UI identity used for preview state.
    #[must_use]
    pub fn from_product_ref(product: ProductRef) -> Self {
        match product {
            ProductRef::Visual(product) => Self::from_visual_product(product),
            ProductRef::Control(product) => Self::from_control_product(product),
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

    /// Convert this identity back into a visual product when possible.
    #[must_use]
    pub fn visual_product(self) -> Option<VisualProduct> {
        match self {
            Self::Visual { node_id, output } => {
                Some(VisualProduct::new(NodeId::new(node_id), output))
            }
            Self::Control { .. } => None,
        }
    }
}

/// Small, serializable-enough preview state for a produced product.
///
/// Browser-specific DOM/canvas state belongs in the web crate. This DTO only
/// carries bounded preview bytes and durable error/loading state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiProductPreview {
    /// The product slot has no product value yet.
    Empty,
    /// A probe has been requested or the product is waiting for its first probe.
    Pending,
    /// RGB8 visual preview bytes in row-major order.
    VisualSrgb8 {
        width: u32,
        height: u32,
        revision: i64,
        bytes: Vec<u8>,
    },
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
            UiProductKind::Control | UiProductKind::Other => Self::MetadataOnly,
        }
    }
}

/// A produced output that deserves primary visual treatment in the node pane.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiProducedProduct {
    /// Product slot or friendly output name.
    pub name: String,
    /// Product family for presentation and labeling.
    pub kind: UiProductKind,
    /// Concrete product identity used by controllers to attach preview state.
    pub product: Option<UiProductRef>,
    /// Current preview state for this product.
    pub preview: UiProductPreview,
    /// Optional size, shape, or sample-count detail.
    pub detail: Option<String>,
    /// Binding and revision metadata for the product.
    pub binding: UiProducedBinding,
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
            detail: None,
            binding: UiProducedBinding::none(),
            dirty: UiNodeDirtyState::Clean,
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

    /// Shared detail aspects for produced product popups.
    pub fn visible_aspects(&self) -> Vec<UiSlotAspect> {
        vec![
            produced_product_info_aspect(self),
            self.binding.output_aspect(),
        ]
    }
}

impl UiProductKind {
    /// Compact label for product detail rows.
    pub fn detail_label(self) -> &'static str {
        match self {
            Self::Empty => "Empty product",
            Self::Visual => "Visual product",
            Self::Control => "Control product",
            Self::Other => "Product",
        }
    }
}

fn produced_product_info_aspect(product: &UiProducedProduct) -> UiSlotAspect {
    let mut shape_row = UiSlotAspectRow::new("Shape", product.kind.detail_label());
    if let Some(detail) = product.detail.as_ref() {
        shape_row = shape_row.with_detail(detail.clone());
    }

    UiSlotAspect::new(UiSlotAspectKind::TypeInfo, "Info")
        .with_row(UiSlotAspectRow::new("Name", product.name.clone()))
        .with_row(shape_row)
}
