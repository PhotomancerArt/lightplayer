//! `agent-battery`: loop the supervised prompt battery through the
//! headless session runner. Spend-gated exactly like `agent-run` — BOTH
//! provider env AND `LPA_SPEND_OK=1`, or a clean refusal with instructions
//! and no network construction. See `lpa_agent_harness::runner::battery`.

use std::path::PathBuf;

use lpa_agent_harness::parse_dump;
use lpa_agent_harness::runner::battery::{BatteryFile, BatteryRow, evaluate_run, render_summary};
use lpa_agent_harness::runner::run_artifacts::{resolve_out_dir, write_artifact};
use lpa_agent_harness::runner::spend_gate::SPEND_POLICY_LINE;
use lpa_agent_harness::runner::{ModeDecision, RunMode, RunnerArgs, decide_mode, run};
use lpa_studio_core::harness::ShaderFrontend;

fn help_text() -> String {
    format!(
        "agent-battery — loop the supervised prompt battery through the headless runner

USAGE:
  just agent-battery [flags=\"...\"]
  cargo run -p lpa-agent-harness --features runner --bin agent-battery -- [FLAGS]

FLAGS:
  --battery <file.json>      prompt battery (default: the crate's battery.json)
  --frontend naga|lps-glsl   GLSL frontend (default: naga — browser parity)
  --out <dir>                run-dir root (default: target/agent-runs/battery-<timestamp>)
  --model <id>               model id override
  --max-turns <n>            per-run turn cap override
  --help                     this text

Each prompt gets its own run dir (<out>/<prompt-id>/ with the usual four
artifacts); a summary table lands in <out>/summary.md and on stdout.

SPEND GATE (identical to agent-run):
  Real-provider runs need BOTH provider env (ANTHROPIC_API_KEY, or
  LPA_EVAL_BASE_URL + LPA_EVAL_MODEL) AND LPA_SPEND_OK=1; missing either,
  this prints instructions and exits without any network call. There is no
  scripted mode here — the token-free battery lives in `just test`.
  {SPEND_POLICY_LINE}"
    )
}

struct BatteryArgs {
    battery: PathBuf,
    frontend: ShaderFrontend,
    out: Option<PathBuf>,
    model: Option<String>,
    max_turns: Option<u32>,
}

/// Parse the flags; `Ok(None)` means `--help`.
fn parse_args(args: &[String]) -> Result<Option<BatteryArgs>, String> {
    let mut parsed = BatteryArgs {
        battery: BatteryFile::default_path(),
        frontend: ShaderFrontend::Naga,
        out: None,
        model: None,
        max_turns: None,
    };
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut value_for = |flag: &str| {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        match arg.as_str() {
            "--help" | "-h" => return Ok(None),
            "--battery" => parsed.battery = PathBuf::from(value_for("--battery")?),
            "--frontend" => {
                parsed.frontend = match value_for("--frontend")?.as_str() {
                    "naga" => ShaderFrontend::Naga,
                    "lps-glsl" => ShaderFrontend::LpsGlsl,
                    other => return Err(format!("unknown --frontend {other} (naga|lps-glsl)")),
                };
            }
            "--out" => parsed.out = Some(PathBuf::from(value_for("--out")?)),
            "--model" => parsed.model = Some(value_for("--model")?),
            "--max-turns" => {
                let value = value_for("--max-turns")?;
                parsed.max_turns = Some(
                    value
                        .parse::<u32>()
                        .map_err(|_| format!("--max-turns needs a number, got {value}"))?,
                );
            }
            other => return Err(format!("unexpected argument {other:?}")),
        }
    }
    Ok(Some(parsed))
}

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse_args(&raw) {
        Ok(Some(args)) => args,
        Ok(None) => {
            println!("{}", help_text());
            return;
        }
        Err(error) => {
            eprintln!("agent-battery: {error}");
            eprintln!("agent-battery: --help for usage");
            std::process::exit(2);
        }
    };

    let battery = match BatteryFile::load(&args.battery) {
        Ok(battery) => battery,
        Err(error) => {
            eprintln!("agent-battery: {error}");
            std::process::exit(2);
        }
    };

    // The spend gate, once for the whole battery: this bin has no scripted
    // mode, so `None` forces the real-provider decision — provider env AND
    // LPA_SPEND_OK=1, or a clean refusal (no network, exit 0).
    let cfg = match decide_mode(None, &|name| std::env::var(name).ok()) {
        ModeDecision::Run(RunMode::Real(cfg)) => cfg,
        ModeDecision::Run(RunMode::Scripted(_)) => unreachable!("no scripted mode requested"),
        ModeDecision::Refuse { message } => {
            // Shared gate text, retargeted: the battery has no scripted
            // mode — its token-free counterpart is the `just test` battery.
            println!(
                "{}",
                message
                    .replace("agent-run prompt=\"...\"", "agent-battery")
                    .replace("agent-run", "agent-battery")
                    .replace(
                        "Token-free alternative: --scripted <script.json> needs no key and no \
                         gate.",
                        "Token-free alternative: the scripted battery already rides `just test`."
                    )
            );
            return;
        }
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs();
    let out_root = args.out.clone().unwrap_or_else(|| {
        // Sibling of the single-run dirs: target/agent-runs/battery-<ts>.
        let runs = resolve_out_dir(None, now);
        let stamp = runs.file_name().expect("timestamp dir").to_owned();
        let mut name = std::ffi::OsString::from("battery-");
        name.push(stamp);
        runs.with_file_name(name)
    });

    println!(
        "agent-battery: {} prompts → {}",
        battery.prompts.len(),
        out_root.display()
    );
    let mut rows: Vec<BatteryRow> = Vec::new();
    for prompt in &battery.prompts {
        println!("\n=== [{}] {} ===", prompt.class, prompt.id);
        let out_dir = out_root.join(&prompt.id);
        let runner_args = RunnerArgs {
            prompt: prompt.prompt.clone(),
            frontend: args.frontend,
            out: Some(out_dir.clone()),
            model: args.model.clone(),
            scripted: None,
            max_turns: args.max_turns,
        };
        let row = match run(&runner_args, RunMode::Real(cfg.clone())) {
            Ok(summary) => {
                let report = std::fs::read_to_string(out_dir.join("debug.json"))
                    .ok()
                    .and_then(|json| parse_dump(&json).ok())
                    .map(|dump| evaluate_run(&dump))
                    .unwrap_or_default();
                let triage = summary
                    .triage
                    .iter()
                    .filter(|line| line.starts_with('['))
                    .cloned()
                    .collect();
                BatteryRow {
                    id: prompt.id.clone(),
                    class: prompt.class.clone(),
                    status: summary.status,
                    report,
                    triage,
                    cost: summary.estimated_cost,
                }
            }
            Err(error) => {
                eprintln!("agent-battery: run failed: {error}");
                BatteryRow {
                    id: prompt.id.clone(),
                    class: prompt.class.clone(),
                    status: format!("error: {error}"),
                    report: Default::default(),
                    triage: Vec::new(),
                    cost: None,
                }
            }
        };
        rows.push(row);
    }

    let summary = render_summary(&rows);
    println!("\n{summary}");
    match write_artifact(&out_root, "summary.md", &summary) {
        Ok(path) => println!("agent-battery: summary written to {}", path.display()),
        Err(error) => eprintln!("agent-battery: {error}"),
    }
    let failed = rows.iter().any(|row| row.status != "idle");
    if failed {
        std::process::exit(1);
    }
}
