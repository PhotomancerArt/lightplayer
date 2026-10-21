//! `lp-fw-split`: build the ESP32-C6 split image, or run one of its steps.
//!
//!   lp-fw-split build --out target/fw-split/<slug> [--features esp32c6,server]
//!   lp-fw-split reach <elf> <map> --emit-ld <engine.x>
//!   lp-fw-split verify <elf> <map>
//!   lp-fw-split headroom <out>/split.json [--margin 65536]
//!   lp-fw-split segments <core.bin>...
//!   lp-fw-split bootloader <out>   (rebuild only our second stage into a build)

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
        /// The IDF bootloader the IDF door's image (`merged-idf.bin`) carries
        /// (default: the esp32c6-4mb build def's IDF bootloader,
        /// lp-fw/bootloaders/esp32c6-bootloader-idf-v5.1.bin).
        #[arg(long)]
        bootloader: Option<PathBuf>,
        /// The second stage `merged.bin` boots through: `lightplayer` (our
        /// own, the default) or `idf`. Both doors' images are always written.
        #[arg(long, default_value = "lightplayer")]
        door: lp_fw_split::Door,
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
    /// Rebuild our second stage (the loader's bootloader flavour) into an
    /// existing build: `bootloader.bin`, `merged-lp.bin`, and `merged.bin`
    /// when that build's door is ours. Nothing else is relinked.
    Bootloader {
        /// A build's output directory.
        out: PathBuf,
        /// The repository root (default: the current directory).
        #[arg(long, default_value = ".")]
        repo: PathBuf,
    },
    /// SPIKE (`spike-s3-split-image`): the split for fw-esp32s3.
    BuildS3 {
        #[arg(long)]
        out: PathBuf,
        /// fw-esp32s3 features beyond its defaults.
        #[arg(long)]
        features: Option<String>,
        /// The S3 loader's ELF; without it the build stops at core/engine.
        #[arg(long)]
        loader_elf: Option<PathBuf>,
        #[arg(long, default_value = ".")]
        repo: PathBuf,
    },
    /// SPIKE: verify one fw-esp32s3 pass-2 link.
    VerifyS3 { elf: PathBuf, map: PathBuf },
    /// Check one (pass-2) link: no core section in the engine region.
    Verify { elf: PathBuf, map: PathBuf },
    /// List core images' segments and fail when the loader cannot place one
    /// (an address-0 padding segment, or any outside the core's flash and RAM).
    Segments {
        #[arg(required = true)]
        images: Vec<PathBuf>,
    },
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
            door,
            partitions,
            repo,
            target,
        } => {
            let mut opts = BuildOptions::new(repo, out);
            opts.features = features;
            if let Some(b) = bootloader {
                opts.bootloader = Some(b);
            }
            opts.door = door;
            opts.target = target;
            if let Some(p) = partitions {
                opts.partitions = p;
            }
            lp_fw_split::build(&opts)?;
            Ok(true)
        }
        Command::BuildS3 {
            out,
            features,
            loader_elf,
            repo,
        } => {
            lp_fw_split::s3::build_s3(&lp_fw_split::s3::S3Options {
                repo,
                out,
                features,
                loader_elf,
            })?;
            Ok(true)
        }
        Command::VerifyS3 { elf, map } => {
            let v = lp_fw_split::s3::verify_s3(&elf, &map)?;
            print!("{}", v.report(30));
            Ok(v.passed())
        }
        Command::Bootloader { out, repo } => {
            lp_fw_split::split_build::rebuild_bootloader(&repo, &out)?;
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
        Command::Segments { images } => {
            let mut ok = true;
            for path in images {
                let segs = lp_fw_split::core_segments::segments(&std::fs::read(&path)?)?;
                let refused = segs.iter().filter(|s| s.placement().is_err()).count();
                println!(
                    "{}: {} segments, {refused} the loader cannot place",
                    path.display(),
                    segs.len()
                );
                for s in &segs {
                    println!("{}", s.line());
                }
                ok &= refused == 0;
            }
            Ok(ok)
        }
        Command::Verify { elf, map } => {
            let v = lp_fw_split::verify(&elf, &map)?;
            print!("{}", v.report(25));
            Ok(v.passed())
        }
    }
}
