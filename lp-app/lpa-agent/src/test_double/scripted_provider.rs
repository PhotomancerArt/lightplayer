//! [`ScriptedProvider`]: a `ModelProvider` that plays back scripted turns.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use crate::provider::model_provider::{BoxStream, ModelProvider, TurnEvent, TurnRequest};

/// One-turn-script provider: `run_turn` call `n` streams the `n`-th script
/// (front-first; an exhausted queue streams nothing, which the session
/// reports as an incomplete turn). The request log is `Rc`-shared so
/// assertions survive provider hand-off — factory rebuilds in the studio
/// e2e harness, session-owned providers in unit tests.
pub struct ScriptedProvider {
    /// Remaining turn scripts, consumed front-first (one per `run_turn`).
    pub turns: RefCell<VecDeque<Vec<TurnEvent>>>,
    /// Every [`TurnRequest`] this provider received.
    pub requests: Rc<RefCell<Vec<TurnRequest>>>,
}

impl ScriptedProvider {
    /// A provider over `turns` with a fresh request log.
    pub fn new(turns: Vec<Vec<TurnEvent>>) -> Self {
        Self {
            turns: RefCell::new(turns.into()),
            requests: Rc::new(RefCell::new(Vec::new())),
        }
    }
}

impl ModelProvider for ScriptedProvider {
    fn run_turn(&self, req: TurnRequest) -> BoxStream<'_, TurnEvent> {
        self.requests.borrow_mut().push(req);
        let events = self.turns.borrow_mut().pop_front().unwrap_or_default();
        Box::pin(futures_util::stream::iter(events))
    }
}

/// By-reference use: tests keep the provider and hand `&provider` to the
/// session so scripts and the request log stay inspectable afterwards.
impl ModelProvider for &ScriptedProvider {
    fn run_turn(&self, req: TurnRequest) -> BoxStream<'_, TurnEvent> {
        (**self).run_turn(req)
    }
}
