//! `lp-fw-split`: build the ESP32-C6 split image, or run one of its steps.
//!
//!   lp-fw-split build --out target/fw-split/<slug> [--features esp32c6,server]
//!   lp-fw-split reach <elf> <map> --emit-ld <engine.x>
//!   lp-fw-split verify <elf> <map>

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};
use lp_fw_split::reachability::Split;
use lp_fw_split::section_graph::SectionGraph;
use lp_fw_split::{BuildOptions, engine_script::EngineRules};

#[derive(Parser)]
#[command(about = "Build the ESP32-C6 split image: loader, boot records, core and engine")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// The whole pipeline: two passes, the verifier, the split, the loader,
    /// app.bin and merged.bin.
    Build {
        /// Where the outputs go.
        #[arg(long)]
        out: PathBuf,
        /// fw-esp32c6 features (its defaults are always on).
        #[arg(long, default_value = "esp32c6,server")]
        features: String,
        /// A bootloader to use instead of espflash 3.3.0's bundled one.
        #[arg(long)]
        bootloader: Option<PathBuf>,
        /// The partition table (default: lp-fw/fw-esp32c6/partitions.csv).
        #[arg(long)]
        partitions: Option<PathBuf>,
        /// The repository root (default: the current directory).
        #[arg(long, default_value = ".")]
        repo: PathBuf,
    },
    /// Split one link and write the engine's placement script.
    Reach {
        elf: PathBuf,
        map: PathBuf,
        #[arg(long)]
        emit_ld: PathBuf,
    },
    /// Check one (pass-2) link: no core section in the engine region.
    Verify { elf: PathBuf, map: PathBuf },
}

fn main() -> ExitCode {
    match run(Cli::parse().command) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("lp-fw-split: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(command: Command) -> Result<bool> {
    match command {
        Command::Build {
            out,
            features,
            bootloader,
            partitions,
            repo,
        } => {
            let mut opts = BuildOptions::new(repo, out);
            opts.features = features;
            opts.bootloader = bootloader;
            if let Some(p) = partitions {
                opts.partitions = p;
            }
            lp_fw_split::build(&opts)?;
            Ok(true)
        }
        Command::Reach { elf, map, emit_ld } => {
            let graph = SectionGraph::load(&elf, &map)?;
            let roots = lp_fw_split::split_build::core_roots(&graph, &std::fs::read(&elf)?)?;
            let split = Split::compute(&graph, &roots);
            let rules = EngineRules::from_split(&graph, &split);
            std::fs::write(&emit_ld, rules.script())?;
            eprintln!(
                "emitted {}: {} rodata + {} text input sections",
                emit_ld.display(),
                rules.rodata.len(),
                rules.text.len()
            );
            Ok(true)
        }
        Command::Verify { elf, map } => {
            let v = lp_fw_split::verify(&elf, &map)?;
            print!("{}", v.report(25));
            Ok(v.passed())
        }
    }
}
