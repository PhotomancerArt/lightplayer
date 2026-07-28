//! `agent-run` argument parsing (std-only, no clap).

use std::path::PathBuf;

use lpa_studio_core::harness::ShaderFrontend;

use crate::runner::spend_gate::SPEND_POLICY_LINE;

/// Parsed `agent-run` arguments.
pub struct RunnerArgs {
    /// The user message the session starts with.
    pub prompt: String,
    /// GLSL frontend the in-process server compiles with. DEFAULT Naga —
    /// browser parity: the naga + lpvm-wasm emission class is where the
    /// expensive live bugs were.
    pub frontend: ShaderFrontend,
    /// Artifact directory (default `target/agent-runs/<timestamp>`).
    pub out: Option<PathBuf>,
    /// Model id override (settings-level; provider default otherwise).
    pub model: Option<String>,
    /// Scripted mode: play back this script file (token-free, no gate).
    pub scripted: Option<PathBuf>,
    /// Per-run turn cap override (default: lpa-agent's MAX_TURNS_PER_RUN).
    pub max_turns: Option<u32>,
}

/// `--help` text (includes the spend policy, per the gate spec).
pub fn help_text() -> String {
    format!(
        "agent-run — one headless shader-agent session against the real in-process studio stack

USAGE:
  just agent-run prompt=\"<message>\" [flags=\"...\"]
  cargo run -p lpa-agent-harness --features runner --bin agent-run -- \"<message>\" [FLAGS]

FLAGS:
  --frontend naga|lps-glsl   GLSL frontend for the in-process server
                             (default: naga — browser-parity emission path)
  --out <dir>                artifact directory
                             (default: target/agent-runs/<timestamp>)
  --model <id>               model id override
  --scripted <script.json>   scripted provider (token-free; no key, no gate)
  --max-turns <n>            per-run turn cap override
  --help                     this text

ARTIFACTS (per run, in the out dir):
  debug.json   the raw model-facing transcript dump (format 1)
  final.glsl   the staged shader source at run end
  run.md       the chat log as markdown
  triage.txt   known bug-class signature matches

SPEND GATE:
  Real-provider mode needs BOTH provider env (ANTHROPIC_API_KEY, or
  LPA_EVAL_BASE_URL + LPA_EVAL_MODEL) AND LPA_SPEND_OK=1; missing either,
  the runner prints instructions and exits without any network call.
  {SPEND_POLICY_LINE}"
    )
}

/// Parse the args (everything after the binary name). `Ok(None)` means
/// `--help` was requested (the caller prints [`help_text`] and exits 0).
pub fn parse_args(args: &[String]) -> Result<Option<RunnerArgs>, String> {
    let mut prompt: Option<String> = None;
    let mut frontend = ShaderFrontend::Naga;
    let mut out = None;
    let mut model = None;
    let mut scripted = None;
    let mut max_turns = None;

    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value_for = |flag: &str| {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        match arg.as_str() {
            "--help" | "-h" => return Ok(None),
            "--frontend" => {
                frontend = match value_for("--frontend")?.as_str() {
                    "naga" => ShaderFrontend::Naga,
                    "lps-glsl" => ShaderFrontend::LpsGlsl,
                    other => return Err(format!("unknown --frontend {other} (naga|lps-glsl)")),
                };
            }
            "--out" => out = Some(PathBuf::from(value_for("--out")?)),
            "--model" => model = Some(value_for("--model")?),
            "--scripted" => scripted = Some(PathBuf::from(value_for("--scripted")?)),
            "--max-turns" => {
                let value = value_for("--max-turns")?;
                max_turns = Some(
                    value
                        .parse::<u32>()
                        .map_err(|_| format!("--max-turns needs a number, got {value}"))?,
                );
            }
            other if other.starts_with("--") => return Err(format!("unknown flag {other}")),
            other => {
                if prompt.is_some() {
                    return Err(format!("unexpected extra argument {other:?}"));
                }
                prompt = Some(other.to_string());
            }
        }
    }

    let prompt = prompt.ok_or_else(|| "missing the prompt argument (see --help)".to_string())?;
    Ok(Some(RunnerArgs {
        prompt,
        frontend,
        out,
        model,
        scripted,
        max_turns,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn defaults_are_naga_and_unset_flags() {
        let args = parse_args(&strings(&["make it pulse"]))
            .expect("parses")
            .expect("not help");
        assert_eq!(args.prompt, "make it pulse");
        assert!(matches!(args.frontend, ShaderFrontend::Naga));
        assert!(args.out.is_none());
        assert!(args.scripted.is_none());
        assert!(args.max_turns.is_none());
    }

    #[test]
    fn all_flags_parse() {
        let args = parse_args(&strings(&[
            "--frontend",
            "lps-glsl",
            "--out",
            "/tmp/x",
            "--model",
            "claude-haiku-4-5",
            "--scripted",
            "s.json",
            "--max-turns",
            "3",
            "go",
        ]))
        .expect("parses")
        .expect("not help");
        assert!(matches!(args.frontend, ShaderFrontend::LpsGlsl));
        assert_eq!(args.out.as_deref(), Some(std::path::Path::new("/tmp/x")));
        assert_eq!(args.model.as_deref(), Some("claude-haiku-4-5"));
        assert_eq!(args.max_turns, Some(3));
        assert_eq!(args.prompt, "go");
    }

    #[test]
    fn help_short_circuits_and_carries_the_policy() {
        assert!(parse_args(&strings(&["--help"])).expect("parses").is_none());
        assert!(help_text().contains("Yona's explicit permission"));
        assert!(help_text().contains("LPA_SPEND_OK=1"));
    }

    #[test]
    fn errors_are_actionable() {
        assert!(parse_args(&strings(&[])).is_err());
        assert!(parse_args(&strings(&["--frontend", "wgsl", "x"])).is_err());
        assert!(parse_args(&strings(&["--max-turns", "many", "x"])).is_err());
        assert!(parse_args(&strings(&["--bogus", "x"])).is_err());
    }
}
