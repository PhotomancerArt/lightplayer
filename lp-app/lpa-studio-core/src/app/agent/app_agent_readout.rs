//! The app agent's readout: what the user sees right now, compact, sent with
//! every user turn and after every tool round (PD3, D9; focus v1 = A8).
//!
//! A dedicated projection of the core view model, never the DOM. It leads
//! with **where the user is** (M7, D3): the page, the node they are looking
//! at, what is selected and what is open over the page — and the actions
//! there, in full. Then the open project (its board, whether it is saved,
//! each node's status and where each output port lands) and the devices on
//! the roster. Every other action is **counted**, per area: the other
//! nodes' verbs as one line of verb counts, each device-side owner as one
//! line of verb names. `read` on a node or a device lists its actions in
//! full. That keeps the readout bounded however many verbs each node card
//! grows, while every offer in the view's tree stays pressable by its path
//! (`project/save`, `project/demo.module/orbit.shader/remove`): `act` looks
//! a path up in the whole tree, never just in what was listed.
//!
//! One exception to "counted elsewhere": with no real board connected or
//! attached, `devices/connect-usb` and `devices/connect-ble` are listed in
//! full on every page, not folded into the devices area's counted line —
//! they are the user's next step wherever they are, and only a full listing
//! carries the `[needs the user's click]` mark that tells the agent to
//! `act` it rather than describe the button.

use std::fmt::Write as _;

use serde_json::Value;

use crate::app::project::agent_focus::AgentNodeFocus;
use crate::{
    ActionConsequence, ActionEnablement, DeviceRosterView, OfferArgError, OfferChoice,
    OfferNearness, OfferParam, OfferParamKind, OfferPath, UiOffer, UiOfferFocus, UiPage, UiPlace,
};

/// The readout as the controller builds it after a batch: where the user
/// is, the rest of the text, and the offer tree's offers in publish order
/// with the focus they are ranked by.
#[derive(Clone, Debug, Default)]
pub struct AppReadoutSnapshot {
    /// The "where the user is" lines: page, focus, selection, panels.
    pub lead: String,
    /// The project and devices lines.
    pub text: String,
    /// Every offer in the tree, in publish order.
    pub offers: Vec<UiOffer>,
    /// Where the user is, as offer prefixes: which offers are listed in
    /// full and which are counted.
    pub focus: UiOfferFocus,
    /// Whether a real (non-sim) board is connected or attached right now.
    /// `false` keeps `devices/connect-usb` and `devices/connect-ble` listed
    /// in full on every page, not just a devices-focused one: getting a
    /// board onto the bus is the user's next step and the only one only
    /// they can take, so its `[needs the user's click]` mark (and the
    /// doctrine tied to it) must show instead of being folded into a
    /// counted "devices: connect-usb, connect-ble" line the agent reads
    /// past (live corpus S18, 2026-10-03: with no board attached, the
    /// agent told the user to press "Connect a board via USB" instead of
    /// `act`ing the offer that hands them the card).
    pub has_real_board: bool,
}

impl AppReadoutSnapshot {
    /// The readout text: the lead, the actions there in full (each with its
    /// label, `[disabled: …]` when it cannot be pressed — `[choose a board
    /// in args]` when all it waits for is a value — and what pressing it
    /// does: `[needs the user's click]`, a card the user presses, or
    /// `[undoable]`, it takes something away that Revert brings back), the
    /// project and devices, then every other action counted per area.
    pub fn render(&self) -> String {
        let mut text = self.lead.clone();
        if self.offers.is_empty() {
            text.push_str(&self.text);
            text.push_str("actions: none offered\n");
            return text;
        }
        let (listed, counted): (Vec<&UiOffer>, Vec<&UiOffer>) = self
            .offers
            .iter()
            .partition(|offer| self.lists_in_full(offer));
        if listed.is_empty() {
            text.push_str("actions here: none\n");
        } else {
            text.push_str("actions here (press one with `act` by its path):\n");
            for offer in listed {
                text.push_str(&offer_lines(offer));
            }
        }
        text.push_str(&self.text);
        text.push_str(&counted_lines(&counted));
        text
    }

    /// The offer at `path`, if the tree holds one (listed or counted).
    pub fn offer(&self, path: &OfferPath) -> Option<&UiOffer> {
        self.offers.iter().find(|offer| &offer.path == path)
    }

    /// Whether `offer` is listed in full: a verb of the node in focus, a
    /// verb on the page's own area that is not some other node's, or — with
    /// no real board on the bus — one of the two add-a-board offers,
    /// wherever the user is.
    fn lists_in_full(&self, offer: &UiOffer) -> bool {
        if !self.has_real_board && is_add_board_offer(&offer.path) {
            return true;
        }
        match self.focus.nearness(&offer.path) {
            OfferNearness::Own => true,
            OfferNearness::Under | OfferNearness::Elsewhere => false,
            OfferNearness::Area => !owned_by_a_node(offer),
        }
    }
}

/// Whether `path` is one of the add-a-board offers (`devices/connect-usb`,
/// `devices/connect-ble`): with no real board attached, the user's next
/// step from anywhere in the app.
fn is_add_board_offer(path: &OfferPath) -> bool {
    path.owner().as_ref() == Some(&OfferPath::devices())
        && matches!(path.last(), Some("connect-usb") | Some("connect-ble"))
}

/// One offer as the readout (and `read`) lists it: `- <path>: <label>`
/// with its state, and what it takes on the next line when it takes
/// anything.
pub fn offer_lines(offer: &UiOffer) -> String {
    let mut text = String::new();
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
    // An offer that takes a secret is always the user's card, whatever its
    // level: the agent never fills a password.
    if meta.needs_user() || offer.takes_a_secret() {
        text.push_str(" [needs the user's click]");
    } else if meta.consequence == ActionConsequence::Undoable {
        text.push_str(" [undoable]");
    }
    text.push('\n');
    if !offer.params().is_empty() {
        let params: Vec<String> = offer.params().iter().map(param_text).collect();
        let _ = writeln!(text, "  takes {}", params.join("; "));
    }
    text
}

/// Whether `offer` is a node's verb, directly (`…/orbit.shader/remove`) or
/// in a group of the node's (`…/dome.fixture/patch/assign`).
fn owned_by_a_node(offer: &UiOffer) -> bool {
    offer.path.node_and_verb().is_some()
}

/// The actions not listed in full, counted: every node verb on one line,
/// as verb counts (bounded by how many kinds of verb there are, not by how
/// many nodes), then one line per other owner, naming its verbs.
fn counted_lines(counted: &[&UiOffer]) -> String {
    if counted.is_empty() {
        return String::new();
    }
    let mut nodes: Vec<OfferPath> = Vec::new();
    let mut node_verbs: Vec<(String, usize)> = Vec::new();
    let mut owners: Vec<(OfferPath, Vec<&str>)> = Vec::new();
    for offer in counted {
        // A node's verb counts under its node, a grouped one by its group
        // (`patch/assign ×2`).
        if let Some((node, verb)) = offer.path.node_and_verb() {
            if !nodes.contains(&node) {
                nodes.push(node);
            }
            match node_verbs.iter_mut().find(|(name, _)| *name == verb) {
                Some((_, count)) => *count += 1,
                None => node_verbs.push((verb, 1)),
            }
        } else {
            let owner = offer.path.owner().unwrap_or_else(|| offer.path.clone());
            let verb = offer.path.last().unwrap_or("?");
            match owners.iter_mut().find(|(path, _)| *path == owner) {
                Some((_, verbs)) => verbs.push(verb),
                None => owners.push((owner, vec![verb])),
            }
        }
    }
    let mut text = "more actions, counted (`read` a node or a device to list its own; \
                    `act` takes any path):\n"
        .to_string();
    if !nodes.is_empty() {
        let verbs: Vec<String> = node_verbs
            .iter()
            .map(|(verb, count)| format!("{verb} ×{count}"))
            .collect();
        let _ = writeln!(
            text,
            "- on {} other node{}: {}",
            nodes.len(),
            if nodes.len() == 1 { "" } else { "s" },
            verbs.join(", ")
        );
    }
    for (owner, verbs) in owners {
        let _ = writeln!(text, "- {owner}: {}", verbs.join(", "));
    }
    text
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
                // A choice's own filter text widens it while it finds
                // something (a version the box finds): say so, not "on".
                let _ = match param.filter.as_deref() == Some(toggle) {
                    true => write!(
                        text,
                        "; {toggle} text that finds them also offers {}",
                        choice_list(&widened, limit)
                    ),
                    false => write!(
                        text,
                        "; with {toggle} on, also {}",
                        choice_list(&widened, limit)
                    ),
                };
            }
            if let Some(preselect) = preselect {
                let _ = write!(text, " [default {preselect}]");
            }
            text
        }
        // A secret is the user's to type: the agent is told so, never what
        // it may hold, and `act` refuses any value for it.
        OfferParamKind::Text { secret: true, .. } => {
            format!("{} (secret — the user types it)", param.name)
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
        | OfferArgError::OptionDisabled { name, .. }
        | OfferArgError::Invalid { name, .. } => Some(name.as_str()),
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

/// The page line: the page the web reports, or — before it has (and in the
/// headless tests and evals) — what core shows.
pub fn page_line(home: bool, page: Option<&UiPage>) -> String {
    match page {
        Some(page) if home && page.is_editor() => {
            format!("page: {} (no project open yet)\n", page.describe())
        }
        Some(page) => format!("page: {}\n", page.describe()),
        None if home => "page: home (no project open)\n".to_string(),
        None => "page: project editor\n".to_string(),
    }
}

/// The line under the page line while an open from Home is in flight: the
/// project is starting the device it runs on (a sim, in this tab) and lands
/// in the editor by itself. Without it, the agent read Home after
/// `project/new` answered "Waiting for the device", decided the open had
/// failed, and pressed open again and again (activity corpus S4, S18 and
/// S19, 2026-10-03).
pub fn opening_line(key: &str, title: Option<&str>) -> String {
    let name = match title {
        Some(title) => format!("{title:?} ({key})"),
        None => key.to_string(),
    };
    format!(
        "opening: {name} — its device is starting; the editor opens by itself when it is \
         up, so do not press open again\n"
    )
}

/// What the user is looking at: the node in focus (or that none is), what
/// the patch surface has selected, and what is open over the page. Empty
/// off the editor with nothing open: the page line says it all.
pub(crate) fn looking_at_lines(
    editor: bool,
    node: Option<&AgentNodeFocus>,
    selection: Option<String>,
    place: Option<&UiPlace>,
) -> String {
    let mut text = String::new();
    if editor {
        match node {
            Some(node) => {
                let _ = write!(
                    text,
                    "you are looking at: node {} ({}), {}",
                    node.name, node.kind, node.status
                );
                if !node.open.is_empty() {
                    let _ = write!(text, "; its card has {} open", node.open.join(", "));
                }
                let _ = writeln!(text, "; its actions are at {}/…", node.prefix);
            }
            None => text.push_str("you are looking at: no node in particular\n"),
        }
        if let Some(selection) = selection {
            let _ = writeln!(text, "selected: {selection}");
        }
    }
    if let Some(place) = place
        && !place.panels.is_empty()
    {
        let panels: Vec<&str> = place.panels.iter().map(|panel| panel.describe()).collect();
        let _ = writeln!(text, "open over the page: {}", panels.join(", "));
    }
    text
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
            lpa_devices::FirmwareFace::CoreOnly { .. } => "LightPlayer, running only its core",
            lpa_devices::FirmwareFace::Unknown => "not identified yet",
            _ => "other firmware",
        };
        // What it runs is the board's own report; before its first one,
        // nothing is claimed — but a LightPlayer that has not reported yet
        // says so, because right after a flash or an update that is the
        // state the agent must not read as done. A board that runs nothing
        // says it plainly: its lights are dark until a project is pushed
        // (the corpus's S15 and S19 stopped there, at a blank board).
        let lightplayer = matches!(
            device.firmware_face,
            lpa_devices::FirmwareFace::LightPlayer { .. }
        );
        let loaded = match &device.loaded_project {
            lpa_devices::view::LoadedProject::Running { label } => format!("; running {label:?}"),
            lpa_devices::view::LoadedProject::Empty => {
                " — no project on it; it runs nothing and its lights stay dark until one \
                 is pushed"
                    .to_string()
            }
            lpa_devices::view::LoadedProject::Unknown if lightplayer => {
                "; has not said yet what it runs".to_string()
            }
            lpa_devices::view::LoadedProject::Unknown => String::new(),
        };
        // The firmware update's own line, when the board has an update
        // story: heal and finish start by themselves, so the agent reads
        // them here rather than finding an action for them.
        let update = roster
            .updates
            .get(&device.id)
            .map(|update| format!("; firmware: {}", update.line))
            .unwrap_or_default();
        let _ = writeln!(
            text,
            "- {:?}: chip {}; board {}; {firmware}; {}{loaded}{update}",
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
            lead: "page: project editor\n".to_string(),
            focus: UiOfferFocus {
                node: Some(node.clone()),
                areas: vec![OfferPath::project()],
            },
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
            ..AppReadoutSnapshot::default()
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
    fn the_focused_nodes_verbs_are_listed_and_the_rest_counted_per_area() {
        let mut readout = snapshot();
        let other = |path: &str| {
            let address = ProjectNodeAddress::parse(path).unwrap();
            OfferPath::project_node(&address)
        };
        for node in ["/demo.module/clock.clock", "/demo.module/fixture.fixture"] {
            readout.offers.push(UiOffer::new(
                other(node).child("remove"),
                "remove",
                save_action().with_label("Remove node"),
            ));
        }
        readout.offers.push(UiOffer::new(
            other("/demo.module/fixture.fixture").child("revert"),
            "revert",
            save_action().with_label("Revert"),
        ));
        for verb in ["connect-usb", "connect-ble"] {
            readout.offers.push(UiOffer::new(
                OfferPath::devices().child(verb),
                "usb",
                save_action().with_label(verb),
            ));
        }
        readout.text = "project: \"Demo\"\n".to_string();
        // A real board is on the bus, so the add-a-board offers fold into
        // the counted devices line like any other off-area offer.
        readout.has_real_board = true;

        let text = readout.render();
        assert!(
            text.starts_with(
                "page: project editor\n\
                 actions here (press one with `act` by its path):\n\
                 - project/save: Save\n"
            ),
            "the lead, then the actions near the user: {text}"
        );
        assert!(text.contains("- project/demo.module/orbit.shader/remove: Remove"));
        assert!(
            !text.contains("clock.clock/remove: Remove node"),
            "an unfocused node's verb is counted, not listed: {text}"
        );
        assert!(
            text.ends_with(
                "project: \"Demo\"\n\
                 more actions, counted (`read` a node or a device to list its own; \
                 `act` takes any path):\n\
                 - on 2 other nodes: remove ×2, revert ×1\n\
                 - devices: connect-usb, connect-ble\n"
            ),
            "{text}"
        );
        let clock = other("/demo.module/clock.clock").child("remove");
        assert!(
            readout.offer(&clock).is_some(),
            "a counted verb is still pressable"
        );
    }

    /// Live corpus S18 (2026-10-03): in the project editor with no real
    /// board attached, the agent read "devices: connect-usb, connect-ble"
    /// — a counted line with no `[needs the user's click]` mark — and told
    /// the user to press the button itself instead of `act`ing the offer
    /// that hands them the card. With no real board, the two add-a-board
    /// offers must list in full even while the focus area is the project,
    /// not devices.
    #[test]
    fn with_no_real_board_the_add_board_offers_list_in_full_on_every_page() {
        let mut readout = snapshot();
        // Both real offers need a real click (`navigator.serial
        // .requestPort()`, `navigator.bluetooth.requestDevice()`) — the
        // mark this fix exists to surface.
        readout.offers.push(UiOffer::new(
            OfferPath::devices().child("connect-usb"),
            "usb",
            save_action()
                .with_label("Connect a board via USB")
                .needs_user_activation(),
        ));
        readout.offers.push(UiOffer::new(
            OfferPath::devices().child("connect-ble"),
            "bluetooth",
            save_action()
                .with_label("Connect a board via Bluetooth")
                .needs_user_activation(),
        ));
        readout.has_real_board = false;
        // The focus is the project, not devices — the usual rule would
        // count both offers under "- devices: …" instead of listing them.
        assert_eq!(readout.focus.areas, vec![OfferPath::project()]);

        let text = readout.render();
        assert!(
            text.contains(
                "- devices/connect-usb: Connect a board via USB [needs the user's click]\n"
            ),
            "{text}"
        );
        assert!(
            text.contains(
                "- devices/connect-ble: Connect a board via Bluetooth \
                 [needs the user's click]\n"
            ),
            "{text}"
        );
        assert!(
            !text.contains("devices: connect-usb"),
            "the add-board offers are listed, not folded into a counted line: {text}"
        );
    }

    #[test]
    fn a_page_with_no_area_lists_nothing_in_full() {
        let mut readout = snapshot();
        readout.focus = UiOfferFocus::none();
        let text = readout.render();
        assert!(text.contains("actions here: none\n"), "{text}");
        assert!(
            text.contains("- on 1 other node: remove ×1\n- project: save, revert\n"),
            "{text}"
        );
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
                OfferParam::text("password", "password", "unchanged")
                    .optional()
                    .secret(),
            ],
            OfferBinder::new(move |_: &OfferArgs| Ok(bound.clone())),
            save,
        );
        let text = AppReadoutSnapshot {
            offers: vec![offer],
            focus: devices_page(),
            ..AppReadoutSnapshot::default()
        }
        .render();
        assert!(
            text.contains(
                "  takes board: one of xiao (XIAO ESP32-C6), devkit (ESP32-C6 DevKit; not now: \
                 no build) [default xiao]; name: optional text; note: text, at most 8 \
                 characters; loud: true or false [now false]; password (secret — the user \
                 types it)\n"
            ),
            "{text}"
        );
        assert!(
            text.contains("flash: Save [choose a note in args] [needs the user's click]\n"),
            "an offer that takes a secret is the user's card: {text}"
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
            offers: vec![offer.clone()],
            focus: devices_page(),
            ..AppReadoutSnapshot::default()
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

    fn devices_page() -> UiOfferFocus {
        UiOfferFocus {
            node: None,
            areas: vec![OfferPath::devices()],
        }
    }

    fn save_action() -> UiAction {
        UiAction::from_op(ControllerId::new("studio|project"), ProjectOp::SaveOverlay)
    }

    /// A board that runs nothing says so in words the agent cannot read
    /// as done (S15, S19), and a LightPlayer that has not reported yet
    /// says that instead of nothing.
    #[test]
    fn a_board_that_runs_nothing_says_so_plainly() {
        use lpa_devices::view::LoadedProject;
        let line = |loaded: LoadedProject| {
            let mut roster = DeviceRosterView::default();
            roster.roster.devices.push(lightplayer_board(loaded));
            device_lines(&roster)
        };
        assert_eq!(
            line(LoadedProject::Empty),
            "devices:\n- \"Bench board\": chip esp32c6; board seeed/xiao-esp32-c6; \
             LightPlayer; Ready — no project on it; it runs nothing and its lights stay \
             dark until one is pushed\n"
        );
        assert!(line(LoadedProject::Unknown).ends_with("; Ready; has not said yet what it runs\n"));
        assert!(
            line(LoadedProject::Running {
                label: "porch".to_string()
            })
            .ends_with("; Ready; running \"porch\"\n")
        );
    }

    fn lightplayer_board(loaded: lpa_devices::view::LoadedProject) -> lpa_devices::DeviceView {
        lpa_devices::DeviceView {
            id: crate::DeviceId(7),
            title: "Bench board".to_string(),
            status: lpa_devices::device::DeviceStatus::Ready,
            state_label: "Ready".to_string(),
            detail: None,
            freshness_label: None,
            identity_label: None,
            detected_chip: Some("esp32c6".to_string()),
            board_id: Some("seeed/xiao-esp32-c6".to_string()),
            firmware_face: lpa_devices::FirmwareFace::LightPlayer {
                firmware: None,
                wire: lpa_devices::WireVersion::Match,
                age: lpa_devices::FirmwareAge::Unknown,
            },
            remembered_firmware: None,
            degraded: None,
            loaded_project: loaded,
            engine_fps: None,
            link_counters: None,
            can_receive_project: true,
            can_remove_project: false,
            activity: None,
            last_outcome: None,
            terminal: Vec::new(),
            terminal_dropped: 0,
            firmware_blocked: None,
            held_elsewhere: None,
            escapes: Vec::new(),
            update_blocked: None,
            last_update_outcome: None,
        }
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
