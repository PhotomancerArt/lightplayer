//! [`Scenario`]: one headline eval, read from
//! `tests/fixtures/app_agent/scenarios/<name>.toml`.
//!
//! Scenarios are data so Yona can edit a prompt, a budget or the colourful
//! allowlist without touching code. The fields are documented in the
//! fixtures' `README.md`.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The scenario fixtures directory.
pub(crate) fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/app_agent")
}

/// One eval scenario.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Scenario {
    /// File stem (`e1-sean-from-empty`).
    pub(crate) name: String,
    /// One line for the report.
    pub(crate) summary: String,
    /// Where the run starts.
    pub(crate) start: ScenarioStart,
    /// What the app knows before the user speaks (rides the agent's state
    /// block — the board, the connected device).
    pub(crate) context: String,
    /// The user's message.
    pub(crate) user: String,
    /// The LED count the project should end with.
    pub(crate) leds: u32,
    /// The committed golden that proves this scenario's checks pass
    /// (`golden/<name>/`), when one exists.
    #[serde(default)]
    pub(crate) golden: Option<String>,
    /// Canned answers to the agent's questions.
    #[serde(default)]
    pub(crate) replies: Vec<ScriptedReply>,
    pub(crate) budget: Budget,
    #[serde(default)]
    pub(crate) playlist: PlaylistExpectation,
    /// The checks that decide pass/fail, in report order.
    pub(crate) checks: Vec<CheckId>,
}

/// Where a scenario's run starts.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ScenarioStart {
    /// `HomeOp::CreateProject { template: Blank }` — the root module only.
    Blank,
    /// A committed golden project, opened and saved.
    Golden(String),
}

/// One canned user answer. The agent ending its turn with a question about
/// `when_asked_about` consumes the next unused reply; a second question
/// with no reply left ends the run.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ScriptedReply {
    pub(crate) when_asked_about: String,
    pub(crate) text: String,
}

/// Per-run limits; the run stops (and fails `budget`) past either.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Budget {
    /// Model turns across the whole scenario (replies included).
    pub(crate) turns: u32,
    pub(crate) usd: f64,
}

/// What `playlist_cycles` wants.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlaylistExpectation {
    pub(crate) min_entries: usize,
    /// Inclusive cycle step range, seconds.
    pub(crate) step_seconds: [f64; 2],
    /// Catalog pattern slugs that count as colourful.
    pub(crate) colourful: Vec<String>,
}

impl Default for PlaylistExpectation {
    fn default() -> Self {
        Self {
            min_entries: 3,
            step_seconds: [10.0, 60.0],
            colourful: Vec::new(),
        }
    }
}

/// The stage-A checks a scenario can name.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CheckId {
    OutputOnD6,
    TargetIsXiaoC6,
    StripOf,
    PlaylistCycles,
    GraphWired,
    AllNodesOk,
    Saved,
    AskedAboutBoard,
    NoDLabelBeforeBoard,
    MinimalDiff,
}

impl CheckId {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::OutputOnD6 => "output_on_d6",
            Self::TargetIsXiaoC6 => "target_is_xiao_c6",
            Self::StripOf => "strip_of",
            Self::PlaylistCycles => "playlist_cycles",
            Self::GraphWired => "graph_wired",
            Self::AllNodesOk => "all_nodes_ok",
            Self::Saved => "saved",
            Self::AskedAboutBoard => "asked_about_board",
            Self::NoDLabelBeforeBoard => "no_d_label_before_board",
            Self::MinimalDiff => "minimal_diff",
        }
    }

    /// Whether the check judges the agent's conversation rather than the
    /// project (a golden has no conversation to judge).
    pub(crate) fn needs_transcript(self) -> bool {
        matches!(self, Self::AskedAboutBoard | Self::NoDLabelBeforeBoard)
    }
}

impl Scenario {
    /// Load `scenarios/<name>.toml`.
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

    /// Resolve a `<prefix>` (`e1`, `e1-sean-from-empty`) or `all`.
    pub(crate) fn select(which: &str) -> Result<Vec<Self>, String> {
        let all = Self::all()?;
        if which == "all" {
            return Ok(all);
        }
        let picked: Vec<Self> = all
            .into_iter()
            .filter(|s| s.name == which || s.name.starts_with(&format!("{which}-")))
            .collect();
        if picked.is_empty() {
            return Err(format!("no scenario named {which:?}"));
        }
        Ok(picked)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_committed_scenario_parses() {
        let all = Scenario::all().expect("scenarios load");
        let names: Vec<&str> = all.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "e1-sean-from-empty",
                "e2-make-it-300",
                "e3-never-guess-the-board"
            ]
        );
        assert_eq!(Scenario::select("e2").expect("e2")[0].leds, 300);
        assert!(Scenario::select("e9").is_err());
    }
}
