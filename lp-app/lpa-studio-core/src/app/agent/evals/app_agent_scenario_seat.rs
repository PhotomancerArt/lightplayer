//! [`ScenarioSeat`]: where a scenario's conversation happens.
//!
//! Two seats implement it, and neither forks the app chat — both seat the
//! product's own session, tools, offers and cards on a real
//! [`StudioController`]:
//!
//! - the **project seat** ([`AgentEvalStudio`]): a headless Studio over an
//!   in-process server wearing the scenario's board's pin map;
//! - the **device seat** (`studio_device_e2e_tests/agent_device_seat.rs`):
//!   the device bench, a fake board behind a scripted USB port, blank or
//!   running something.
//!
//! The driver loop ([`super::app_agent_eval_driver::drive_scenario`]) is
//! the same for both.
//!
//! [`AgentEvalStudio`]: super::app_agent_eval_driver::AgentEvalStudio

use std::time::Instant;

use lpa_agent::TokenUsage;

use super::app_agent_checks::NodeStatusRow;
use super::app_agent_project_tree::ProjectTree;
use super::app_agent_scenario::Scenario;
use super::app_agent_transcript::EvalStep;
use crate::{
    OfferArgs, StudioController, UiAction, UiAgentCard, UiAgentStatus, UiAgentTurn, UiOfferTree,
};

/// When a run is stopped mid-flight (the way Stop stops it, between
/// events).
#[derive(Clone, Copy, Debug)]
pub(crate) struct RunLimits {
    pub(crate) deadline: Instant,
    /// Reported cost across the scenario, US dollars.
    pub(crate) usd: f64,
    /// Model turns across the scenario.
    pub(crate) turns: u32,
    /// Tokens in + out across the scenario.
    pub(crate) tokens: Option<u64>,
}

impl RunLimits {
    /// Whether `usage` over `turns` has passed a limit.
    pub(crate) fn passed(&self, usage: &TokenUsage, turns: u32) -> bool {
        Instant::now() > self.deadline
            || usage.reported_cost_usd().unwrap_or(0.0) > self.usd
            || turns > self.turns
            || self.tokens.is_some_and(|cap| total_tokens(usage) > cap)
    }
}

/// Tokens in (cache reads and writes included) plus out.
pub(crate) fn total_tokens(usage: &TokenUsage) -> u64 {
    u64::from(usage.input_tokens)
        + u64::from(usage.cache_read_tokens)
        + u64::from(usage.cache_write_tokens)
        + u64::from(usage.output_tokens)
}

/// What the board ended up as (device seat), for the board checks and the
/// report.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub(crate) struct DeviceSummary {
    /// Every board the roster shows, settled.
    pub(crate) boards: Vec<BoardRow>,
    /// Pending links (a board still needing firmware shows here).
    pub(crate) pending: Vec<String>,
    /// Board manifests the flash wrote (one per flash), by board id.
    pub(crate) flashed: Vec<String>,
    /// How many pushes reached the board.
    pub(crate) pushes: usize,
}

/// One board card as the run left it.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub(crate) struct BoardRow {
    /// The card's state label (`Ready`, `Needs firmware`, …).
    pub(crate) state: String,
    /// `running "<label>"`, `empty`, or what the card says.
    pub(crate) loaded: String,
    /// Whether the card says it runs a project.
    pub(crate) running: bool,
}

/// A seat a scenario runs in.
pub(crate) trait ScenarioSeat {
    /// The controller the app chat lives on.
    fn controller(&mut self) -> &mut StudioController;

    /// Put the scenario's start in front of the user (the project, the
    /// board, the context line).
    fn start(&mut self, scenario: &Scenario);

    /// Send one message to the app chat and drive the run to its end, or
    /// until a limit is passed.
    fn send(&mut self, text: &str, limits: RunLimits);

    /// The user's click (a card's button), and any run it resumes driven
    /// to its end.
    fn press(&mut self, action: UiAction, limits: RunLimits);

    /// Let the engine (and the board) settle after the conversation.
    fn settle(&mut self);

    /// The open project's saved bytes (project seat), or the project the
    /// board runs (device seat).
    fn saved_tree(&mut self) -> ProjectTree;

    /// Whether unsaved authored edits remain.
    fn unsaved(&mut self) -> bool;

    /// Every node's status, on a server wearing the board's pin map.
    fn node_statuses(&mut self) -> Vec<NodeStatusRow>;

    /// The board's end state (device seat only).
    fn device_summary(&mut self) -> Option<DeviceSummary>;

    /// The offer tree as a click would see it now.
    fn offer_tree(&mut self) -> UiOfferTree {
        self.controller().view().offers
    }

    /// The app chat's cards, in transcript order.
    fn cards(&mut self) -> Vec<UiAgentCard> {
        self.controller()
            .agent_for_test()
            .app_session()
            .mirror
            .turns
            .iter()
            .filter_map(|turn| match turn {
                UiAgentTurn::Card(card) => Some(card.clone()),
                _ => None,
            })
            .collect()
    }

    /// The app chat's last visible assistant text (empty when the user
    /// spoke last).
    fn last_assistant_text(&mut self) -> String {
        self.controller()
            .agent_for_test()
            .app_session()
            .mirror
            .turns
            .iter()
            .rev()
            .find_map(|turn| match turn {
                UiAgentTurn::Assistant { text } => Some(text.clone()),
                UiAgentTurn::User { .. } => Some(String::new()),
                _ => None,
            })
            .unwrap_or_default()
    }

    /// Every notice the app chat has shown, in order.
    fn notices(&mut self) -> Vec<String> {
        self.controller()
            .agent_for_test()
            .app_session()
            .mirror
            .turns
            .iter()
            .filter_map(|turn| match turn {
                UiAgentTurn::Notice { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    /// The app chat's status (an error ends a scenario).
    fn status(&mut self) -> UiAgentStatus {
        self.controller()
            .agent_for_test()
            .app_session()
            .mirror
            .status
            .clone()
    }

    /// The model-facing transcript so far, flattened.
    fn transcript_steps(&mut self) -> Vec<EvalStep> {
        let session = self.controller().agent_for_test().app_session();
        let runtime = session.runtime.borrow();
        let Some(runtime) = runtime.as_ref() else {
            return Vec::new();
        };
        super::app_agent_eval_driver::flatten(&runtime.transcript().messages)
    }

    /// Cumulative usage of the app chat.
    fn usage(&mut self) -> TokenUsage {
        self.controller()
            .agent_for_test()
            .app_session()
            .mirror
            .usage
    }

    /// Model turns so far.
    fn turns(&mut self) -> u32 {
        self.controller()
            .agent_for_test()
            .app_session()
            .mirror
            .turn_stats
            .len() as u32
    }

    /// The click on `card`, with `args` picked over the agent's values: the
    /// card's own button when the person changed nothing, else the offer
    /// it hands over, bound with the merged values, as the card's controls
    /// would. `Err` when the person's values do not bind.
    fn card_click(&mut self, card: &UiAgentCard, args: &OfferArgs) -> Result<UiAction, String> {
        if args.iter().next().is_none() {
            return Ok(card.press.clone());
        }
        let path = card
            .offer
            .clone()
            .ok_or_else(|| format!("card {} hands over no offer to pick values on", card.id))?;
        let mut merged = card.args.clone();
        for (name, value) in args.iter() {
            merged = merged.with(name, value);
        }
        let tree = self.offer_tree();
        let offer = tree
            .get(&path)
            .ok_or_else(|| format!("card {}'s offer `{path}` is not offered any more", card.id))?;
        offer
            .press(&merged)
            .map_err(|error| format!("`{path}` refused {merged:?}: {error}"))
    }
}
