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
    /// Run only the specified target (e.g. cranelift.q32, wasm.q32)
    #[arg(long)]
    target: Option<String>,
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
            let target_filter = if let Some(ref name) = t.target {
                match lp_glsl_filetests::target::Target::from_name(name) {
                    Ok(t) => Some(t),
                    Err(e) => {
                        eprintln!("{e}");
                        std::process::exit(1);
                    }
                }
            } else {
                None
            };
            lp_glsl_filetests::run(&files, t.fix, target_filter)?;
        }
    }

    Ok(())
}
