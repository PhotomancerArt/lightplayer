//! `lp-fw-split`: build a split image (the ESP32-C6's, or the ESP32-S3's
//! with `--chip esp32s3`), or run one of its steps.
//!
//!   lp-fw-split build --out target/fw-split/<slug> [--chip esp32c6] [--features esp32c6,server]
//!   lp-fw-split reach <elf> <map> --emit-ld <engine.x> [--chip …]
//!   lp-fw-split verify <elf> <map> [--chip …]
//!   lp-fw-split why <elf> <map> <section-substring> [--chip …]
//!   lp-fw-split headroom <out>/split.json [--margin 65536]
//!   lp-fw-split segments <core.bin>... [--chip …]
//!   lp-fw-split bootloader <out>   (rebuild only our second stage into a C6 build)

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};
use lp_fw_split::chip_profile::SplitChip;
use lp_fw_split::reachability::{Split, path_to};
use lp_fw_split::section_graph::SectionGraph;
use lp_fw_split::{BuildOptions, engine_script::EngineRules};

#[derive(Parser)]
#[command(
    about = "Build a split image (ESP32-C6, ESP32-S3): loader, boot records, core and engine"
)]
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
        /// The chip: esp32c6 (the default) or esp32s3.
        #[arg(long, default_value = "esp32c6")]
        chip: SplitChip,
        /// Firmware features (its defaults are always on; default: the
        /// chip's, `esp32c6,server` / `esp32s3,server`).
        #[arg(long)]
        features: Option<String>,
        /// The IDF bootloader the IDF door's image (`merged-idf.bin`) carries
        /// (default: the chip's — the esp32c6-4mb build def's
        /// lp-fw/bootloaders/esp32c6-bootloader-idf-v5.1.bin, or espflash's
        /// bundled one for the S3).
        #[arg(long)]
        bootloader: Option<PathBuf>,
        /// The second stage `merged.bin` boots through: `lightplayer` (our
        /// own, the C6's default) or `idf` (the only one the S3 has). Every
        /// door the chip has gets its image written.
        #[arg(long)]
        door: Option<lp_fw_split::Door>,
        /// The partition table (default: the chip's,
        /// lp-fw/fw-<chip>/partitions.csv).
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
        /// The partition table it is flashed with (default: the chip's the
        /// report names).
        #[arg(long)]
        partitions: Option<PathBuf>,
        #[arg(long, default_value_t = 65536)]
        margin: i64,
    },
    /// Split one link and write the engine's placement script.
    Reach {
        elf: PathBuf,
        map: PathBuf,
        #[arg(long)]
        emit_ld: PathBuf,
        #[arg(long, default_value = "esp32c6")]
        chip: SplitChip,
    },
    /// How the core reaches a section: the chain of input sections from a
    /// root to the first whose `object:(section)` contains `needle`.
    Why {
        elf: PathBuf,
        map: PathBuf,
        needle: String,
        #[arg(long, default_value = "esp32c6")]
        chip: SplitChip,
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
    /// Check one (pass-2) link: no core section in the engine region.
    Verify {
        elf: PathBuf,
        map: PathBuf,
        #[arg(long, default_value = "esp32c6")]
        chip: SplitChip,
    },
    /// List core images' segments and fail when the loader cannot place one
    /// (an address-0 padding segment, or any outside the core's flash and RAM).
    Segments {
        #[arg(required = true)]
        images: Vec<PathBuf>,
        #[arg(long, default_value = "esp32c6")]
        chip: SplitChip,
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
            chip,
            features,
            bootloader,
            door,
            partitions,
            repo,
            target,
        } => {
            let mut opts = BuildOptions::for_chip(chip, repo, out);
            if let Some(f) = features {
                opts.features = f;
            }
            if let Some(b) = bootloader {
                opts.bootloader = Some(b);
            }
            if let Some(d) = door {
                opts.door = d;
            }
            opts.target = target;
            if let Some(p) = partitions {
                opts.partitions = p;
            }
            lp_fw_split::build(&opts)?;
            Ok(true)
        }
        Command::Bootloader { out, repo } => {
            lp_fw_split::split_build::rebuild_bootloader(&repo, &out)?;
            Ok(true)
        }
        Command::Reach {
            elf,
            map,
            emit_ld,
            chip,
        } => {
            let profile = chip.profile();
            let graph = SectionGraph::load_for(profile, &elf, &map)?;
            let roots = lp_fw_split::split_build::core_roots(&graph, &std::fs::read(&elf)?)?;
            let split = Split::compute(&graph, &roots);
            let rules = EngineRules::from_split_for(profile, &graph, &split);
            std::fs::write(&emit_ld, rules.script_for(profile))?;
            eprintln!(
                "emitted {}: {} rodata + {} text input sections",
                emit_ld.display(),
                rules.rodata.len(),
                rules.text.len()
            );
            Ok(true)
        }
        Command::Why {
            elf,
            map,
            needle,
            chip,
        } => {
            let profile = chip.profile();
            let graph = SectionGraph::load_for(profile, &elf, &map)?;
            let bytes = std::fs::read(&elf)?;
            let roots = lp_fw_split::split_build::core_roots(&graph, &bytes)?;
            let names = lp_fw_split::section_graph::demangled_symbols(&bytes)?;
            match path_to(&graph, &roots, &|i| graph.nodes[i].desc.contains(&needle)) {
                Some(chain) => {
                    for i in chain {
                        let n = &graph.nodes[i];
                        let sym = names
                            .range(n.vma..n.vma + n.size.max(1))
                            .next()
                            .and_then(|(_, v)| v.first())
                            .map(|(_, s)| s.as_str())
                            .unwrap_or("");
                        println!(
                            "{:#010x} {} {} {sym}",
                            n.vma,
                            n.out,
                            n.object_and_section().1
                        );
                    }
                    Ok(true)
                }
                None => {
                    println!("the core does not reach a section matching `{needle}`");
                    Ok(true)
                }
            }
        }
        Command::Headroom {
            split_json,
            partitions,
            margin,
        } => {
            let report: lp_fw_split::SplitReport =
                serde_json::from_str(&std::fs::read_to_string(&split_json)?)?;
            let partitions =
                partitions.unwrap_or_else(|| PathBuf::from(report.chip.profile().partitions));
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
        Command::Segments { images, chip } => {
            let rules = chip.profile().segments;
            let mut ok = true;
            for path in images {
                let segs = lp_fw_split::core_segments::segments(&std::fs::read(&path)?)?;
                let refused = segs
                    .iter()
                    .filter(|s| s.placement_in(rules).is_err())
                    .count();
                println!(
                    "{}: {} segments, {refused} the loader cannot place",
                    path.display(),
                    segs.len()
                );
                for s in &segs {
                    println!("{}", s.line_in(rules));
                }
                ok &= refused == 0;
            }
            Ok(ok)
        }
        Command::Verify { elf, map, chip } => {
            let v = lp_fw_split::split_build::verify_for(chip.profile(), &elf, &map)?;
            print!("{}", v.report(25));
            Ok(v.passed())
        }
    }
}
