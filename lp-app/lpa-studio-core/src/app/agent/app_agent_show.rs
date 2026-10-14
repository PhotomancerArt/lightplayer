//! Show: the app chat's link from a row to where its press or edit lives
//! (agentic-UI roadmap M8).
//!
//! Three pieces, each a plain function over what the studio already knows,
//! so the studio controller only gathers the facts:
//!
//! - [`place_phrase`]: where a control is, in the page's words ("in the
//!   project header", "on the fixture card");
//! - [`show_offer`]: the `show/<target>` offer — Show is a verb the user
//!   presses, so core builds it into the one offer tree like every other;
//! - [`place_turns`]: each chat row's [`UiAgentPlace`], from the activity
//!   history and what the tree offers now.

use crate::app::agent::agent_activity::{AgentActivity, AgentActivityEntry};
use crate::{
    AgentController, AgentOp, ControllerId, OfferPath, ProjectNodeAddress, UiAction, UiAgentPlace,
    UiAgentTurn, UiOffer, UiOfferTree,
};

/// Where the control at `target` is, as a phrase after its label.
///
/// `node_name` names the node a `project/<node>/…` target belongs to (its
/// name as the chat says it; empty for the project's root module), and
/// `device_title` the board a `devices/<board>/…` target belongs to.
pub(crate) fn place_phrase(
    target: &OfferPath,
    node_name: Option<&str>,
    device_title: Option<&str>,
) -> String {
    let segments = target.segments();
    match segments.first().map(String::as_str) {
        Some(OfferPath::PROJECT) => match node_name {
            Some("") => "on the project's root card".to_string(),
            Some(name) => format!("on the {name} card"),
            None => "in the project header".to_string(),
        },
        Some(OfferPath::DEVICES) if segments.len() <= 2 => "in Connect a board".to_string(),
        Some(OfferPath::DEVICES) => match device_title {
            Some(title) => format!("on {title}'s card"),
            None => "on the board's card".to_string(),
        },
        _ => String::new(),
    }
}

/// The node prefix a `project/…` target belongs to: the target itself when
/// it names a node (an edit), its owner when it is a node's verb, `None`
/// for a project-level verb (`project/save`) or anything else.
pub(crate) fn node_prefix_of(target: &OfferPath) -> Option<OfferPath> {
    if target.segments().first().map(String::as_str) != Some(OfferPath::PROJECT) {
        return None;
    }
    if target.names_node() {
        return Some(target.clone());
    }
    target.owner().filter(OfferPath::names_node)
}

/// The Show offer for `entry`'s target, at `show/<target>`. `blocked` is
/// why the control cannot be shown from where the user is (another page);
/// the offer is published disabled with it, so the row says why instead
/// of offering a link that does nothing.
pub(crate) fn show_offer(entry: &AgentActivityEntry, blocked: Option<String>) -> UiOffer {
    let label = format!("Show {}", entry.label);
    let summary = match entry.place.as_str() {
        "" => format!("Bring {} into view and light it.", entry.label),
        place => format!("Bring {} {place} into view and light it.", entry.label),
    };
    let action = UiAction::from_op(
        ControllerId::new(AgentController::NODE_ID),
        AgentOp::Show {
            target: entry.target.clone(),
        },
    )
    .with_label(label)
    .with_summary(summary);
    let action = match blocked {
        Some(reason) => action.disabled(reason),
        None => action,
    };
    UiOffer::new(OfferPath::show_of(&entry.target), "show", action)
}

/// Give every chat row its place: each finished `act` row, and each edit
/// line that landed on a node. The words come from the activity history
/// (recorded at the press, when the control was there to name); Show is
/// set when the tree publishes one for that target.
pub(crate) fn place_turns(
    turns: &mut [UiAgentTurn],
    activity: &AgentActivity,
    offers: &UiOfferTree,
) {
    let place = |target: &OfferPath| -> Option<UiAgentPlace> {
        let entry = activity.latest(target)?;
        let show = OfferPath::show_of(target);
        Some(UiAgentPlace {
            label: entry.label.clone(),
            place: entry.place.clone(),
            show: offers.get(&show).is_some().then_some(show),
        })
    };
    for turn in turns {
        let UiAgentTurn::Tool(row) = turn else {
            continue;
        };
        if let Some(act) = &row.act {
            row.place = place(&act.path);
        }
        if let Some(edits) = &mut row.edits {
            for line in &mut edits.lines {
                line.place = line
                    .node
                    .as_deref()
                    .and_then(|node| ProjectNodeAddress::parse(node).ok())
                    .and_then(|address| place(&OfferPath::project_node(&address)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::agent::agent_activity::AgentActivityKind;
    use crate::{UiAgentActPress, UiAgentEditBatch, UiAgentToolRow};

    fn node(path: &str) -> OfferPath {
        OfferPath::project_node(&ProjectNodeAddress::parse(path).unwrap())
    }

    #[test]
    fn places_read_in_the_pages_words() {
        let save = OfferPath::project().child("save");
        assert_eq!(place_phrase(&save, None, None), "in the project header");
        let remove = node("/demo.module/fixture.fixture").child("remove");
        assert_eq!(
            place_phrase(&remove, Some("fixture"), None),
            "on the fixture card"
        );
        assert_eq!(
            place_phrase(&node("/demo.module"), Some(""), None),
            "on the project's root card"
        );
        let flash = OfferPath::parse("devices/mac-a0f26287b48c/flash").unwrap();
        assert_eq!(
            place_phrase(&flash, None, Some("Shelf lamp")),
            "on Shelf lamp's card"
        );
        assert_eq!(
            place_phrase(&OfferPath::devices().child("connect-usb"), None, None),
            "in Connect a board"
        );
    }

    #[test]
    fn a_target_belongs_to_its_node_card_or_to_no_node() {
        let fixture = node("/demo.module/fixture.fixture");
        assert_eq!(node_prefix_of(&fixture), Some(fixture.clone()));
        assert_eq!(
            node_prefix_of(&fixture.clone().child("remove")),
            Some(fixture)
        );
        assert_eq!(node_prefix_of(&OfferPath::project().child("save")), None);
        assert_eq!(
            node_prefix_of(&OfferPath::parse("devices/new-3/flash").unwrap()),
            None
        );
    }

    #[test]
    fn show_is_an_offer_at_show_target_that_presses_agent_show() {
        let mut activity = AgentActivity::default();
        let save = OfferPath::project().child("save");
        activity.record(
            save.clone(),
            AgentActivityKind::Pressed,
            "Save",
            "in the project header",
            0.0,
        );
        let offer = show_offer(activity.latest(&save).unwrap(), None);
        assert_eq!(offer.path.to_string(), "show/project/save");
        assert_eq!(offer.label(), "Show Save");
        assert!(offer.is_enabled());
        assert_eq!(
            offer.action.op_as::<AgentOp>(),
            Some(&AgentOp::Show {
                target: save.clone()
            })
        );
        let blocked = show_offer(
            activity.latest(&save).unwrap(),
            Some("open the project to see it".into()),
        );
        assert!(!blocked.is_enabled());
    }

    #[test]
    fn rows_take_their_place_and_show_from_the_history_and_the_tree() {
        let mut activity = AgentActivity::default();
        let save = OfferPath::project().child("save");
        let fixture = node("/demo.module/fixture.fixture");
        activity.record(
            save.clone(),
            AgentActivityKind::Pressed,
            "Save",
            "in the project header",
            0.0,
        );
        activity.record(
            fixture.clone(),
            AgentActivityKind::Edited,
            "fixture",
            "on the fixture card",
            0.0,
        );
        let mut offers = UiOfferTree::new();
        offers.publish(show_offer(activity.latest(&fixture).unwrap(), None));

        let mut act = UiAgentToolRow::started("a1").for_tool("act");
        act.done = true;
        act.act = Some(UiAgentActPress {
            path: save,
            card: None,
        });
        let mut edit = UiAgentToolRow::started("e1").for_tool("edit_project");
        edit.done = true;
        edit.edits = UiAgentEditBatch::from_summary(&serde_json::json!({ "rows": [
            { "edit": "set", "target": "fixture", "path": "count", "value": 250, "ok": true,
              "node": "/demo.module/fixture.fixture" },
            { "edit": "set_target", "target": "seeed/xiao-esp32-c6", "ok": true }
        ] }));
        let mut turns = vec![UiAgentTurn::Tool(act), UiAgentTurn::Tool(edit)];
        place_turns(&mut turns, &activity, &offers);

        let UiAgentTurn::Tool(act) = &turns[0] else {
            unreachable!()
        };
        let place = act.place.as_ref().expect("placed");
        assert_eq!(act.summary_line(), "pressed Save in the project header");
        assert_eq!(place.show, None, "Save is gone after saving: no Show");
        let UiAgentTurn::Tool(edit) = &turns[1] else {
            unreachable!()
        };
        let lines = &edit.edits.as_ref().unwrap().lines;
        assert_eq!(
            lines[0].place.as_ref().and_then(|place| place.show.clone()),
            Some(OfferPath::show_of(&fixture))
        );
        assert_eq!(lines[1].place, None, "the board is no node");
    }
}
