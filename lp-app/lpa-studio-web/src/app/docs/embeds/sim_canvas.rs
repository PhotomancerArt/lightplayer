//! `sim-canvas`: one docs sim's live output, drawn by the **real** editor
//! preview component.
//!
//! There is no docs-specific renderer here. [`ProductPreview`] is the same
//! component the node cards, the module face hero, and play mode render
//! their previews with — `view=map` lands on its control-product branch
//! (the 2D lamp layout) and `view=product` on the visual pixel grid. A
//! reader looking at an article is looking at Studio.
//!
//! Product selection walks the lensed session's node tree and takes the
//! first product of the asked-for family: the fixture's control product
//! for a map, the module's visual mirror for a product. Selecting by
//! *kind* rather than by populated preview state is deliberate — the
//! product appears in the view before its first frame does, and
//! `ProductPreview` already draws its own settling skeleton at the right
//! aspect ratio, so the box never resizes under the reader.

use dioxus::prelude::*;
use lpa_studio_core::{
    UiNodeChild, UiNodeFace, UiNodeView, UiProducedProduct, UiProductKind, UiStudioView,
    UiViewContent,
};

use crate::app::node::ProductPreview;

use super::docs_sims::DocsSimRegistry;
use super::embed_frame::{EmbedFrame, EmbedLoading, EmbedProblem};

/// Which face of a sim's output the fence asked for.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum SimCanvasView {
    /// The 2D lamp layout — where the LEDs actually are.
    #[default]
    Map,
    /// The rendered visual buffer — what the shader drew.
    Product,
}

impl SimCanvasView {
    /// Parse a `view=` argument; unknown words are author error, reported
    /// by the caller.
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "map" => Some(Self::Map),
            "product" => Some(Self::Product),
            _ => None,
        }
    }

    /// The product family this view renders.
    fn kind(self) -> UiProductKind {
        match self {
            Self::Map => UiProductKind::Control,
            Self::Product => UiProductKind::Visual,
        }
    }

    /// What the loading state says it is waiting for.
    fn waiting_for(self) -> &'static str {
        match self {
            Self::Map => "Starting the simulator — the lamp layout appears here.",
            Self::Product => "Starting the simulator — the rendered frame appears here.",
        }
    }
}

/// The fence, resolved against page context.
///
/// No provider (the story book, host builds) renders exactly what "the sim
/// has not produced anything yet" renders — the calm loading box — so an
/// article never shows scaffolding it cannot fill.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn SimCanvasEmbed(
    /// The article's `sim=` handle.
    sim: String,
    #[props(default)] view: SimCanvasView,
) -> Element {
    let Some(registry) = try_consume_context::<DocsSimRegistry>() else {
        return rsx! {
            DocsSimCanvas { view }
        };
    };
    let Some(entry) = registry.get(&sim) else {
        return rsx! {
            EmbedProblem {
                message: format!(
                    "`sim-canvas` names sim `{sim}`, which this page does not declare.",
                ),
            }
        };
    };
    let product = canvas_product(&entry.view.read(), view);
    rsx! {
        DocsSimCanvas { product, view }
    }
}

/// One sim's output surface, in the shared embed chrome.
///
/// `product` is the resolved [`UiProducedProduct`]; `None` means the sim
/// has not produced one yet (it is still booting), which renders the calm
/// reserved-height loading state. Stories pass a fixture straight in —
/// that is the seam that keeps live workers out of the story book.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn DocsSimCanvas(
    /// The product to draw, when the sim has one.
    #[props(default)]
    product: Option<UiProducedProduct>,
    /// Which face was asked for (drives the loading copy).
    #[props(default)]
    view: SimCanvasView,
    /// Caption above the surface.
    #[props(default)]
    caption: Option<String>,
) -> Element {
    rsx! {
        EmbedFrame {
            caption,
            note: if product.is_none() { Some("Loading".to_string()) } else { None },
            match product {
                Some(product) => rsx! {
                    // The real preview component, dispatch-free: a docs
                    // canvas is something to look at, not a control.
                    ProductPreview {
                        kind: product.kind,
                        preview: product.preview.clone(),
                        tracking: product.tracking,
                        frame: product.frame,
                        focus_action: None,
                        on_action: None,
                    }
                },
                None => rsx! {
                    // 320px is `ux-produced-product-frame-capped`'s own cap:
                    // a square-ish map (both of the article's) lands exactly
                    // there, so the box does not resize when it arrives.
                    EmbedLoading { message: view.waiting_for().to_string(), min_height: 320 }
                },
            }
        }
    }
}

/// The first product of `view`'s family anywhere in the lensed session's
/// node tree — the fixture's control product for a map, the module's
/// visual mirror for a product.
pub(crate) fn canvas_product(
    studio_view: &UiStudioView,
    view: SimCanvasView,
) -> Option<UiProducedProduct> {
    let kind = view.kind();
    studio_view.panes.iter().find_map(|pane| {
        let UiViewContent::ProjectEditor(editor) = &pane.body else {
            return None;
        };
        editor
            .nodes
            .iter()
            .find_map(|node| node_product(node, kind))
    })
}

/// Depth-first over a workspace node and everything under it.
fn node_product(node: &UiNodeView, kind: UiProductKind) -> Option<UiProducedProduct> {
    face_product(node.face.as_ref(), kind).or_else(|| {
        node.children
            .iter()
            .find_map(|child| child_product(child, kind))
    })
}

/// Depth-first over a nested child card and everything under it.
fn child_product(child: &UiNodeChild, kind: UiProductKind) -> Option<UiProducedProduct> {
    face_product(child.face.as_ref(), kind).or_else(|| {
        child
            .children
            .iter()
            .find_map(|nested| child_product(nested, kind))
    })
}

/// The product a face carries, when it is of the wanted family. Faces
/// without a hero preview (output, playlist, clock, bare control groups)
/// carry nothing this embed can draw.
fn face_product(face: Option<&UiNodeFace>, kind: UiProductKind) -> Option<UiProducedProduct> {
    let product = match face? {
        UiNodeFace::Module(face) => face.preview.clone()?,
        UiNodeFace::Fixture(face) => face.preview.clone(),
        UiNodeFace::Shader(face) => face.preview.clone(),
        UiNodeFace::Output(_)
        | UiNodeFace::Playlist(_)
        | UiNodeFace::Clock(_)
        | UiNodeFace::Controls(_) => return None,
    };
    (product.kind == kind).then_some(product)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_arguments_parse_to_the_two_faces() {
        assert_eq!(SimCanvasView::parse("map"), Some(SimCanvasView::Map));
        assert_eq!(
            SimCanvasView::parse("product"),
            Some(SimCanvasView::Product)
        );
        assert_eq!(SimCanvasView::parse("maps"), None);
        assert_eq!(SimCanvasView::parse(""), None);
    }

    #[test]
    fn each_view_asks_for_its_own_product_family() {
        assert_eq!(SimCanvasView::Map.kind(), UiProductKind::Control);
        assert_eq!(SimCanvasView::Product.kind(), UiProductKind::Visual);
    }

    /// A view with nothing in it (the sim is still booting) resolves no
    /// product rather than panicking — that is the loading state's input.
    #[test]
    fn an_empty_view_has_no_product_yet() {
        let view = UiStudioView::empty();
        assert!(canvas_product(&view, SimCanvasView::Map).is_none());
        assert!(canvas_product(&view, SimCanvasView::Product).is_none());
    }

    #[test]
    fn a_faceless_node_contributes_nothing() {
        assert!(face_product(None, UiProductKind::Control).is_none());
    }
}
