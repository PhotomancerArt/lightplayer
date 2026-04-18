//! Performance column selection for filetest summaries (`--perf`).

use lp_riscv_emu::CycleModel;

/// Display / accounting mode for the guest RV32 cost column in filetest tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PerfModel {
    /// Retired-instruction count (1 cycle per instruction in the model).
    Insts,
    /// ESP32-C6 coarse cycle estimate (see `lp-riscv-emu` cost table).
    Esp32c6,
}

impl Default for PerfModel {
    fn default() -> Self {
        PerfModel::Esp32c6
    }
}

impl PerfModel {
    /// Every variant, for help text and `--perf` error messages.
    pub const ALL: &'static [PerfModel] = &[PerfModel::Insts, PerfModel::Esp32c6];

    /// Stable CLI / config token (e.g. `insts`, `esp32c6`).
    pub fn name(self) -> &'static str {
        match self {
            PerfModel::Insts => "insts",
            PerfModel::Esp32c6 => "esp32c6",
        }
    }

    /// One-line explanation for `--help` and parse errors.
    pub fn description(self) -> &'static str {
        match self {
            PerfModel::Insts => {
                "Raw retired-instruction count (1 cycle per instruction). Useful for codegen size comparisons."
            }
            PerfModel::Esp32c6 => {
                "ESP32-C6 (Andes N22) basic cycle estimate. Per-class cost table + branch-taken accounting."
            }
        }
    }

    /// Optional background article (shown in `--perf` errors when relevant).
    pub fn article_url(self) -> Option<&'static str> {
        match self {
            PerfModel::Insts => None,
            PerfModel::Esp32c6 => {
                Some("https://ctrlsrc.io/posts/2023/counting-cpu-cycles-on-esp32c3-esp32c6/")
            }
        }
    }

    /// Column title for the summary table (matches `--perf` selection).
    pub fn column_header(self) -> &'static str {
        match self {
            PerfModel::Insts => "total inst",
            PerfModel::Esp32c6 => "esp32c6 est. cyc.",
        }
    }

    /// Maps to the emulator's [`CycleModel`] before each guest call.
    pub fn cycle_model(self) -> CycleModel {
        match self {
            PerfModel::Insts => CycleModel::InstructionCount,
            PerfModel::Esp32c6 => CycleModel::Esp32C6,
        }
    }

    /// Metric used for the perf column and for `vs fastest` (both always tracked in stats).
    pub fn metric_value(self, stats: &crate::test_run::TestCaseStats) -> u64 {
        match self {
            PerfModel::Insts => stats.guest_instructions_total,
            PerfModel::Esp32c6 => stats.guest_cycles_total,
        }
    }

    /// Parses a `--perf` token (case-insensitive); error string lists valid values with descriptions.
    pub fn parse(s: &str) -> Result<Self, String> {
        let s = s.trim().to_ascii_lowercase();
        for &m in Self::ALL {
            if m.name() == s {
                return Ok(m);
            }
        }
        Err(perf_parse_error(&s))
    }
}

fn perf_parse_error(bad: &str) -> String {
    let mut msg = String::from("invalid value for --perf\n");
    msg.push_str("Valid values:\n");
    for m in PerfModel::ALL {
        msg.push_str(&format!("  {} — {}", m.name(), m.description()));
        if let Some(url) = m.article_url() {
            msg.push_str(&format!("\n    See {url}"));
        }
        msg.push('\n');
    }
    msg.push_str(&format!("(got {bad:?})"));
    msg
}

#[cfg(test)]
mod tests {
    use super::PerfModel;

    #[test]
    fn parse_insts_and_esp32c6() {
        assert_eq!(PerfModel::parse("insts").unwrap(), PerfModel::Insts);
        assert_eq!(PerfModel::parse("esp32c6").unwrap(), PerfModel::Esp32c6);
    }

    #[test]
    fn parse_bogus_lists_options() {
        let err = PerfModel::parse("foo").unwrap_err();
        let d = err;
        assert!(d.contains("insts"));
        assert!(d.contains("esp32c6"));
        assert!(d.contains("Raw retired-instruction"));
        assert!(d.contains("ESP32-C6"));
    }
}
