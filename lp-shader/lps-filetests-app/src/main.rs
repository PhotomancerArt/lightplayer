use clap::{Args, Parser, Subcommand};
use lps_filetests::output_mode::OutputMode;

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
    /// Like --mark-unimplemented, but only before `// run:` lines that already have `@unimplemented(<TARGET>)` for this baseline (e.g. rv32.q32 when duplicating onto rv32fa). Requires exactly one `--target`.
    #[arg(long, value_name = "TARGET")]
    mark_unimplemented_if_baseline: Option<String>,
    /// With --mark-unimplemented, skip typing `yes` (non-interactive)
    #[arg(long)]
    assume_yes: bool,
    /// Run only the specified target(s): comma-separated and/or backend shorthand (jit, wasm, rv32)
    /// or full names (jit.q32). Example: `-t wasm,jit` or `--target rv32`.
    #[arg(short, long)]
    target: Option<String>,
    #[command(flatten)]
    output_mode: OutputModeCli,
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
            lps_filetests::run(
                &files,
                t.fix,
                t.mark_unimplemented,
                t.assume_yes,
                t.mark_unimplemented_if_baseline,
                target_spec,
                output_override,
            )?;
        }
    }

    Ok(())
}
