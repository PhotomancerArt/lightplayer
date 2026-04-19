use anyhow::Result;
use clap::Parser;

mod client;
mod commands;
mod config;
mod debug_ui;
mod error;
mod messages;
mod server;

use commands::{create, dev, profile, serve, shader_debug, shader_lpir, upload};

#[derive(Parser)]
#[command(name = "lp-cli")]
#[command(about = "LightPlayer CLI - Server and client modes")]
enum Cli {
    /// Run server from a directory
    Serve {
        /// Server directory (defaults to current directory)
        dir: Option<std::path::PathBuf>,
        /// Initialize server directory (create server.json if missing)
        #[arg(long)]
        init: bool,
        /// Use in-memory filesystem instead of disk
        #[arg(long)]
        memory: bool,
    },
    /// Connect to server and sync local project
    Dev {
        /// Project directory
        dir: std::path::PathBuf,
        /// Push local project to server. Optionally specify remote host (e.g., ws://localhost:2812/, serial:auto, or emu).
        /// If --push is specified without a host, uses in-memory server.
        #[arg(long, value_name = "HOST")]
        push: Option<Option<String>>,
        /// Run without UI (headless mode)
        #[arg(long)]
        headless: bool,
    },
    /// Upload project to host and exit (non-interactive)
    Upload {
        /// Project directory
        dir: std::path::PathBuf,
        /// Host to upload to (e.g. serial:auto, ws://localhost:2812/)
        host: String,
    },
    /// Create a new project
    Create {
        /// Project directory
        dir: std::path::PathBuf,
        /// Project name (defaults to directory name)
        #[arg(long)]
        name: Option<String>,
        /// Project UID (auto-generated if not provided)
        #[arg(long)]
        uid: Option<String>,
    },
    /// Run a profiling session or compare profiles (`profile diff` is a stub in m0).
    Profile(profile::ProfileCli),
    /// Compile a GLSL file to LPIR text (stdout). Uses the same Naga → LPIR path as the JIT.
    ShaderLpir {
        /// Path to a `.glsl` file (filetest-style snippet; LPFX preamble is applied like `lps-frontend::compile`)
        path: std::path::PathBuf,
        /// Print per-function op/vreg counts to stderr (stdout stays pure LPIR for piping)
        #[arg(long)]
        stats: bool,
        /// Print LPIR even if validation fails (warnings to log); use for debugging
        #[arg(long)]
        skip_validate: bool,
    },
    /// Unified debug output for shader compilation (replaces shader-rv32c, shader-rv32n).
    ShaderDebug {
        #[command(flatten)]
        args: shader_debug::Args,
    },
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let cli = Cli::parse();

    match cli {
        Cli::Serve { dir, init, memory } => {
            serve::handle_serve(serve::ServeArgs { dir, init, memory })
        }
        Cli::Dev {
            dir,
            push,
            headless,
        } => dev::handle_dev(dev::DevArgs {
            dir,
            push_host: push,
            headless,
        }),
        Cli::Upload { dir, host } => upload::handle_upload(upload::UploadArgs { dir, host }),
        Cli::Create { dir, name, uid } => {
            create::handle_create(create::CreateArgs { dir, name, uid })
        }
        Cli::Profile(cli) => match cli.subcommand {
            Some(profile::ProfileSubcommand::Diff(args)) => profile::handle_profile_diff(args),
            None => profile::handle_profile(cli.run),
        },
        Cli::ShaderLpir {
            path,
            stats,
            skip_validate,
        } => shader_lpir::handle_shader_lpir(shader_lpir::ShaderLpirArgs {
            path,
            stats,
            skip_validate,
        }),
        Cli::ShaderDebug { args } => shader_debug::handle_shader_debug(args),
    }
}
