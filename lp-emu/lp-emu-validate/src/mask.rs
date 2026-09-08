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

/// The `[WS281X]` telemetry line's own timestamp.
///
/// The one field in that line that is a clock rather than a count: the module
/// reports when ten seconds of *its* uptime have passed, so `t_ms` is a
/// property of when the run started printing, not of the refill race. Every
/// other field on the line — the frame counters, the refill counts, the two
/// histograms — is left comparable, because they are what the line exists to
/// say.
pub static WS281X_TIMESTAMP: MaskRule = MaskRule::new(
    "ws281x-timestamp",
    "the telemetry line's own uptime stamp; the counters beside it are the claim",
    FieldClass::Timing,
    r"(\[WS281X\] )t_ms=[0-9]+",
    "${1}t_ms=N",
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

/// The `jit-math-perf` bench line's cycle figures:
/// `[jit-math-perf] bench <label> median=N per_call=N avg=N min=N max=N
/// calls=N checksum=N`. `calls` (corpus size) and `checksum` (a deterministic
/// XOR of the kernel's outputs) are left alone — only the five numbers with a
/// clock in them are masked, because the structured `jit-bench` records carry
/// the same figures and are compared instead.
pub static JIT_BENCH_CYCLES: MaskRule = MaskRule::new(
    "jit-bench-cycles",
    "cycle counts in the jit-math-perf bench line; the structured jit-bench \
     records carry the same numbers and are compared instead",
    FieldClass::Timing,
    r"(median|per_call|avg|min|max)=[0-9]+",
    "$1=N",
);

/// `[jit-math-perf] overhead summary: empty=N cycles, counter-pair=N cycles`.
pub static JIT_OVERHEAD_CYCLES: MaskRule = MaskRule::new(
    "jit-overhead-cycles",
    "the overhead baseline's raw cycle counts; the jit-bench records for \
     overhead/empty and overhead/read-counter-pair carry the same numbers",
    FieldClass::Timing,
    r"(empty|counter-pair)=[0-9]+ cycles",
    "$1=N cycles",
);

/// The hello frame's build provenance: `"commit":"d6cfaa2051ae","dirty":true`.
///
/// Not a claim about the chip — a claim about the tree the image was built
/// from, which the sidecar carries as `firmware_commit` / `firmware_dirty` and
/// which `Transcript::load` already refuses to let disagree with an in-band
/// header. Comparing it here as well would fail every replay of one image
/// captured at two commits, for a difference the header states plainly.
pub static HELLO_BUILD_PROVENANCE: MaskRule = MaskRule::new(
    "hello-build-provenance",
    "the hello's build commit and dirty flag; the sidecar's firmware_commit \
     and firmware_dirty are the provenance, and they are checked against the \
     in-band header rather than diffed as prose",
    FieldClass::Structural,
    r#""commit":"[0-9a-f]*","dirty":(true|false)"#,
    r#""commit":"N","dirty":N"#,
);

/// The chip identity a wire frame carries: `baseMac`, `chipRevision`, `eui64`.
///
/// These come from the eFuse block, and the sidecar carries them as `mac` and
/// `silicon_rev`. The runner seeds an emulated configuration's eFuse from the
/// configuration's entry in `validate.toml` (`--efuse-mac` / `--efuse-rev`),
/// so an emulated transcript and a silicon one of the same desk board agree by
/// construction; masking here keeps the *human* view stable when they are not
/// the same board, which is a fact about the desk and not about the model.
pub static WIRE_IDENTITY: MaskRule = MaskRule::new(
    "wire-identity",
    "baseMac / chipRevision / eui64 in a wire frame: eFuse content, carried by \
     the sidecar as mac and silicon_rev and seeded into an emulated \
     configuration from validate.toml",
    FieldClass::Wire,
    r#""(baseMac|chipRevision|eui64)":"[^"]*""#,
    r#""$1":"N""#,
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

/// The `jit-math-perf` set: normalise, then mask the cycle counts. `label`,
/// `calls` and `checksum` stay comparable — memory is not in play here, only
/// timing, and cycle-cost payloads exist to report it, not hide it.
pub static JIT_MATH_PERF: MaskSet = MaskSet {
    name: "jit-math-perf",
    description: "ANSI + the cycle counts in the bench and overhead lines; \
                  labels, call counts and checksums are left comparable",
    rules: &[&ANSI, &JIT_BENCH_CYCLES, &JIT_OVERHEAD_CYCLES],
};

/// The shipped-image boot set: normalise, drop build provenance and chip
/// identity, mask everything with a clock in it — and leave **memory alone**.
///
/// `HEAP_LEDGER_*`, `HEARTBEAT_MEMORY` and `STACK_HIGH_WATER` are deliberately
/// absent: the heap ledger at the idle heartbeat and the stack high-water mark
/// are the two numbers this payload exists to compare, and the `heartbeat` and
/// `stack-heartbeat` series carry them as `Memory`.
pub static BOOT_IDLE: MaskSet = MaskSet {
    name: "boot-idle",
    description: "ANSI, build provenance, chip identity and every clock-derived \
                  field; the heap and stack figures are left comparable",
    rules: &[
        &ANSI,
        &HELLO_BUILD_PROVENANCE,
        &WIRE_IDENTITY,
        &HEARTBEAT_TIMING,
        &SAMPLE_STATS,
        &PROSE_TIMING,
        &BOOT_TIMESTAMP,
    ],
};

/// The `rmt-chase` set: ANSI, the boot banner's stamps, and the telemetry
/// line's own uptime stamp — and **nothing else**.
///
/// Deliberately the shortest set in the file after `normalize`. Every number
/// this payload prints is a claim it exists to make: the per-frame checksums
/// are the frames, and the `[WS281X]` counters are what the driver believes
/// about the refill race. `PROSE_TIMING` is absent on purpose: it rewrites
/// `frame=N`, which is one character from this payload's `frames=768`, and a
/// mask that erases the gate is worse than no mask at all.
pub static RMT_CHASE: MaskSet = MaskSet {
    name: "rmt-chase",
    description: "ANSI, boot timestamps and the telemetry line's t_ms; every \
                  counter and checksum is left comparable",
    rules: &[&ANSI, &BOOT_TIMESTAMP, &WS281X_TIMESTAMP],
};

/// The render-loop summary line's per-frame microseconds.
///
/// `PROSE_TIMING` does not reach them — it knows `elapsed=`, `frame=` and
/// `tick=`, and this line says `mean=`, `min=`, `max=` and `first=` in
/// microseconds. The numbers are not lost by masking them here: the
/// `render-loop-summary` RECORD beside this line carries every one of them as
/// a `Timing`-graded field, which is where a replay compares them properly.
pub static RENDER_LOOP_FRAME_US: MaskRule = MaskRule::new(
    "render-loop-frame-us",
    "the summary line's per-frame microseconds; the record beside it carries      the same numbers as graded fields",
    FieldClass::Timing,
    r"(mean|min|max|first)=[0-9]+us",
    "$1=Nus",
);

/// The render-loop summary line's frames-per-second.
///
/// Its own rule rather than `PROSE_TIMING`'s `fps=[0-9]+`, which would match
/// the integer part of `fps=65.99` and leave `fps=N.99` behind — a half-masked
/// number that still differs between two runs and reads as if it had been
/// handled. Whole field or nothing.
pub static RENDER_LOOP_FPS: MaskRule = MaskRule::new(
    "render-loop-fps",
    "the summary line's fps, masked whole rather than to its decimals",
    FieldClass::Timing,
    r"fps=[0-9]+\.[0-9]+",
    "fps=N",
);

/// The `render-loop` set: everything with a clock in it, and **memory left
/// alone**.
///
/// Same shape as `BOOT_IDLE` and for the same reason. What this payload exists
/// to say is how long a frame takes and how much heap a loaded project costs,
/// and those arrive as graded fields on its two records rather than as prose —
/// so the prose restatements are masked and `HEAP_LEDGER_*` is deliberately
/// absent, leaving the `[mem]` bracket around `load_project` comparable.
///
/// `PROSE_TIMING` is included, unlike in `RMT_CHASE`: it rewrites `frame=N`,
/// and this payload's line says `frames=` — one character further along, which
/// the regex's `=` anchor does not reach. It does reach the shader compile
/// line's `elapsed=52ms`, which is exactly a clock and exactly what should go.
pub static RENDER_LOOP: MaskSet = MaskSet {
    name: "render-loop",
    description: "ANSI, build provenance, chip identity and every clock-derived \
                  field; the heap figures are left comparable",
    rules: &[
        &ANSI,
        &HELLO_BUILD_PROVENANCE,
        &WIRE_IDENTITY,
        &HEARTBEAT_TIMING,
        &SAMPLE_STATS,
        &PROSE_TIMING,
        &BOOT_TIMESTAMP,
        &RENDER_LOOP_FRAME_US,
        &RENDER_LOOP_FPS,
    ],
};

pub static ALL_SETS: &[&MaskSet] = &[
    &NORMALIZE,
    &COMPILE_HARNESS,
    &WALK,
    &BOOT_LOG,
    &JIT_MATH_PERF,
    &BOOT_IDLE,
    &RMT_CHASE,
    &RENDER_LOOP,
];

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
    fn the_boot_idle_set_hides_provenance_and_identity_but_not_the_heap() {
        let hello = r#"M!{"id":0,"msg":{"hello":{"proto":20,"build":{"features":[],"#.to_string()
            + r#""package":"fw-esp32c6","commit":"d6cfaa2051ae","dirty":true,"#
            + r#""profile":"release-esp32"},"hardware":{"boardId":"seeed/xiao-esp32-c6","#
            + r#""baseMac":"a0:f2:62:87:b4:8c","chipRevision":"0.2","eui64":"a0:f2:62:87:b4:8c:00:00"}}}}"#;
        let masked = BOOT_IDLE.apply(&hello);
        assert!(masked.contains(r#""commit":"N","dirty":N"#), "{masked}");
        assert!(masked.contains(r#""baseMac":"N""#), "{masked}");
        assert!(masked.contains(r#""chipRevision":"N""#), "{masked}");
        assert!(masked.contains(r#""eui64":"N""#), "{masked}");
        // The board profile is not identity: it stays.
        assert!(
            masked.contains(r#""boardId":"seeed/xiao-esp32-c6""#),
            "{masked}"
        );

        // The two numbers the payload exists for survive the mask.
        let beat = r#"M!{"id":0,"msg":{"heartbeat":{"uptime_ms":5000,"memory":{"freeBytes":266688,"usedBytes":58848,"totalBytes":325536}}}}"#;
        let masked = BOOT_IDLE.apply(beat);
        assert!(masked.contains(r#""freeBytes":266688"#), "{masked}");
        assert!(masked.contains(r#""uptime_ms":N"#), "{masked}");
        let stack = "[stack] heartbeat: high-water 11432 B of 71960 B (60528 B headroom)";
        assert_eq!(BOOT_IDLE.apply(stack), stack);
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

    #[test]
    fn jit_math_perf_masks_cycles_but_not_labels_calls_or_checksum() {
        let line = "[jit-math-perf] bench mul/helper-saturating median=812 per_call=1 \
                    avg=815 min=808 max=990 calls=441 checksum=123456";
        let masked = JIT_MATH_PERF.apply(line);
        assert_eq!(
            masked,
            "[jit-math-perf] bench mul/helper-saturating median=N per_call=N \
             avg=N min=N max=N calls=441 checksum=123456"
        );
    }

    #[test]
    fn jit_math_perf_masks_the_overhead_summary_line() {
        let line = "[jit-math-perf] overhead summary: empty=6 cycles, counter-pair=14 cycles";
        assert_eq!(
            JIT_MATH_PERF.apply(line),
            "[jit-math-perf] overhead summary: empty=N cycles, counter-pair=N cycles"
        );
    }
}
