//! THE patch panel's states, one story each.
//!
//! The panel is pure derivation: it holds no selection state of its own, so a
//! story is exactly a surface plus a selection plus (for the arm) the frame's
//! armed verb — pose those three and the panel renders the state a walk-up
//! user would be standing in. That is why these are panel stories rather than
//! workbench ones: the whole point of the panel is that it can be reasoned
//! about from the DTOs alone.
//!
//! **Determinism.** Frames are deliberately absent from the shared fixtures,
//! and every animated thing in the panel is gated on frames FLOWING: the
//! armed button's pulse and the counterpart section's attention ring only
//! wear their animation classes while an output has published something
//! (`surface_is_live`), and the unmapped chase preview freezes at
//! [`lpa_studio_core::FROZEN_PREVIEW_PHASE`] when no frame has ever arrived.
//! So these captures show the SETTLED state of each language — the arm's
//! selection colours without the breathing, the chase at its most legible
//! still — and two capture runs produce the same pixels.
//!
//! **The two poses.** Half the states only exist in the MANUAL world:
//! auto-mapped fixtures place their own objects, so nothing is ever waiting
//! and there is no link to arm. The shared fixtures keep the auto pose (it is
//! the creation-time default, and the lean panel is its own state worth
//! pinning); [`mini_dome_walkup_surface`] is the manual counterpart with two
//! objects still off the wire.
//!
//! **The two mounts.** The panel's home is the Patching view's Props DOCK
//! (round 2, D1); below the workbench's ≤820px fold, where there are no
//! docks, it keeps its old center-bottom mount instead. The two differ in
//! height model and density, so most states pose at page width through
//! [`panel_frame`] (the fold form, whole panel on screen) and the
//! representative pair poses again in [`docked_frame`] — an exact 320px dock
//! box, the width the restyle is actually for.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use super::patch_panel::PatchPanel;
use super::patch_story_fixtures::{
    mini_dome_surface, mini_dome_walkup_surface, peach_manual_surface,
};
use crate::app::editor_shell::patching::ArmedVerb;
use lpa_studio_core::{NodeId, UiPatchSurface, UiPatchTarget};

/// The panel in its FOLD form, at page width: the center-bottom mount the
/// workbench keeps below 820px, where there are no docks to live in.
///
/// That mount caps itself at 45% of the center column so the canvas above
/// always keeps room; a story has no canvas to protect, and a percentage cap
/// against an indefinite height resolves to no cap at all — which is what we
/// want here, the whole panel on screen rather than its top half over a
/// scrollbar. [`docked_frame`] poses the other form.
fn panel_frame(
    surface: UiPatchSurface,
    selection: Option<UiPatchTarget>,
    armed: Option<ArmedVerb>,
) -> Element {
    rsx! {
        div { class: "tw:flex tw:w-full tw:flex-col tw:overflow-hidden tw:rounded-md tw:border tw:border-border-strong tw:bg-background",
            PatchPanel {
                surface,
                selection,
                armed,
                on_action: move |_| {},
            }
        }
    }
}

/// The panel in its HOME form, in an exact right-dock box (320px wide, its
/// 2.5 padding, the dock's own fill and scroll) — the Patching view's Props
/// panel as a user actually meets it. Same mount as the workbench stories'
/// `dock_frame`, so the two sets of captures are comparable.
///
/// The docked variant drops the 45% cap and the top border: the dock body
/// already scrolls, and a percentage of an indefinite height is no cap at
/// all — it would silently do nothing where the fold mount really needs it.
fn docked_frame(
    surface: UiPatchSurface,
    selection: Option<UiPatchTarget>,
    armed: Option<ArmedVerb>,
) -> Element {
    rsx! {
        div { class: "tw:flex tw:h-[520px] tw:w-[320px] tw:flex-col tw:overflow-y-auto tw:rounded-md tw:border tw:border-border-strong tw:bg-card-subtle tw:p-2.5",
            PatchPanel {
                surface,
                selection,
                armed,
                docked: true,
                on_action: move |_| {},
            }
        }
    }
}

/// The dome fixture of the mini-dome fixtures.
fn dome() -> NodeId {
    NodeId::new(2)
}

/// Box 1 — the output whose `IO13` the walk-up pose leaves empty.
fn box_one() -> NodeId {
    NodeId::new(10)
}

#[story(
    description = "The panel with NOTHING selected (D8 — the panel is always present, empty states included). Both sections keep their heads and say what would fill them: the object section names the two ways in (an object, or free space on a port), the output section states plainly that no port segment is selected. The keys row is the same in every state — it REPLACED the help overlay, so the grammar is readable without arming anything."
)]
fn patch_panel_empty() -> Element {
    panel_frame(mini_dome_walkup_surface(), None, None)
}

#[story(
    description = "A whole FIXTURE selected on an AUTO-mapped fixture: the fixture CARD (Q8), not an object. Fixture-grain facts (lamps, placed count, flow), the mapping SELECTOR as two explaining cards rather than a toggle that names one state and leaves the other to guess, and the honest line that auto-mapped objects flow onto the wire by themselves. No chase strip and no transport: a fixture with sub-objects has no single direction to show and no single run to rotate. The output section has no counterpart to derive and nothing to invite — a card is not a link."
)]
fn patch_panel_fixture_card() -> Element {
    panel_frame(
        mini_dome_surface(false),
        Some(UiPatchTarget::Fixture { node: dome() }),
        None,
    )
}

#[story(
    description = "The LEAN panel (Q11): one of an AUTO-mapped fixture's objects. Facts and the object's strip, then one line saying why there is nothing to press — auto reflow would fight every transport verb, so the grammar is not offered here; the selector that unlocks it is one click away on the fixture card. The output section still derives the wire this object landed on, because that is a FACT about it, not a gesture."
)]
fn patch_panel_auto_object() -> Element {
    panel_frame(
        mini_dome_surface(false),
        Some(UiPatchTarget::Instance {
            node: dome(),
            path: "/sector/2".to_string(),
        }),
        None,
    )
}

#[story(
    description = "PAIRED (the spike's #paired): a MAPPED object on a manual fixture. Object section primary (the blue edge follows core's language matrix, not a second opinion in the panel), full fixture-side transport — rotate, flip, unmap, and the dashed lamps−/+ that is deliberately mock-level room for a mapping write that is not a patch verb — over the output section showing the wire window this object derived. Both strips stand at their honest empty: story fixtures publish no frames, so there is no signal to decode and the track shows through rather than a field of invented black lamps."
)]
fn patch_panel_paired() -> Element {
    panel_frame(
        mini_dome_walkup_surface(),
        Some(UiPatchTarget::Instance {
            node: dome(),
            path: "/sector/2".to_string(),
        }),
        None,
    )
}

#[story(
    description = "DERIVED (the spike's #derived): the selection is a wire RUN — the cell a user clicks in the Outputs dock — and the panel answers with the object that owns it. The head names the door, the facts are the object's, and the output window is marked ‘derived’ so nobody reads it as a window they picked. The run selects as its cell, which speaks the fixture's language (D9), so the object section keeps the blue edge even though the click happened on the wire."
)]
fn patch_panel_derived() -> Element {
    panel_frame(
        mini_dome_walkup_surface(),
        Some(UiPatchTarget::Cell {
            id: "doors:0:9:30".to_string(),
        }),
        None,
    )
}

#[story(
    description = "ARMED (the spike's #armed — the segment-first flow): free port space selected as a SEGMENT, auto-sized to the object waiting for it, with the assign ARMED. The banner names the gesture the next click completes; the button keeps its label and wears the selection colours with its `a` chip beside it (round 3: an armed button never re-words itself). The wire side is primary and carries the segment's own nudges — walk it, narrow it, widen it, take the next free one — none of which write anything: a window is not a patch until the arm completes. Armed here also rings the counterpart section and pulses the button, both CSS animations gated on live frames, so this settled capture shows the colours without the motion."
)]
fn patch_panel_armed() -> Element {
    panel_frame(
        mini_dome_walkup_surface(),
        Some(UiPatchTarget::Segment {
            node: box_one(),
            port: 1,
            start: 39,
            lamps: 30,
        }),
        Some(ArmedVerb::Assign),
    )
}

#[story(
    description = "OBJECT-FIRST (the spike's #objfirst): an UNMAPPED object on a manual fixture. Its strip carries the CHASE — blue head, red tail, the sweep in object order — computed once core-side and frozen at the still where head, dot and tail read at once, the very colours the canvas sprites paint for the same object (Q9: one selection, one chase, painted once). The transport refuses politely (nothing to rotate off the wire), and the output section invites: arm, or pick a destination from cards that state each port's occupancy, because a destination you cannot judge is not a choice."
)]
fn patch_panel_objfirst() -> Element {
    let mut surface = mini_dome_walkup_surface();
    let selection = UiPatchTarget::Instance {
        node: dome(),
        path: "/sector/4".to_string(),
    };
    // The controller's own computation, run over the story's surface — a
    // story cannot drift into showing a chase production would not produce.
    // Zero frames seen is the frozen still, which is exactly the state a
    // capture must land on.
    surface.chase_preview = lpa_studio_core::chase_preview(&surface, Some(&selection), 0);
    panel_frame(surface, Some(selection), None)
}

#[story(
    description = "The SCARF (Q8's exception): a fixture with no object table is one strand that IS its own object, so it keeps the object treatment — facts, strip, transport — and wears the flow selector and `unmap all` here, since it has no fixture card of its own to carry them. The peach's leaf is the shape: format-1 range grain, one run, one output."
)]
fn patch_panel_scarf() -> Element {
    panel_frame(
        peach_manual_surface(),
        Some(UiPatchTarget::Fixture {
            node: NodeId::new(3),
        }),
        None,
    )
}

#[story(
    description = "PAIRED at DOCK width — the panel in its actual home, the Patching view's Props panel (round 2, D1). Same state as the paired story, restyled for a 320px column: no height cap and no top border (the dock body already scrolls, and a percentage cap against an indefinite height would silently be no cap at all), tighter section gutters, the fixture-side transport wrapping at a deliberate break rather than wherever the last button landed, and the keys row in two columns with the long phrases claiming both. The question at the gate is legibility at this width, not the state."
)]
fn patch_panel_docked_paired() -> Element {
    docked_frame(
        mini_dome_walkup_surface(),
        Some(UiPatchTarget::Instance {
            node: dome(),
            path: "/sector/2".to_string(),
        }),
        None,
    )
}

#[story(
    description = "OBJECT-FIRST at DOCK width: the invitation state in the Props panel, the densest thing the panel has to fit — the object's frozen chase strip, the refusing transport, and the output section's arm plus the port cards that state each destination's occupancy. The cards are deliberately NOT restyled this pass (they are the next phase's subject); everything around them is what this width is being judged on."
)]
fn patch_panel_docked_objfirst() -> Element {
    let mut surface = mini_dome_walkup_surface();
    let selection = UiPatchTarget::Instance {
        node: dome(),
        path: "/sector/4".to_string(),
    };
    // The controller's own computation over the story's surface, frozen at
    // zero frames — the same still the page-width objfirst story lands on.
    surface.chase_preview = lpa_studio_core::chase_preview(&surface, Some(&selection), 0);
    docked_frame(surface, Some(selection), None)
}
