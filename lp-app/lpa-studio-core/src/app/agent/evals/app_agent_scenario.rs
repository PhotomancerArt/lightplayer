//! [`Scenario`]: one agent activity in the corpus, read from
//! `tests/fixtures/app_agent/scenarios/<name>.toml` (plan
//! `lp2025/2026-10-01-1255-agentic-ui-roadmap/m-agent-activity-corpus`).
//!
//! A scenario is one small story: who the person is, where things start
//! (the project, and the board — known to the app, plugged in, blank or
//! running something), what they say first, how they answer the agent's
//! questions, which cards they click, and the checks that decide pass.
//! Scenarios are data so Yona can edit a message, a reply or a check
//! without touching code. The fields are documented in the fixtures'
//! `README.md`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::app_agent_check_spec::CheckSpec;

/// The scenario fixtures directory.
pub(crate) fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/app_agent")
}

/// One corpus scenario.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Scenario {
    /// The corpus id (`S1`), as `scenarios.md` numbers it.
    pub(crate) id: String,
    /// File stem (`e1-sean-from-empty`, `s18-festival-2am`).
    pub(crate) name: String,
    /// One line for the report.
    pub(crate) summary: String,
    /// Who is asking.
    pub(crate) persona: Persona,
    /// What the scenario exercises, for the report's rollups and the
    /// runner's `--tag` selector.
    pub(crate) tags: Vec<String>,
    /// `pending` scenarios parse and run their golden in CI, but the live
    /// runner skips them unless asked (`--include-pending`).
    #[serde(default)]
    pub(crate) status: ScenarioStatus,
    /// What a pending scenario waits for (`M10`, `xiao-c6-d4-d5 fix`,
    /// `feature:schedules`).
    #[serde(default)]
    pub(crate) waits_for: String,
    /// Why, in a line or two (shown in the dry run and the report).
    #[serde(default)]
    pub(crate) note: String,
    /// The committed golden that proves this scenario's project checks
    /// pass (`golden/<name>/`), when one exists.
    #[serde(default)]
    pub(crate) golden: Option<String>,
    pub(crate) start: ScenarioStart,
    pub(crate) user: UserScript,
    pub(crate) budget: Budget,
    /// The checks that decide pass/fail, in report order.
    #[serde(rename = "check")]
    pub(crate) checks: Vec<CheckSpec>,
}

/// The kinds of people the corpus is written for (`scenarios.md`).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Persona {
    /// The savvy newcomer.
    Sean,
    /// The semi-technical LED artist.
    Luna,
    /// The new LED artist who asks a lot.
    Viatrix,
    /// The venue consumer.
    Jordan,
    /// The power user.
    Yona,
}

impl Persona {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Sean => "sean",
            Self::Luna => "luna",
            Self::Viatrix => "viatrix",
            Self::Jordan => "jordan",
            Self::Yona => "yona",
        }
    }
}

/// Whether the live runner runs a scenario.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ScenarioStatus {
    #[default]
    Active,
    /// The product cannot do it yet; `waits_for` names what will.
    Pending,
}

/// Where a scenario's run starts.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ScenarioStart {
    /// The project in front of the user.
    pub(crate) project: StartProject,
    /// The board: what the app knows about it, and (device seat) what is
    /// plugged in.
    #[serde(default)]
    pub(crate) board: StartBoard,
    /// The context line the agent reads, verbatim, in place of the one
    /// generated from `board`. Leave empty to generate it.
    #[serde(default)]
    pub(crate) context: String,
    /// Appended to the context line.
    #[serde(default)]
    pub(crate) note: String,
    /// The node path selected in the editor. Recorded for the scenarios
    /// that need place (M7/M8); the harness does not select yet.
    #[serde(default)]
    pub(crate) selection: String,
}

/// The project a run starts with.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum StartProject {
    /// No project open (a device-seat start: the board comes first).
    None,
    /// `HomeOp::CreateProject { template: Blank }` — the root module only.
    Blank,
    /// A committed golden project, opened and saved (on a running board,
    /// the project the board runs and the library holds).
    Golden(String),
}

/// The board a scenario starts with.
///
/// The **project seat** (a headless Studio over an in-process server)
/// reads `known` / `chip` / `actual`: what the app knows, and the board
/// the server's outputs really open against. The **device seat** (the
/// device bench, a fake board plugged in over USB) adds `state`.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StartBoard {
    /// The board the app already knows (its catalog id).
    #[serde(default)]
    pub(crate) known: Option<String>,
    /// The chip the app knows (`esp32c6`) when it does not know the board.
    #[serde(default)]
    pub(crate) chip: Option<String>,
    /// The board it really is, when the app does not know: its pin map is
    /// the one outputs open against. Never told to the agent.
    #[serde(default)]
    pub(crate) actual: Option<String>,
    /// Device seat: what the plugged-in board is running.
    #[serde(default)]
    pub(crate) state: Option<BoardState>,
    /// Device seat: the port is already granted (the board shows in the
    /// roster at the start), rather than waiting for a connect click.
    #[serde(default)]
    pub(crate) connected: bool,
    /// Device seat, `state = "foreign"`: which other firmware.
    #[serde(default)]
    pub(crate) firmware: Option<ForeignFirmware>,
}

/// What a plugged-in board runs at the start (device seat).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BoardState {
    /// Out of the bag: blank flash.
    Blank,
    /// Somebody else's firmware (`firmware` says whose).
    Foreign,
    /// LightPlayer, running the start project.
    Running,
    /// An older LightPlayer, running the start project: an update is
    /// offered.
    Older,
}

/// Which other firmware a `foreign` board runs.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ForeignFirmware {
    /// The Seeed XIAO C6's factory demo.
    #[default]
    FactoryDemo,
    /// WLED.
    Wled,
}

/// Where a scenario is run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SeatKind {
    /// A headless Studio over an in-process server wearing the board's pin
    /// map.
    Project,
    /// The device bench: a fake board behind a scripted USB port.
    Device,
}

/// The person's side of the conversation.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UserScript {
    /// The opening message.
    pub(crate) say: String,
    /// Follow-up messages, sent in order after a turn that ends without a
    /// question.
    #[serde(default)]
    pub(crate) then: Vec<String>,
    /// The persona's answer to a question nobody scripted (used once; a
    /// second unscripted question ends the run).
    #[serde(default)]
    pub(crate) otherwise: Option<String>,
    /// Scripted answers, matched against the question the agent ends a
    /// turn on.
    #[serde(default, rename = "reply")]
    pub(crate) replies: Vec<ScriptedReply>,
    /// What the person does with cards whose offer matches.
    #[serde(default, rename = "card")]
    pub(crate) cards: Vec<CardRule>,
    /// What the person does with a card no rule names: clicks it (the
    /// default, as `scenarios.md` reads), or leaves it.
    #[serde(default)]
    pub(crate) unlisted_cards: CardDo,
}

/// One canned answer.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ScriptedReply {
    /// The topic the checks name (`asked { topic = "board" }`); the first
    /// `when` keyword when unset.
    #[serde(default)]
    pub(crate) topic: Option<String>,
    /// Any of these, case-insensitive, at the start of a word in the
    /// question, picks this reply.
    pub(crate) when: Vec<String>,
    pub(crate) text: String,
}

impl ScriptedReply {
    pub(crate) fn topic(&self) -> &str {
        self.topic
            .as_deref()
            .or_else(|| self.when.first().map(String::as_str))
            .unwrap_or("")
    }
}

/// What the person does with a card.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CardRule {
    /// A glob over the card's offer path (`devices/*/flash`): `*` matches
    /// within one segment, `**` any number of segments.
    pub(crate) offer: String,
    #[serde(default, rename = "do")]
    pub(crate) action: CardDo,
    /// Values the person picks before clicking (over the agent's).
    #[serde(default)]
    pub(crate) args: BTreeMap<String, String>,
}

/// Click a card, or leave it.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CardDo {
    #[default]
    Click,
    Leave,
}

/// Per-run limits; the run stops (and the report says why) past any.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Budget {
    /// Model turns across the whole scenario (replies included).
    pub(crate) turns: u32,
    /// Reported cost, US dollars.
    pub(crate) usd: f64,
    /// Tokens in + out: a model-neutral cap (bake-offs price differently).
    #[serde(default)]
    pub(crate) tokens: Option<u64>,
}

/// Which scenarios a run takes (the runner's selectors).
#[derive(Clone, Debug, Default)]
pub(crate) struct Selection {
    /// `all`, a name, a name prefix (`e1`, `s18`), or nothing.
    pub(crate) which: Option<String>,
    /// Ids or names (`S4`, `s07-luna-mushrooms`); empty: no filter.
    pub(crate) only: Vec<String>,
    /// Any of these tags; empty: no filter.
    pub(crate) tags: Vec<String>,
    /// Any of these personas; empty: no filter.
    pub(crate) personas: Vec<String>,
    /// Run pending scenarios too.
    pub(crate) include_pending: bool,
}

impl Selection {
    /// Read the runner's environment (`LPA_APP_EVAL_SCENARIO`, `_ONLY`,
    /// `_TAGS`, `_PERSONAS`, `_INCLUDE_PENDING`).
    pub(crate) fn from_env() -> Self {
        let list = |name: &str| -> Vec<String> {
            std::env::var(name)
                .unwrap_or_default()
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_string)
                .collect()
        };
        Self {
            which: std::env::var("LPA_APP_EVAL_SCENARIO").ok(),
            only: list("LPA_APP_EVAL_ONLY"),
            tags: list("LPA_APP_EVAL_TAGS"),
            personas: list("LPA_APP_EVAL_PERSONAS"),
            include_pending: std::env::var("LPA_APP_EVAL_INCLUDE_PENDING")
                .is_ok_and(|value| value == "1"),
        }
    }

    /// Whether `scenario` is picked, or why it is not.
    pub(crate) fn picks(&self, scenario: &Scenario) -> Result<(), String> {
        if let Some(which) = self.which.as_deref().filter(|which| *which != "all")
            && !scenario.answers_to(which)
        {
            return Err(format!("not {which}"));
        }
        if !self.only.is_empty() && !self.only.iter().any(|item| scenario.answers_to(item)) {
            return Err("not in --only".to_string());
        }
        if !self.tags.is_empty() && !self.tags.iter().any(|tag| scenario.tags.contains(tag)) {
            return Err("no selected tag".to_string());
        }
        if !self.personas.is_empty()
            && !self
                .personas
                .iter()
                .any(|persona| persona == scenario.persona.name())
        {
            return Err("another persona".to_string());
        }
        if scenario.status == ScenarioStatus::Pending && !self.include_pending {
            return Err(format!("pending on {}", scenario.waits_for));
        }
        Ok(())
    }
}

impl Scenario {
    /// Load `scenarios/<name>.toml` and check it is well formed.
    pub(crate) fn load(name: &str) -> Result<Self, String> {
        let path = fixtures_dir()
            .join("scenarios")
            .join(format!("{name}.toml"));
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let scenario: Self =
            toml::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
        if scenario.name != name {
            return Err(format!(
                "{}: name {:?} does not match the file name",
                path.display(),
                scenario.name
            ));
        }
        scenario
            .validate()
            .map_err(|error| format!("{}: {error}", path.display()))?;
        Ok(scenario)
    }

    /// Every scenario under `scenarios/`, sorted by name.
    pub(crate) fn all() -> Result<Vec<Self>, String> {
        let dir = fixtures_dir().join("scenarios");
        let mut names: Vec<String> = std::fs::read_dir(&dir)
            .map_err(|error| format!("{}: {error}", dir.display()))?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().to_string();
                name.strip_suffix(".toml").map(str::to_string)
            })
            .collect();
        names.sort();
        names.iter().map(|name| Self::load(name)).collect()
    }

    /// Resolve a `<prefix>` (`e1`, `e1-sean-from-empty`), an id (`S1`) or
    /// `all`.
    pub(crate) fn select(which: &str) -> Result<Vec<Self>, String> {
        let all = Self::all()?;
        if which == "all" {
            return Ok(all);
        }
        let picked: Vec<Self> = all
            .into_iter()
            .filter(|scenario| scenario.answers_to(which))
            .collect();
        if picked.is_empty() {
            return Err(format!("no scenario named {which:?}"));
        }
        Ok(picked)
    }

    /// Whether `key` names this scenario: its id (any case), its name, or
    /// a name prefix up to a dash.
    pub(crate) fn answers_to(&self, key: &str) -> bool {
        self.id.eq_ignore_ascii_case(key)
            || self.name == key
            || self.name.starts_with(&format!("{key}-"))
    }

    /// Where the scenario runs: on the device bench when a board is
    /// plugged in, else in a headless Studio.
    pub(crate) fn seat(&self) -> SeatKind {
        match self.start.board.state {
            Some(_) => SeatKind::Device,
            None => SeatKind::Project,
        }
    }

    /// The board whose pin map outputs open against: the known board, else
    /// the actual one.
    pub(crate) fn board_id(&self) -> Option<&str> {
        self.start
            .board
            .known
            .as_deref()
            .or(self.start.board.actual.as_deref())
    }

    /// The chip family (`esp32c6`): the one named, else the board's.
    pub(crate) fn chip(&self) -> Option<String> {
        self.start.board.chip.clone().or_else(|| {
            self.board_id()
                .and_then(lpa_boards::board_by_id)
                .map(|board| board.family.clone())
        })
    }

    /// The facts the readout cannot show, as the agent reads them.
    pub(crate) fn context_line(&self) -> String {
        let start = &self.start;
        let mut line = if !start.context.is_empty() {
            start.context.clone()
        } else if self.seat() == SeatKind::Device {
            String::new()
        } else if let Some(known) = &start.board.known {
            let name = lpa_boards::board_by_id(known)
                .map(|board| format!("{} {}", board.manufacturer, board.display_name))
                .unwrap_or_else(|| known.clone());
            format!(
                "A {name} is connected over USB. Its board is known: {name} (board id {known})."
            )
        } else if let Some(chip) = &start.board.chip {
            format!(
                "An {} chip is connected over USB. Which board it is mounted on is NOT known.",
                chip_display(chip)
            )
        } else {
            "Nothing is known about the user's board yet: no board is connected.".to_string()
        };
        if start.context.is_empty()
            && matches!(start.project, StartProject::Golden(_))
            && self.seat() == SeatKind::Project
        {
            line.push_str(" The open project is the user's saved LED setup.");
        }
        if !start.note.is_empty() {
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(&start.note);
        }
        line
    }

    /// What stage B (the emulated C6) decodes: the pad and the LED count,
    /// when this is a C6 scenario whose checks name a pin and a strip.
    pub(crate) fn stage_b(&self) -> Option<StageB> {
        if self.chip().as_deref() != Some("esp32c6") {
            return None;
        }
        let pin = self.checks.iter().find_map(CheckSpec::pin)?;
        let leds = self.checks.iter().find_map(CheckSpec::leds)?;
        let board = lpa_boards::board_by_id(self.board_id()?)?;
        let (_, gpio) = board.output_wires().find(|(label, _)| *label == pin)?;
        Some(StageB { pad: gpio, leds })
    }

    /// The well-formedness rules serde cannot say.
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.tags.is_empty() {
            return Err("a scenario needs at least one tag".to_string());
        }
        if self.status == ScenarioStatus::Pending && self.waits_for.is_empty() {
            return Err("a pending scenario names what it waits for (`waits_for`)".to_string());
        }
        if self.checks.is_empty() {
            return Err("a scenario needs at least one check".to_string());
        }
        let board = &self.start.board;
        for id in [&board.known, &board.actual].into_iter().flatten() {
            if lpa_boards::runtime_manifest_json(id).is_none() {
                return Err(format!("board {id:?} has no runtime pin map"));
            }
        }
        if self.board_id().is_none() {
            return Err(
                "the start names no board to open outputs against (`known` or `actual`)"
                    .to_string(),
            );
        }
        match self.seat() {
            SeatKind::Project => {
                if self.start.project == StartProject::None {
                    return Err("the project seat starts with a project".to_string());
                }
                if let Some(check) = self.checks.iter().find(|check| check.needs_device()) {
                    return Err(format!(
                        "{} needs a board (the device seat: `start.board.state`)",
                        check.name()
                    ));
                }
            }
            SeatKind::Device => {
                let state = board.state.expect("the device seat has a state");
                let running = matches!(state, BoardState::Running | BoardState::Older);
                let golden = matches!(self.start.project, StartProject::Golden(_));
                if running != golden {
                    return Err(
                        "a running board starts with a golden project, and only a running one"
                            .to_string(),
                    );
                }
                if state != BoardState::Foreign && board.firmware.is_some() {
                    return Err("`firmware` names a foreign board's firmware".to_string());
                }
            }
        }
        for name in [self.golden.as_deref(), self.start_golden()]
            .into_iter()
            .flatten()
        {
            let dir = fixtures_dir().join("golden").join(name);
            if !dir.is_dir() {
                return Err(format!("golden {name:?} is not under golden/"));
            }
        }
        if !self.start.selection.is_empty() && self.status == ScenarioStatus::Active {
            return Err(
                "the harness cannot select a node yet (`start.selection` needs place, M7/M8): \
                 an active scenario cannot start with one"
                    .to_string(),
            );
        }
        for reply in &self.user.replies {
            if reply.when.is_empty() {
                return Err(format!("the reply {:?} has no `when` keyword", reply.text));
            }
        }
        for check in &self.checks {
            check.validate()?;
        }
        Ok(())
    }

    /// The golden the run starts from, if any.
    pub(crate) fn start_golden(&self) -> Option<&str> {
        match &self.start.project {
            StartProject::Golden(name) => Some(name),
            _ => None,
        }
    }
}

/// What stage B decodes for a scenario.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StageB {
    /// The GPIO the scenario's pin is on.
    pub(crate) pad: u8,
    pub(crate) leds: u32,
}

/// `esp32c6` → `ESP32-C6`.
fn chip_display(chip: &str) -> String {
    match chip.strip_prefix("esp32") {
        Some("") => "ESP32".to_string(),
        Some(rest) => format!("ESP32-{}", rest.to_uppercase()),
        None => chip.to_uppercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_committed_scenario_parses() {
        let all = Scenario::all().expect("scenarios load");
        let ids: Vec<&str> = all.iter().map(|s| s.id.as_str()).collect();
        let mut expected: Vec<String> = (1..=19).map(|n| format!("S{n}")).collect();
        expected.sort();
        let mut sorted: Vec<String> = ids.iter().map(|id| id.to_string()).collect();
        sorted.sort();
        assert_eq!(sorted, expected, "one file per scenario S1–S19");
        // E1–E3 kept their file names (the bake-off history names them).
        assert_eq!(Scenario::select("e1").expect("e1")[0].id, "S1");
        assert_eq!(
            Scenario::select("S3").expect("S3")[0].name,
            "e2-make-it-300"
        );
        assert!(Scenario::select("e9").is_err());
        let mut names: Vec<&str> = all.iter().map(|s| s.name.as_str()).collect();
        names.dedup();
        assert_eq!(names.len(), all.len(), "names are unique");
    }

    #[test]
    fn the_board_line_is_generated_from_the_start() {
        let s1 = Scenario::load("e1-sean-from-empty").expect("S1");
        // E1–E3 keep their exact context lines (no behaviour change).
        assert_eq!(
            s1.context_line(),
            "A Seeed XIAO ESP32-C6 is connected over USB. Its board is known: Seeed XIAO \
             ESP32-C6 (board id seeed/xiao-esp32-c6)."
        );
        let s7 = Scenario::all()
            .expect("all")
            .into_iter()
            .find(|s| s.id == "S7")
            .expect("S7");
        assert_eq!(
            s7.context_line(),
            "An ESP32-C6 chip is connected over USB. Which board it is mounted on is NOT known."
        );
        assert_eq!(chip_display("esp32"), "ESP32");
        assert_eq!(chip_display("esp32s3"), "ESP32-S3");
    }

    #[test]
    fn stage_b_reads_the_pad_off_the_boards_pin_map() {
        let s1 = Scenario::load("e1-sean-from-empty").expect("S1");
        assert_eq!(s1.stage_b(), Some(StageB { pad: 16, leds: 250 }));
        let all = Scenario::all().expect("all");
        let s18 = all.iter().find(|s| s.id == "S18").expect("S18");
        assert_eq!(s18.stage_b(), Some(StageB { pad: 18, leds: 60 }));
        // A classic ESP32 has no stage B (the emulator runs C6 images).
        let s19 = all.iter().find(|s| s.id == "S19").expect("S19");
        assert_eq!(s19.stage_b(), None);
    }

    #[test]
    fn the_selectors_pick_by_id_tag_persona_and_status() {
        let all = Scenario::all().expect("all");
        let picked = |selection: &Selection| -> Vec<String> {
            all.iter()
                .filter(|s| selection.picks(s).is_ok())
                .map(|s| s.id.clone())
                .collect()
        };
        let only = Selection {
            only: vec!["S7".into(), "s18".into()],
            ..Selection::default()
        };
        assert_eq!(picked(&only), ["S7", "S18"], "file order");
        let pending = Selection {
            only: vec!["S9".into()],
            ..Selection::default()
        };
        assert!(picked(&pending).is_empty(), "pending is skipped by default");
        let pending = Selection {
            include_pending: true,
            ..pending
        };
        assert_eq!(picked(&pending), ["S9"]);
        let jordan = Selection {
            personas: vec!["jordan".into()],
            ..Selection::default()
        };
        assert!(
            picked(&jordan)
                .iter()
                .all(|id| ["S13", "S15"].contains(&id.as_str())),
            "{:?}",
            picked(&jordan)
        );
        let honesty = Selection {
            tags: vec!["honesty".into()],
            ..Selection::default()
        };
        assert!(picked(&honesty).contains(&"S6".to_string()));
    }
}
