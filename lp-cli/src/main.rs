use anyhow::Result;
use clap::Parser;

mod client;
mod commands;
mod config;
mod debug_ui;
mod error;
mod messages;
mod server;

use commands::{
    create, dev, firmware, fwcheck, hardware, profile, schema, serve, shader_debug, shader_lpir,
    upload,
};

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
    ///
    /// Waits, after the deploy is acked, for evidence the newly deployed
    /// project is running before exiting — otherwise the compile line an
    /// operator observes describes the *previous* upload (see
    /// docs/defects/2026-07-30-deploy-compiles-previous-upload.md).
    Upload {
        /// Project directory
        dir: std::path::PathBuf,
        /// Host to upload to (e.g. serial:auto, ws://localhost:2812/)
        host: String,
        /// Skip waiting for evidence the deployed project is running;
        /// disconnect the instant the deploy is acked (pre-P5 behaviour).
        #[arg(long)]
        no_wait: bool,
        /// Seconds to wait for evidence the deployed project is running
        /// before exiting nonzero. The deploy itself may already have
        /// succeeded even if this times out. Ignored with `--no-wait`.
        #[arg(
            long = "wait-timeout",
            value_name = "SECS",
            default_value_t = upload::DEFAULT_WAIT_TIMEOUT_SECS
        )]
        wait_timeout: u64,
    },
    /// Create a new project
    Create {
        /// Project directory
        dir: std::path::PathBuf,
        /// Project name (defaults to directory name)
        #[arg(long)]
        name: Option<String>,
    },
    /// Run a profiling session or compare profiles (`profile diff` is a stub in m0).
    Profile(profile::ProfileCli),
    /// Inspect firmware build artifacts via their embedded manifest core.
    Firmware(firmware::FirmwareCli),
    /// Run firmware checks on hardware or firmware targets.
    Fwcheck(fwcheck::FwcheckCli),
    /// Developer hardware manifest and calibration tools.
    Hardware(hardware::HardwareCli),
    /// Generate or verify the checked-in schemas/ tree (JSON Schemas + slot shape dumps).
    Schema(schema::SchemaCli),
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
        Cli::Upload {
            dir,
            host,
            no_wait,
            wait_timeout,
        } => upload::handle_upload(upload::UploadArgs {
            dir,
            host,
            no_wait,
            wait_timeout_secs: wait_timeout,
        }),
        Cli::Create { dir, name } => create::handle_create(create::CreateArgs { dir, name }),
        Cli::Hardware(cli) => hardware::handle_hardware(cli),
        Cli::Schema(cli) => schema::handle_schema(cli),
        Cli::Profile(cli) => match cli.subcommand {
            Some(profile::ProfileSubcommand::Diff(args)) => profile::handle_profile_diff(args),
            Some(profile::ProfileSubcommand::Function(args)) => {
                profile::handle_profile_function(args)
            }
            None => profile::handle_profile(cli.run),
        },
        Cli::Firmware(cli) => firmware::handle_firmware(cli),
        Cli::Fwcheck(cli) => fwcheck::handle_fwcheck(cli),
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
