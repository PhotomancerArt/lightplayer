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

/// How a silicon run watches the board.
///
/// A host-side property, deliberately not mirrored in `fw-checks`: the image
/// is the same bytes either way, and what differs is when the operator's side
/// opens the port. It lives on the payload because it is not a free choice —
/// a payload whose subject is *what the device did while nobody was reading*
/// is destroyed by a reader that attaches at the flash.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capture {
    /// espflash's own `--monitor`, opened the instant the flash finishes. The
    /// board is watched from its first byte, which is what every payload that
    /// makes a claim about output wants.
    Monitor,
    /// Flash with **no** monitor, leave the port closed for this many
    /// seconds, then open a non-resetting reader.
    ///
    /// The negative control: for the whole of the wait a host is attached
    /// (the cable is in, SOF keeps arriving) and no application is draining
    /// the port, which is the one state the firmware cannot report on at the
    /// time and can only remember. Everything the device says during the wait
    /// is lost on purpose — that is the measurement.
    FlashThenOpenAfter(u64),
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
    /// Structured record kinds it emits behind `[fw-check-json] `.
    pub record_kinds: &'static [&'static str],
    /// The mask set that makes two of its transcripts comparable.
    pub mask_set: &'static str,
    pub fields: &'static [FieldSpec],
    pub series: &'static [&'static SeriesSpec],
    /// When the host side opens the port, on a silicon run. See [`Capture`].
    pub capture: Capture,
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

/// The connection monitor's own record of a silence, riding the heartbeat's
/// `link` object (M6 P1b).
///
/// ```text
/// …,"link":{"parseFailures":0,"rxErrors":0,"queueFullDrops":0,
///   "stalePartialFlushes":0,"hostNotDrainingMs":1402,
///   "hostDrainingAgainMs":8137,"notDrainingCount":1},…
/// ```
///
/// The capture order is serde's struct order on `lpc_wire::server::
/// LinkCounters`, and the pattern pins it: the four loss counters, then the
/// two stamps, then the count. Reordering those fields would be a wire change
/// and this is one of the things that would say so.
///
/// **The match is itself the claim.** Both stamps are
/// `skip_serializing_if = "Option::is_none"`, so this pattern matches only a
/// frame from a device that both latched and recovered — the "latched then
/// resumed" transition, present and in that order. A run where the host was
/// reading from the first byte produces no entry in this series at all,
/// which is what the negative control's *positive* control looks like.
///
/// `count` is the `UsbSerialJtag` field, and that class is **hard** in
/// replay: a configuration that says one silence where another says two is a
/// difference, not a ratio. The two millisecond figures are `Timing` and are
/// reported with their ratio (PD9/D13 — no host gate on emulated
/// microseconds), which is exactly the shape the director ruled in DD33.
///
/// # The same facts without a heartbeat
///
/// The three numbers are relaxed atomics in the image, so a configuration
/// that has no host reading it at all can still be asked. Their demangled
/// paths, for `lp-emu-esp32c6 --probe <path>@<ms>` and for `peek_symbol`:
///
/// ```text
/// fw_esp32_common::serial::link_counters::HOST_NOT_DRAINING_MS
/// fw_esp32_common::serial::link_counters::HOST_DRAINING_AGAIN_MS
/// fw_esp32_common::serial::link_counters::NOT_DRAINING_COUNT
/// ```
///
/// The two `_MS` cells read `u32::MAX` for "never this boot" — the sentinel
/// the wire spells `None`, translated once inside `link_counters::current()`,
/// so a probe reads the raw form and a transcript reads the wire form. M6 P2's
/// attached-idle gate probes exactly these.
pub static LINK_MONITOR: SeriesSpec = SeriesSpec {
    name: "link-monitor",
    description: "the USB link's own record of when it stopped being drained, and when it resumed",
    pattern: concat!(
        r#""(?<link>link)":\{[^}]*"hostNotDrainingMs":(?<not_draining_ms>\d+)"#,
        r#"[^}]*"hostDrainingAgainMs":(?<draining_again_ms>\d+)"#,
        r#"[^}]*"notDrainingCount":(?<count>\d+)"#,
    ),
    key: "link",
    fields: &[
        ("count", FieldClass::UsbSerialJtag),
        ("not_draining_ms", FieldClass::Timing),
        ("draining_again_ms", FieldClass::Timing),
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
        capture: Capture::Monitor,
    },
    Payload {
        name: "gpio-calibrate",
        display_name: "Host-driven GPIO square-wave calibration",
        fw_check_slug: "gpio-calibrate",
        firmware_features: &["test_gpio_calibrate"],
        fw_checks_feature: Some("check-gpio-calibrate"),
        emits_header: true,
        sentinel: Sentinel::Ready("CAL READY target="),
        record_kinds: &[],
        mask_set: "normalize",
        fields: &[],
        series: &[&CAL_PULSE],
        capture: Capture::Monitor,
    },
    Payload {
        name: "uart-bridge",
        display_name: "Transparent USB-Serial-JTAG <-> UART0 bridge",
        fw_check_slug: "uart-bridge",
        firmware_features: &["test_uart_bridge"],
        fw_checks_feature: Some("check-uart-bridge"),
        emits_header: true,
        sentinel: Sentinel::Ready("UART-BRIDGE READY "),
        record_kinds: &[],
        mask_set: "normalize",
        fields: &[],
        series: &[&BRIDGE_READY],
        capture: Capture::Monitor,
    },
    Payload {
        name: "jit-math-perf",
        display_name: "JIT Q32 math perf",
        fw_check_slug: "jit-math-perf",
        firmware_features: &["test_jit_math_perf"],
        fw_checks_feature: Some("check-jit-math-perf"),
        emits_header: true,
        sentinel: Sentinel::Done("[jit-math-perf] === DONE ==="),
        record_kinds: &["jit-bench"],
        mask_set: "jit-math-perf",
        fields: JIT_BENCH_FIELDS,
        series: &[],
        capture: Capture::Monitor,
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
        record_kinds: &[],
        mask_set: "boot-idle",
        fields: &[],
        series: &[&HELLO, &HEARTBEAT, &STACK_HEARTBEAT],
        capture: Capture::Monitor,
    },
    Payload {
        name: "usb-negative-control",
        display_name: "Shipped image with the port closed from boot",
        fw_check_slug: "usb-negative-control",
        // The shipped image, flash-backed — `boot-idle` minus `memory_fs`.
        // The subject here is the USB link, not the heap, and the link is the
        // same one the product ships; running a `memory_fs` variant would
        // measure a different image for no reason on the only configuration
        // that can answer the question today. M6 P4's emulator twin does use
        // `memory_fs` until M4 lands SPI1, and says so.
        firmware_features: &["server", "radio"],
        fw_checks_feature: None,
        emits_header: false,
        // The stack heartbeat that FOLLOWS the first heartbeat after the
        // open, so the run captures one whole heartbeat — the `link` object
        // is the entire point and a sentinel on the heartbeat itself would
        // cut the capture off inside it.
        sentinel: Sentinel::Done("[stack] heartbeat: high-water"),
        record_kinds: &[],
        mask_set: "boot-idle",
        fields: &[],
        // No `HELLO`. The hello goes out at server start, seconds before the
        // reader attaches, into a port nobody has open — it is dropped, and
        // expecting it here would make every run of this payload fail for the
        // reason the payload exists to demonstrate.
        series: &[&HEARTBEAT, &STACK_HEARTBEAT, &LINK_MONITOR],
        // Boot ≈ 1 s, the hello plus two 250 ms write timeouts ≈ +0.6 s, and
        // one 5 s heartbeat interval of margin so the wait cannot land inside
        // the transition it is trying to observe.
        capture: Capture::FlashThenOpenAfter(8),
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

    /// The `link` object as `lpc_wire::server::LinkCounters` serialises it,
    /// in serde's declaration order. The three P1b fields come last, after
    /// the four loss counters, and the two stamps are only there at all
    /// because this device both latched and recovered.
    #[test]
    fn the_link_monitor_series_parses_a_heartbeat_from_a_link_that_went_quiet() {
        let beat = concat!(
            r#"M!{"id":0,"msg":{"heartbeat":{"fps":{"avg":967,"sdev":0,"min":967,"max":967},"#,
            r#""frame_count":9670,"loaded_projects":[],"uptime_ms":10000,"#,
            r#""memory":{"freeBytes":266688,"usedBytes":58848,"totalBytes":325536,"#,
            r#""largestFreeBlock":200633},"recovery":{"level":"green","#,
            r#""resetReason":"power-on","bootCount":1,"safeMode":false},"#,
            r#""link":{"parseFailures":0,"rxErrors":0,"queueFullDrops":0,"#,
            r#""stalePartialFlushes":0,"hostNotDrainingMs":1402,"#,
            r#""hostDrainingAgainMs":8137,"notDrainingCount":1},"#,
            r#""identity":{"baseMac":"a0:f2:62:87:b4:8c"}}}}"#,
        );
        let caps = LINK_MONITOR
            .regex()
            .captures(beat)
            .expect("a latched-then-resumed link parses");
        assert_eq!(&caps["link"], "link");
        assert_eq!(&caps["not_draining_ms"], "1402");
        assert_eq!(&caps["draining_again_ms"], "8137");
        assert_eq!(&caps["count"], "1");

        // The same heartbeat also carries the heap ledger and the frame
        // counter, which is how the negative control proves the device kept
        // rendering throughout the silence rather than sitting wedged.
        let beat_caps = HEARTBEAT.regex().captures(beat).expect("heartbeat parses");
        assert_eq!(&beat_caps["frame_count"], "9670");

        // A link that has been drained since boot sends neither stamp, so the
        // series is empty rather than zero — presence IS the claim.
        let quiet = beat.replace(
            r#""stalePartialFlushes":0,"hostNotDrainingMs":1402,"hostDrainingAgainMs":8137,"#,
            r#""stalePartialFlushes":0,"#,
        );
        assert!(
            LINK_MONITOR.regex().captures(&quiet).is_none(),
            "a link that never latched must not match: {quiet}"
        );

        // Latched but not yet recovered is also not a transition: the second
        // stamp is absent, and half a silence is not a measurement.
        let still_quiet = beat.replace(r#""hostDrainingAgainMs":8137,"#, "");
        assert!(
            LINK_MONITOR.regex().captures(&still_quiet).is_none(),
            "a link that never came back must not match: {still_quiet}"
        );
    }

    /// The negative control does not expect a hello: it went out before the
    /// reader attached, into a port nobody had open.
    #[test]
    fn the_negative_control_watches_the_board_only_after_a_wait() {
        let p = find_payload("usb-negative-control").unwrap();
        assert_eq!(p.capture, Capture::FlashThenOpenAfter(8));
        let names: Vec<_> = p.series.iter().map(|s| s.name).collect();
        assert_eq!(names, vec!["heartbeat", "stack-heartbeat", "link-monitor"]);
        assert!(
            !names.contains(&"hello"),
            "the hello is dropped by the state this payload measures"
        );
        // The shipped image, flash-backed: the product's own link.
        assert_eq!(p.firmware_features, &["server", "radio"]);

        // Every other payload is watched from its first byte.
        for other in ALL_PAYLOADS.iter().filter(|o| o.name != p.name) {
            assert_eq!(other.capture, Capture::Monitor, "{}", other.name);
        }
    }

    #[test]
    fn lookup_reports_the_known_set() {
        assert!(find_payload("shader-compile-stress").is_ok());
        let err = find_payload("nope").unwrap_err().to_string();
        assert!(err.contains("shader-compile-stress"), "{err}");
    }
}
