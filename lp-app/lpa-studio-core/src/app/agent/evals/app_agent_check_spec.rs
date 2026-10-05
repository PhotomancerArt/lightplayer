//! [`CheckSpec`]: one `[[check]]` in a scenario, with its parameters.
//!
//! Each kind is small and pure over the run: the project it left, the
//! conversation it had, the cards it handed over, and (device seat) what
//! the board ended up running. The verdicts are in `app_agent_checks.rs`
//! (project and board) and `app_agent_conversation_checks.rs`.
//!
//! The E1–E3 ids keep parsing as shorthand kinds (`output_on_d6`,
//! `target_is_xiao_c6`, `asked_about_board`, `no_d_label_before_board`),
//! each the parameterised check it always meant.

use serde::Deserialize;
use serde_json::Value;

/// The XIAO ESP32-C6's catalog board id (`lpc-hardware/boards/seeed/xiao-esp32-c6.json`).
pub(crate) const XIAO_C6_BOARD_ID: &str = "seeed/xiao-esp32-c6";

/// One check, as a scenario names it.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum CheckSpec {
    // --- the project ---------------------------------------------------
    /// The manifest's `target` is `board`.
    TargetIs { board: String },
    /// Exactly one Output, and one of its ports drives
    /// `ws281x:local:<pin>`.
    OutputOn { pin: String },
    /// Exactly one Fixture mapping `leds` lamps in strip order.
    StripOf { leds: u32 },
    /// The Fixtures map `leds` lamps between them, in any shape.
    LampCount { leds: u32 },
    /// A Playlist on Cycle stepping within `step_seconds`, with at least
    /// `min_entries` entries that are catalog patterns from `from` (any
    /// catalog pattern when `from` is empty).
    #[serde(alias = "playlist_cycles")]
    Playlist {
        #[serde(default = "three")]
        min_entries: usize,
        #[serde(default)]
        step_seconds: Option<[f64; 2]>,
        #[serde(default, alias = "colourful")]
        from: Vec<String>,
    },
    /// Clock → playlist → fixture → output over the bus.
    GraphWired,
    /// No node failed, and something runs.
    AllNodesOk,
    /// No unsaved edits remain.
    Saved,
    /// From the start project only what `allow` names changed (and
    /// something did): `strip_size` (the fixture's render size, its map
    /// body, an output port's count — the default), `<Kind>.<path>` (that
    /// field of every node of that kind), or `modules` (pattern folders
    /// added or removed).
    MinimalDiff {
        #[serde(default)]
        allow: Vec<String>,
    },
    /// The project's bytes, read by meaning, equal the start's.
    Unchanged,
    /// The one root-level node of kind `node` has `path` (dotted) equal to
    /// `equals`, or a number within `between`. `default` stands in for an
    /// absent field (the engine's own default).
    Field {
        node: String,
        path: String,
        #[serde(default)]
        equals: Option<Value>,
        #[serde(default)]
        between: Option<[f64; 2]>,
        #[serde(default)]
        default: Option<Value>,
    },
    /// No playlist entry plays any of these catalog patterns any more.
    EntriesRemoved { patterns: Vec<String> },
    /// Every one of these catalog patterns still plays, its folder
    /// unchanged from the start.
    EntriesKept { patterns: Vec<String> },
    /// At least `min` playlist entries play a catalog pattern the start
    /// did not (from `from`, when it is not empty).
    EntriesAdded {
        min: usize,
        #[serde(default)]
        from: Vec<String>,
    },
    /// Any one of `of` passes.
    AnyOf { of: Vec<CheckSpec> },

    // --- the conversation ----------------------------------------------
    /// The run used a scripted reply about `topic`: the agent asked.
    Asked { topic: String },
    /// The agent asked about `topic` before `before` happened.
    AskedBefore { topic: String, before: Gate },
    /// At most `n` turns ended on a question; with `per_turn`, no
    /// question turn asked more than that many things at once.
    MaxQuestions {
        n: usize,
        #[serde(default)]
        per_turn: Option<usize>,
    },
    /// At most `n` model turns.
    MaxTurns { n: u32 },
    /// The agent handed over a card for an offer matching `offer` (a
    /// glob).
    CardHanded { offer: String },
    /// Something the agent must never do: `pressed:<glob>` (its own `act`
    /// went through on an offer), `act:<glob>` (it tried), `card:<glob>`
    /// (it handed that card), `tool:<name>` (it called that tool).
    Never { what: String },
    /// The agent said at least one of `words` (in its last message, with
    /// `last`).
    SaidAny {
        words: Vec<String>,
        #[serde(default)]
        last: bool,
    },
    /// The agent said none of `words`.
    SaidNone { words: Vec<String> },

    // --- the board (device seat) ---------------------------------------
    /// The board reports running a project.
    BoardRunsProject,
    /// What firmware the board ended with.
    BoardFirmware { is: FirmwareIs },

    // --- E1–E3 shorthands ----------------------------------------------
    /// `output_on { pin = "D6" }`.
    OutputOnD6,
    /// `target_is { board = "seeed/xiao-esp32-c6" }`.
    TargetIsXiaoC6,
    /// `asked { topic = "board" }`.
    AskedAboutBoard,
    /// `asked_before { topic = "board", before = "pin" }`, for D-labels.
    NoDLabelBeforeBoard,
}

/// What `asked_before` holds the question to.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Gate {
    /// Any tool call that writes an LED pin (`ws281x:local:…`).
    Pin,
    /// Any `edit_project` call.
    Edit,
    /// An `act` on a `devices/*/flash`.
    Flash,
    /// An `act` on a `devices/*/push`.
    Push,
}

/// What `board_firmware` wants.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FirmwareIs {
    /// The board runs LightPlayer (Ready).
    Lightplayer,
    /// Firmware was written to it during the run.
    Flashed,
    /// Nothing was written to it.
    Untouched,
}

fn three() -> usize {
    3
}

impl CheckSpec {
    /// The check's name in the report: its kind and its key parameter.
    pub(crate) fn name(&self) -> String {
        match self {
            Self::TargetIs { board } => format!("target_is({board})"),
            Self::OutputOn { pin } => format!("output_on({pin})"),
            Self::StripOf { leds } => format!("strip_of({leds})"),
            Self::LampCount { leds } => format!("lamp_count({leds})"),
            Self::Playlist { .. } => "playlist".to_string(),
            Self::GraphWired => "graph_wired".to_string(),
            Self::AllNodesOk => "all_nodes_ok".to_string(),
            Self::Saved => "saved".to_string(),
            Self::MinimalDiff { .. } => "minimal_diff".to_string(),
            Self::Unchanged => "unchanged".to_string(),
            Self::Field { node, path, .. } => format!("field({node}.{path})"),
            Self::EntriesRemoved { .. } => "entries_removed".to_string(),
            Self::EntriesKept { .. } => "entries_kept".to_string(),
            Self::EntriesAdded { .. } => "entries_added".to_string(),
            Self::AnyOf { of } => format!(
                "any_of({})",
                of.iter().map(Self::name).collect::<Vec<_>>().join(" | ")
            ),
            Self::Asked { topic } => format!("asked({topic})"),
            Self::AskedBefore { topic, before } => {
                format!("asked_before({topic}, {before:?})").to_lowercase()
            }
            Self::MaxQuestions { n, .. } => format!("max_questions({n})"),
            Self::MaxTurns { n } => format!("max_turns({n})"),
            Self::CardHanded { offer } => format!("card_handed({offer})"),
            Self::Never { what } => format!("never({what})"),
            Self::SaidAny { .. } => "said_any".to_string(),
            Self::SaidNone { .. } => "said_none".to_string(),
            Self::BoardRunsProject => "board_runs_project".to_string(),
            Self::BoardFirmware { is } => format!("board_firmware({is:?})").to_lowercase(),
            Self::OutputOnD6 => "output_on_d6".to_string(),
            Self::TargetIsXiaoC6 => "target_is_xiao_c6".to_string(),
            Self::AskedAboutBoard => "asked_about_board".to_string(),
            Self::NoDLabelBeforeBoard => "no_d_label_before_board".to_string(),
        }
    }

    /// The kind, for grouping failures in the report.
    pub(crate) fn kind(&self) -> String {
        let name = self.name();
        name.split('(').next().unwrap_or(&name).to_string()
    }

    /// Whether the check judges the conversation (a golden has none).
    pub(crate) fn needs_transcript(&self) -> bool {
        match self {
            Self::Asked { .. }
            | Self::AskedBefore { .. }
            | Self::MaxQuestions { .. }
            | Self::MaxTurns { .. }
            | Self::CardHanded { .. }
            | Self::Never { .. }
            | Self::SaidAny { .. }
            | Self::SaidNone { .. }
            | Self::AskedAboutBoard
            | Self::NoDLabelBeforeBoard => true,
            Self::AnyOf { of } => of.iter().any(Self::needs_transcript),
            _ => false,
        }
    }

    /// Whether the check judges a board (the device seat).
    pub(crate) fn needs_device(&self) -> bool {
        match self {
            Self::BoardRunsProject | Self::BoardFirmware { .. } => true,
            Self::AnyOf { of } => of.iter().any(Self::needs_device),
            _ => false,
        }
    }

    /// The pin an output check names (stage B's pad).
    pub(crate) fn pin(&self) -> Option<&str> {
        match self {
            Self::OutputOn { pin } => Some(pin),
            Self::OutputOnD6 => Some("D6"),
            _ => None,
        }
    }

    /// The LED count a strip check names (stage B's frame length).
    pub(crate) fn leds(&self) -> Option<u32> {
        match self {
            Self::StripOf { leds } => Some(*leds),
            _ => None,
        }
    }

    /// The parameter rules serde cannot say.
    pub(crate) fn validate(&self) -> Result<(), String> {
        match self {
            Self::Field {
                equals, between, ..
            } if equals.is_some() == between.is_some() => Err(format!(
                "{} names exactly one of `equals` and `between`",
                self.name()
            )),
            Self::Playlist {
                step_seconds: Some([lo, hi]),
                ..
            }
            | Self::Field {
                between: Some([lo, hi]),
                ..
            } if lo > hi => Err(format!("{}: the range {lo}..{hi} is empty", self.name())),
            Self::AnyOf { of } if of.is_empty() => Err("any_of with nothing in it".to_string()),
            Self::AnyOf { of } => of.iter().try_for_each(Self::validate),
            Self::Never { what } => match what.split_once(':') {
                Some(("pressed" | "act" | "card" | "tool", rest)) if !rest.is_empty() => Ok(()),
                _ => Err(format!(
                    "never: {what:?} is not pressed:<glob>, act:<glob>, card:<glob> or tool:<name>"
                )),
            },
            Self::SaidAny { words, .. } | Self::SaidNone { words } if words.is_empty() => {
                Err(format!("{} with no words", self.name()))
            }
            _ => Ok(()),
        }
    }
}
