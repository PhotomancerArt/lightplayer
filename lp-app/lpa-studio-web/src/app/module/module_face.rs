//! The module card's permanent face — **one face, every depth**.
//!
//! `docs/design/modules.md` §5. The root module wears this as the single
//! top-level workspace card (the flat-root reversal); an embedded module
//! wears the identical component as a child card inside its host, one level
//! in; play mode renders the panel alone with no face at all
//! ([`super::PlayModeSurface`]).
//!
//! Top-down, in the flat [`NodeCardSection`] grammar:
//!
//! 1. `output` — the module's product hero. It leads with the scope's
//!    `control.out` — the lamps a fixture project actually drives — and a
//!    two-state block in its upper right switches to the R7 `visual.out`
//!    mirror when the scope resolves both. A module resolving neither has
//!    no hero, which is a legitimate shape (E6).
//! 2. `panel` — bus-as-**controls** (R8): this scope's channels plus each
//!    child module's panel as a nested group.
//! 3. `wiring` — bus-as-**writers/readers**, as a drawer: exactly the
//!    retired sidebar bus pane's content, scoped to this module and hung
//!    off it (P3). The split is deliberate: controls are the product
//!    surface, wiring is an authoring diagnostic. Its open state is
//!    core-owned (`NodeCardUiState::wiring_open`), like every other card
//!    drawer.
//! 4. provenance — a quiet footer line (§8), derived from the module def's
//!    authored `ProvenanceDef` fields.
//!
//! **Children are NOT here.** They expand under the card as full sibling
//! cards, via [`crate::app::node::NodeChildren`] — the grammar the playlist
//! face and the old project node already use. All of them render, with no
//! active-child filtering: a module's children are collaborators, not
//! branches. A child module therefore wears this same face in a card of its
//! own, which is the "one face at every depth" claim made literal *and*
//! keeps the host card a fixed, readable height.
//!
//! An embedded module's controls consequently appear twice — as a nested
//! group on this panel, and on the child module's own card below. That is
//! deliberate, and it is the playlist precedent (a bound control shows on
//! the parent's face and on the child's card): one control, two views
//! (`panel.md` P1), which the shared `(scope, channel)` identity keeps in
//! lockstep.
//!
//! Widget writes ride their own wire op (`PanelWriteOp`, panel.md P8) via
//! the control's `panel_target`; nothing in this face depends on which
//! path carries the value.

use dioxus::prelude::*;
use lpa_studio_core::{
    ModuleHeroProduct, NodeCardDrawer, NodeUiOp, UiAction, UiModuleFace as UiModuleFaceData,
};

use crate::app::WiringDrawerBody;
use crate::app::node::{NodeCardSection, ProductPreview, node_ui_action};
use crate::base::{NodeKindIcon, StudioIcon, StudioIconName};

use super::{ModulePanel, PanelGesture};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ModuleFace(
    face: UiModuleFaceData,
    /// The module's address path — the card UI state key the wiring
    /// drawer's toggle op carries back. `None` in story fixtures that
    /// render a face outside a card, where there is nothing to key.
    #[props(default = None)]
    node: Option<String>,
    #[props(default = None)] on_panel: Option<EventHandler<PanelGesture>>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let preview = face.preview.clone();
    let hero_choice = face.hero_choice;
    let hero_node = node.clone();
    let wiring_open = face.wiring_open;
    let wiring_summary = face.wiring.as_ref().map(|wiring| {
        let channels = wiring.channels.len();
        let noun = if channels == 1 { "channel" } else { "channels" };
        format!("{channels} {noun} · writers → readers")
    });

    rsx! {
        if let Some(preview) = preview {
            NodeCardSection { label: "output", first: true,
                // The wrapper shrink-wraps the capped preview frame (which
                // advertises an intrinsic width) so the corner overlay lands
                // on the MEDIA's corner, not the section's.
                div { class: "tw:relative tw:mx-auto tw:w-fit tw:max-w-full",
                    ProductPreview {
                        kind: preview.kind,
                        preview: preview.preview.clone(),
                        tracking: preview.tracking,
                        frame: preview.frame,
                        focus_action: None,
                        on_action,
                    }
                    if let Some(current) = hero_choice {
                        HeroProductToggle { node: hero_node, current, on_action }
                    }
                }
            }
        }
        // The panel's teaching treatment (spike gate 2): the panel-primary
        // wash + the ▶ rail icon mark THE performable surface — this
        // section is what play mode renders. Leaf cards say "settings";
        // only modules wear "panel".
        NodeCardSection {
            label: "panel",
            first: face.preview.is_none(),
            panel_tint: true,
            icon: StudioIconName::Play,
            ModulePanel {
                panel: face.panel.clone(),
                auto_save: face.auto_save,
                on_panel,
                on_action,
            }
        }
        if let Some(wiring) = face.wiring.clone() {
            NodeCardSection {
                label: "wiring",
                summary: wiring_summary,
                open: Some(wiring_open),
                on_toggle: move |()| {
                    // Core-owned disclosure, exactly like the other card
                    // drawers: the op is keyed by the module's address so
                    // the open state survives re-render and is e2e-drivable.
                    if let (Some(handler), Some(node)) = (on_action, node.as_ref()) {
                        handler.call(node_ui_action(NodeUiOp::SetDrawer {
                            node: node.clone(),
                            drawer: NodeCardDrawer::Wiring,
                            open: !wiring_open,
                        }));
                    }
                },
                div { class: "tw:grid tw:min-w-0 tw:gap-2 tw:px-4 tw:py-3",
                    p { class: "tw:m-0 tw:text-xs tw:leading-snug tw:text-dim-foreground",
                        "Every channel in this module's scope, and what writes and reads it. "
                        "The panel above is the same bus, presented for playing rather than patching."
                    }
                    WiringDrawerBody { view: wiring, on_action: move |action| {
                        if let Some(handler) = on_action {
                            handler.call(action);
                        }
                    } }
                }
            }
        }
        if let Some(provenance) = face.provenance.clone() {
            div { class: "tw:border-t tw:border-border-strong tw:px-4 tw:py-2 tw:text-xs tw:text-dim-foreground",
                "{provenance}"
            }
        }
    }
}

/// The hero's product switch, overlaid in the preview's UPPER RIGHT — the
/// corner and the chip weight the gallery card's tier badge already uses
/// (`home/card_thumb.rs`), so a small mark riding a live preview reads the
/// same everywhere.
///
/// Rendered only when the face offers a choice (both primaries resolve on
/// the module's scope); a one-product module has nothing to switch between,
/// so its hero keeps a clean corner.
///
/// A discrete two-state control, so **squared blocks** rather than a slider
/// or a pill (the panel's discrete-control language). Each block names its
/// own state and dispatches that state outright rather than "the other
/// one", so the active block is simply inert instead of flipping under a
/// second click.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn HeroProductToggle(
    /// The module's address path — the card UI state key the op carries
    /// back. `None` in story fixtures rendering a face outside a card,
    /// where the blocks show their states but have nothing to write to.
    node: Option<String>,
    /// Which product the hero is showing now.
    current: ModuleHeroProduct,
    on_action: Option<EventHandler<UiAction>>,
) -> Element {
    rsx! {
        div {
            class: "tw:absolute tw:right-1.5 tw:top-1.5 tw:flex tw:divide-x tw:divide-border tw:overflow-hidden tw:rounded-sm tw:border tw:border-border tw:bg-background/70",
            role: "group",
            aria_label: "Hero product",
            HeroProductBlock {
                node: node.clone(),
                current,
                product: ModuleHeroProduct::Control,
                // The node-kind glyphs read as the products themselves here:
                // the fixture's bulb IS the lamps, the texture's frame IS the
                // raster.
                icon: StudioIconName::NodeKind(NodeKindIcon::Fixture),
                label: "lamps",
                on_action,
            }
            HeroProductBlock {
                node,
                current,
                product: ModuleHeroProduct::Visual,
                icon: StudioIconName::NodeKind(NodeKindIcon::Texture),
                label: "raster",
                on_action,
            }
        }
    }
}

/// One state of the hero toggle.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn HeroProductBlock(
    node: Option<String>,
    current: ModuleHeroProduct,
    product: ModuleHeroProduct,
    icon: StudioIconName,
    label: &'static str,
    on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let active = current == product;
    let title = format!("Show the {label}");

    rsx! {
        button {
            class: hero_block_class(active),
            r#type: "button",
            aria_pressed: if active { "true" } else { "false" },
            aria_label: "{title}",
            title: "{title}",
            onclick: move |event| {
                // The hero sits inside the card's own click surface; a
                // product switch is not a card selection.
                event.stop_propagation();
                if active {
                    return;
                }
                if let (Some(handler), Some(node)) = (on_action, node.as_ref()) {
                    handler
                        .call(
                            node_ui_action(NodeUiOp::SetHeroProduct {
                                node: node.clone(),
                                product,
                            }),
                        );
                }
            },
            StudioIcon { name: icon, size: 11 }
            span { class: "tw:leading-none", "{label}" }
        }
    }
}

/// The two block states. One geometry, two weights — squared, hairline-
/// divided, and small enough to sit on the preview without competing with
/// it.
fn hero_block_class(active: bool) -> &'static str {
    if active {
        "tw:flex tw:cursor-default tw:items-center tw:gap-1 tw:border-0 tw:bg-card-subtle tw:px-1.5 tw:py-1 tw:text-[0.6rem] tw:font-bold tw:uppercase tw:tracking-[0.08em] tw:text-strong-foreground"
    } else {
        "tw:flex tw:cursor-pointer tw:items-center tw:gap-1 tw:border-0 tw:bg-transparent tw:px-1.5 tw:py-1 tw:text-[0.6rem] tw:font-bold tw:uppercase tw:tracking-[0.08em] tw:text-muted-foreground tw:transition-colors tw:hover:bg-card-muted tw:hover:text-strong-foreground tw:motion-reduce:transition-none"
    }
}

#[cfg(test)]
mod tests {
    use super::hero_block_class;

    #[test]
    fn the_two_hero_blocks_share_one_geometry() {
        // Two states of ONE control: only the weight may differ, or the
        // pair stops reading as a single switch.
        for geometry in ["tw:px-1.5", "tw:py-1", "tw:text-[0.6rem]", "tw:gap-1"] {
            assert!(hero_block_class(true).contains(geometry));
            assert!(hero_block_class(false).contains(geometry));
        }
        // Squared, never a pill — the discrete-control language.
        assert!(!hero_block_class(true).contains("rounded-full"));
        assert!(!hero_block_class(false).contains("rounded-full"));
        // Only the idle block invites a click.
        assert!(hero_block_class(false).contains("cursor-pointer"));
        assert!(hero_block_class(true).contains("cursor-default"));
    }
}
