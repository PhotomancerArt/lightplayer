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
//! **The height.** The panel is the Patching center's bottom region at every
//! width, at a FIXED height (G1 round 2): the canvas edge above it must not
//! move when the selection does, so the box is a constant per breakpoint and
//! poses that outgrow it scroll inside it. These stories pose the panel on its
//! own through [`panel_frame`], which gives it no definite height to take a
//! percentage of — so what you see is the fixed 300px box, scrollbar and dead
//! space included, exactly as the workbench mounts it.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use super::patch_panel::PatchPanel;
use super::patch_story_fixtures::{
    mini_dome_surface, mini_dome_walkup_surface, peach_manual_surface,
};
use crate::app::editor_shell::patching::ArmedVerb;
use lpa_studio_core::{NodeId, UiPatchSurface, UiPatchTarget};

/// The panel alone, at page width — the center-bottom mount, without the
/// canvas above it.
///
/// The frame gives no definite height on purpose: the panel's `max-h-[45%]`
/// short-window guard resolves to no cap at all against an indefinite parent,
/// so the capture shows the FIXED box itself (300px above the fold, 260 below)
/// rather than some fraction of a story page. A pose that outgrows it scrolls
/// inside it, which is the point of the box.
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

/// [`panel_frame`], with the output-picker POPOVER posed open (round 2, P3)
/// and room above the panel for it to rise into.
///
/// The void stands in for the canvas: the card is anchored to the panel's
/// bottom and grows UPWARD over it, which is the relationship the gate is
/// reading. The popover is a MOUNT of the real Outputs panel, so posing it is
/// exactly posing the panel's `picker_open` — there is no second surface to
/// fixture. Deterministic like everything else here: the story fixtures
/// publish no frames, so the Outputs panel's counterpart glow shows its
/// settled colours without the breathing.
fn panel_frame_with_picker(
    surface: UiPatchSurface,
    selection: Option<UiPatchTarget>,
    armed: Option<ArmedVerb>,
) -> Element {
    rsx! {
        div { class: "tw:flex tw:w-full tw:flex-col tw:overflow-hidden tw:rounded-md tw:border tw:border-border-strong tw:bg-background",
            div { class: "tw:h-[220px] tw:flex-none" }
            PatchPanel {
                surface,
                selection,
                armed,
                picker_open: true,
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
    description = "THE OUTPUT PICKER, open over the bottom panel (round 2, P3). Pressing assign on an unmapped object arms the verb and brings the ports up: the popover HOSTS the real Outputs panel — the same component the Outputs dock mounts, with the same patch_verbs grammar — so every free run in it is a click target that lands the object at the EXACT clicked lamp. That is why the panel's own flat port lists are gone: a destination you cannot judge is not a choice, and this surface shows the box/port tree, each port's occupancy, and the counterpart glow that a text row could only summarise. The card is anchored to the panel's bottom and WIDTH-CAPPED at 420px, rising over the canvas (the void above stands in for it) rather than spanning the panel's full width as a wall of rows. Dismissal is ONE rule at two sizes — a selection move or a patch write closes it, esc closes it (the ladder's first rung), a click outside closes it. The card covers the panel's armed banner while it is up, so its header carries the arm instead — an armed thing always names itself. The free runs' counterpart glow is a live-frames animation and the story fixtures publish none, so this capture is the settled state. Below the fold the same panel arrives full-screen instead (see workbench_patching_mobile_pick)."
)]
fn patch_panel_picker() -> Element {
    let mut surface = mini_dome_walkup_surface();
    let selection = UiPatchTarget::Instance {
        node: dome(),
        path: "/sector/4".to_string(),
    };
    surface.chase_preview = lpa_studio_core::chase_preview(&surface, Some(&selection), 0);
    panel_frame_with_picker(surface, Some(selection), Some(ArmedVerb::Assign))
}
