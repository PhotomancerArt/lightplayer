use clap::{Parser, Subcommand};

/// lp-glsl filetest utility.
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
    /// With --mark-unimplemented, skip typing `yes` (non-interactive)
    #[arg(long)]
    assume_yes: bool,
    /// Run only the specified target(s): comma-separated and/or backend shorthand (jit, wasm, rv32)
    /// or full names (jit.q32). Example: `--target wasm,jit` or `--target rv32`.
    #[arg(long)]
    target: Option<String>,
    /// Force summary mode even for a single test file
    #[arg(long)]
    summary: bool,
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
            lp_glsl_filetests::run(
                &files,
                t.fix,
                t.mark_unimplemented,
                t.assume_yes,
                target_spec,
                t.summary,
            )?;
        }
    }

    Ok(())
}
