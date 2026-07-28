//! `agent-run`: one headless shader-agent session against the real
//! in-process studio stack. See `lpa_agent_harness::runner` and
//! `--help` (which includes the spend policy).

use lpa_agent_harness::runner::{ModeDecision, decide_mode, help_text, parse_args, run};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse_args(&args) {
        Ok(Some(args)) => args,
        Ok(None) => {
            println!("{}", help_text());
            return;
        }
        Err(error) => {
            eprintln!("agent-run: {error}");
            eprintln!("agent-run: --help for usage");
            std::process::exit(2);
        }
    };

    // The spend gate: scripted mode passes untouched; real mode needs
    // provider env AND LPA_SPEND_OK=1, else a clean exit with NO network
    // construction (see runner::spend_gate).
    let mode = match decide_mode(args.scripted.clone(), &|name| std::env::var(name).ok()) {
        ModeDecision::Run(mode) => mode,
        ModeDecision::Refuse { message } => {
            println!("{message}");
            return;
        }
    };

    match run(&args, mode) {
        Ok(summary) => {
            if summary.status != "idle" {
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("agent-run: {error}");
            std::process::exit(1);
        }
    }
}
