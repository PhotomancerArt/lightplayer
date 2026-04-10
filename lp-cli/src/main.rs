use anyhow::Result;
use clap::Parser;

mod client;
mod commands;
mod config;
mod debug_ui;
mod error;
mod messages;
mod server;

use commands::{create, dev, heap_summary, mem_profile, serve, shader_lpir, shader_rv32, upload};

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
    /// Run a project in the emulator with allocation tracing
    MemProfile {
        /// Project directory (default: examples/mem-profile)
        #[arg(default_value = "examples/mem-profile")]
        dir: std::path::PathBuf,
        /// Number of frames to run
        #[arg(long, default_value = "10")]
        frames: u32,
        /// Short note appended to trace directory name (kebab-cased)
        #[arg(long)]
        note: Option<String>,
    },
    /// Summarize heap allocations from a mem-profile output directory
    HeapSummary {
        /// Trace directory (e.g. traces/2026-03-08-185520-simple-test)
        trace_dir: std::path::PathBuf,
        /// Number of top entries to show in live/hotspot sections (default: 20)
        #[arg(long, default_value = "20")]
        top: usize,
    },
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
    /// Compile a GLSL file to annotated RV32 assembly (`lpvm-native`, stdout).
    ShaderRv32 {
        path: std::path::PathBuf,
        #[arg(short, long)]
        output: Option<std::path::PathBuf>,
        #[arg(long, default_value = "q32")]
        float_mode: String,
        #[arg(long)]
        hex: bool,
        /// Print register allocation trace to stderr (off by default)
        #[arg(long)]
        alloc_trace: bool,
        /// Codegen pipeline: `linear` (default) or `fast` (straight-line PInst fastalloc).
        #[arg(long, default_value = "linear")]
        pipeline: String,
        /// With `--pipeline fast`, print VInst listing to stderr.
        #[arg(long)]
        show_vinst: bool,
        /// With `--pipeline fast`, print PInst listing to stderr.
        #[arg(long)]
        show_PInst: bool,
        /// With `--pipeline fast`, print raw instruction disassembly to stderr.
        #[arg(long)]
        disassemble: bool,
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
        Cli::MemProfile { dir, frames, note } => {
            mem_profile::handle_mem_profile(mem_profile::MemProfileArgs { dir, frames, note })
        }
        Cli::HeapSummary { trace_dir, top } => {
            heap_summary::handle_heap_summary(&heap_summary::HeapSummaryArgs { trace_dir, top })
        }
        Cli::ShaderLpir {
            path,
            stats,
            skip_validate,
        } => shader_lpir::handle_shader_lpir(shader_lpir::ShaderLpirArgs {
            path,
            stats,
            skip_validate,
        }),
        Cli::ShaderRv32 {
            path,
            output,
            float_mode,
            hex,
            alloc_trace,
            pipeline,
            show_vinst,
            show_PInst,
            disassemble,
        } => shader_rv32::handle_shader_rv32(shader_rv32::ShaderRv32Args {
            path,
            output,
            float_mode,
            hex,
            alloc_trace,
            pipeline,
            show_vinst,
            show_PInst,
            disassemble,
        }),
    }
}
