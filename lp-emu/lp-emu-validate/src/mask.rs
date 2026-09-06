//! Masking: the `scripts/spike/esp-emu/mask-transcript.sh` rules, as code.
//!
//! Two transcripts of the same payload on two configurations differ in ways
//! that mean nothing (a heap pointer's low digits, a uart timestamp) and in
//! ways that mean everything (a peak_used that moved). A mask rule names the
//! first kind, once, with a reason, so the difference between "we ignored it"
//! and "we forgot it" is written down.
//!
//! Every rule carries the field class it belongs to. That is what lets the
//! replay report say "the timing fields diverged 2.4x" instead of silently
//! erasing them: masking here means *reported but not compared*, never
//! *deleted*.

use std::sync::OnceLock;

use anyhow::{Result, bail};
use regex::Regex;

use crate::grade::FieldClass;

/// One substitution that makes two transcripts comparable.
pub struct MaskRule {
    pub name: &'static str,
    /// Why this difference is not a regression.
    pub why: &'static str,
    pub class: FieldClass,
    pattern: &'static str,
    replacement: &'static str,
    compiled: OnceLock<Regex>,
}

impl MaskRule {
    const fn new(
        name: &'static str,
        why: &'static str,
        class: FieldClass,
        pattern: &'static str,
        replacement: &'static str,
    ) -> Self {
        Self {
            name,
            why,
            class,
            pattern,
            replacement,
            compiled: OnceLock::new(),
        }
    }

    fn regex(&self) -> &Regex {
        self.compiled
            .get_or_init(|| Regex::new(self.pattern).expect("mask rule pattern compiles"))
    }

    pub fn pattern(&self) -> &'static str {
        self.pattern
    }

    pub fn apply(&self, line: &str) -> String {
        self.regex()
            .replace_all(line, self.replacement)
            .into_owned()
    }
}

impl std::fmt::Debug for MaskRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MaskRule")
            .field("name", &self.name)
            .field("class", &self.class)
            .field("pattern", &self.pattern)
            .finish()
    }
}

/// ANSI escapes. esp-emu colours its log lines; a `script(1)` capture of
/// espflash carries progress-bar erases. Neither is content.
pub static ANSI: MaskRule = MaskRule::new(
    "ansi",
    "terminal colour and erase sequences are not transcript content",
    FieldClass::Structural,
    r"\x1b\[[0-9;]*[A-Za-z]",
    "",
);

/// The heap ledger's human line: `321600 B free / 3936 B used (314k / 3k)`.
pub static HEAP_LEDGER_BYTES: MaskRule = MaskRule::new(
    "heap-ledger-bytes",
    "heap digits in the prose ledger; the structured records carry the same \
     numbers and are compared instead",
    FieldClass::Memory,
    r"[0-9]+ B free / [0-9]+ B used \([0-9]+k / [0-9]+k\)",
    "N B free / N B used (Nk / Nk)",
);

/// The short form: `314k free / 3k used`.
pub static HEAP_LEDGER_K: MaskRule = MaskRule::new(
    "heap-ledger-k",
    "rounded heap ledger; see heap-ledger-bytes",
    FieldClass::Memory,
    r"[0-9]+k free / [0-9]+k used",
    "Nk free / Nk used",
);

/// Heartbeat memory fields inside a wire JSON message.
pub static HEARTBEAT_MEMORY: MaskRule = MaskRule::new(
    "heartbeat-memory",
    "live heartbeat heap sampling is asynchronous to the walk; the load-gate \
     ledger lines are the measured figures",
    FieldClass::Memory,
    r#""(freeBytes|usedBytes|largestFreeBlock)":[0-9.]+"#,
    r#""$1":N"#,
);

/// Heartbeat counters and clocks inside a wire JSON message.
pub static HEARTBEAT_TIMING: MaskRule = MaskRule::new(
    "heartbeat-timing",
    "wall-clock-derived heartbeat fields; the host's own clock decides them",
    FieldClass::Timing,
    r#""(uptime_ms|frame_count|frame_num|frame_delta_ms|frame_total_ms|last_frame_time_us|theoretical_fps)":[0-9.]+"#,
    r#""$1":N"#,
);

/// `"avg"`, `"min"`, `"max"` — the fps sample statistics.
pub static SAMPLE_STATS: MaskRule = MaskRule::new(
    "sample-stats",
    "fps sample statistics are a wall-clock measurement",
    FieldClass::Timing,
    r#""(avg|min|max)":[0-9.]+"#,
    r#""$1":N"#,
);

/// `elapsed=123ms`, `tick=7`, `fps=60` in prose log lines.
pub static PROSE_TIMING: MaskRule = MaskRule::new(
    "prose-timing",
    "elapsed/frame/fps/recv/tick/send/total counters in human log lines",
    FieldClass::Timing,
    r"(elapsed|frame|fps|recv|tick|send|total)=[0-9]+(ms)?",
    "$1=N$2",
);

/// The stack high-water report.
pub static STACK_HIGH_WATER: MaskRule = MaskRule::new(
    "stack-high-water",
    "stack high-water is a memory figure compared structurally, not by prose",
    FieldClass::Memory,
    r"high-water [0-9]+ B of [0-9]+ B \([0-9]+ B headroom\)",
    "high-water N B of N B (N B headroom)",
);

/// IDF bootloader timestamps: `I (23) boot: ...`.
///
/// The spike measured 4-51 ms emulated against 23-235 ms on silicon — flash at
/// 40 MHz is not free. Same words, different clock.
pub static BOOT_TIMESTAMP: MaskRule = MaskRule::new(
    "boot-timestamp",
    "IDF bootloader millisecond stamps: silicon pays real flash wait states, \
     an emulator does not (spike report 11.1)",
    FieldClass::Timing,
    r"^I \([0-9]+\)",
    "I (N)",
);

/// esp-emu's intercepted ROM `printf` drops width and zero-pad flags, so
/// `paddr=00010020 … ( 70428)` arrives as `paddr=10020 … (70428)`. Same values,
/// different columns.
pub static ROM_PRINTF_COLUMNS: MaskRule = MaskRule::new(
    "rom-printf-columns",
    "esp-emu's ROM printf intercept drops width and zero-pad flags; same \
     words, same values, different columns (spike report 11.1)",
    FieldClass::BootLog,
    r"[ \t]+",
    " ",
);

/// A named, ordered set of rules.
pub struct MaskSet {
    pub name: &'static str,
    pub description: &'static str,
    pub rules: &'static [&'static MaskRule],
}

impl MaskSet {
    /// Apply every rule, in order, to one line.
    pub fn apply(&self, line: &str) -> String {
        self.rules
            .iter()
            .fold(line.to_string(), |acc, rule| rule.apply(&acc))
    }

    /// The rules that belong to a field class.
    pub fn rules_for(&self, class: FieldClass) -> impl Iterator<Item = &&'static MaskRule> {
        self.rules.iter().filter(move |r| r.class == class)
    }
}

/// Normalisation only: strip ANSI. Never masks a number.
///
/// Carriage returns and the `\r`-terminated line endings esp-emu emits are
/// handled by the transcript loader, not here, because they are framing rather
/// than content.
pub static NORMALIZE: MaskSet = MaskSet {
    name: "normalize",
    description: "ANSI escapes only; no number is touched",
    rules: &[&ANSI],
};

/// The compile-harness set: normalise, then mask everything with a clock in it.
///
/// Memory is deliberately *not* masked here — memory parity is the claim the
/// shader-compile payload exists to make.
pub static COMPILE_HARNESS: MaskSet = MaskSet {
    name: "compile-harness",
    description: "ANSI + every clock-derived field; memory is left comparable",
    rules: &[&ANSI, &BOOT_TIMESTAMP, &PROSE_TIMING],
};

/// The walk set: the full `mask-transcript.sh` rule list, for wire transcripts
/// where the subject is the protocol rather than the numbers.
pub static WALK: MaskSet = MaskSet {
    name: "walk",
    description: "the mask-transcript.sh rules in full: heap digits, clocks, \
                  heartbeat fields, boot timestamps",
    rules: &[
        &ANSI,
        &HEAP_LEDGER_BYTES,
        &HEAP_LEDGER_K,
        &HEARTBEAT_MEMORY,
        &HEARTBEAT_TIMING,
        &SAMPLE_STATS,
        &PROSE_TIMING,
        &STACK_HIGH_WATER,
        &BOOT_TIMESTAMP,
    ],
};

/// The boot-log set: for diffing a ROM/bootloader banner across
/// configurations, where column formatting is known to differ.
pub static BOOT_LOG: MaskSet = MaskSet {
    name: "boot-log",
    description: "ANSI, bootloader timestamps, and ROM printf column padding",
    rules: &[&ANSI, &BOOT_TIMESTAMP, &ROM_PRINTF_COLUMNS],
};

pub static ALL_SETS: &[&MaskSet] = &[&NORMALIZE, &COMPILE_HARNESS, &WALK, &BOOT_LOG];

pub fn mask_set(name: &str) -> Result<&'static MaskSet> {
    match ALL_SETS.iter().find(|s| s.name == name) {
        Some(s) => Ok(s),
        None => bail!(
            "unknown mask set `{name}` (known: {})",
            ALL_SETS
                .iter()
                .map(|s| s.name)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_rule_pattern_compiles() {
        for set in ALL_SETS {
            for rule in set.rules {
                let _ = rule.regex();
            }
        }
    }

    #[test]
    fn ansi_strip_matches_the_shell_rule() {
        let line = "\x1b[0;32mI (4) boot: chip revision: v0.3\x1b[0m";
        assert_eq!(ANSI.apply(line), "I (4) boot: chip revision: v0.3");
    }

    #[test]
    fn heap_ledger_masks_both_forms() {
        assert_eq!(
            HEAP_LEDGER_BYTES
                .apply("[mem] load_project after: 216056 B free / 45000 B used (211k / 43k)"),
            "[mem] load_project after: N B free / N B used (Nk / Nk)"
        );
        assert_eq!(
            HEAP_LEDGER_K.apply("compute shader compile before: 167k free / 152k used"),
            "compute shader compile before: Nk free / Nk used"
        );
    }

    #[test]
    fn boot_timestamps_mask_only_at_line_start() {
        assert_eq!(
            BOOT_TIMESTAMP.apply("I (235) boot: Loaded app"),
            "I (N) boot: Loaded app"
        );
        // Not at the start of the line: not a bootloader stamp.
        assert_eq!(
            BOOT_TIMESTAMP.apply("prefix I (235) boot:"),
            "prefix I (235) boot:"
        );
    }

    #[test]
    fn heartbeat_fields_mask_by_name() {
        let line = r#"M!{"uptime_ms":91234,"memory":{"freeBytes":152320,"usedBytes":173216}}"#;
        let masked = HEARTBEAT_TIMING.apply(&HEARTBEAT_MEMORY.apply(line));
        assert_eq!(
            masked,
            r#"M!{"uptime_ms":N,"memory":{"freeBytes":N,"usedBytes":N}}"#
        );
    }

    #[test]
    fn compile_harness_leaves_memory_comparable() {
        let line = "case=examples-basic tick=7 slice_us=286 mem_before=293188 free/32348 used";
        let masked = COMPILE_HARNESS.apply(line);
        assert!(masked.contains("mem_before=293188"), "{masked}");
        assert!(masked.contains("tick=N"), "{masked}");
    }

    #[test]
    fn walk_set_masks_memory_too() {
        let line = "[mem] load_project after: 216056 B free / 45000 B used (211k / 43k)";
        assert!(WALK.apply(line).contains("N B free"));
    }

    #[test]
    fn rom_printf_column_rule_collapses_padding() {
        let silicon = "I (N) boot:  0 nvs              WiFi data        01 02 00009000 00005000";
        let emu = "I (N) boot: 0 nvs WiFi data 1 2 9000 5000";
        // Column collapse alone is not enough — leading zeros differ too. The
        // rule is honest about doing only half the job.
        assert_ne!(ROM_PRINTF_COLUMNS.apply(silicon), emu);
        assert_eq!(
            ROM_PRINTF_COLUMNS.apply(silicon),
            "I (N) boot: 0 nvs WiFi data 01 02 00009000 00005000"
        );
    }

    #[test]
    fn mask_set_lookup() {
        assert!(mask_set("compile-harness").is_ok());
        assert!(mask_set("no-such-set").is_err());
    }

    #[test]
    fn rules_for_class_filters() {
        let timing: Vec<_> = WALK.rules_for(FieldClass::Timing).map(|r| r.name).collect();
        assert!(timing.contains(&"prose-timing"));
        assert!(!timing.contains(&"heap-ledger-k"));
    }
}
