//! The visitor strip (spike `project-share` §3-A, ruling G3): full-width
//! under the chrome, tinted by state, actions in place.
//!
//! The banner's whole job is honesty about state (D5): a visitor edits a
//! real tracking copy, and "read-only" means the server refuses the push —
//! so the strip says which of two worlds the copy is in:
//!
//! - **Pristine** — the copy is the service's line (at its head, or about
//!   to fast-forward to it). Live palette; updates arrive as they happen.
//! - **Edited** — local saves diverged the copy. Warn palette; updates are
//!   paused, and the fork is the way to keep the work.
//!
//! A third, calmer variant covers the `access == Edit` link-holder (not in
//! the spike; one line, live palette, no fork nag): their saves go live
//! for everyone, and the line says exactly that.
//!
//! # The strip states; it no longer offers the fork twice
//!
//! Relationship-control D2/P5: the project segment's popover owns the
//! fork-family verb for every standing, and for a visit (a TRANSIENT view
//! session — the shape every `/p/` View link opens as since the examples
//! vision) its hero slot is "Fork — make it yours". The pristine strip's
//! own Fork button was the same offer one row lower, so it retired; the
//! strip keeps Copy link and says what world the copy is in.
//!
//! The EDITED variant keeps its fork, and deliberately. That state is only
//! reachable on a **persistent tracking copy** (an Edit link, or a
//! pre-examples-vision View copy the Q4 leave-alone ruling preserves),
//! which derives as `MineLocal` — the popover offers Duplicate there, not
//! [`VisitorSession::fork`](super::visitor_session::VisitorSession::fork)'s
//! fork-at-the-copy's-head with its provenance. Dropping it would take the
//! only affordance that names what happened to those sessions. Noted for
//! G1: the honest fix is teaching the derivation about tracking copies, not
//! deleting the button.
//!
//! # State detection is local-first, service-checked
//!
//! [`banner_state`] classifies the **last-seen service frontier**
//! ([`CloudBinding::last_seen_heads`](lpa_cloud_client::CloudBinding))
//! against the local history:
//!
//! - a seen head that IS the local head → pristine;
//! - a seen head **inside** the local history → the copy advanced past
//!   what the service last showed (a local save landed) → edited;
//! - a frontier the local line does not contain → the service moved and
//!   this copy has not applied it yet. That alone cannot distinguish
//!   "behind, pull imminent" from "both sides moved", so the last pull's
//!   service-side [`SyncRelation`] breaks the tie — `Behind` (or no pull
//!   yet) reads pristine, `Diverged` reads edited.
//!
//! An **unsaved dirty overlay is deliberately not consulted** (CFS-Q1):
//! before a save the overlay is session-local, so the banner stays
//! pristine until `record_save` actually diverges the history. The same
//! caution runs the other way — [`should_apply_fast_forward`] defers an
//! inbound update while the overlay is dirty (D18), so a fast-forward is
//! never applied over an open editing session.

use dioxus::prelude::*;
use dioxus_icons::lucide::{GitBranch, Link2, Pencil, Radio};
use lpa_studio_core::ActionPriority;
use lpc_history::{ContentHash, ProjectHistory, SyncRelation};

use crate::core::solid_action_class;

/// Which of the strip's two worlds the tracking copy is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BannerState {
    /// The copy is the service's line: live tint, updates arrive.
    Pristine,
    /// Local saves diverged the copy: warn tint, updates paused.
    Edited,
}

/// Classify the copy for the strip. See the module docs for the rules.
pub fn banner_state(
    seen_heads: &[ContentHash],
    history: &ProjectHistory,
    service_relation: Option<SyncRelation>,
) -> BannerState {
    if seen_heads.is_empty() {
        return BannerState::Pristine;
    }
    let mut any_behind = false;
    for head in seen_heads {
        match history.classify(*head) {
            SyncRelation::AtHead => return BannerState::Pristine,
            SyncRelation::Behind => any_behind = true,
            SyncRelation::Diverged => {}
        }
    }
    if any_behind {
        // The service's last-known head is inside our history: we saved
        // past it. That is the edited world whatever the service has done
        // since.
        return BannerState::Edited;
    }
    // The seen frontier is not in our line at all: the service moved.
    // Whether WE also moved is the last pull's service-side call.
    match service_relation {
        Some(SyncRelation::Diverged) => BannerState::Edited,
        _ => BannerState::Pristine,
    }
}

/// The D18 gate: an inbound fast-forward is applied only to a clean
/// session. A dirty overlay defers it — the pull banked everything, so
/// deferring costs nothing and clobbering an open edit would cost trust.
pub fn should_apply_fast_forward(can_fast_forward: bool, overlay_dirty: bool) -> bool {
    can_fast_forward && !overlay_dirty
}

/// What the strip renders — the two §3-A states plus the edit-link line.
#[derive(Clone, Debug, PartialEq)]
pub enum VisitorBannerView {
    /// View-visitor, copy pristine: live tint, Copy link (the fork offer
    /// is the project popover's now — see the module docs).
    ViewPristine { name: String },
    /// View-visitor, copy edited: warn tint, Discard + Fork-to-keep.
    ViewEdited,
    /// Edit-link visitor: one calm live line, Copy link only.
    EditLive { name: String },
}

/// The strip itself. Pure — the coordinator owns every consequence.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn VisitorBanner(
    view: VisitorBannerView,
    #[props(default)] on_copy_link: Option<EventHandler<()>>,
    #[props(default)] on_fork: Option<EventHandler<()>>,
    #[props(default)] on_discard: Option<EventHandler<()>>,
) -> Element {
    match view {
        VisitorBannerView::ViewPristine { name } => rsx! {
            div { class: "{STRIP_BASE} {LIVE_TINT}", role: "status",
                span { class: "tw:flex tw:flex-none tw:text-status-live-foreground",
                    Radio { size: 14 }
                }
                span { class: STRIP_TEXT,
                    "Viewing "
                    strong { class: STRONG_TEXT, "{name}" }
                    " — updates arrive as they happen."
                }
                span { class: ACTIONS,
                    QuietButton { label: "Copy link", on_press: on_copy_link }
                }
            }
        },
        VisitorBannerView::ViewEdited => rsx! {
            div { class: "{STRIP_BASE} {WARN_TINT}", role: "status",
                span { class: "tw:flex tw:flex-none tw:text-status-warning-foreground",
                    GitBranch { size: 14 }
                }
                span { class: STRIP_TEXT,
                    "You've made local changes — updates are "
                    strong { class: STRONG_TEXT, "paused" }
                    ". They stay on this device unless you fork."
                }
                span { class: ACTIONS,
                    QuietButton { label: "Discard changes", on_press: on_discard }
                    ForkButton { label: "Fork to keep your version", on_press: on_fork }
                }
            }
        },
        VisitorBannerView::EditLive { name } => rsx! {
            div { class: "{STRIP_BASE} {LIVE_TINT}", role: "status",
                span { class: "tw:flex tw:flex-none tw:text-status-live-foreground",
                    Pencil { size: 14 }
                }
                span { class: STRIP_TEXT,
                    "Editing "
                    strong { class: STRONG_TEXT, "{name}" }
                    " — saves go live for everyone."
                }
                span { class: ACTIONS,
                    QuietButton { label: "Copy link", on_press: on_copy_link }
                }
            }
        },
    }
}

/// A quiet in-strip action (Copy link, Discard changes).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn QuietButton(label: &'static str, on_press: Option<EventHandler<()>>) -> Element {
    rsx! {
        button {
            class: solid_action_class(ActionPriority::Tertiary),
            r#type: "button",
            onclick: move |_| {
                if let Some(on_press) = on_press {
                    on_press.call(());
                }
            },
            if label == "Copy link" {
                Link2 { size: 12 }
            }
            span { class: "tw:text-[11px] tw:font-semibold", "{label}" }
        }
    }
}

/// The one loud action left in the strip: the diverged tracking copy's
/// fork. Every other fork offer moved to the project popover's action row
/// (P5) — see the module docs for why this one stayed.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ForkButton(label: &'static str, on_press: Option<EventHandler<()>>) -> Element {
    rsx! {
        button {
            class: solid_action_class(ActionPriority::Primary),
            r#type: "button",
            onclick: move |_| {
                if let Some(on_press) = on_press {
                    on_press.call(());
                }
            },
            GitBranch { size: 12 }
            span { class: "tw:text-[11px] tw:font-bold", "{label}" }
        }
    }
}

/// Full-width, actions pushed to the end; wraps rather than clips on a
/// narrow viewport.
const STRIP_BASE: &str = "tw:mb-3 tw:flex tw:min-w-0 tw:flex-wrap tw:items-center tw:gap-x-3 tw:gap-y-2 tw:rounded-md tw:border tw:px-4 tw:py-2";
const LIVE_TINT: &str = "tw:border-status-live-border tw:bg-status-live-bg";
const WARN_TINT: &str = "tw:border-status-warning-border tw:bg-status-warning-bg";
const STRIP_TEXT: &str = "tw:min-w-0 tw:text-xs tw:leading-snug tw:text-muted-foreground";
const STRONG_TEXT: &str = "tw:font-bold tw:text-strong-foreground";
const ACTIONS: &str = "tw:ml-auto tw:flex tw:flex-none tw:items-center tw:gap-2";

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_history::{EventKind, HistoryEvent};

    fn version(n: u8) -> ContentHash {
        ContentHash::of(&[n])
    }

    /// A history: origin, then one save per given version.
    fn history(saves: &[ContentHash]) -> ProjectHistory {
        let mut history = ProjectHistory::new(HistoryEvent {
            at: 1.0,
            kind: EventKind::Created,
        })
        .expect("origin");
        for (i, hash) in saves.iter().enumerate() {
            history.record_save(*hash, 2.0 + i as f64);
        }
        history
    }

    #[test]
    fn at_the_seen_head_is_pristine() {
        let v1 = version(1);
        assert_eq!(
            banner_state(&[v1], &history(&[v1]), None),
            BannerState::Pristine
        );
    }

    /// A local save past the seen frontier is THE edited signal — no pull
    /// needed to know it.
    #[test]
    fn a_local_save_past_the_frontier_reads_edited() {
        let (v1, v2) = (version(1), version(2));
        assert_eq!(
            banner_state(&[v1], &history(&[v1, v2]), None),
            BannerState::Edited
        );
    }

    /// The service moved and we did not: the frontier is unknown to our
    /// line, but the last pull said Behind — pristine, pull imminent.
    #[test]
    fn a_remote_move_over_a_clean_copy_reads_pristine() {
        let (v1, v2) = (version(1), version(2));
        assert_eq!(
            banner_state(&[v2], &history(&[v1]), Some(SyncRelation::Behind)),
            BannerState::Pristine
        );
        // ...and before any pull has classified it, pristine still — an
        // un-asked question is not local edits.
        assert_eq!(
            banner_state(&[v2], &history(&[v1]), None),
            BannerState::Pristine
        );
    }

    /// Both sides moved: the service-side call breaks the tie.
    #[test]
    fn a_true_divergence_reads_edited() {
        let (v1, v2, v3) = (version(1), version(2), version(3));
        // seen = the service's new head v3; local went v1 → v2; a pull
        // classified our head as diverged.
        assert_eq!(
            banner_state(&[v3], &history(&[v1, v2]), Some(SyncRelation::Diverged)),
            BannerState::Edited
        );
    }

    /// Nothing observed yet (a binding that has not exchanged): pristine —
    /// there is no frontier to have diverged from.
    #[test]
    fn an_empty_frontier_is_pristine() {
        assert_eq!(
            banner_state(&[], &history(&[version(1)]), None),
            BannerState::Pristine
        );
    }

    /// The D18 deferral: a fast-forward never lands on a dirty overlay —
    /// and a clean one takes it.
    #[test]
    fn dirty_overlay_defers_the_apply() {
        assert!(should_apply_fast_forward(true, false));
        assert!(!should_apply_fast_forward(true, true));
        assert!(!should_apply_fast_forward(false, false));
        assert!(!should_apply_fast_forward(false, true));
    }

    // The strip's Copy/Fork buttons now render through
    // `solid_action_class` (P4 consolidation) — its own no-preflight
    // background coverage lives in `action_button.rs`.
}
