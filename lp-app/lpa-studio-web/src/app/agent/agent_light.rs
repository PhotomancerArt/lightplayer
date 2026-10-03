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
use lpa_studio_core::{OfferPath, UiAgentActivity, UiAgentReveal};

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

/// The newest light on `path` (its seq), memoized so a control re-renders
/// only when its own light changes.
pub(crate) fn use_agent_lit(path: Option<OfferPath>) -> Memo<Option<u64>> {
    let activity = use_agent_activity();
    let mut at = use_signal(|| path.clone());
    if *at.peek() != path {
        at.set(path);
    }
    use_memo(move || {
        let at = at.read();
        let path = at.as_ref()?;
        activity.read().lit_at(path).map(|lit| lit.seq)
    })
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
