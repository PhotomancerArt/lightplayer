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
//!
//! # `fixture=` (G1 round 2)
//!
//! One project with two fixtures makes "first of the family" ambiguous:
//! both `disc` and `grid` produce a control product. `fixture=<node>`
//! names which one, matched against the node's own identity in the view
//! (`UiNodeChild::label` / `UiNodeView::header.title`, falling back to the
//! last segment of its address) — the same names `module.json` keys its
//! nodes by. A `fixture=` that resolves to nothing renders the loading
//! box rather than silently drawing the *other* shape: a map labelled
//! "the ring" that shows the grid is worse than one that has not arrived.
//!
//! `view=product` (no `fixture=`) stays first-of-family — the shader's own
//! render, which the article uses as its hero and which this embed
//! presents wider than the map boxes (`ux-docs-hero-product`).

use dioxus::prelude::*;
use lpa_studio_core::{
    UiNodeChild, UiNodeFace, UiNodeView, UiProducedProduct, UiProductKind, UiStudioView,
    UiViewContent,
};

use crate::app::node::ProductPreview;

use super::docs_sims::DocsSimRegistry;
use super::embed_frame::{EmbedFrame, EmbedLoading, EmbedProblem};

/// Reserved height for a map box — `ux-produced-product-frame-capped`'s own
/// 320px cap, so a square-ish map lands exactly where the loading box was.
const MAP_HEIGHT: u32 = 320;

/// Reserved height for the hero, matching `ux-docs-hero-product`'s widened
/// cap (a square visual product fills it exactly).
const HERO_HEIGHT: u32 = 512;

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
    /// The article's `fixture=` handle: which node's product to draw when
    /// the sim has more than one of the asked-for family.
    #[props(default)]
    fixture: Option<String>,
) -> Element {
    let hero = view == SimCanvasView::Product && fixture.is_none();
    let Some(registry) = try_consume_context::<DocsSimRegistry>() else {
        return rsx! {
            DocsSimCanvas { view, hero }
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
    let product = canvas_product(&entry.view.read(), view, fixture.as_deref());
    rsx! {
        DocsSimCanvas { product, view, hero }
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
    /// The article's hero presentation: the same real preview, widened past
    /// the map boxes' 320px cap. Reserved height follows, so the swap from
    /// loading to live is still a no-jump swap.
    #[props(default)]
    hero: bool,
    /// Caption above the surface.
    #[props(default)]
    caption: Option<String>,
) -> Element {
    let body_class = if hero { "ux-docs-hero-product" } else { "" };
    let reserved = if hero { HERO_HEIGHT } else { MAP_HEIGHT };
    rsx! {
        EmbedFrame {
            caption,
            note: if product.is_none() { Some("Loading".to_string()) } else { None },
            div { class: "{body_class}",
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
                        EmbedLoading { message: view.waiting_for().to_string(), min_height: reserved }
                    },
                }
            }
        }
    }
}

/// The product `view` asks for in the lensed session's node tree: the one
/// belonging to the node `fixture` names, or — with no `fixture` — the
/// first of the family (the module's visual mirror, for the hero).
pub(crate) fn canvas_product(
    studio_view: &UiStudioView,
    view: SimCanvasView,
    fixture: Option<&str>,
) -> Option<UiProducedProduct> {
    let kind = view.kind();
    studio_view.panes.iter().find_map(|pane| {
        let UiViewContent::ProjectEditor(editor) = &pane.body else {
            return None;
        };
        editor
            .nodes
            .iter()
            .find_map(|node| node_product(node, kind, fixture))
    })
}

/// Depth-first over a workspace node and everything under it.
fn node_product(
    node: &UiNodeView,
    kind: UiProductKind,
    fixture: Option<&str>,
) -> Option<UiProducedProduct> {
    named(&node.header.title, &node.header.path, fixture)
        .then(|| face_product(node.face.as_ref(), kind))
        .flatten()
        .or_else(|| {
            node.children
                .iter()
                .find_map(|child| child_product(child, kind, fixture))
        })
}

/// Depth-first over a nested child card and everything under it.
fn child_product(
    child: &UiNodeChild,
    kind: UiProductKind,
    fixture: Option<&str>,
) -> Option<UiProducedProduct> {
    named(&child.label, &child.detail, fixture)
        .then(|| face_product(child.face.as_ref(), kind))
        .flatten()
        .or_else(|| {
            child
                .children
                .iter()
                .find_map(|nested| child_product(nested, kind, fixture))
        })
}

/// Whether this node answers to the article's `fixture=` handle. No handle
/// asked means every node answers (the first-of-family walk).
///
/// A node carries two identities in the view — its display label (the use
/// name `module.json` keyed it by) and its address — and the article's
/// handle may legitimately be either spelling, so both are tried. Case is
/// ignored because a display label may be title-cased where the file's key
/// is not.
fn named(label: &str, address: &str, fixture: Option<&str>) -> bool {
    let Some(wanted) = fixture else {
        return true;
    };
    if label.eq_ignore_ascii_case(wanted) {
        return true;
    }
    address
        .rsplit(['/', '.'])
        .next()
        .is_some_and(|segment| segment.eq_ignore_ascii_case(wanted))
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
        assert!(canvas_product(&view, SimCanvasView::Map, None).is_none());
        assert!(canvas_product(&view, SimCanvasView::Product, None).is_none());
        assert!(canvas_product(&view, SimCanvasView::Map, Some("disc")).is_none());
    }

    #[test]
    fn a_faceless_node_contributes_nothing() {
        assert!(face_product(None, UiProductKind::Control).is_none());
    }

    /// The whole point of `fixture=`: with two fixtures in one project,
    /// only the named node's product may answer.
    #[test]
    fn a_fixture_handle_matches_the_nodes_label_or_its_address_tail() {
        assert!(named("disc", "/plasma.module/disc", Some("disc")));
        assert!(named("Disc", "/plasma.module/disc", Some("disc")));
        assert!(named("Ring", "/plasma.module/disc", Some("disc")));
        assert!(!named("grid", "/plasma.module/grid", Some("disc")));
    }

    /// No `fixture=` is the first-of-family walk, so every node answers.
    #[test]
    fn no_fixture_handle_matches_every_node() {
        assert!(named("grid", "/plasma.module/grid", None));
        assert!(named("", "", None));
    }
}
