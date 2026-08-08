//! The shader hero's 1D/2D preview checkboxes and the stacked view they
//! produce (dimensionality plan-B P4 item 3, vision D15, spike §5).
//!
//! **Checkboxes, not radios.** 1D and 2D are independent questions — "what
//! does this shader actually render" and "how will it look on my matrix" —
//! so turning both on gives the stacked view as a free best-of-both rather
//! than a third mode. One box always stays on: an empty hero is not a
//! state the control can reach, and the invariant is enforced in core
//! (`NodeUiOp::toggle_preview_space` returns no ops for the last box), so
//! this bar cannot route around it.
//!
//! **The checked set is read back from the DATA, not from a local signal.**
//! `UiProducedProduct::spaces` is exactly what `ProjectSync` fanned probes
//! out for, so the boxes show what is actually being probed — a toggle that
//! the reducer dropped leaves the box visibly on, which is what "one must
//! stay on" should look like at the gesture.
//!
//! **Captions are the honesty rule (D11).** Every stacked frame names the
//! space it rendered in, the projection that filled it, and WHERE that
//! projection came from — `native · 1D`, `in 2D · radial (declared)`,
//! `in 2D · extrude (consumer default)`. Wording lives in
//! `super::space_section` with the rest of the G1 ruling.

use dioxus::prelude::*;
use lpa_studio_core::{
    NodeUiOp, UiAction, UiPreviewSpaces, UiProducedProduct, UiProductPreviewFrame,
    UiProductSpaceView, UiVisualSpace,
};

use crate::app::node::ProductPreview;

use super::node_ui_action;
use super::space_section::{
    PREVIEW_SPACES_LAST_ON, PREVIEW_SPACES_TITLE_ONE_D, PREVIEW_SPACES_TITLE_TWO_D,
    preview_space_caption, visual_space_label,
};

/// How flat a 1D preview draws.
///
/// A 1D probe frame is literally `N × 1`, and an aspect ratio of 32:1 puts
/// a ~10px sliver on the card — technically the shape of the data, visually
/// a hairline. The strip renders as a BAND instead (the spike's 384×40),
/// which is the same information at a height the eye can read. Only the
/// frame's display aspect is adjusted; the probe, the bytes, and the canvas
/// are untouched.
const ONE_D_BAND_ASPECT: u32 = 8;

/// The 1D/2D checkbox bar (the `MapViewToggles` shape and its
/// `ux-map-toggle` classes — this is the same kind of control on the same
/// kind of bar, and two toggle vocabularies on one card would be one too
/// many).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn PreviewSpaceToggles(
    /// The node's address path — the key the card-UI op carries back.
    /// `None` (stories rendering the face bare) leaves the bar inert.
    #[props(default = None)]
    node: Option<String>,
    /// The RESOLVED checked set (see [`preview_space_state`]).
    spaces: UiPreviewSpaces,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    rsx! {
        for space in [UiVisualSpace::OneD, UiVisualSpace::TwoD] {
            {
                let checked = spaces.is_checked(space);
                // The last checked box has nothing to dispatch, and says so
                // rather than firing an op the reducer will drop.
                let last_on = checked && spaces.toggled(space, false).is_none();
                let node = node.clone();
                rsx! {
                    button {
                        key: "{space:?}",
                        class: if checked { "ux-map-toggle ux-map-toggle-on" } else { "ux-map-toggle" },
                        r#type: "button",
                        title: if last_on { PREVIEW_SPACES_LAST_ON } else { toggle_title(space) },
                        aria_pressed: "{checked}",
                        onclick: move |event| {
                            event.stop_propagation();
                            let (Some(node), Some(handler)) = (node.clone(), on_action) else {
                                return;
                            };
                            for op in NodeUiOp::toggle_preview_space(&node, spaces, space, !checked)
                            {
                                handler.call(node_ui_action(op));
                            }
                        },
                        span { class: "tw:text-[11px] tw:font-bold tw:leading-none",
                            "{visual_space_label(space)}"
                        }
                    }
                }
            }
        }
    }
}

/// The product's hero: one framed preview per checked space, each under its
/// own caption.
///
/// A product with no per-space views — every space-unaware surface, and
/// every hand-built story fixture — renders exactly today's single preview,
/// captionless. That is the whole compatibility story: `spaces` empty is
/// the ordinary state.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn SpacedProductPreview(
    product: UiProducedProduct,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    if product.spaces.is_empty() {
        return rsx! {
            ProductPreview {
                kind: product.kind,
                preview: product.preview.clone(),
                tracking: product.tracking,
                frame: product.frame,
                focus_action: None,
                on_action,
            }
        };
    }

    rsx! {
        for (index , view) in product.spaces.clone().into_iter().enumerate() {
            div {
                key: "{view.space:?}",
                class: if index == 0 {
                    "tw:grid tw:min-w-0"
                } else {
                    "tw:grid tw:min-w-0 tw:border-t tw:border-border-strong"
                },
                ProductPreview {
                    kind: product.kind,
                    preview: view.preview.clone(),
                    tracking: product.tracking,
                    frame: display_frame(&view),
                    focus_action: None,
                    on_action,
                }
                // Always rendered, whatever the frame turned out to be: a
                // GPU-resident runtime answers with a no-readback refusal
                // (`UiProductPreview::Unsupported`, which the frame renders
                // as its warning message), and the caption is then the only
                // thing that still says WHICH space was asked for.
                span { class: CAPTION_CLASS, "{preview_space_caption(view.space, view.meta)}" }
            }
        }
    }
}

/// The checked set this card's bar shows, resolved exactly as core resolved
/// it: the probed spaces when a fan-out has landed, and the producer's
/// primary alone before it (`NodeCardUiState::preview_spaces_for`'s
/// default, mirrored here so the bar never contradicts the hero).
pub(crate) fn preview_space_state(
    product: &UiProducedProduct,
    declared: Option<UiVisualSpace>,
) -> UiPreviewSpaces {
    if product.spaces.is_empty() {
        return UiPreviewSpaces::only(primary_space(product, declared));
    }
    UiPreviewSpaces {
        one_d: has_space(product, UiVisualSpace::OneD),
        two_d: has_space(product, UiVisualSpace::TwoD),
    }
}

fn has_space(product: &UiProducedProduct, space: UiVisualSpace) -> bool {
    product.spaces.iter().any(|view| view.space == space)
}

/// The producer's native space: the probe's answer when one has landed, the
/// card's own declaration otherwise, and 2D when neither is known (which is
/// exactly what an unregistered product is probed as).
fn primary_space(product: &UiProducedProduct, declared: Option<UiVisualSpace>) -> UiVisualSpace {
    product
        .spaces
        .iter()
        .find_map(|view| view.meta.map(|meta| meta.primary))
        .or(declared)
        .unwrap_or(UiVisualSpace::TwoD)
}

/// The aspect a space's frame DISPLAYS at (see [`ONE_D_BAND_ASPECT`]).
fn display_frame(view: &UiProductSpaceView) -> UiProductPreviewFrame {
    match view.space {
        UiVisualSpace::OneD => UiProductPreviewFrame::new(
            view.frame.width,
            view.frame.width.div_ceil(ONE_D_BAND_ASPECT),
        ),
        UiVisualSpace::TwoD => view.frame,
    }
}

fn toggle_title(space: UiVisualSpace) -> &'static str {
    match space {
        UiVisualSpace::OneD => PREVIEW_SPACES_TITLE_ONE_D,
        UiVisualSpace::TwoD => PREVIEW_SPACES_TITLE_TWO_D,
    }
}

const CAPTION_CLASS: &str = "tw:px-2 tw:py-1 tw:text-center tw:font-mono tw:text-[10.5px] tw:leading-none tw:text-dim-foreground";

#[cfg(test)]
mod tests {
    use lpa_studio_core::{UiProductPreview, UiVisualProductSpace};

    use super::*;

    fn view(space: UiVisualSpace, primary: UiVisualSpace) -> UiProductSpaceView {
        UiProductSpaceView {
            space,
            preview: UiProductPreview::Pending,
            frame: UiProductPreviewFrame::new(
                32,
                if space == UiVisualSpace::OneD { 1 } else { 32 },
            ),
            meta: Some(UiVisualProductSpace {
                space,
                projection: None,
                origin: None,
                primary,
            }),
            hero: space == primary,
        }
    }

    /// Before any fan-out lands, the bar shows the producer's primary and
    /// only that — D15's default, and the same one core resolves.
    #[test]
    fn an_unprobed_card_shows_its_primary_space_alone() {
        let product = UiProducedProduct::visual("output");
        assert_eq!(
            preview_space_state(&product, Some(UiVisualSpace::OneD)),
            UiPreviewSpaces::only(UiVisualSpace::OneD)
        );
        assert_eq!(
            preview_space_state(&product, None),
            UiPreviewSpaces::only(UiVisualSpace::TwoD),
            "an unregistered product is probed in 2D, so that is what the bar may claim"
        );
    }

    /// Once views land they ARE the checked set — the probe answer outranks
    /// the declaration (a shader whose slot says 1D but whose product
    /// reports 2D must not show a bar that disagrees with its own hero).
    #[test]
    fn the_probed_views_are_the_checked_set() {
        let mut product = UiProducedProduct::visual("output");
        product.spaces = vec![
            view(UiVisualSpace::OneD, UiVisualSpace::OneD),
            view(UiVisualSpace::TwoD, UiVisualSpace::OneD),
        ];
        assert_eq!(
            preview_space_state(&product, Some(UiVisualSpace::TwoD)),
            UiPreviewSpaces {
                one_d: true,
                two_d: true
            }
        );

        product.spaces = vec![view(UiVisualSpace::TwoD, UiVisualSpace::TwoD)];
        assert_eq!(
            preview_space_state(&product, None),
            UiPreviewSpaces::only(UiVisualSpace::TwoD)
        );
    }

    /// A 1D frame draws as a band, not as the 32:1 hairline its probe
    /// geometry literally is.
    #[test]
    fn a_one_d_frame_displays_as_a_band() {
        let strip = display_frame(&view(UiVisualSpace::OneD, UiVisualSpace::OneD));
        assert_eq!((strip.width, strip.height), (32, 4));
        let square = display_frame(&view(UiVisualSpace::TwoD, UiVisualSpace::TwoD));
        assert_eq!((square.width, square.height), (32, 32));
    }
}
