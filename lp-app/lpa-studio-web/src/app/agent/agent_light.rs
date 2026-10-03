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
//! Motion: the light rises, holds and fades over the entry's lifetime. With
//! reduced motion it is a still ring until core puts it out, and the
//! stories' capture (which freezes animation) shows that same still ring.

use dioxus::prelude::*;
use lpa_studio_core::{
    AgentActivityKind, OfferPath, ProjectSlotAddress, UiAgentActivity, UiAgentReveal,
};

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
pub(crate) fn use_agent_lit(
    path: Option<OfferPath>,
) -> Memo<Option<(u64, AgentActivityKind)>> {
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

/// Wrap the control that renders the offer at `path` (a node card: its
/// node's prefix). It adds no box: the light lands on the child.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn AgentMark(#[props(into)] path: Option<OfferPath>, children: Element) -> Element {
    let lit = use_agent_lit(path.clone())();
    let path = path.map(|path| path.to_string());
    let edited = matches!(lit, Some((_, AgentActivityKind::Edited)));
    let class = agent_mark_class(lit.map(|(seq, _)| seq));
    let class = if edited {
        format!("{class} ux-agent-lit-edited")
    } else {
        class.to_string()
    };
    rsx! {
        div { class, "data-offer-path": path, {children} }
    }
}

// -- Indicator spike (2026-10-03): slot-level lights + the edited chip ------
//
// SPIKE ONLY. The edited-node variants light the slot the assistant changed
// instead of (or beside) the whole card. Core does not carry the changed
// slots yet: a real version adds them to the Edited entry
// (`AgentEditLanding` already knows each edit's `ProjectSlotAddress`), and
// `UiAgentLit` would carry `slots: Vec<ProjectSlotAddress>`. Until then a
// story provides them through this context.

/// The slots lit right now (with the light's seq), shared like the activity.
#[derive(Clone, Copy)]
struct AgentSlotLightsContext(Signal<Vec<(ProjectSlotAddress, u64)>>);

/// The light on the slot at `address`, if one is lit.
pub(crate) fn use_agent_slot_lit(address: Option<ProjectSlotAddress>) -> Option<u64> {
    let context = use_hook(try_consume_context::<AgentSlotLightsContext>)?;
    let address = address?;
    context
        .0
        .read()
        .iter()
        .rev()
        .find(|(lit, _)| *lit == address)
        .map(|(_, seq)| *seq)
}

/// The classes a lit slot (a panel control, a settings row) wears: the
/// same light as a mark's child, on the slot's own box.
pub(crate) fn agent_slot_class(lit: Option<u64>) -> &'static str {
    match lit {
        None => "",
        Some(seq) if seq % 2 == 0 => "ux-agent-slot ux-agent-slot-a",
        Some(_) => "ux-agent-slot ux-agent-slot-b",
    }
}

/// Stories (spike): light `slots` as the assistant's edit would.
#[cfg(feature = "stories")]
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn AgentSlotLightsProvider(slots: Vec<ProjectSlotAddress>, children: Element) -> Element {
    let lit = slots.into_iter().map(|slot| (slot, 1)).collect::<Vec<_>>();
    use_context_provider(|| AgentSlotLightsContext(Signal::new(lit)));
    children
}

/// "Changed by the assistant": the edited card's header chip. Drawn only
/// while the card is lit as Edited; the spike's CSS shows it only under
/// the chip variant (`.ux-agent-e-chip`), so other variants keep the header
/// exactly as it was.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn AgentEditedChip(#[props(into)] path: Option<OfferPath>) -> Element {
    let lit = use_agent_lit(path)();
    let Some((seq, AgentActivityKind::Edited)) = lit else {
        return rsx! {};
    };
    let class = if seq % 2 == 0 {
        "ux-agent-chip ux-agent-chip-a"
    } else {
        "ux-agent-chip ux-agent-chip-b"
    };
    rsx! {
        span { class, title: "The assistant changed this node",
            crate::base::StudioIcon { name: crate::base::StudioIconName::Agent, size: 11 }
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
            })
            .collect(),
        reveal: None,
    }
}

/// Indicator spike: the looks a lit control can wear, as (label, ancestor
/// class). `ring` is the shipped light (no class).
#[cfg(feature = "stories")]
pub(crate) const SPIKE_LOOKS: [(&str, &str); 5] = [
    ("ring (shipped)", ""),
    ("spectrum", "ux-agent-v-spectrum"),
    ("badge", "ux-agent-v-badge"),
    ("sweep", "ux-agent-v-sweep"),
    ("fill", "ux-agent-v-fill"),
];

/// Indicator spike: one labelled cell of a comparison story. `class` picks
/// the look (and edit mode); the frame-strip capture finds the cell by
/// `data-spike-cell`.
#[cfg(feature = "stories")]
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn SpikeCell(label: String, class: String, children: Element) -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:content-start tw:gap-2 tw:p-4 {class}",
            "data-spike-cell": "{label}",
            span { class: "tw:font-mono tw:text-[11px] tw:text-subtle-foreground", "{label}" }
            {children}
        }
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
pub(crate) fn agent_mark_class(lit: Option<u64>) -> &'static str {
    match lit {
        None => "ux-agent-mark",
        Some(seq) if seq % 2 == 0 => "ux-agent-mark ux-agent-lit ux-agent-lit-a",
        Some(_) => "ux-agent-mark ux-agent-lit ux-agent-lit-b",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unlit_mark_is_only_the_boxless_wrapper() {
        assert_eq!(agent_mark_class(None), "ux-agent-mark");
    }

    #[test]
    fn a_lit_mark_lights_and_a_relight_restarts_its_animation() {
        let first = agent_mark_class(Some(7));
        let again = agent_mark_class(Some(8));
        assert!(first.contains("ux-agent-lit "), "{first}");
        assert!(again.contains("ux-agent-lit "), "{again}");
        assert_ne!(first, again, "the next light swaps the animation name");
        assert_eq!(agent_mark_class(Some(9)), first);
    }
}
