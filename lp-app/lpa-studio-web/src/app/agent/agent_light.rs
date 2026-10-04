//! The agent light (agentic-UI roadmap M8): when the app agent presses an
//! offer, hands one over on a card, or edits a node, the control it touched
//! wears a brief light in place, so watching the agent teaches where things
//! are.
//!
//! Core decides what is lit (`UiAppAgentView::activity`, by offer path, on
//! the injected clock); this module only draws it. Any control that renders
//! an offer already knows the offer's path, so wrapping it in
//! [`AgentMark`] is the whole integration: the mark is `display: contents`
//! (no box, no layout change) and lights its child through
//! `.ux-agent-lit > *` (style.css). The light is passive — it never takes
//! focus and never scrolls. Only the user's Show (a core offer) moves the
//! view, through [`use_agent_reveal`].
//!
//! An edited node is drawn differently: its card stays dark, the slots the
//! edit wrote light instead (a face control or a settings row asks
//! [`use_agent_slot_lit`] by its own slot address), and the card carries
//! the [`AgentEditedChip`] tab. An edit with no visible slot is the tab
//! alone.
//!
//! Look: the assistant's orchid gradient ring, spinning (style.css,
//! `--studio-agent-*`). Motion: the light rises, holds and fades over the
//! entry's lifetime. With reduced motion it is a still ring until core puts
//! it out, and the stories' capture (which freezes animation) shows that
//! same still ring.

use dioxus::prelude::*;
use lpa_studio_core::{
    AgentActivityKind, OfferPath, ProjectSlotAddress, UiAgentActivity, UiAgentReveal,
};

use crate::base::{StudioIcon, StudioIconName};

/// The agent's activity, shared through Dioxus context like the offer tree.
#[derive(Clone, Copy)]
struct AgentActivityContext(Signal<UiAgentActivity>);

/// Provide `activity` to every component below the caller. Call it on every
/// render with the view's current activity; readers are notified only when
/// it changes.
pub(crate) fn use_provide_agent_activity(activity: &UiAgentActivity) {
    let mut shared = use_context_provider(|| AgentActivityContext(Signal::new(activity.clone()))).0;
    if *shared.peek() != *activity {
        shared.set(activity.clone());
    }
}

/// The agent's activity, or nothing lit outside any provider (a story that
/// renders a surface on its own).
fn use_agent_activity() -> Signal<UiAgentActivity> {
    let fallback = use_signal(UiAgentActivity::default);
    use_hook(try_consume_context::<AgentActivityContext>).map_or(fallback, |context| context.0)
}

/// The newest light on `path` (its seq and kind), memoized so a control
/// re-renders only when its own light changes.
pub(crate) fn use_agent_lit(path: Option<OfferPath>) -> Memo<Option<(u64, AgentActivityKind)>> {
    let activity = use_agent_activity();
    let mut at = use_signal(|| path.clone());
    if *at.peek() != path {
        at.set(path);
    }
    use_memo(move || {
        let at = at.read();
        let path = at.as_ref()?;
        activity.read().lit_at(path).map(|lit| (lit.seq, lit.kind))
    })
}

/// The newest edit light on the slot at `address` (its seq): an edit that
/// wrote exactly there or, with `or_below`, anywhere under it (a control
/// that edits its value whole; see [`UiAgentActivity::slot_lit`]).
/// Memoized like [`use_agent_lit`].
pub(crate) fn use_agent_slot_lit(
    address: Option<ProjectSlotAddress>,
    or_below: bool,
) -> Option<u64> {
    let activity = use_agent_activity();
    let mut at = use_signal(|| (address.clone(), or_below));
    if at.peek().0 != address || at.peek().1 != or_below {
        at.set((address, or_below));
    }
    let lit = use_memo(move || {
        let at = at.read();
        let address = at.0.as_ref()?;
        activity.read().slot_lit(address, at.1).map(|lit| lit.seq)
    });
    lit()
}

/// Wrap the control that renders the offer at `path` (a node card: its
/// node's prefix). It adds no box: the light lands on the child.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn AgentMark(#[props(into)] path: Option<OfferPath>, children: Element) -> Element {
    let lit = use_agent_lit(path.clone())();
    let path = path.map(|path| path.to_string());
    rsx! {
        div { class: agent_mark_class(lit), "data-offer-path": path, {children} }
    }
}

/// "Changed by the assistant": the tab on an edited card's top edge, drawn
/// while the card is lit as Edited (style.css places it out of the
/// header's flow). It says the card changed even when nothing that changed
/// is a slot on view — a created node, a file's text, a collapsed card.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn AgentEditedChip(#[props(into)] path: Option<OfferPath>) -> Element {
    let lit = use_agent_lit(path)();
    let Some(class) = agent_chip_class(lit) else {
        return rsx! {};
    };
    rsx! {
        span { class, title: "The assistant changed this node",
            StudioIcon { name: StudioIconName::Agent, size: 11 }
            "changed by the assistant"
        }
    }
}

/// Provide `activity` to `children` — the stories' way to light controls
/// the way the shell would.
#[cfg(feature = "stories")]
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn AgentActivityProvider(activity: UiAgentActivity, children: Element) -> Element {
    use_provide_agent_activity(&activity);
    children
}

/// Stories: the activity with `paths` lit, as core would publish it a
/// moment after the assistant touched them (captures freeze the light at
/// its full, still look — the reduced-motion look).
#[cfg(feature = "stories")]
pub(crate) fn story_activity(
    paths: impl IntoIterator<Item = (OfferPath, lpa_studio_core::AgentActivityKind)>,
) -> UiAgentActivity {
    UiAgentActivity {
        lit: paths
            .into_iter()
            .enumerate()
            .map(|(seq, (path, kind))| lpa_studio_core::UiAgentLit {
                path,
                seq: seq as u64 + 1,
                kind,
                slots: Vec::new(),
            })
            .collect(),
        reveal: None,
    }
}

/// Stories: the activity a moment after the assistant edited the node at
/// `node` (its prefix), writing `slots`.
#[cfg(feature = "stories")]
pub(crate) fn story_edit_activity(
    node: OfferPath,
    slots: Vec<ProjectSlotAddress>,
) -> UiAgentActivity {
    UiAgentActivity {
        lit: vec![lpa_studio_core::UiAgentLit {
            path: node,
            seq: 1,
            kind: AgentActivityKind::Edited,
            slots,
        }],
        reveal: None,
    }
}

/// Answer the user's Show: when core publishes a new reveal, scroll the
/// marked control into view once the page has rendered the move (a Show on
/// a node focuses its card first). Seeded with the reveal already in the
/// view, so a page that loads with one does not scroll on its own.
pub(crate) fn use_agent_reveal(reveal: Option<UiAgentReveal>) {
    let mut seen = use_signal(|| reveal.as_ref().map(|reveal| reveal.generation));
    use_effect(use_reactive!(|reveal| {
        let Some(reveal) = reveal else {
            return;
        };
        if *seen.peek() == Some(reveal.generation) {
            return;
        }
        seen.set(Some(reveal.generation));
        let path = reveal.path.to_string();
        spawn(async move {
            // One frame's grace: the focused card renders, then we scroll.
            gloo_timers::future::TimeoutFuture::new(REVEAL_AFTER_MS).await;
            crate::base::reveal_offer_path(&path);
        });
    }));
}

/// How long the reveal waits for the page to render what Show changed.
const REVEAL_AFTER_MS: u32 = 60;

/// The mark's classes: always the box-less mark, and while lit the light
/// plus one of two identical animations by the light's seq — a re-light of
/// a still-lit control changes the animation's name, which starts it over.
/// An edit adds `ux-agent-lit-edited`: the card stays dark (its slots and
/// its tab carry the light).
pub(crate) fn agent_mark_class(lit: Option<(u64, AgentActivityKind)>) -> &'static str {
    match lit {
        None => "ux-agent-mark",
        Some((seq, AgentActivityKind::Edited)) if seq % 2 == 0 => {
            "ux-agent-mark ux-agent-lit ux-agent-lit-a ux-agent-lit-edited"
        }
        Some((_, AgentActivityKind::Edited)) => {
            "ux-agent-mark ux-agent-lit ux-agent-lit-b ux-agent-lit-edited"
        }
        Some((seq, _)) if seq % 2 == 0 => "ux-agent-mark ux-agent-lit ux-agent-lit-a",
        Some(_) => "ux-agent-mark ux-agent-lit ux-agent-lit-b",
    }
}

/// The classes a lit slot wears (a settings row; with `pad`, a face
/// control, whose ring sits outside its box), with the same `-a`/`-b`
/// restart as a mark. Empty while unlit.
pub(crate) fn agent_slot_class(lit: Option<u64>, pad: bool) -> &'static str {
    match (lit, pad) {
        (None, _) => "",
        (Some(seq), false) if seq % 2 == 0 => "ux-agent-slot ux-agent-slot-a",
        (Some(_), false) => "ux-agent-slot ux-agent-slot-b",
        (Some(seq), true) if seq % 2 == 0 => "ux-agent-slot ux-agent-slot-pad ux-agent-slot-a",
        (Some(_), true) => "ux-agent-slot ux-agent-slot-pad ux-agent-slot-b",
    }
}

/// The edited tab's classes, while the card is lit as Edited.
fn agent_chip_class(lit: Option<(u64, AgentActivityKind)>) -> Option<&'static str> {
    match lit? {
        (seq, AgentActivityKind::Edited) if seq % 2 == 0 => Some("ux-agent-chip ux-agent-chip-a"),
        (_, AgentActivityKind::Edited) => Some("ux-agent-chip ux-agent-chip-b"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRESSED: AgentActivityKind = AgentActivityKind::Pressed;
    const EDITED: AgentActivityKind = AgentActivityKind::Edited;

    #[test]
    fn an_unlit_mark_is_only_the_boxless_wrapper() {
        assert_eq!(agent_mark_class(None), "ux-agent-mark");
    }

    #[test]
    fn a_lit_mark_lights_and_a_relight_restarts_its_animation() {
        let first = agent_mark_class(Some((7, PRESSED)));
        let again = agent_mark_class(Some((8, PRESSED)));
        assert!(first.contains("ux-agent-lit "), "{first}");
        assert!(again.contains("ux-agent-lit "), "{again}");
        assert_ne!(first, again, "the next light swaps the animation name");
        assert_eq!(agent_mark_class(Some((9, PRESSED))), first);
        assert!(!first.contains("edited"), "{first}");
    }

    #[test]
    fn an_edited_card_says_so_and_wears_the_tab() {
        let edited = agent_mark_class(Some((7, EDITED)));
        assert!(edited.contains("ux-agent-lit-edited"), "{edited}");
        assert_eq!(agent_chip_class(None), None);
        assert_eq!(
            agent_chip_class(Some((7, PRESSED))),
            None,
            "a press has no tab"
        );
        let tab = agent_chip_class(Some((7, EDITED))).expect("the tab");
        assert_ne!(
            Some(tab),
            agent_chip_class(Some((8, EDITED))),
            "a re-light restarts it"
        );
    }

    #[test]
    fn a_lit_slot_restarts_like_a_mark_and_a_face_control_gets_the_pad() {
        assert_eq!(agent_slot_class(None, true), "");
        let row = agent_slot_class(Some(1), false);
        assert!(
            row.contains("ux-agent-slot ") && !row.contains("pad"),
            "{row}"
        );
        assert_ne!(row, agent_slot_class(Some(2), false));
        assert!(agent_slot_class(Some(1), true).contains("ux-agent-slot-pad"));
    }
}
