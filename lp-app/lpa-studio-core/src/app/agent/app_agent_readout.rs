//! The app agent's readout: what the user sees right now, compact, sent with
//! every user turn and after every tool round (PD3, D9; focus v1 = A8).
//!
//! A dedicated projection of the core view model, never the DOM: the page,
//! the open project (its board, whether it is saved, each node's status and
//! where each output port lands), what is selected, the devices on the
//! roster, and every offer in the view's offer tree, listed by its path
//! (`project/save`, `project/demo.module/orbit.shader/remove`). A path is
//! the offer's stable id, so `act` names it directly and the host looks it
//! up in the tree as it is at the press.

use std::fmt::Write as _;

use serde_json::Value;

use crate::{
    ActionConsequence, ActionEnablement, DeviceRosterView, OfferArgError, OfferChoice, OfferParam,
    OfferParamKind, OfferPath, UiOffer,
};

/// The readout as the controller builds it after a batch: the text, and the
/// offer tree's offers in publish order.
#[derive(Clone, Debug, Default)]
pub struct AppReadoutSnapshot {
    pub text: String,
    pub offers: Vec<UiOffer>,
}

impl AppReadoutSnapshot {
    /// The readout text with its offers listed by path, each with its
    /// label, `[disabled: …]` when it cannot be pressed (`[choose a board
    /// in args]` when all it waits for is a value), and what pressing
    /// it does: `[needs the user's click]` (a card the user presses) or
    /// `[undoable]` (it takes something away that Revert brings back).
    pub fn render(&self) -> String {
        let mut text = self.text.clone();
        if self.offers.is_empty() {
            text.push_str("actions: none offered\n");
            return text;
        }
        text.push_str("actions (press one with `act` by its path):\n");
        for offer in &self.offers {
            let meta = offer.action.meta();
            let _ = write!(text, "- {}: {}", offer.path, meta.label);
            match (&meta.enablement, unbound_label(offer)) {
                // Waiting only for a value the agent can pass: not disabled.
                (ActionEnablement::Disabled { .. }, Some(label)) => {
                    let _ = write!(text, " [choose a {label} in args]");
                }
                (ActionEnablement::Disabled { reason }, None) => {
                    let _ = write!(text, " [disabled: {reason}]");
                }
                (ActionEnablement::Enabled, _) => {}
            }
            if meta.needs_user() {
                text.push_str(" [needs the user's click]");
            } else if meta.consequence == ActionConsequence::Undoable {
                text.push_str(" [undoable]");
            }
            text.push('\n');
            if !offer.params().is_empty() {
                let params: Vec<String> = offer.params().iter().map(param_text).collect();
                let _ = writeln!(text, "  takes {}", params.join("; "));
            }
        }
        text
    }

    /// The offer at `path`, if this readout lists one.
    pub fn offer(&self, path: &OfferPath) -> Option<&UiOffer> {
        self.offers.iter().find(|offer| &offer.path == path)
    }
}

/// The label of the required parameter a press with no values misses,
/// when that is all that stands between the offer and a press (a Flash with
/// two boards that fit and nothing preselected).
fn unbound_label(offer: &UiOffer) -> Option<String> {
    if offer.params().is_empty() {
        return None;
    }
    match offer.press(&crate::OfferArgs::new()) {
        Err(OfferArgError::Missing { label, .. }) => Some(label),
        _ => None,
    }
}

/// How many options of a choice the readout names before it says "and K
/// more": a push lists every example and library project, and a flash with
/// show-all every served board, which would drown the rest of the readout.
/// The rest are one refusal away — a value that is not an option is refused
/// with every option ([`press_refusal`]).
const CHOICES_LISTED: usize = 8;

/// One parameter as the readout lists it: `board: one of xiao (XIAO
/// ESP32-C6), … [default xiao]`, `name: optional text`, `enabled: true or
/// false [now true]`. A long choice names its first [`CHOICES_LISTED`]
/// options and counts the rest; options offered only with a toggle on are
/// listed after the rest, under that toggle.
fn param_text(param: &OfferParam) -> String {
    param_text_with(param, Some(CHOICES_LISTED))
}

/// [`param_text`] naming at most `limit` options per group (`None`: all).
fn param_text_with(param: &OfferParam, limit: Option<usize>) -> String {
    match &param.kind {
        OfferParamKind::Choice { options, preselect } => {
            let narrowed: Vec<&OfferChoice> = options
                .iter()
                .filter(|option| option.only_with.is_none())
                .collect();
            let mut text = format!("{}: one of {}", param.name, choice_list(&narrowed, limit));
            let mut toggles: Vec<&str> = Vec::new();
            for toggle in options
                .iter()
                .filter_map(|option| option.only_with.as_deref())
            {
                if !toggles.contains(&toggle) {
                    toggles.push(toggle);
                }
            }
            for toggle in toggles {
                let widened: Vec<&OfferChoice> = options
                    .iter()
                    .filter(|option| option.only_with.as_deref() == Some(toggle))
                    .collect();
                let _ = write!(
                    text,
                    "; with {toggle} on, also {}",
                    choice_list(&widened, limit)
                );
            }
            if let Some(preselect) = preselect {
                let _ = write!(text, " [default {preselect}]");
            }
            text
        }
        OfferParamKind::Text {
            max_len, optional, ..
        } => {
            let mut text = format!(
                "{}: {}",
                param.name,
                if *optional { "optional text" } else { "text" }
            );
            if let Some(limit) = max_len {
                let _ = write!(text, ", at most {limit} characters");
            }
            text
        }
        OfferParamKind::Toggle { value } => format!("{}: true or false [now {value}]", param.name),
    }
}

/// `a (A), b (B; not now: why)`, naming at most `limit` and counting the
/// rest.
fn choice_list(options: &[&OfferChoice], limit: Option<usize>) -> String {
    let shown = limit.map_or(options.len(), |limit| limit.min(options.len()));
    let mut names: Vec<String> = options[..shown]
        .iter()
        .map(|option| match &option.disabled {
            Some(reason) => format!("{} ({}; not now: {reason})", option.value, option.label),
            None => format!("{} ({})", option.value, option.label),
        })
        .collect();
    if options.len() > shown {
        names.push(format!("and {} more", options.len() - shown));
    }
    if names.is_empty() {
        return "nothing right now".to_string();
    }
    names.join(", ")
}

/// Why `act` did not press `offer` with the agent's values, in words the
/// agent acts on: the refusal, then — when it is about one parameter's
/// value — every value that parameter takes, so a guess is corrected in one
/// step. A disabled verb reads as its label and the reason.
pub fn press_refusal(offer: &UiOffer, error: &OfferArgError) -> String {
    if let OfferArgError::Unavailable { reason } = error {
        return format!("{:?} is disabled: {reason}", offer.label());
    }
    let mut text = format!("{} refused: {error}", offer.path);
    let about = match error {
        OfferArgError::Missing { name, .. }
        | OfferArgError::NotAnOption { name, .. }
        | OfferArgError::OptionDisabled { name, .. } => Some(name.as_str()),
        _ => None,
    };
    let params: Vec<&OfferParam> = match about {
        Some(name) => offer
            .params()
            .iter()
            .filter(|param| param.name == name)
            .collect(),
        None if matches!(error, OfferArgError::Unknown { .. }) => offer.params().iter().collect(),
        None => Vec::new(),
    };
    if !params.is_empty() {
        let takes: Vec<String> = params
            .iter()
            .map(|param| param_text_with(param, None))
            .collect();
        let _ = write!(text, ". It takes {}", takes.join("; "));
    }
    text
}

/// The page line.
pub fn page_line(home: bool) -> String {
    if home {
        "page: home (no project open)\n".to_string()
    } else {
        "page: project editor\n".to_string()
    }
}

/// The open project, from its compact summary (`agent_project_summary`).
pub fn project_lines(name: &str, summary: &Value) -> String {
    let mut text = String::new();
    // Desktop is what a new project is set to before anyone chose: to the
    // agent it must read as "not chosen", or a board-less chip looks
    // decided (the bake-off's E3 misses read "Desktop" as an answer).
    let board = summary["board"]
        .as_str()
        .filter(|board| *board != crate::app::library::DESKTOP_BOARD_ID)
        .map(|board| {
            format!(
                "{board} ({})",
                crate::app::roster::board_display_name(board)
            )
        })
        .unwrap_or_else(|| {
            "not chosen yet — which board this is decides what a pin label means".to_string()
        });
    let unsaved = if summary["unsaved"].as_bool().unwrap_or(false) {
        "yes"
    } else {
        "no"
    };
    let _ = writeln!(
        text,
        "project: {name:?}; board: {board}; unsaved edits: {unsaved}"
    );
    let nodes = summary["nodes"].as_array().cloned().unwrap_or_default();
    if nodes.is_empty() {
        text.push_str("nodes: none (an empty project)\n");
    } else {
        text.push_str("nodes:\n");
        for node in nodes {
            let _ = write!(
                text,
                "- {} ({}) {}",
                node["node"].as_str().unwrap_or("?"),
                node["kind"].as_str().unwrap_or("?"),
                node["status"].as_str().unwrap_or("?"),
            );
            if let Some(message) = node["message"].as_str() {
                let _ = write!(text, ": {message}");
            }
            text.push('\n');
        }
    }
    for output in summary["outputs"].as_array().into_iter().flatten() {
        for port in output["ports"].as_array().into_iter().flatten() {
            let _ = write!(
                text,
                "- output {} port {} → {}",
                output["node"].as_str().unwrap_or("?"),
                port["port"].as_str().unwrap_or("?"),
                port["endpoint"].as_str().unwrap_or("?")
            );
            if let Some(pin) = port["pin"].as_str() {
                let _ = write!(text, " ({pin})");
            }
            if let Some(problem) = port["problem"].as_str() {
                let _ = write!(text, " — problem: {problem}");
            }
            text.push('\n');
        }
    }
    text
}

/// The selection line, when something is selected.
pub fn selection_line(selection: Option<String>) -> String {
    match selection {
        Some(selection) => format!("selected: {selection}\n"),
        None => String::new(),
    }
}

/// The device roster, one row per device: its name, chip, board, firmware,
/// state, and what it runs once it has said.
pub fn device_lines(roster: &DeviceRosterView) -> String {
    let devices = &roster.roster.devices;
    if devices.is_empty() {
        return "devices: none connected\n".to_string();
    }
    let mut text = "devices:\n".to_string();
    for device in devices {
        let firmware = match &device.firmware_face {
            lpa_devices::FirmwareFace::LightPlayer { .. } => "LightPlayer",
            lpa_devices::FirmwareFace::Unknown => "not identified yet",
            _ => "other firmware",
        };
        // What it runs is the board's own report; before its first one,
        // nothing is claimed.
        let loaded = match &device.loaded_project {
            lpa_devices::view::LoadedProject::Running { label } => format!("; running {label:?}"),
            lpa_devices::view::LoadedProject::Empty => "; no project loaded".to_string(),
            lpa_devices::view::LoadedProject::Unknown => String::new(),
        };
        let _ = writeln!(
            text,
            "- {:?}: chip {}; board {}; {firmware}; {}{loaded}",
            device.title,
            device.detected_chip.as_deref().unwrap_or("unknown"),
            device.board_id.as_deref().unwrap_or("unknown"),
            device.state_label,
        );
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActionConfirmation, ControllerId, ProjectNodeAddress, ProjectOp, UiAction};

    fn snapshot() -> AppReadoutSnapshot {
        let project = ControllerId::new("studio|project");
        let node = OfferPath::project_node(
            &ProjectNodeAddress::parse("/demo.module/orbit.shader").unwrap(),
        );
        AppReadoutSnapshot {
            text: "page: project editor\n".to_string(),
            offers: vec![
                UiOffer::new(
                    OfferPath::project().child("save"),
                    "save",
                    UiAction::from_op(project.clone(), ProjectOp::SaveOverlay),
                ),
                UiOffer::new(
                    OfferPath::project().child("revert"),
                    "revert",
                    UiAction::from_op(project.clone(), ProjectOp::RevertAllEdits)
                        .with_label("Revert to saved")
                        .with_consequence(ActionConsequence::Lasting(ActionConfirmation::new(
                            "Revert to saved?",
                            "Every unsaved edit is lost.",
                            "Revert",
                        ))),
                ),
                UiOffer::new(
                    node.child("remove"),
                    "remove",
                    UiAction::from_op(project, ProjectOp::RevertAllEdits)
                        .with_label("Remove")
                        .with_consequence(ActionConsequence::Undoable)
                        .disabled("the root cannot go"),
                ),
            ],
        }
    }

    #[test]
    fn offers_are_listed_by_path_with_what_pressing_does() {
        let text = snapshot().render();
        assert!(text.contains("- project/save: Save\n"), "{text}");
        assert!(
            text.contains("- project/revert: Revert to saved [needs the user's click]\n"),
            "{text}"
        );
        assert!(
            text.contains(
                "- project/demo.module/orbit.shader/remove: Remove \
                 [disabled: the root cannot go] [undoable]\n"
            ),
            "{text}"
        );
        assert_eq!(text, snapshot().render(), "the same view reads the same");
    }

    #[test]
    fn an_offers_parameters_are_listed_under_it() {
        use crate::{OfferArgs, OfferBinder, OfferChoice, OfferParam};
        let save = UiAction::from_op(ControllerId::new("studio|project"), ProjectOp::SaveOverlay);
        let bound = save.clone();
        let offer = UiOffer::with_params(
            OfferPath::parse("devices/mac-a0f26287b48c/flash").unwrap(),
            "flash",
            vec![
                OfferParam::choice(
                    "board",
                    "board",
                    vec![
                        OfferChoice::new("xiao", "XIAO ESP32-C6"),
                        OfferChoice::new("devkit", "ESP32-C6 DevKit").disabled("no build"),
                    ],
                    Some("xiao".to_string()),
                ),
                OfferParam::text("name", "name", "blank").optional(),
                OfferParam::text("note", "note", "").max_len(8),
                OfferParam::toggle("loud", "loud", false),
            ],
            OfferBinder::new(move |_: &OfferArgs| Ok(bound.clone())),
            save,
        );
        let text = AppReadoutSnapshot {
            text: String::new(),
            offers: vec![offer],
        }
        .render();
        assert!(
            text.contains(
                "  takes board: one of xiao (XIAO ESP32-C6), devkit (ESP32-C6 DevKit; not now: \
                 no build) [default xiao]; name: optional text; note: text, at most 8 \
                 characters; loud: true or false [now false]\n"
            ),
            "{text}"
        );
    }

    #[test]
    fn a_long_choice_names_the_first_few_and_counts_the_rest() {
        use crate::{OfferArgs, OfferBinder, OfferChoice, OfferParam};
        let mut options: Vec<OfferChoice> = (1..=10)
            .map(|n| OfferChoice::new(format!("p{n}"), format!("Pattern {n}")))
            .collect();
        options.extend(
            (1..=3)
                .map(|n| OfferChoice::new(format!("w{n}"), format!("Wide {n}")).only_with("all")),
        );
        let offer = UiOffer::with_params(
            OfferPath::parse("devices/new-1/push").unwrap(),
            "push",
            vec![
                OfferParam::choice("source", "project", options, None),
                OfferParam::toggle("all", "everything", false),
            ],
            OfferBinder::new(|_: &OfferArgs| Ok(save_action())),
            save_action(),
        );
        let text = AppReadoutSnapshot {
            text: String::new(),
            offers: vec![offer.clone()],
        }
        .render();
        assert!(
            text.contains(
                "  takes source: one of p1 (Pattern 1), p2 (Pattern 2), p3 (Pattern 3), \
                 p4 (Pattern 4), p5 (Pattern 5), p6 (Pattern 6), p7 (Pattern 7), \
                 p8 (Pattern 8), and 2 more; with all on, also w1 (Wide 1), w2 (Wide 2), \
                 w3 (Wide 3); all: true or false [now false]\n"
            ),
            "{text}"
        );

        // A guess is answered with every option.
        let error = offer
            .press(&OfferArgs::new().with("source", "p11"))
            .unwrap_err();
        let refusal = press_refusal(&offer, &error);
        assert!(
            refusal.starts_with("devices/new-1/push refused: `source` must be one of p1, "),
            "{refusal}"
        );
        assert!(
            refusal.contains(". It takes source: one of p1 (Pattern 1), ")
                && refusal.contains("p10 (Pattern 10); with all on, also w1 (Wide 1)"),
            "the refusal names every option: {refusal}"
        );
        let missing = press_refusal(&offer, &offer.press(&OfferArgs::new()).unwrap_err());
        assert!(
            missing.starts_with(
                "devices/new-1/push refused: `source` is required: choose a project. \
                 It takes source: one of p1 (Pattern 1)"
            ),
            "{missing}"
        );
        let unknown = press_refusal(
            &offer,
            &offer
                .press(&OfferArgs::new().with("colour", "red"))
                .unwrap_err(),
        );
        assert!(
            unknown.contains("no parameter `colour`; it takes source, all. It takes source: "),
            "{unknown}"
        );
        let disabled = UiOffer::new(
            OfferPath::project().child("save"),
            "save",
            save_action().disabled("nothing to save"),
        );
        assert_eq!(
            press_refusal(&disabled, &disabled.press(&OfferArgs::new()).unwrap_err()),
            "\"Save\" is disabled: nothing to save"
        );
    }

    fn save_action() -> UiAction {
        UiAction::from_op(ControllerId::new("studio|project"), ProjectOp::SaveOverlay)
    }

    #[test]
    fn an_offer_is_found_by_its_path() {
        let readout = snapshot();
        let save = OfferPath::parse("project/save").unwrap();
        assert_eq!(readout.offer(&save).map(UiOffer::label), Some("Save"));
        assert!(
            readout
                .offer(&OfferPath::parse("project/nope").unwrap())
                .is_none()
        );
        assert!(
            AppReadoutSnapshot::default()
                .render()
                .contains("actions: none offered")
        );
    }
}
