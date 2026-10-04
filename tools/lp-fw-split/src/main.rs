//! `lp-fw-split`: build the ESP32-C6 split image, or run one of its steps.
//!
//!   lp-fw-split build --out target/fw-split/<slug> [--features esp32c6,server]
//!   lp-fw-split reach <elf> <map> --emit-ld <engine.x>
//!   lp-fw-split verify <elf> <map>
//!   lp-fw-split headroom <out>/split.json [--margin 65536]

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
        /// The target (`lp-fw/builds/` id) to embed in the manifest core;
        /// omitted, the image says `unknown`.
        #[arg(long)]
        target: Option<String>,
    },
    /// Report the split image's headrooms and gate the smallest of the
    /// steady and update ones against a margin.
    Headroom {
        /// A build's `split.json`.
        split_json: PathBuf,
        /// The partition table it is flashed with.
        #[arg(long, default_value = "lp-fw/fw-esp32c6/partitions.csv")]
        partitions: PathBuf,
        #[arg(long, default_value_t = 65536)]
        margin: i64,
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
            target,
        } => {
            let mut opts = BuildOptions::new(repo, out);
            opts.features = features;
            opts.bootloader = bootloader;
            opts.target = target;
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
        Command::Headroom {
            split_json,
            partitions,
            margin,
        } => {
            let report: lp_fw_split::SplitReport =
                serde_json::from_str(&std::fs::read_to_string(&split_json)?)?;
            let (_, factory_len) = lp_fw_split::split_build::factory_extent(&partitions)?;
            let h = lp_fw_split::headroom::Headroom::of(&report, factory_len);
            let gated = h.gated();
            let mut lines = h.lines();
            lines.push(format!(
                "split headroom gate: {gated} B (smallest of steady and update; margin {margin} B)"
            ));
            for line in &lines {
                println!("{line}");
            }
            if let Ok(summary) = std::env::var("GITHUB_STEP_SUMMARY") {
                use std::io::Write as _;
                let mut f = std::fs::OpenOptions::new().append(true).open(summary)?;
                for line in &lines {
                    writeln!(f, "- {line}")?;
                }
            }
            if gated < margin {
                eprintln!("FAIL: split headroom {gated} B is under the {margin} B margin");
                return Ok(false);
            }
            Ok(true)
        }
        Command::Verify { elf, map } => {
            let v = lp_fw_split::verify(&elf, &map)?;
            print!("{}", v.report(25));
            Ok(v.passed())
        }
    }
}
