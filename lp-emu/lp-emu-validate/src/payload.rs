//! The payload registry: what a transcript is a transcript *of*.
//!
//! A payload is a module in `fw-checks` behind a cargo feature (vision D14),
//! runnable many per image. This registry is the host's half: the payload's
//! name, the firmware feature that builds it, the marker that says it arrived,
//! and — the part that makes replay possible — which fields in its output mean
//! what.
//!
//! **This registry mirrors `fw-checks`, it does not import it.** `fw-checks` is
//! AGPL and lives outside the `lp-emu/` fence; importing it would breach
//! `just lint-emu-fence`. `lp-cli` depends on both and owns the parity test
//! (`lp-cli/tests/validate_registry_parity.rs`), so the duplication cannot
//! drift silently.

use std::sync::OnceLock;

use anyhow::{Result, bail};
use regex::Regex;

use crate::grade::FieldClass;

/// The line that says the payload got where it was going.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sentinel {
    /// The payload runs to completion and prints this.
    Done(&'static str),
    /// The payload announces readiness and then serves until told to stop.
    /// Host-driven payloads (the GPIO calibration protocol) work this way.
    Ready(&'static str),
}

impl Sentinel {
    pub const fn marker(self) -> &'static str {
        match self {
            Self::Done(m) | Self::Ready(m) => m,
        }
    }

    /// The `done_marker` a matching `fw-checks` registry entry must declare.
    /// `Ready` payloads never finish, so theirs is `None`.
    pub const fn fw_checks_done_marker(self) -> Option<&'static str> {
        match self {
            Self::Done(m) => Some(m),
            Self::Ready(_) => None,
        }
    }
}

/// One field of one structured record, and what class of claim it makes.
#[derive(Clone, Copy, Debug)]
pub struct FieldSpec {
    pub record: &'static str,
    pub field: &'static str,
    pub class: FieldClass,
}

/// A repeated line the payload prints, parsed into an indexed series.
///
/// The compile harness's per-tick line is the motivating case: 92 lines, four
/// memory numbers and two timing numbers each, and the claim "all 184 memory
/// values identical" is only checkable if the series is parsed rather than
/// diffed as prose.
pub struct SeriesSpec {
    pub name: &'static str,
    pub description: &'static str,
    /// A regex with named captures. One capture is the index (`key`); the rest
    /// are values, each with a class.
    pattern: &'static str,
    pub key: &'static str,
    pub fields: &'static [(&'static str, FieldClass)],
    compiled: OnceLock<Regex>,
}

impl SeriesSpec {
    pub fn regex(&self) -> &Regex {
        self.compiled
            .get_or_init(|| Regex::new(self.pattern).expect("series pattern compiles"))
    }

    pub fn pattern(&self) -> &'static str {
        self.pattern
    }
}

impl std::fmt::Debug for SeriesSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SeriesSpec")
            .field("name", &self.name)
            .field("key", &self.key)
            .finish()
    }
}

/// A payload: one named, runnable question.
#[derive(Debug)]
pub struct Payload {
    pub name: &'static str,
    pub display_name: &'static str,
    /// The `fw-checks` `FwCheck::slug()` this mirrors.
    pub fw_check_slug: &'static str,
    /// The cargo features on the firmware crate that build it, on top of the
    /// crate's defaults.
    ///
    /// A list rather than one feature because of `boot-idle`: a shipped-image
    /// payload is not a `test_*` module, it is the product image, and what
    /// names it is the feature set the image was built with.
    pub firmware_features: &'static [&'static str],
    /// The cargo feature on `fw-checks` that compiles its shared module, or
    /// `None` for a payload with no module of its own.
    pub fw_checks_feature: Option<&'static str>,
    /// Does the image print the in-band `[fw-checks-header] ` line?
    ///
    /// Only a payload with a `fw-checks` module does. `boot-idle` does not, so
    /// its provenance is the sidecar alone — which the loader already allows
    /// (the in-band header is optional; when present it must agree).
    pub emits_header: bool,
    pub sentinel: Sentinel,
    /// A `--uart0-script` this payload's walk needs, repo-root-relative, or
    /// `None` for a payload the host never speaks to.
    ///
    /// A walk is a conversation, and the half a *payload* owns is what the
    /// host says. Recording it as a file the configuration driver hands to
    /// the machine is what makes the walk a payload at all: without it the
    /// only reproducible transcript is a boot. On silicon the same
    /// conversation is the client's, over a port — the file is the
    /// emulated configurations' stand-in for it, and a driver that cannot
    /// use one says so in its plan.
    pub host_script: Option<&'static str>,
    /// Structured record kinds it emits behind `[fw-check-json] `.
    pub record_kinds: &'static [&'static str],
    /// The mask set that makes two of its transcripts comparable.
    pub mask_set: &'static str,
    pub fields: &'static [FieldSpec],
    pub series: &'static [&'static SeriesSpec],
}

impl Payload {
    pub fn class_of(&self, record: &str, field: &str) -> Option<FieldClass> {
        self.fields
            .iter()
            .find(|f| f.record == record && f.field == field)
            .map(|f| f.class)
    }
}

/// The compile harness's per-tick line.
///
/// ```text
/// [inc-shader-compile] case=examples-basic tick=1 stage= slice_cycles=173595 \
///   slice_us=1084 mem_before=321600 free/3936 used mem_after=308508 free/17028 used
/// ```
pub static COMPILE_TICK: SeriesSpec = SeriesSpec {
    name: "compile-tick",
    description: "one incremental-compile slice: its cost and the heap either side",
    pattern: concat!(
        r"case=(?<case>\S+) tick=(?<tick>\d+) stage=(?<stage>\S*) ",
        r"slice_cycles=(?<slice_cycles>\d+) slice_us=(?<slice_us>\d+) ",
        r"mem_before=(?<mem_before_free>\d+) free/(?<mem_before_used>\d+) used ",
        r"mem_after=(?<mem_after_free>\d+) free/(?<mem_after_used>\d+) used",
    ),
    key: "tick",
    fields: &[
        ("case", FieldClass::Structural),
        ("stage", FieldClass::Structural),
        ("slice_cycles", FieldClass::Timing),
        ("slice_us", FieldClass::Timing),
        ("mem_before_free", FieldClass::Memory),
        ("mem_before_used", FieldClass::Memory),
        ("mem_after_free", FieldClass::Memory),
        ("mem_after_used", FieldClass::Memory),
    ],
    compiled: OnceLock::new(),
};

/// The GPIO calibration protocol's pulse report.
///
/// ```text
/// CAL PULSE gpio=18 duty=40
/// ```
pub static CAL_PULSE: SeriesSpec = SeriesSpec {
    name: "cal-pulse",
    description: "one duty-ramp report from the host-driven GPIO calibration payload",
    pattern: r"^CAL PULSE gpio=(?<gpio>\d+) duty=(?<duty>\d+)$",
    key: "gpio",
    fields: &[("duty", FieldClass::Pin)],
    compiled: OnceLock::new(),
};

/// The bridge harness's one line of its own.
///
/// ```text
/// UART-BRIDGE READY baud=115200 tx=gpio16 rx=gpio17 prev_drop_to_uart=0 prev_drop_to_usb=0
/// ```
///
/// Keyed on the pin it listens to: there is one line per boot today, and the
/// index is the one that stays right if a variant ever taps two UARTs at once.
///
/// The two `prev_drop_*` figures are `Wire`, not `Structural`: they are a claim
/// about bytes that crossed a wire, and the whole point of the instrument is
/// that a transcript captured through it is only trustworthy while they are
/// zero. `4294967295` is the bridge saying UART0's hardware FIFO overran and it
/// cannot know by how much.
pub static BRIDGE_READY: SeriesSpec = SeriesSpec {
    name: "bridge-ready",
    description: "the UART bridge's boot line, carrying the previous run's byte losses",
    pattern: concat!(
        r"^UART-BRIDGE READY baud=(?<baud>\d+) tx=gpio(?<tx>\d+) rx=gpio(?<rx>\d+) ",
        r"prev_drop_to_uart=(?<prev_drop_to_uart>\d+) prev_drop_to_usb=(?<prev_drop_to_usb>\d+)$",
    ),
    key: "rx",
    fields: &[
        ("baud", FieldClass::Structural),
        ("tx", FieldClass::Structural),
        ("prev_drop_to_uart", FieldClass::Wire),
        ("prev_drop_to_usb", FieldClass::Wire),
    ],
    compiled: OnceLock::new(),
};

/// The shipped image's hello frame, the first thing a boot puts on the wire.
///
/// ```text
/// M!{"id":0,"msg":{"hello":{"proto":20,"build":{"features":["node.button",…],
///   "package":"fw-esp32c6","commit":"d6cfaa2051ae","dirty":true,
///   "profile":"release-esp32"},"hardware":{"radio":true,"totalLedBudget":null,
///   "button":true,"boardId":"seeed/xiao-esp32-c6","baseMac":"a0:f2:62:87:b4:8c",
///   "chipRevision":"0.2","eui64":"a0:f2:62:87:b4:8c:00:00"},"deviceUid":null}}}
/// ```
///
/// `proto` is `Wire`: it is the protocol version the device announces, and a
/// configuration that got it wrong would be lying about the link. Everything
/// else here is `Structural` — which build, which board profile.
///
/// Three fields are matched and deliberately **not** captured. `commit` and
/// `dirty` are build provenance, which the sidecar carries as
/// `firmware_commit` / `firmware_dirty`; the identity trio
/// (`baseMac`, `chipRevision`, `eui64`) is eFuse content, which the sidecar
/// carries as `mac` / `silicon_rev` and which the runner seeds an emulated
/// configuration's eFuse from, so the two agree by construction rather than by
/// luck. Both are masked in the human view by [`crate::mask::BOOT_IDLE`].
pub static HELLO: SeriesSpec = SeriesSpec {
    name: "hello",
    description: "the wire hello frame: protocol version, build and board profile",
    pattern: concat!(
        r#""hello":\{"proto":(?<proto>\d+),"build":\{"features":\[(?<features>[^\]]*)\],"#,
        r#""package":"(?<package>[^"]+)","commit":"[0-9a-f]*","dirty":(?:true|false),"#,
        r#""profile":"(?<profile>[^"]+)"\},"hardware":\{"radio":(?<radio>true|false),"#,
        r#""totalLedBudget":(?<total_led_budget>[^,]+),"button":(?<button>true|false),"#,
        r#""boardId":"(?<board_id>[^"]+)""#,
    ),
    key: "package",
    fields: &[
        ("proto", FieldClass::Wire),
        ("features", FieldClass::Structural),
        ("profile", FieldClass::Structural),
        ("radio", FieldClass::Structural),
        ("total_led_budget", FieldClass::Structural),
        ("button", FieldClass::Structural),
        ("board_id", FieldClass::Structural),
    ],
    compiled: OnceLock::new(),
};

/// The idle heartbeat's heap sample and the counters beside it.
///
/// Keyed on the literal message name, so a run that printed several
/// heartbeats compares its **last** one (`Transcript::series` is last-writes,
/// as it is for the calibration payload). The `boot-idle` payload's sentinel
/// stops the run at the first stack heartbeat, so in practice there is one.
///
/// `largestFreeBlock` is `Timing`, not `Memory`, and that is a decision worth
/// stating: it is a fragmentation *snapshot*, whose value depends on which
/// allocations happened to be live at the microsecond the sample was taken.
/// `freeBytes`/`usedBytes`/`totalBytes` are the allocator's own ledger.
pub static HEARTBEAT: SeriesSpec = SeriesSpec {
    name: "heartbeat",
    description: "one idle heartbeat: the heap ledger, the frame counters and the recovery state",
    pattern: concat!(
        r#""msg":\{"(?<msg>heartbeat)":\{"fps":\{"avg":(?<fps_avg>[0-9.]+)[^}]*\},"#,
        r#""frame_count":(?<frame_count>\d+),"loaded_projects":\[(?<loaded_projects>[^\]]*)\],"#,
        r#""uptime_ms":(?<uptime_ms>\d+),"memory":\{"freeBytes":(?<free_bytes>\d+),"#,
        r#""usedBytes":(?<used_bytes>\d+),"totalBytes":(?<total_bytes>\d+),"#,
        r#""largestFreeBlock":(?<largest_free_block>\d+)\},"#,
        r#""recovery":\{"level":"(?<recovery_level>[^"]+)","resetReason":"(?<reset_reason>[^"]+)","#,
        r#""bootCount":(?<boot_count>\d+)"#,
    ),
    key: "msg",
    fields: &[
        ("free_bytes", FieldClass::Memory),
        ("used_bytes", FieldClass::Memory),
        ("total_bytes", FieldClass::Memory),
        ("largest_free_block", FieldClass::Timing),
        ("fps_avg", FieldClass::Timing),
        ("frame_count", FieldClass::Timing),
        ("uptime_ms", FieldClass::Timing),
        ("loaded_projects", FieldClass::Structural),
        ("recovery_level", FieldClass::Structural),
        ("reset_reason", FieldClass::Structural),
        ("boot_count", FieldClass::Structural),
    ],
    compiled: OnceLock::new(),
};

/// The stack probe's heartbeat line.
///
/// ```text
/// [stack] heartbeat: high-water 11432 B of 71960 B (60528 B headroom)
/// ```
///
/// Keyed on the stack's size, which is a link-time constant: two transcripts
/// of the same image are talking about the same stack or they are not
/// comparable at all. Both figures are `Memory`.
///
/// The high-water mark **moves with the time grade** — 11,432 B under `t1`
/// and 11,752 B under `t2` on the same image (M3 P6) — because interleaving
/// decides how deep the deepest interrupted call chain got. That is not a
/// memory-model difference, and it is why `t2_memory_equals_t1` is scoped to
/// the single-task harness.
pub static STACK_HEARTBEAT: SeriesSpec = SeriesSpec {
    name: "stack-heartbeat",
    description: "the stack probe's high-water mark against the stack it was given",
    pattern: concat!(
        r"\[stack\] heartbeat: high-water (?<high_water>\d+) B of (?<stack_bytes>\d+) B ",
        r"\((?<headroom>\d+) B headroom\)",
    ),
    key: "stack_bytes",
    fields: &[
        ("high_water", FieldClass::Memory),
        ("headroom", FieldClass::Memory),
    ],
    compiled: OnceLock::new(),
};

/// The littlefs mount's outcome on the flash partition.
///
/// ```text
/// [FS] Mount failed (filesystem corrupt), formatting partition...
/// [FS] Formatted and mounted fresh filesystem
/// ```
///
/// Two rows, keyed on which of the two lines it is, because the pair is the
/// claim: a first boot on an erased chip formats and says so, and a second
/// boot on the same chip prints neither line. Structural, not memory —
/// *whether* the filesystem mounted is a fact about the flash controller,
/// and the heap it costs is the heartbeat's business.
pub static FS_MOUNT: SeriesSpec = SeriesSpec {
    name: "fs-mount",
    description: "the littlefs mount's outcome on the lpfs partition",
    pattern: r"lp_fs: \[FS\] (?<action>Mount failed|Formatted and mounted)(?<detail>[^\r\n]*)",
    key: "action",
    fields: &[("detail", FieldClass::Structural)],
    compiled: OnceLock::new(),
};

/// The server's heap gates around a project load.
///
/// ```text
/// [mem] load_project after: 220532 B free / 105004 B used (215k / 102k)
/// ```
///
/// One row per gate name, all four figures `Memory`: these are the numbers
/// the spike report's §5.3 and §11.2 compare across silicon, esp-emu and
/// this machine, and the load gate is the one the heap-budget record is
/// built from. The `Nk / Nk` suffix is the same numbers rounded, so it is
/// not captured twice.
pub static LOAD_GATE: SeriesSpec = SeriesSpec {
    name: "load-gate",
    description: "the allocator's ledger at each of the server's project-load gates",
    pattern: concat!(
        r"\[mem\] (?<gate>[a-z_]+ (?:before|after)): (?<free_bytes>\d+) B free / ",
        r"(?<used_bytes>\d+) B used",
    ),
    key: "gate",
    fields: &[
        ("free_bytes", FieldClass::Memory),
        ("used_bytes", FieldClass::Memory),
    ],
    compiled: OnceLock::new(),
};

/// A filesystem write acknowledgement on the wire.
///
/// ```text
/// M!{"id":2,"msg":{"filesystem":{"write":{"path":"/projects/Basic/clock.json","error":null}}}}
/// ```
///
/// Keyed on the path, so the series is "every file the upload wrote, and
/// whether the device took it". `error` is structural and is the point:
/// `null` on all of them or the upload did not happen.
pub static FS_WRITE: SeriesSpec = SeriesSpec {
    name: "fs-write",
    description: "each filesystem write the upload made, and the device's answer",
    // `writeChunk`'s answer carries `"offset"` and `"written"` between the
    // path and the error; `write`'s does not. The middle is skipped rather
    // than made optional, because what the series claims is *which file, and
    // did it land* — a chunk's offset is the client's bookkeeping.
    pattern: concat!(
        r#""filesystem":\{"(?<op>write|writeChunk)":\{"path":"(?<path>[^"]+)""#,
        r#"[^}]*?"error":(?<error>null|\{[^}]*\})"#,
    ),
    key: "path",
    fields: &[
        ("op", FieldClass::Structural),
        ("error", FieldClass::Structural),
    ],
    compiled: OnceLock::new(),
};

/// The on-device shader compile's summary line.
///
/// ```text
/// [shader-node] compilation succeeded (node=, elapsed=51ms, lpir_inst_count=573,
///   lpir_func_count=12, lpir_import_count=7, final_inst_count=2048,
///   final_code_size=8192 bytes, float=fixed)
/// ```
///
/// The counts and the code size are compiler **outputs** — the same source
/// produces the same numbers wherever it compiles, and §11.2 records that
/// they are identical on silicon and esp-emu. `elapsed` is the one clock in
/// the line and is graded `Timing`: it reads 51 ms under this machine's
/// script and 52 ms under the live client on the same image.
pub static SHADER_COMPILE: SeriesSpec = SeriesSpec {
    name: "shader-compile",
    description: "the on-device shader compile's outputs, and how long it took",
    pattern: concat!(
        r"\[shader-node\] (?<kind>compilation succeeded) \(node=[^,]*, ",
        r"elapsed=(?<elapsed_ms>\d+)ms, lpir_inst_count=(?<lpir_inst_count>\d+), ",
        r"lpir_func_count=(?<lpir_func_count>\d+), lpir_import_count=(?<lpir_import_count>\d+), ",
        r"final_inst_count=(?<final_inst_count>\d+), final_code_size=(?<final_code_size>\d+) bytes, ",
        r"float=(?<float_mode>\w+)\)",
    ),
    key: "kind",
    fields: &[
        ("elapsed_ms", FieldClass::Timing),
        ("lpir_inst_count", FieldClass::Structural),
        ("lpir_func_count", FieldClass::Structural),
        ("lpir_import_count", FieldClass::Structural),
        ("final_inst_count", FieldClass::Structural),
        ("final_code_size", FieldClass::Structural),
        ("float_mode", FieldClass::Structural),
    ],
    compiled: OnceLock::new(),
};

/// One `jit-math-perf` bench measurement.
///
/// ```text
/// [fw-check-json] {"kind":"jit-bench","label":"mul/helper-saturating","calls":441,
///   "median":812,"per_call":1,"avg":815,"min":808,"max":990,"checksum":123456}
/// ```
///
/// `label` and `calls` are structural (which kernel, how much corpus); the
/// five cycle figures are timing (esp-emu has no cycle model — `validate.toml`
/// grades it `modeled` — and this crate's own `lp-emu:*` has none yet either);
/// `checksum` is a deterministic XOR of the kernel's outputs, not a clock, so
/// it is compared like any other structural field.
static JIT_BENCH_FIELDS: &[FieldSpec] = &[
    FieldSpec {
        record: "jit-bench",
        field: "label",
        class: FieldClass::Structural,
    },
    FieldSpec {
        record: "jit-bench",
        field: "calls",
        class: FieldClass::Structural,
    },
    FieldSpec {
        record: "jit-bench",
        field: "median",
        class: FieldClass::Timing,
    },
    FieldSpec {
        record: "jit-bench",
        field: "per_call",
        class: FieldClass::Timing,
    },
    FieldSpec {
        record: "jit-bench",
        field: "avg",
        class: FieldClass::Timing,
    },
    FieldSpec {
        record: "jit-bench",
        field: "min",
        class: FieldClass::Timing,
    },
    FieldSpec {
        record: "jit-bench",
        field: "max",
        class: FieldClass::Timing,
    },
    FieldSpec {
        record: "jit-bench",
        field: "checksum",
        class: FieldClass::Structural,
    },
];

pub static ALL_PAYLOADS: &[Payload] = &[
    Payload {
        name: "shader-compile-stress",
        display_name: "Incremental shader compile stress",
        fw_check_slug: "shader-compile-stress",
        firmware_features: &["test_shader_compile_incremental"],
        fw_checks_feature: Some("check-shader-compile"),
        emits_header: true,
        sentinel: Sentinel::Done("[inc-shader-compile] === DONE ==="),
        host_script: None,
        record_kinds: &["case-summary", "total-summary"],
        mask_set: "compile-harness",
        fields: &[
            FieldSpec {
                record: "case-summary",
                field: "check",
                class: FieldClass::Structural,
            },
            FieldSpec {
                record: "case-summary",
                field: "case",
                class: FieldClass::Structural,
            },
            FieldSpec {
                record: "case-summary",
                field: "ticks",
                class: FieldClass::Structural,
            },
            FieldSpec {
                record: "case-summary",
                field: "max_slice_stage",
                class: FieldClass::Structural,
            },
            FieldSpec {
                record: "case-summary",
                field: "build_us",
                class: FieldClass::Timing,
            },
            FieldSpec {
                record: "case-summary",
                field: "max_slice_us",
                class: FieldClass::Timing,
            },
            FieldSpec {
                record: "case-summary",
                field: "peak_used",
                class: FieldClass::Memory,
            },
            FieldSpec {
                record: "case-summary",
                field: "resident_used",
                class: FieldClass::Memory,
            },
            FieldSpec {
                record: "case-summary",
                field: "after_drop_used",
                class: FieldClass::Memory,
            },
            FieldSpec {
                record: "total-summary",
                field: "check",
                class: FieldClass::Structural,
            },
            FieldSpec {
                record: "total-summary",
                field: "cases",
                class: FieldClass::Structural,
            },
            FieldSpec {
                record: "total-summary",
                field: "build_us",
                class: FieldClass::Timing,
            },
            FieldSpec {
                record: "total-summary",
                field: "worst_slice_us",
                class: FieldClass::Timing,
            },
            FieldSpec {
                record: "total-summary",
                field: "worst_peak_used",
                class: FieldClass::Memory,
            },
        ],
        series: &[&COMPILE_TICK],
    },
    Payload {
        name: "gpio-calibrate",
        display_name: "Host-driven GPIO square-wave calibration",
        fw_check_slug: "gpio-calibrate",
        firmware_features: &["test_gpio_calibrate"],
        fw_checks_feature: Some("check-gpio-calibrate"),
        emits_header: true,
        sentinel: Sentinel::Ready("CAL READY target="),
        host_script: None,
        record_kinds: &[],
        mask_set: "normalize",
        fields: &[],
        series: &[&CAL_PULSE],
    },
    Payload {
        name: "uart-bridge",
        display_name: "Transparent USB-Serial-JTAG <-> UART0 bridge",
        fw_check_slug: "uart-bridge",
        firmware_features: &["test_uart_bridge"],
        fw_checks_feature: Some("check-uart-bridge"),
        emits_header: true,
        sentinel: Sentinel::Ready("UART-BRIDGE READY "),
        host_script: None,
        record_kinds: &[],
        mask_set: "normalize",
        fields: &[],
        series: &[&BRIDGE_READY],
    },
    Payload {
        name: "jit-math-perf",
        display_name: "JIT Q32 math perf",
        fw_check_slug: "jit-math-perf",
        firmware_features: &["test_jit_math_perf"],
        fw_checks_feature: Some("check-jit-math-perf"),
        emits_header: true,
        sentinel: Sentinel::Done("[jit-math-perf] === DONE ==="),
        host_script: None,
        record_kinds: &["jit-bench"],
        mask_set: "jit-math-perf",
        fields: JIT_BENCH_FIELDS,
        series: &[],
    },
    Payload {
        name: "boot-idle",
        display_name: "Shipped image to the idle loop",
        fw_check_slug: "boot-idle",
        // The shipped-image walk as a payload (vision Q1: "shipped-image walks
        // are a distinct scenario kind"). There is no `test_*` module and no
        // `fw-checks` feature — the payload IS the product image, and the
        // features are the image's own, on top of the crate's defaults.
        //
        // `memory_fs` is the firmware's "no flash" switch. The flash-backed
        // shipped image mounts `lpfs` through the SPI1 controller, which is M4
        // (DD23): on `lp-emu:esp32c6:*` it stops at `SPIN SPI1+0x000 cmd` at
        // 11 ms, and the figures it would then print — spike report §5.1's
        // `freeBytes 265392`, `[stack] high-water 11844 B of 71328 B` and the
        // `[FS] Mount failed … Formatted and mounted` pair — are recorded as
        // **expected at M4**, not compared now. What this payload records is
        // the §5.4 variant.
        firmware_features: &["server", "radio", "memory_fs"],
        fw_checks_feature: None,
        emits_header: false,
        // The first stack heartbeat, at 5 s of uptime; a run has to be at
        // least 5.5 s long to reach it. The marker is a prefix of its line,
        // so the machine's `--exit-on` runs on to that line's newline before
        // it stops — otherwise the two figures the payload exists for would
        // be cut off mid-line.
        sentinel: Sentinel::Done("[stack] heartbeat: high-water"),
        host_script: None,
        record_kinds: &[],
        mask_set: "boot-idle",
        fields: &[],
        series: &[&HELLO, &HEARTBEAT, &STACK_HEARTBEAT],
    },
    Payload {
        name: "boot-idle-flash",
        display_name: "Shipped image to the idle loop, from flash",
        fw_check_slug: "boot-idle-flash",
        // `boot-idle` without `memory_fs`: the image that mounts `lpfs` from
        // the flash chip. M3 could not run it — it stopped at
        // `SPIN SPI1+0x000 cmd` at 11 ms — and M4's first gate is that it
        // now prints the `[FS]` pair and reaches the same idle heartbeat
        // with the spike report §5.1 figures.
        firmware_features: &["server", "radio"],
        fw_checks_feature: None,
        emits_header: false,
        host_script: None,
        sentinel: Sentinel::Done("[stack] heartbeat: high-water"),
        record_kinds: &[],
        mask_set: "boot-idle",
        fields: &[],
        series: &[&HELLO, &FS_MOUNT, &HEARTBEAT, &STACK_HEARTBEAT],
    },
    Payload {
        name: "upload-walk",
        display_name: "Project upload walk (examples/basic)",
        fw_check_slug: "upload-walk",
        // The same flash-backed image with a host on the other end. The
        // conversation is `lp-cli upload examples/basic`, captured from the
        // real client over a socket and replayed from a script so the
        // transcript is a function of guest time (`walks/README.md`).
        firmware_features: &["server", "radio"],
        fw_checks_feature: None,
        emits_header: false,
        host_script: Some("lp-emu/esp/lp-emu-esp32c6/walks/examples-basic.script"),
        sentinel: Sentinel::Done("[shader-node] compilation succeeded"),
        record_kinds: &[],
        mask_set: "boot-idle",
        fields: &[],
        series: &[&HELLO, &FS_MOUNT, &FS_WRITE, &LOAD_GATE, &SHADER_COMPILE],
    },
];

pub fn find_payload(name: &str) -> Result<&'static Payload> {
    match ALL_PAYLOADS.iter().find(|p| p.name == name) {
        Some(p) => Ok(p),
        None => bail!(
            "unknown payload `{name}` (known: {})",
            ALL_PAYLOADS
                .iter()
                .map(|p| p.name)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_names_are_unique() {
        let mut names: Vec<_> = ALL_PAYLOADS.iter().map(|p| p.name).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "duplicate payload name");
    }

    #[test]
    fn every_payload_names_a_known_mask_set() {
        for p in ALL_PAYLOADS {
            crate::mask::mask_set(p.mask_set).unwrap_or_else(|e| panic!("payload {}: {e}", p.name));
        }
    }

    #[test]
    fn every_series_pattern_compiles_and_names_its_key() {
        for p in ALL_PAYLOADS {
            for s in p.series {
                let re = s.regex();
                let names: Vec<_> = re.capture_names().flatten().collect();
                assert!(
                    names.contains(&s.key),
                    "{}: key `{}` is not a capture in {:?}",
                    s.name,
                    s.key,
                    names
                );
                for (field, _) in s.fields {
                    assert!(
                        names.contains(field),
                        "{}: field `{field}` is not a capture",
                        s.name
                    );
                }
            }
        }
    }

    #[test]
    fn compile_tick_parses_a_real_line() {
        let line = "[INFO] fw_esp32c6::tests::incremental_shader_compile::runner: \
                    [inc-shader-compile] case=examples-basic tick=1 stage= \
                    slice_cycles=173595 slice_us=1084 mem_before=321600 free/3936 used \
                    mem_after=308508 free/17028 used";
        let caps = COMPILE_TICK.regex().captures(line).expect("matches");
        assert_eq!(&caps["tick"], "1");
        assert_eq!(&caps["case"], "examples-basic");
        assert_eq!(&caps["stage"], "");
        assert_eq!(&caps["slice_us"], "1084");
        assert_eq!(&caps["mem_before_used"], "3936");
        assert_eq!(&caps["mem_after_used"], "17028");
    }

    #[test]
    fn cal_pulse_parses_a_protocol_line() {
        let caps = CAL_PULSE
            .regex()
            .captures("CAL PULSE gpio=18 duty=40")
            .expect("matches");
        assert_eq!(&caps["gpio"], "18");
        assert_eq!(&caps["duty"], "40");
        assert!(
            CAL_PULSE
                .regex()
                .captures("CAL READY target=esp32c6")
                .is_none()
        );
    }

    #[test]
    fn bridge_ready_parses_a_clean_boot_and_a_dirty_one() {
        let clean = BRIDGE_READY
            .regex()
            .captures(
                "UART-BRIDGE READY baud=115200 tx=gpio16 rx=gpio17 \
                 prev_drop_to_uart=0 prev_drop_to_usb=0",
            )
            .expect("matches");
        assert_eq!(&clean["baud"], "115200");
        assert_eq!(&clean["rx"], "17");
        assert_eq!(&clean["prev_drop_to_usb"], "0");

        let dirty = BRIDGE_READY
            .regex()
            .captures(
                "UART-BRIDGE READY baud=921600 tx=gpio16 rx=gpio17 \
                 prev_drop_to_uart=7 prev_drop_to_usb=4294967295",
            )
            .expect("matches");
        assert_eq!(&dirty["prev_drop_to_uart"], "7");
        assert_eq!(
            &dirty["prev_drop_to_usb"], "4294967295",
            "the hardware-overrun sentinel value must survive the parse"
        );
    }

    /// The three `boot-idle` series against the lines an M3 P6 run actually
    /// produced (`target/emu-ref/d6cfaa205-boot-idle-memfs`, `t1`, strict).
    #[test]
    fn the_boot_idle_series_parse_the_lines_the_shipped_image_prints() {
        let hello = concat!(
            r#"M!{"id":0,"msg":{"hello":{"proto":20,"build":{"features":["node.button","#,
            r#""gfx.lpvm"],"package":"fw-esp32c6","commit":"d6cfaa2051ae","dirty":true,"#,
            r#""profile":"release-esp32"},"hardware":{"radio":true,"totalLedBudget":null,"#,
            r#""button":true,"boardId":"seeed/xiao-esp32-c6","baseMac":"a0:f2:62:87:b4:8c","#,
            r#""chipRevision":"0.2","eui64":"a0:f2:62:87:b4:8c:00:00"},"deviceUid":null}}}"#,
        );
        let caps = HELLO.regex().captures(hello).expect("the hello parses");
        assert_eq!(&caps["proto"], "20");
        assert_eq!(&caps["package"], "fw-esp32c6");
        assert_eq!(&caps["profile"], "release-esp32");
        assert_eq!(&caps["board_id"], "seeed/xiao-esp32-c6");
        assert_eq!(&caps["total_led_budget"], "null");
        // Provenance and identity are matched, never captured.
        let names: Vec<_> = HELLO.regex().capture_names().flatten().collect();
        for absent in ["commit", "dirty", "baseMac", "chipRevision", "eui64"] {
            assert!(!names.contains(&absent), "`{absent}` must not be captured");
        }

        let beat = concat!(
            r#"M!{"id":0,"msg":{"heartbeat":{"fps":{"avg":967,"sdev":0,"min":967,"max":967},"#,
            r#""frame_count":4835,"loaded_projects":[],"uptime_ms":5000,"#,
            r#""memory":{"freeBytes":266688,"usedBytes":58848,"totalBytes":325536,"#,
            r#""largestFreeBlock":200633},"recovery":{"level":"green","#,
            r#""resetReason":"power-on","bootCount":1,"safeMode":false}}}}"#,
        );
        let caps = HEARTBEAT
            .regex()
            .captures(beat)
            .expect("the heartbeat parses");
        assert_eq!(&caps["msg"], "heartbeat");
        assert_eq!(&caps["free_bytes"], "266688");
        assert_eq!(&caps["total_bytes"], "325536");
        assert_eq!(&caps["largest_free_block"], "200633");
        assert_eq!(&caps["reset_reason"], "power-on");
        assert_eq!(&caps["boot_count"], "1");
        // esp-emu's sample prints a fractional average (spike report §5.1).
        let float = beat.replace(r#""avg":967"#, r#""avg":491.60336"#);
        assert_eq!(
            &HEARTBEAT.regex().captures(&float).unwrap()["fps_avg"],
            "491.60336"
        );

        let stack = "[INFO] fw_esp32c6::stack_probe: [stack] heartbeat: \
                     high-water 11432 B of 71960 B (60528 B headroom)";
        let caps = STACK_HEARTBEAT
            .regex()
            .captures(stack)
            .expect("the stack line parses");
        assert_eq!(&caps["high_water"], "11432");
        assert_eq!(&caps["stack_bytes"], "71960");
        assert_eq!(&caps["headroom"], "60528");
    }

    #[test]
    fn lookup_reports_the_known_set() {
        assert!(find_payload("shader-compile-stress").is_ok());
        let err = find_payload("nope").unwrap_err().to_string();
        assert!(err.contains("shader-compile-stress"), "{err}");
    }
}
