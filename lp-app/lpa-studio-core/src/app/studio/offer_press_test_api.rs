//! Tests press offers by path, the way a click and the app agent do.
//!
//! Core's tests are the offer tree's third consumer, after the web and the
//! agent. A test that builds `ProjectOp::SaveOverlay` itself never notices
//! when the UI stops offering Save; a test that presses `project/save`
//! fails that moment. [`OfferPressTestApi`] is that press, for every bench:
//! a bench says where its offer tree is and how a click reaches it, and
//! gets `press` / `press_lasting` / `offered` / `not_offered` /
//! `offer_reason` for free.
//!
//! `just lint-core-test-ops` is the ratchet on the tests that still build a
//! user-verb action directly (`scripts/check-core-test-ops.py`). See
//! `docs/adr/2026-10-01-agentic-control-offers-in-core.md`.

use core::fmt::Display;

use crate::app::studio::studio_edit_e2e_tests::drive;
use crate::app::studio::studio_view_channel::CommandSender;
use crate::{
    DeviceId, OfferArgs, OfferPath, StudioActor, StudioCommand, StudioController, UiAction,
    UiOffer, UiOfferTree, UiResult,
};

/// Press offers by path on a test bench.
///
/// A bench implements the two required methods; every other method is the
/// same everywhere. Paths are anything that prints as one (`"project/save"`,
/// an [`OfferPath`]), so a test reads like the agent's `act` call.
pub(crate) trait OfferPressTestApi {
    /// What one press hands back: the dispatch's result where the bench
    /// dispatches in place, `()` where it queues the click (an actor).
    type Outcome;

    /// The offer tree as a click would see it now.
    fn offer_tree(&mut self) -> UiOfferTree;

    /// Dispatch `action` the way a click does.
    fn dispatch_press(&mut self, action: UiAction) -> Self::Outcome;

    /// Press the offer at `path` with `args`: it must be published and
    /// enabled, `args` must bind ([`UiOffer::press`]), and it must not be
    /// Lasting — a Lasting verb arms on the user's first click, and a test
    /// says where that second click is with [`Self::press_lasting`].
    #[track_caller]
    fn press(&mut self, path: impl Display, args: OfferArgs) -> Self::Outcome {
        let (path, action) = bind(&self.offer_tree(), path, &args);
        if let Some(copy) = action.meta().consequence.copy() {
            panic!(
                "`{path}` is Lasting (\"{}\"): the user arms it and clicks again; press it with \
                 `press_lasting`, so the test shows where that confirmation is",
                copy.title
            );
        }
        self.dispatch_press(action)
    }

    /// Press a Lasting offer: everything [`Self::press`] checks, plus that
    /// the verb IS Lasting. This is the user's second click, after the arm.
    #[track_caller]
    fn press_lasting(&mut self, path: impl Display, args: OfferArgs) -> Self::Outcome {
        let (path, action) = bind(&self.offer_tree(), path, &args);
        assert!(
            action.meta().consequence.arms(),
            "`{path}` is {:?}, not Lasting: press it with `press`",
            action.meta().consequence
        );
        self.dispatch_press(action)
    }

    /// The offer at `path`, which must be published (enabled or not).
    #[track_caller]
    fn offered(&mut self, path: impl Display) -> UiOffer {
        let tree = self.offer_tree();
        find(&tree, &parse(path)).clone()
    }

    /// Assert nothing is published at `path`.
    #[track_caller]
    fn not_offered(&mut self, path: impl Display) {
        let path = parse(path);
        let tree = self.offer_tree();
        assert!(
            tree.get(&path).is_none(),
            "`{path}` is offered, and should not be; offered: {}",
            published(&tree)
        );
    }

    /// Why the offer at `path` is disabled; it must be published and
    /// disabled.
    #[track_caller]
    fn offer_reason(&mut self, path: impl Display) -> String {
        let path = parse(path);
        let tree = self.offer_tree();
        match &find(&tree, &path).action.meta().enablement {
            crate::ActionEnablement::Disabled { reason } => reason.clone(),
            crate::ActionEnablement::Enabled => panic!("`{path}` is enabled; it has no reason"),
        }
    }

    /// Where `device`'s verb `verb` lives (`devices/<board ref>/<verb>`):
    /// the board ref is core's to work out, never the test's.
    #[track_caller]
    fn device_verb(&mut self, device: DeviceId, verb: &str) -> OfferPath {
        let tree = self.offer_tree();
        tree.device_prefix(device)
            .unwrap_or_else(|| panic!("{device:?} has no place in the offer tree"))
            .clone()
            .child(verb)
    }
}

/// A controller dispatches a press in place, as the web's click handler
/// does.
impl OfferPressTestApi for StudioController {
    type Outcome = UiResult;

    fn offer_tree(&mut self) -> UiOfferTree {
        self.view().offers
    }

    fn dispatch_press(&mut self, action: UiAction) -> UiResult {
        drive(self.dispatch(action))
    }
}

/// An actor bench's clicks: the press goes onto the command queue, as the
/// web sends it, and one batch runs (emitting its snapshot).
pub(crate) struct ActorClicks<'a, MakeTimer> {
    actor: &'a mut StudioActor<MakeTimer>,
    tx: &'a CommandSender,
}

/// Click on the studio `actor` runs, through its command queue `tx`.
pub(crate) fn actor_clicks<'a, MakeTimer>(
    actor: &'a mut StudioActor<MakeTimer>,
    tx: &'a CommandSender,
) -> ActorClicks<'a, MakeTimer> {
    ActorClicks { actor, tx }
}

impl<MakeTimer, Timer> OfferPressTestApi for ActorClicks<'_, MakeTimer>
where
    MakeTimer: FnMut(core::time::Duration) -> Timer + Clone + 'static,
    Timer: core::future::Future<Output = ()> + 'static,
{
    type Outcome = ();

    /// The controller's current view: what the next snapshot shows.
    fn offer_tree(&mut self) -> UiOfferTree {
        self.actor.controller_mut_for_test().view().offers
    }

    fn dispatch_press(&mut self, action: UiAction) {
        self.tx.send(StudioCommand::Action(action));
        drive(self.actor.run_one_batch_for_test());
    }
}

/// Find the offer at `path` in `tree` and bind `args`, or panic saying why
/// a click could not.
#[track_caller]
fn bind(tree: &UiOfferTree, path: impl Display, args: &OfferArgs) -> (OfferPath, UiAction) {
    let path = parse(path);
    let offer = find(tree, &path);
    if offer.params().is_empty()
        && let crate::ActionEnablement::Disabled { reason } = &offer.action.meta().enablement
    {
        panic!("`{path}` is offered disabled: {reason}");
    }
    match offer.press(args) {
        Ok(action) => (path, action),
        Err(error) => panic!("`{path}` refused the press with {args:?}: {error}"),
    }
}

#[track_caller]
fn find<'a>(tree: &'a UiOfferTree, path: &OfferPath) -> &'a UiOffer {
    tree.get(path)
        .unwrap_or_else(|| panic!("`{path}` is not offered; offered: {}", published(tree)))
}

#[track_caller]
fn parse(path: impl Display) -> OfferPath {
    let text = path.to_string();
    OfferPath::parse(&text).unwrap_or_else(|error| panic!("`{text}`: {error}"))
}

fn published(tree: &UiOfferTree) -> String {
    let paths: Vec<String> = tree.iter().map(|offer| offer.path.to_string()).collect();
    if paths.is_empty() {
        "nothing".to_string()
    } else {
        paths.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActionConfirmation, ControllerId, ProjectOp};

    #[test]
    fn a_press_dispatches_the_bound_action() {
        let mut bench = Bench::new();
        bench.press("project/save", OfferArgs::new());
        assert_eq!(bench.dispatched, ["Save"]);
        assert!(bench.offered("project/save").is_enabled());
        bench.not_offered("project/nope");
        assert_eq!(bench.offer_reason("project/revert"), "nothing to revert");
    }

    #[test]
    #[should_panic(expected = "`project/forget` is Lasting (\"Forget it?\")")]
    fn a_plain_press_on_a_lasting_offer_names_press_lasting() {
        Bench::new().press("project/forget", OfferArgs::new());
    }

    #[test]
    fn press_lasting_is_the_second_click() {
        let mut bench = Bench::new();
        bench.press_lasting("project/forget", OfferArgs::new());
        assert_eq!(bench.dispatched, ["Forget"]);
    }

    #[test]
    #[should_panic(expected = "not Lasting: press it with `press`")]
    fn press_lasting_refuses_a_routine_offer() {
        Bench::new().press_lasting("project/save", OfferArgs::new());
    }

    #[test]
    #[should_panic(expected = "`project/revert` is offered disabled: nothing to revert")]
    fn a_disabled_offer_is_not_pressed() {
        Bench::new().press("project/revert", OfferArgs::new());
    }

    #[test]
    #[should_panic(expected = "`project/nope` is not offered; offered: project/save")]
    fn a_missing_offer_lists_what_is_offered() {
        Bench::new().press("project/nope", OfferArgs::new());
    }

    #[test]
    #[should_panic(expected = "`project/save` is offered, and should not be")]
    fn not_offered_fails_on_a_published_path() {
        Bench::new().not_offered("project/save");
    }

    /// A tree with Save (Routine), Revert (disabled) and Forget (Lasting);
    /// a press records the label of what it dispatched.
    struct Bench {
        tree: UiOfferTree,
        dispatched: Vec<String>,
    }

    impl Bench {
        fn new() -> Self {
            let action = |op: ProjectOp| UiAction::from_op(ControllerId::new("studio|project"), op);
            let mut tree = UiOfferTree::new();
            for (verb, action) in [
                ("save", action(ProjectOp::SaveOverlay)),
                (
                    "revert",
                    action(ProjectOp::RevertAllEdits).disabled("nothing to revert"),
                ),
                (
                    "forget",
                    action(ProjectOp::DetachLens)
                        .with_label("Forget")
                        .lasting(ActionConfirmation::new("Forget it?", "Gone.", "Forget")),
                ),
            ] {
                tree.publish(UiOffer::new(OfferPath::project().child(verb), verb, action));
            }
            Self {
                tree,
                dispatched: Vec::new(),
            }
        }
    }

    impl OfferPressTestApi for Bench {
        type Outcome = ();

        fn offer_tree(&mut self) -> UiOfferTree {
            self.tree.clone()
        }

        fn dispatch_press(&mut self, action: UiAction) {
            self.dispatched.push(action.meta().label.clone());
        }
    }
}
