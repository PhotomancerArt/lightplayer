//! Produced product data for primary node output surfaces.

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
    /// A product whose presentation is not known by Studio yet.
    Other,
}

/// A produced output that deserves primary visual treatment in the node pane.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiProducedProduct {
    /// Product slot or friendly output name.
    pub name: String,
    /// Product family for presentation and labeling.
    pub kind: UiProductKind,
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
    let mut shape_row = UiSlotAspectRow::shape(UiSlotShape::Product(
        product.kind.detail_label().to_string(),
    ));
    if let Some(detail) = product.detail.as_ref() {
        shape_row = shape_row.with_detail(detail.clone());
    }

    UiSlotAspect::new(UiSlotAspectKind::TypeInfo, "Info")
        .with_row(UiSlotAspectRow::new("Name", product.name.clone()))
        .with_row(shape_row)
}
