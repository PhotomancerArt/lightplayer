//! The transient confirmation line (spike `project-share`'s `.toast`): one
//! sentence at the bottom of the page, **replaced** rather than stacked.
//!
//! Sharing is full of acts with no visible consequence — a link lands on the
//! clipboard, an access level flips on a server, a project leaves the
//! library for the archive. Each needs one line of "that happened", and none
//! of them needs a dialog.
//!
//! # Shape
//!
//! One [`Signal<Option<ToastMessage>>`] in context, provided once by
//! `web_app.rs` ([`use_toast_provider`]) and rendered once by [`ToastHost`].
//! Components take the [`Toasts`] handle in their body (it is `Copy`, so it
//! rides into event handlers) and speak through it.
//!
//! House rule: every consumer uses `try_consume_context` and stays inert
//! without it — stories mount these surfaces with no provider at all, and
//! nothing here may make a component undrawable.
//!
//! # Why the dismissal is CSS
//!
//! The line fades itself out through one keyframe animation
//! (`ux-toast-life` in `style.css`) instead of a timer: a timer would need a
//! runtime the host test build does not have, and a message that outlives
//! its animation is harmless — it is already invisible and
//! `visibility: hidden`. A new message re-keys the node, so the animation
//! restarts even when the text is identical.

use dioxus::prelude::*;

/// One line, and the sequence number that restarts its fade.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToastMessage {
    /// What happened, as a sentence.
    pub text: String,
    /// Plain confirmation, or the warn palette for a refusal.
    pub tone: ToastTone,
    /// Bumped per message, so saying the same thing twice still animates.
    pub seq: u32,
}

/// A toast is either "that worked" or "that did not".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastTone {
    /// The default: a confirmation, in the page's own palette.
    Info,
    /// A refusal (the write did not land) — the warn palette.
    Warn,
}

/// The handle a component says something through. `Copy`, so it can be
/// captured by an event handler without cloning.
#[derive(Clone, Copy, PartialEq)]
pub struct Toasts {
    slot: Signal<Option<ToastMessage>>,
}

impl Toasts {
    /// Confirm something that happened.
    pub fn say(&mut self, text: impl Into<String>) {
        self.push(text.into(), ToastTone::Info);
    }

    /// Report something that did NOT happen.
    pub fn warn(&mut self, text: impl Into<String>) {
        self.push(text.into(), ToastTone::Warn);
    }

    fn push(&mut self, text: String, tone: ToastTone) {
        let seq = next_seq(self.slot.peek().as_ref());
        self.slot.set(Some(ToastMessage { text, tone, seq }));
    }
}

/// Provide the one toast slot for the app. Call once, above [`ToastHost`].
pub fn use_toast_provider() -> Toasts {
    let slot = use_context_provider(|| Signal::new(None::<ToastMessage>));
    use_context_provider(|| Toasts { slot })
}

/// The line itself, painted at the bottom of the page. Renders nothing
/// without the context (stories) and nothing before the first message.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ToastHost() -> Element {
    let Some(slot) = try_consume_context::<Signal<Option<ToastMessage>>>() else {
        return rsx! {};
    };
    let Some(message) = slot() else {
        return rsx! {};
    };
    let tone_class = match message.tone {
        ToastTone::Info => "",
        ToastTone::Warn => "ux-toast-warn",
    };
    rsx! {
        div {
            // Re-keyed per message so the fade animation restarts even when
            // the text has not changed.
            key: "{message.seq}",
            class: "ux-toast {tone_class}",
            role: "status",
            "aria-live": "polite",
            span { class: "tw:min-w-0", "{message.text}" }
        }
    }
}

/// The next message's sequence number (wrapping — it is an animation key,
/// not a count).
fn next_seq(previous: Option<&ToastMessage>) -> u32 {
    previous.map_or(1, |message| message.seq.wrapping_add(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_message_starts_the_sequence() {
        assert_eq!(next_seq(None), 1);
    }

    /// A repeat of the same text must still get a fresh key, or the fade
    /// would not restart.
    #[test]
    fn a_repeat_still_advances_the_key() {
        let previous = ToastMessage {
            text: "Link copied".to_string(),
            tone: ToastTone::Info,
            seq: 7,
        };
        assert_eq!(next_seq(Some(&previous)), 8);
    }

    #[test]
    fn the_key_wraps_rather_than_overflowing() {
        let previous = ToastMessage {
            text: "…".to_string(),
            tone: ToastTone::Warn,
            seq: u32::MAX,
        };
        assert_eq!(next_seq(Some(&previous)), 0);
    }
}
