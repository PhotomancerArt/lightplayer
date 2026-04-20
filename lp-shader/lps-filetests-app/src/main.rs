use clap::{Args, Parser, Subcommand};
use lps_filetests::output_mode::OutputMode;
use lps_filetests::perf_model::PerfModel;

/// lps filetest utility.
#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run GLSL filetests
    Test(TestOptions),
}

#[derive(Args)]
#[group(id = "output_mode", multiple = false)]
struct OutputModeCli {
    /// Full output plus CLIF/disassembly/emulator sections on failure (same as DEBUG=1)
    #[arg(long, group = "output_mode")]
    debug: bool,
    /// Minimal output even for a single file
    #[arg(long, group = "output_mode")]
    concise: bool,
    /// Verbose per-`// run:` output even when running many files
    #[arg(long, group = "output_mode")]
    detail: bool,
}

fn parse_perf_flag(s: &str) -> Result<PerfModel, String> {
    PerfModel::parse(s)
}

impl OutputModeCli {
    fn as_override(&self) -> Option<OutputMode> {
        if self.debug {
            Some(OutputMode::Debug)
        } else if self.concise {
            Some(OutputMode::Concise)
        } else if self.detail {
            Some(OutputMode::Detail)
        } else {
            None
        }
    }
}

/// Run GLSL filetests
#[derive(Parser)]
struct TestOptions {
    /// Specify input files or directories to test (default: all tests)
    files: Vec<String>,
    /// Automatically remove annotations from tests that now pass
    #[arg(long)]
    fix: bool,
    /// Add @unimplemented(backend=…) to failing tests (baseline for milestone work); requires a single --target
    #[arg(long)]
    mark_unimplemented: bool,
    /// Add @unsupported(backend=...) to failing tests; requires a single --target
    #[arg(long)]
    mark_unsupported: bool,
    /// Add @broken(backend=...) to failing tests; requires a single --target
    #[arg(long)]
    mark_broken: bool,
    /// Like --mark-unimplemented, but only before `// run:` lines that already have `@unimplemented(<TARGET>)` for this baseline (e.g. rv32c.q32 when duplicating onto rv32n). Requires exactly one `--target`.
    #[arg(long, value_name = "TARGET")]
    mark_unimplemented_if_baseline: Option<String>,
    /// Skip the 'yes' confirmation prompt for mutation flags (--fix, --mark-*)
    #[arg(long)]
    assume_yes: bool,
    /// Run only the specified target(s): comma-separated and/or backend shorthand (jit, wasm, rv32c)
    /// or full names (jit.q32). Example: `-t wasm,jit` or `--target rv32c`.
    #[arg(short, long)]
    target: Option<String>,
    #[command(flatten)]
    output_mode: OutputModeCli,
    /// Guest RV32 cost column in the summary table (`vs fastest` uses the same metric).
    #[arg(long, value_name = "MODEL", default_value = "esp32c6", value_parser = parse_perf_flag)]
    perf: PerfModel,
    /// Force-override compiler options across the suite. Format: `key=value`.
    /// Repeatable. Wins over per-file `compile-opt(...)` directives.
    /// Example: `--force-opt inline.mode=never` or `--force-opt q32.mul=wrapping`.
    #[arg(long = "force-opt", value_name = "KEY=VALUE")]
    force_opt: Vec<String>,
}

/// Parse `key=value` for `--force-opt` / `LPS_FILETEST_FORCE_OPT` entries.
fn parse_force_opt_entry(s: &str) -> anyhow::Result<(String, String)> {
    let s = s.trim();
    let eq = s.find('=').ok_or_else(|| {
        anyhow::anyhow!(
            "invalid force-opt {s:?}: expected key=value (from --force-opt or LPS_FILETEST_FORCE_OPT)"
        )
    })?;
    let (k, rest) = s.split_at(eq);
    let v = &rest[1..];
    let k = k.trim();
    if k.is_empty() {
        anyhow::bail!("invalid force-opt {s:?}: empty key");
    }
    Ok((k.to_string(), v.trim().to_string()))
}

/// Merge env (`LPS_FILETEST_FORCE_OPT`, comma-separated) with CLI `--force-opt`.
/// On duplicate keys, CLI wins.
fn resolve_force_opts(cli: &[String], env: Option<&str>) -> anyhow::Result<Vec<(String, String)>> {
    use std::collections::BTreeMap;
    let mut map: BTreeMap<String, String> = BTreeMap::new();
    if let Some(s) = env {
        for part in s.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let (k, v) = parse_force_opt_entry(part)?;
            map.insert(k, v);
        }
    }
    for entry in cli {
        let (k, v) = parse_force_opt_entry(entry)?;
        map.insert(k, v);
    }
    Ok(map.into_iter().collect())
}

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Test(t) => {
            let files = if t.files.is_empty() {
                vec!["**/*.glsl".to_string()]
            } else {
                t.files
            };
            let target_spec = t.target.as_deref();
            let output_override = t.output_mode.as_override();
            // Merge `LPS_FILETEST_FORCE_OPT` (comma-separated) with `--force-opt`; duplicate keys use CLI.
            let env_force = std::env::var("LPS_FILETEST_FORCE_OPT").ok();
            let force_opts = resolve_force_opts(&t.force_opt, env_force.as_deref())?;
            lps_filetests::run(
                &files,
                t.fix,
                t.mark_unimplemented,
                t.mark_unsupported,
                t.mark_broken,
                t.assume_yes,
                t.mark_unimplemented_if_baseline,
                target_spec,
                output_override,
                &force_opts,
                t.perf,
            )?;
        }
    }

    Ok(())
}
