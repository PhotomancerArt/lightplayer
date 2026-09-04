use clap::{Args, Parser, Subcommand, ValueEnum};
use lp_emu_core::profile::frag::CounterfactualSpec;
use std::path::PathBuf;

use super::mode::ProfileMode;

/// `lp-cli profile …` — run a session or `profile diff` (stub).
#[derive(Debug, Parser)]
#[command(
    name = "profile",
    about = "Run a profiling session or compare two profile directories."
)]
pub struct ProfileCli {
    #[command(subcommand)]
    pub subcommand: Option<ProfileSubcommand>,

    #[command(flatten)]
    pub run: ProfileArgs,
}

#[derive(Debug, Subcommand)]
pub enum ProfileSubcommand {
    /// Compare two profile directories (not yet implemented).
    Diff(ProfileDiffArgs),
    /// Explain one function in an existing cpu-profile.json.
    Function(ProfileFunctionArgs),
}

#[derive(Debug, Args)]
pub struct ProfileArgs {
    /// Workload directory (defaults to examples/basic).
    #[arg(default_value = "examples/basic")]
    pub dir: PathBuf,

    /// Collectors to enable (comma-separated). m2 supports: alloc, events, cpu.
    /// Default: cpu. (`events` is auto-included when `cpu` is enabled.)
    #[arg(long, default_value = "cpu", value_delimiter = ',')]
    pub collect: Vec<String>,

    /// Profile mode (state machine over the perf-event stream).
    #[arg(long, value_enum, default_value_t = ProfileMode::SteadyRender)]
    pub mode: ProfileMode,

    /// Cycle attribution model for the CPU collector.
    #[arg(long, value_enum, default_value_t = CycleModelArg::Esp32C6)]
    pub cycle_model: CycleModelArg,

    /// Safety cap on emulated cycles. The run terminates with exit
    /// code 0 and a warning if reached.
    #[arg(long, default_value_t = 200_000_000)]
    pub max_cycles: u64,

    /// Optional human-readable note appended to the profile dir.
    #[arg(long)]
    pub note: Option<String>,

    /// Heap layout the `alloc` collector's fragmentation section replays on:
    /// `classic` (the ESP32 classic's two regions) or `guest` (the emulator's
    /// own single region, the only layout the guest cross-check is meaningful
    /// on). Ignored without `--collect alloc`.
    #[arg(long, value_enum, default_value_t = FragLayoutArg::Classic)]
    pub frag_layout: FragLayoutArg,

    /// Override the replayed region sizes in bytes, in registration order
    /// (e.g. `112640,73728`). Takes precedence over `--frag-layout`.
    #[arg(long, value_delimiter = ',')]
    pub frag_regions: Vec<u32>,

    /// How many of the largest holes to attribute to bounding blocks at each
    /// marker.
    #[arg(long, default_value_t = 10)]
    pub frag_top: usize,

    /// Drop from the fragmentation replay every allocation whose symbolized
    /// call site contains this substring (repeatable). For discounting
    /// emulator-only artifacts — `fw-emu`'s 256-resource board manifest makes
    /// a few sites allocate amounts no device ever will. The report names
    /// every active discount and what it removed.
    #[arg(long, value_name = "SUBSTR")]
    pub frag_discount_site: Vec<String>,

    /// Add one counterfactual row to the fragmentation report: the same trace
    /// replayed with one lever already pulled (repeatable; each `--cf` is one
    /// row). `scratch=<windows>` replaces everything born and freed inside a
    /// window with one arena of its peak; `residents-first=<windows>` hoists
    /// what the window leaves behind to its start; `tlsf` replays through
    /// `rlsf` instead of the first-fit list. Join terms with `+` to combine
    /// them in one row, e.g.
    /// `--cf scratch=shader-compile+residents-first=project-load`.
    #[arg(long = "cf", value_name = "SPEC")]
    pub cf: Vec<String>,

    /// What the profile session makes the guest do after the project is
    /// deployed. `frames` drives frames only (the default, and what the
    /// heap-budget record is baselined on); `studio-sync` additionally sends
    /// Studio's staged initial project read, which is what opens the
    /// `project-read` window.
    #[arg(long, value_enum, default_value_t = WorkloadArg::Frames)]
    pub workload: WorkloadArg,
}

/// Which workload `lp-cli profile` runs against the emulator.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkloadArg {
    /// Deploy the project, then drive frames until the mode's gate stops.
    Frames,
    /// As `frames`, plus Studio's staged initial sync (skeleton read, slot
    /// pages of 16 nodes, one probe read) issued as soon as the project is
    /// loaded.
    StudioSync,
}

/// Heap layout selector for the fragmentation replay.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum FragLayoutArg {
    /// The ESP32 classic: 110 KiB `dram_seg` arena, then the 72 KiB SRAM1 tail.
    Classic,
    /// The emulator guest's single region, as recorded in `meta.json`.
    Guest,
}

impl ProfileArgs {
    /// The layout the fragmentation replay should use: explicit
    /// `--frag-regions` when given, otherwise the named layout.
    pub fn frag_layout(&self) -> lp_emu_core::profile::frag::FragLayout {
        use lp_emu_core::profile::frag::FragLayout;
        if !self.frag_regions.is_empty() {
            return FragLayout::Custom(self.frag_regions.clone());
        }
        match self.frag_layout {
            FragLayoutArg::Classic => FragLayout::Classic,
            FragLayoutArg::Guest => FragLayout::Guest,
        }
    }

    /// The counterfactuals to replay, in the order they were given.
    pub fn counterfactuals(&self) -> anyhow::Result<Vec<CounterfactualSpec>> {
        self.cf
            .iter()
            .map(|spec| CounterfactualSpec::parse(spec).map_err(|e| anyhow::anyhow!("--cf: {e}")))
            .collect()
    }
}

#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum CycleModelArg {
    Esp32C6,
    Uniform,
}

impl CycleModelArg {
    pub fn label(self) -> &'static str {
        match self {
            Self::Esp32C6 => "esp32c6",
            Self::Uniform => "uniform",
        }
    }

    pub fn to_emu(self) -> lp_emu_core::CycleModel {
        match self {
            Self::Esp32C6 => lp_emu_core::CycleModel::Esp32C6,
            Self::Uniform => lp_emu_core::CycleModel::InstructionCount,
        }
    }
}

#[derive(Debug, Args)]
pub struct ProfileDiffArgs {
    pub a: PathBuf,
    pub b: PathBuf,
}

#[derive(Debug, Args)]
pub struct ProfileFunctionArgs {
    /// Profile output directory containing cpu-profile.json.
    pub dir: PathBuf,

    /// Function name substring to inspect.
    pub function: String,

    /// Match the full function name exactly.
    #[arg(long)]
    pub exact: bool,

    /// Maximum rows per section.
    #[arg(long, default_value_t = 20)]
    pub top: usize,

    /// Optional RV32 ELF for addr2line callsite locations.
    #[arg(long)]
    pub elf: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn default_collect_is_cpu() {
        let cli = ProfileCli::parse_from(["lp-cli", "examples/basic"]);
        assert_eq!(cli.run.collect, vec!["cpu".to_string()]);
    }

    #[test]
    fn default_cycle_model_is_esp32c6() {
        let cli = ProfileCli::parse_from(["lp-cli", "examples/basic"]);
        assert!(matches!(cli.run.cycle_model, CycleModelArg::Esp32C6));
    }

    #[test]
    fn discount_sites_are_repeatable() {
        let cli = ProfileCli::parse_from([
            "lp-cli",
            "examples/basic",
            "--frag-discount-site",
            "VirtualWs281xDriver::endpoints",
            "--frag-discount-site",
            "virtual_quad_rmt_gpio_board",
        ]);
        assert_eq!(
            cli.run.frag_discount_site,
            vec![
                "VirtualWs281xDriver::endpoints".to_string(),
                "virtual_quad_rmt_gpio_board".to_string()
            ]
        );
    }

    #[test]
    fn frag_layout_defaults_to_classic() {
        let cli = ProfileCli::parse_from(["lp-cli", "examples/basic"]);
        assert_eq!(cli.run.frag_layout, FragLayoutArg::Classic);
        assert_eq!(cli.run.frag_top, 10);
        assert!(cli.run.frag_regions.is_empty());
    }

    #[test]
    fn explicit_frag_regions_win_over_the_named_layout() {
        let cli = ProfileCli::parse_from([
            "lp-cli",
            "examples/basic",
            "--frag-layout",
            "guest",
            "--frag-regions",
            "1024,2048",
        ]);
        assert_eq!(
            cli.run.frag_layout(),
            lp_emu_core::profile::frag::FragLayout::Custom(vec![1024, 2048])
        );
    }

    #[test]
    fn counterfactuals_are_repeatable_and_validated() {
        let cli = ProfileCli::parse_from([
            "lp-cli",
            "examples/basic",
            "--cf",
            "scratch=shader-compile,project-read",
            "--cf",
            "tlsf",
        ]);
        let specs = cli.run.counterfactuals().expect("both specs parse");
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0].label, "scratch=shader-compile,project-read");

        let bad = ProfileCli::parse_from(["lp-cli", "examples/basic", "--cf", "scratch"]);
        assert!(
            bad.run.counterfactuals().is_err(),
            "a transform with no window list is a typo, not an empty request"
        );
    }

    #[test]
    fn workload_defaults_to_frames() {
        let cli = ProfileCli::parse_from(["lp-cli", "examples/basic"]);
        assert_eq!(cli.run.workload, WorkloadArg::Frames);
        let sync =
            ProfileCli::parse_from(["lp-cli", "examples/basic", "--workload", "studio-sync"]);
        assert_eq!(sync.run.workload, WorkloadArg::StudioSync);
    }

    #[test]
    fn cycle_model_uniform_parses() {
        let cli = ProfileCli::parse_from(["lp-cli", "examples/basic", "--cycle-model", "uniform"]);
        assert!(matches!(cli.run.cycle_model, CycleModelArg::Uniform));
    }
}
