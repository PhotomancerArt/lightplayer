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
    /// The payload's subject is machine **state**, not console output: the
    /// capture is the emulator's own report rather than anything the device
    /// said, and the marker is a line of that report.
    ///
    /// M6's `usb-host-absent` is the case. With no cable the device says
    /// nothing at all — that *is* the finding — so there is no console line
    /// to stop on and no line to record. What the run has to say is read out
    /// of the guest's memory with `--probe`, and the run ends at its
    /// deadline. So no `--exit-on` is passed (there is nothing on a console
    /// to match) and the marker is the name of the last probe: the payload
    /// got where it was going when the last question was answered.
    State(&'static str),
}

impl Sentinel {
    pub const fn marker(self) -> &'static str {
        match self {
            Self::Done(m) | Self::Ready(m) | Self::State(m) => m,
        }
    }

    /// The marker to pass as `--exit-on`, or `None` for a payload that ends
    /// some other way: `Ready` serves for ever, `State` has no console to
    /// match on.
    pub const fn exit_on(self) -> Option<&'static str> {
        match self {
            Self::Done(m) => Some(m),
            Self::Ready(_) | Self::State(_) => None,
        }
    }

    /// The `done_marker` a matching `fw-checks` registry entry must declare.
    /// `Ready` payloads never finish and `State` payloads print nothing, so
    /// theirs is `None`.
    pub const fn fw_checks_done_marker(self) -> Option<&'static str> {
        match self {
            Self::Done(m) => Some(m),
            Self::Ready(_) | Self::State(_) => None,
        }
    }
}

/// Which link carries the payload's output.
///
/// A host-side property in the same sense as [`Capture`]: on silicon the
/// shipped image has exactly one link — the USB-Serial-JTAG the product
/// ships — and this says whether an *emulated* configuration serves that
/// link or is handed the `spike_uart0_link` workaround instead.
///
/// It exists because until M6 every emulated run got `spike_uart0_link`
/// unconditionally (`RunRequest::features`, "neither emulator has a USB
/// host"). That is now false for `lp-emu:*`, and it mattered: a UART0-link
/// image on one side and a USB-link image on the other compares two link
/// drivers' allocations, not two machines (DD30).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Link {
    /// The product's own link. On `lp-emu:*` the machine models the host
    /// (M6), the capture is the USB byte stream — the same bytes a silicon
    /// reader on the port sees — and no `spike_uart0_link` is built.
    UsbSerialJtag,
    /// The spike's UART0 workaround: the host link moved to UART0 by a cargo
    /// feature, so an emulator with no USB host model can still be served.
    /// `esp-emu:*` has no choice (its USB model is a lie — spike report §4);
    /// `shader-compile-stress` stays here on purpose, because its committed
    /// transcripts are of that image and a transcript is never re-baselined
    /// to suit a later idea.
    Uart0Spike,
}

impl Link {
    pub const fn slug(self) -> &'static str {
        match self {
            Self::UsbSerialJtag => "usb-serial-jtag",
            Self::Uart0Spike => "uart0-spike",
        }
    }
}

/// The initial host state an emulated run starts in, and the scripted host
/// actions that follow.
///
/// The script is `--usb-script`'s grammar (M6 P3): absolute emulated
/// milliseconds, one command per line. It is the deterministic twin of the
/// control socket — a socket is host time and has no place in a transcript.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostPlan {
    /// `--usb-host`: `absent`, `attached` (a cable and an open port) or
    /// `attached-idle` (a cable, port closed).
    pub host: &'static str,
    /// `--usb-script` content, or `""` for a run nobody touches.
    pub script: &'static str,
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
    /// When the host side opens the port, on a silicon run. See [`Capture`].
    pub capture: Capture,
    /// Which link carries the output. See [`Link`].
    pub link: Link,
    /// What the emulated configurations build instead, and only because
    /// something is not modelled yet.
    ///
    /// `None` — the honest default — means one image on every configuration,
    /// which is what makes two transcripts of a payload comparable at all.
    /// The one exception is `usb-negative-control`, whose image is
    /// flash-backed: on `lp-emu:*` a flash-backed boot stops at `SPIN
    /// SPI1+0x000 cmd` until M4 lands the flash controller (DD23), so the
    /// emulator twin runs the `memory_fs` variant and **says so in the PR and
    /// in the sidecar**. M6 P5 re-runs it on the flash-backed bytes.
    pub emulator_features: Option<&'static [&'static str]>,
    /// The host's side of an emulated run: where it starts and what happens
    /// to it. `None` for a payload that makes no claim about the host.
    pub host_plan: Option<HostPlan>,
    /// Statics to read out of the guest at a given emulated millisecond
    /// (`--probe <symbol>@<ms>`), for a payload whose subject is state the
    /// device never gets to say out loud.
    pub probes: &'static [(&'static str, u64)],
    /// The emulated seconds a scenario needs, when its subject is a timeline
    /// rather than a line. `None` leaves it to the runner's `--timeout-secs`.
    pub run_secs: Option<u64>,
    /// Does the payload's subject start from an **erased flash chip**?
    ///
    /// On an emulated configuration this is the default and needs no saying:
    /// `--flash` is blank unless a file is named, so every run starts on a
    /// fresh part. On silicon it is the difference between a measurement and
    /// a coincidence — the desk board keeps whatever the last sitting left
    /// on it, so a flash-backed payload that expects to format `lpfs` and
    /// report `bootCount 1` gets, on a board that already holds a project,
    /// an auto-loaded project and a boot ledger three deep instead. An
    /// `espflash erase-flash` before the write is what makes the two sides
    /// the same experiment (M6 P5, DD30 on flash-backed bytes).
    ///
    /// Deliberately not mirrored in `fw-checks`, for [`Capture`]'s reason:
    /// the image is the same bytes either way, and what differs is what the
    /// operator's side did to the board first.
    pub fresh_chip: bool,
    /// Why silicon cannot record this payload, when it cannot.
    ///
    /// `usb-host-absent` is the case, and the reason is the payload: an
    /// absent host records nothing, because recording is what a host does.
    /// The runner refuses a silicon run with this sentence rather than
    /// producing an empty file and calling it evidence.
    pub emulator_only: Option<&'static str>,
}

impl Payload {
    pub fn class_of(&self, record: &str, field: &str) -> Option<FieldClass> {
        self.fields
            .iter()
            .find(|f| f.record == record && f.field == field)
            .map(|f| f.class)
    }

    /// The firmware features this payload builds on `configuration`.
    pub fn features_for(&self, emulated: bool) -> &'static [&'static str] {
        match (emulated, self.emulator_features) {
            (true, Some(f)) => f,
            _ => self.firmware_features,
        }
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
/// **Keyed on the heartbeat's own five-second tick**, so the five-second
/// sample is compared with the five-second sample.
///
/// It used to be keyed on the literal message name, which made
/// `Transcript::series`'s last-writes rule pick each run's *last* heartbeat
/// — fine while every capture stopped in the same place, and wrong the first
/// time one did not. M6 P4 hit it: sitting 1's desk capture of `boot-idle`
/// ran on to fifteen seconds while ours stops at the payload's sentinel at
/// five, so the DD30 arbitration was comparing our 5 s heap sample with
/// silicon's 15 s one and calling the 112 B between them a disagreement. On
/// this payload the difference between two heartbeats of one boot is over a
/// hundred bytes — silicon's own three read 266,392 / 266,496 / 266,388 — so
/// which sample is compared is not a detail.
///
/// The key is the **seconds**, not `uptime_ms`, and the three trailing digits
/// are matched and dropped. A millisecond is a timing figure and two
/// configurations do not share one: `t1` reports `"uptime_ms":5000` and `t2`
/// reports `5001` for the same heartbeat of the same boot, which as a key
/// would make the two grades incomparable — the opposite of what a key is
/// for.
///
/// A run with more heartbeats than the other now says so, as a structural
/// problem naming the samples that appear on one side only. That is the
/// honest report: two captures of different lengths are comparable where
/// they overlap and nowhere else.
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
        r#""uptime_ms":(?<uptime_s>\d+)\d{3},"memory":\{"freeBytes":(?<free_bytes>\d+),"#,
        r#""usedBytes":(?<used_bytes>\d+),"totalBytes":(?<total_bytes>\d+),"#,
        r#""largestFreeBlock":(?<largest_free_block>\d+)\},"#,
        r#""recovery":\{"level":"(?<recovery_level>[^"]+)","resetReason":"(?<reset_reason>[^"]+)","#,
        r#""bootCount":(?<boot_count>\d+)"#,
    ),
    key: "uptime_s",
    fields: &[
        ("free_bytes", FieldClass::Memory),
        ("used_bytes", FieldClass::Memory),
        ("total_bytes", FieldClass::Memory),
        ("largest_free_block", FieldClass::Timing),
        ("fps_avg", FieldClass::Timing),
        ("frame_count", FieldClass::Timing),
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
/// # `notDrainingCount: 1` on its own is not evidence of a host
///
/// Measured on our own machine with **no host at all** (M6 P1b, gate G1b-4,
/// `tests/host_absent.rs`): `notDrainingCount` is 1, `hostNotDrainingMs` is
/// 893, and `hostDrainingAgainMs` is absent. The monitor starts optimistic
/// and the enumeration verdict takes three polls, while one blocked write
/// costs 250 ms and holds io_task's loop for all of it — so two writes are
/// attempted and time out before "no cable" is ever concluded. The third poll
/// then declares the link unenumerated, which resets the latch and gates
/// every write after it.
///
/// The M6 discovery's "absent" row predicted zero and is wrong. So the
/// discriminator is the **pair**, which is why this pattern requires both
/// stamps: absent latches once and never recovers; attached-but-unread
/// latches once and *does* recover the moment somebody opens the port.
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

/// A static read out of the guest's memory at a chosen emulated moment
/// (`lp-emu-esp32c6 --probe <symbol>@<ms>`), as the machine prints it.
///
/// ```text
/// probe cyc=800000000 us=5000000 esp_println::serial_jtag_printer::TIMED_OUT @ 0x4080f2a4 = 0x00000001
/// ```
///
/// This is the vehicle for a payload whose subject is a state the device
/// cannot report at the time. With no host attached the firmware writes
/// nothing at all — that is the whole finding — so the transcript is not
/// what the device said, it is what the machine could see of it. The address
/// is matched and never captured: it moves with every build (DD45), and what
/// the claim is about is the value.
///
/// Every value here is graded `UsbSerialJtag`, which is **hard** in replay,
/// including the two that hold milliseconds. That is deliberate and it is
/// not a timing gate in disguise: on this payload
/// `HOST_DRAINING_AGAIN_MS` reads `0xffffffff`, the monitor's "never this
/// boot" sentinel, and "never" is precisely the claim the negative control
/// exists to make (DD38: the stamp *pair* is the discriminator, never the
/// count). `HOST_NOT_DRAINING_MS` is the other half of the pair, and its
/// value is the firmware's own clock on one machine at one time grade — a
/// figure that moves is a finding to report, never a number to tune.
pub static PROBE: SeriesSpec = SeriesSpec {
    name: "probe",
    description: "a static read out of the guest at a chosen emulated moment",
    pattern: concat!(
        r"^probe cyc=(?<cyc>\d+) us=(?<us>\d+) (?<symbol>\S+) ",
        r"@ 0x[0-9a-f]{8} = 0x(?<value>[0-9a-f]{8})$",
    ),
    key: "symbol",
    fields: &[
        ("value", FieldClass::UsbSerialJtag),
        ("us", FieldClass::Timing),
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
        capture: Capture::Monitor,
        // The one payload that stays on the spike link, and not because the
        // link is right: its silicon, esp-emu and lp-emu transcripts are all
        // of that image, and `scripts/emu/build-reference-image.sh` pins the
        // recipe. Moving it to the real link would invalidate three committed
        // captures to gain nothing this milestone measures.
        link: Link::Uart0Spike,
        emulator_features: None,
        host_plan: None,
        probes: &[],
        run_secs: None,
        fresh_chip: false,
        emulator_only: None,
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
        capture: Capture::Monitor,
        link: Link::UsbSerialJtag,
        emulator_features: None,
        host_plan: None,
        probes: &[],
        run_secs: None,
        fresh_chip: false,
        emulator_only: None,
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
        capture: Capture::Monitor,
        link: Link::UsbSerialJtag,
        emulator_features: None,
        host_plan: None,
        probes: &[],
        run_secs: None,
        fresh_chip: false,
        emulator_only: None,
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
        capture: Capture::Monitor,
        link: Link::Uart0Spike,
        emulator_features: None,
        host_plan: None,
        probes: &[],
        run_secs: None,
        fresh_chip: false,
        emulator_only: None,
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
        capture: Capture::Monitor,
        // The product's own link, on every configuration that can serve it.
        // This is DD26/DD30's arbitration: silicon's capture of this image
        // came off the USB port, and only an emulated run of the same bytes
        // over the same link compares two machines rather than two link
        // drivers.
        link: Link::UsbSerialJtag,
        emulator_features: None,
        // A cable in and an application reading, from the first byte — the
        // same state `espflash --monitor` puts the board in.
        host_plan: Some(HostPlan {
            host: "attached",
            script: "",
        }),
        probes: &[],
        run_secs: None,
        fresh_chip: false,
        emulator_only: None,
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
        // Nothing is sent to it: the measurement is what the device says
        // while nobody is listening.
        host_script: None,
        // The recovery stamp itself — the payload's own claim, and the only
        // marker both sides can reach.
        //
        // P1b's stack heartbeat cannot be one, and finding out why is the
        // M6 P4 finding that corrected this entry. `stack_probe` reports
        // "when the high-water mark has GROWN since the last report", so the
        // first report is at 5 s and there may never be another. On a link
        // nobody was reading, that one report went into a closed port and is
        // gone for ever: our own emulator twin runs 20 s and prints exactly
        // one `[stack]` line, at 5 s, into the dark. A sentinel that depends
        // on a firmware stack growing later is a sentinel that depends on
        // luck.
        //
        // `hostDrainingAgainMs` appears only once the monitor has resumed,
        // in the first heartbeat delivered after the port opens, and a
        // `--until`/`--exit-on` match runs on to that line's newline — so the
        // whole heartbeat is captured, `link` object and all.
        sentinel: Sentinel::Done("\"hostDrainingAgainMs\""),
        record_kinds: &[],
        mask_set: "boot-idle",
        fields: &[],
        // No `HELLO`. The hello goes out at server start, seconds before the
        // reader attaches, into a port nobody has open — it is dropped, and
        // expecting it here would make every run of this payload fail for the
        // reason the payload exists to demonstrate. No `STACK_HEARTBEAT`
        // either, for the reason above: this payload cannot see one.
        series: &[&HEARTBEAT, &LINK_MONITOR],
        // Boot ≈ 1 s, the hello plus two 250 ms write timeouts ≈ +0.6 s, and
        // one 5 s heartbeat interval of margin so the wait cannot land inside
        // the transition it is trying to observe.
        capture: Capture::FlashThenOpenAfter(8),
        link: Link::UsbSerialJtag,
        // The one payload whose emulator twin is a different image, and the
        // reason is a peripheral, not a preference: a flash-backed boot stops
        // at `SPIN SPI1+0x000 cmd` on `lp-emu:*` until M4 lands the flash
        // controller (DD23). M6 P5 re-runs it on the flash-backed bytes.
        emulator_features: Some(&["server", "radio", "memory_fs"]),
        // The emulator twin of [`Capture::FlashThenOpenAfter`]: a cable in
        // and the port closed from boot, opened at the same eight seconds.
        // `the_negative_controls_two_halves_agree` holds the two numbers
        // together so a change to one is a compile-time-visible change to a
        // pair.
        host_plan: Some(HostPlan {
            host: "attached-idle",
            script: "8000  open\n",
        }),
        probes: &[],
        // Latch at ~0.9 s, open at 8 s, the recovery on the first probe of
        // io_task's 2 s grid after it, and the 10 s heartbeat carrying the
        // pair, which stops the run.
        run_secs: Some(12),
        fresh_chip: false,
        emulator_only: None,
    },
    Payload {
        name: "usb-detach-reattach",
        display_name: "The cable out mid-session and back in",
        fw_check_slug: "usb-detach-reattach",
        // The shipped image minus flash, like every M6 emulator scenario
        // until M4: what is under test is the link, and the filesystem has
        // nothing to say about it.
        firmware_features: &["server", "radio", "memory_fs"],
        fw_checks_feature: None,
        emits_header: false,
        // `host_script` is the UART0 walk's wire conversation (M4); what
        // this payload drives is the cable, in `host_plan`.
        host_script: None,
        // Not a stack heartbeat: the one at 5 s crosses before the unplug and
        // the next is at 15 s, five seconds after the scenario has anything
        // left to say. A whole heartbeat arriving *after* the re-open is the
        // stronger claim and the natural end of the timeline — the session
        // recovered, and the proof is a frame that crossed it.
        sentinel: Sentinel::Done("\"uptime_ms\":10000"),
        record_kinds: &[],
        mask_set: "boot-idle",
        fields: &[],
        series: &[&HELLO, &HEARTBEAT, &STACK_HEARTBEAT],
        capture: Capture::Monitor,
        link: Link::UsbSerialJtag,
        emulator_features: None,
        // `scripts/device-scenarios/s7-unplug-mid-op.json`'s shape, which has
        // wanted a device-side trace since M2: read from boot, unplugged at
        // 6 s, back in at 9 s, re-opened at 9.5 s.
        host_plan: Some(HostPlan {
            host: "attached",
            // The initial state is `--usb-host attached` — a cable in and an
            // application reading — so the script carries only what changes.
            script: "\
6000  detach
9000  attach
9500  open
",
        }),
        probes: &[],
        run_secs: Some(12),
        fresh_chip: false,
        emulator_only: None,
    },
    Payload {
        name: "usb-host-absent",
        display_name: "The shipped image with no cable at all",
        fw_check_slug: "usb-host-absent",
        firmware_features: &["server", "radio", "memory_fs"],
        fw_checks_feature: None,
        emits_header: false,
        // `host_script` is the UART0 walk's wire conversation (M4); what
        // this payload drives is the cable, in `host_plan`.
        host_script: None,
        // Nothing is printed, because nothing can be: see [`Sentinel::State`].
        sentinel: Sentinel::State("link_counters::NOT_DRAINING_COUNT"),
        record_kinds: &[],
        mask_set: "normalize",
        fields: &[],
        series: &[&PROBE],
        capture: Capture::Monitor,
        link: Link::UsbSerialJtag,
        emulator_features: None,
        host_plan: Some(HostPlan {
            host: "absent",
            script: "",
        }),
        // esp-println's own latch, then the connection monitor's pair and its
        // count, at 5 s — after the boot has done everything it is going to
        // do into the void. Order matters: the last one named is the payload's
        // sentinel.
        probes: &[
            ("esp_println::serial_jtag_printer::TIMED_OUT", 5_000),
            (
                "fw_esp32_common::serial::link_counters::HOST_NOT_DRAINING_MS",
                5_000,
            ),
            (
                "fw_esp32_common::serial::link_counters::HOST_DRAINING_AGAIN_MS",
                5_000,
            ),
            (
                "fw_esp32_common::serial::link_counters::NOT_DRAINING_COUNT",
                5_000,
            ),
        ],
        run_secs: Some(6),
        fresh_chip: false,
        emulator_only: Some(
            "an absent host records nothing: recording IS what a host does. On silicon this \
             payload is `unplug the board and watch the port that is no longer there`, which is \
             not a capture — it is an empty file. The state it asks about is read out of the \
             guest's memory, which only an emulator can do",
        ),
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
        // Watched from the first byte, like every other boot payload.
        capture: Capture::Monitor,
        // The product's own link, on both sides (M6): the flash-backed twin
        // of `boot-idle`, and P5's closure of DD30 on flash-backed bytes.
        link: Link::UsbSerialJtag,
        emulator_features: None,
        host_plan: Some(HostPlan {
            host: "attached",
            script: "",
        }),
        probes: &[],
        run_secs: None,
        // Both `[FS]` lines its `fs-mount` series asserts are the lines a
        // blank part produces. On silicon that means an erase before the
        // write, or the board's leftover filesystem is in the measurement.
        fresh_chip: true,
        emulator_only: None,
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
        // The walk needs the port open from boot: its first request waits
        // for `[RECOVERY] boot complete`, which a late reader would miss.
        capture: Capture::Monitor,
        // M4 recorded the walk over UART0, because that is the link an
        // emulator had when it was written; its committed transcript is of
        // that image. M6 P5 runs the same conversation over the USB link —
        // which is what silicon's own §11.3 capture was — and that is the
        // like-for-like comparison. Until then, the transcript decides.
        link: Link::Uart0Spike,
        emulator_features: None,
        host_plan: None,
        probes: &[],
        run_secs: None,
        // Both `[FS]` lines its `fs-mount` series asserts are the lines a
        // blank part produces. On silicon that means an erase before the
        // write, or the board's leftover filesystem is in the measurement.
        fresh_chip: true,
        emulator_only: None,
    },
    Payload {
        name: "upload-walk-usb",
        display_name: "Project upload walk (examples/basic), over the USB link",
        fw_check_slug: "upload-walk-usb",
        // The same image, the same conversation, the same series — and the
        // link the product ships. That is the whole difference, and it is the
        // reason for a second payload rather than a flag: `upload-walk`'s
        // committed transcript is of the `spike_uart0_link` image, and a
        // transcript is never re-baselined to suit a later idea. Two payloads,
        // two images, one script, and the pair is the comparison.
        //
        // Silicon's own §11.3 walk went over USB-Serial-JTAG (through
        // `usb-tcp-bridge.py` on the board's own port), so THIS is the
        // like-for-like run and M4's was the proxy.
        firmware_features: &["server", "radio"],
        fw_checks_feature: None,
        emits_header: false,
        host_script: Some("lp-emu/esp/lp-emu-esp32c6/walks/examples-basic.script"),
        sentinel: Sentinel::Done("[shader-node] compilation succeeded"),
        record_kinds: &[],
        mask_set: "boot-idle",
        fields: &[],
        series: &[&HELLO, &FS_MOUNT, &FS_WRITE, &LOAD_GATE, &SHADER_COMPILE],
        capture: Capture::Monitor,
        link: Link::UsbSerialJtag,
        emulator_features: None,
        // A cable in and an application reading, from boot. The walk's first
        // request waits for `[RECOVERY] boot complete`, which a host that
        // attached later would never see.
        host_plan: Some(HostPlan {
            host: "attached",
            script: "",
        }),
        probes: &[],
        run_secs: None,
        fresh_chip: true,
        emulator_only: None,
    },
    Payload {
        name: "meteor-walk-usb",
        display_name: "Project upload walk (examples/meteor), over the USB link",
        fw_check_slug: "meteor-walk-usb",
        // The spike report §11.2's ledger, which is the one heap comparison
        // the desk made on a *loaded* device rather than an idle one: `[mem]
        // load_project after` 216,056 B, steady `freeBytes` 152,320 and
        // `[stack] high-water` 35,768 B. Silicon's column came over USB-SJ
        // through the bridge, so this payload is the like-for-like twin of
        // it; `upload-walk-usb` is the same walk on the smaller project.
        firmware_features: &["server", "radio"],
        fw_checks_feature: None,
        emits_header: false,
        host_script: Some("lp-emu/esp/lp-emu-esp32c6/walks/examples-meteor.script"),
        // `Ready`, not `Done`, and the reason is the measurement. §11.2's
        // "steady" figures are read from a heartbeat with the project
        // **loaded and running** — silicon's came from the 20-60 s window —
        // so the run has to keep going after the walk finishes rather than
        // stop at the first line that matches. `Ready` passes no `--exit-on`
        // and the marker is what says the payload got where it was going:
        // the first heartbeat that names the loaded project.
        //
        // The obvious alternative, stopping at `[stack] heartbeat:
        // high-water`, would end the run at five seconds — `stack_probe`
        // reports on growth, and its first report is the idle one long
        // before the load.
        sentinel: Sentinel::Ready("\"path\":\"/projects/Meteor\""),
        record_kinds: &[],
        mask_set: "boot-idle",
        fields: &[],
        series: &[
            &HELLO,
            &FS_MOUNT,
            &FS_WRITE,
            &LOAD_GATE,
            &SHADER_COMPILE,
            &HEARTBEAT,
            &STACK_HEARTBEAT,
        ],
        capture: Capture::Monitor,
        link: Link::UsbSerialJtag,
        emulator_features: None,
        host_plan: Some(HostPlan {
            host: "attached",
            script: "",
        }),
        probes: &[],
        // The load, two compiles, and then TWO heartbeats past it. The
        // first heartbeat after a load is not the steady one — P4 measured
        // a ~108 B live allocation that comes and goes between samples, and
        // it is 104 B low at twenty seconds — so the figure §11.2 calls
        // steady is the second, at twenty-five.
        run_secs: Some(26),
        fresh_chip: true,
        emulator_only: None,
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
        assert_eq!(
            &caps["uptime_s"], "5",
            "the key is the tick, not the millisecond"
        );
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
        assert_eq!(names, vec!["heartbeat", "link-monitor"]);
        assert!(
            !names.contains(&"hello"),
            "the hello is dropped by the state this payload measures"
        );
        assert!(
            !names.contains(&"stack-heartbeat"),
            "the stack probe reports only when the high-water grows, so the one report \
             this payload could have seen went into a closed port"
        );
        // The shipped image, flash-backed: the product's own link.
        assert_eq!(p.firmware_features, &["server", "radio"]);

        // Every other payload is watched from its first byte.
        for other in ALL_PAYLOADS.iter().filter(|o| o.name != p.name) {
            assert_eq!(other.capture, Capture::Monitor, "{}", other.name);
        }
    }

    /// The negative control is one measurement expressed twice — a silicon
    /// wait and an emulated `open` — and the two have to be the same number
    /// or the twin is not a twin.
    #[test]
    fn the_negative_controls_two_halves_agree() {
        let p = find_payload("usb-negative-control").unwrap();
        let Capture::FlashThenOpenAfter(secs) = p.capture else {
            panic!("the negative control is watched after a wait");
        };
        let plan = p.host_plan.expect("the emulator twin has a host plan");
        assert_eq!(plan.host, "attached-idle", "a cable in, the port closed");
        assert!(
            plan.script.contains(&format!("{}  open", secs * 1_000)),
            "the emulated open is at {secs} s, the same wait silicon takes: {:?}",
            plan.script
        );
    }

    /// The emulator twin's image differs from silicon's, and exactly one
    /// payload is allowed that — with a reason that names what is missing.
    #[test]
    fn only_the_negative_control_builds_a_different_image_on_an_emulator() {
        for p in ALL_PAYLOADS {
            let differs = p.emulator_features.is_some();
            assert_eq!(
                differs,
                p.name == "usb-negative-control",
                "payload `{}` builds a different image on an emulator",
                p.name
            );
            assert_eq!(p.features_for(false), p.firmware_features);
        }
        let p = find_payload("usb-negative-control").unwrap();
        assert_eq!(p.features_for(true), &["server", "radio", "memory_fs"]);
    }

    /// `usb-host-absent` is the first payload silicon cannot record, and the
    /// registry says why rather than leaving a runner to fail confusingly.
    #[test]
    fn the_absent_host_payload_is_emulator_only_and_says_why() {
        let p = find_payload("usb-host-absent").unwrap();
        let why = p.emulator_only.expect("a reason, not a flag");
        assert!(why.contains("records nothing"), "{why}");
        assert!(matches!(p.sentinel, Sentinel::State(_)));
        assert_eq!(p.sentinel.exit_on(), None, "there is no console to match");
        // The sentinel is the last probe: the run got where it was going when
        // the last question was answered.
        let last = p.probes.last().expect("probes").0;
        assert!(
            last.ends_with(p.sentinel.marker()),
            "`{last}` does not end with the sentinel `{}`",
            p.sentinel.marker()
        );
        for other in ALL_PAYLOADS.iter().filter(|o| o.name != p.name) {
            assert!(other.emulator_only.is_none(), "{}", other.name);
        }
    }

    /// The probe series against a line the machine actually printed
    /// (`host_absent`'s own symbols, `--probe …@5000`).
    #[test]
    fn the_probe_series_parses_a_line_the_machine_prints() {
        let line = "probe cyc=800000000 us=5000000 \
                    esp_println::serial_jtag_printer::TIMED_OUT @ 0x4080f2a4 = 0x00000001";
        let caps = PROBE.regex().captures(line).expect("the probe line parses");
        assert_eq!(
            &caps["symbol"],
            "esp_println::serial_jtag_printer::TIMED_OUT"
        );
        assert_eq!(&caps["value"], "00000001");
        assert_eq!(&caps["us"], "5000000");
        // The address moves with every build (DD45) and is never captured.
        let names: Vec<_> = PROBE.regex().capture_names().flatten().collect();
        assert!(!names.contains(&"address"));

        // The "never this boot" sentinel, which is this payload's claim.
        let never = "probe cyc=800000000 us=5000000 \
                     fw_esp32_common::serial::link_counters::HOST_DRAINING_AGAIN_MS \
                     @ 0x4080f2b0 = 0xffffffff";
        assert_eq!(
            &PROBE.regex().captures(never).expect("parses")["value"],
            "ffffffff"
        );

        // A symbol the machine could not find is not a sample.
        assert!(
            PROBE
                .regex()
                .captures("probe cyc=800000000 NOPE: no such symbol in the app or the ROM")
                .is_none()
        );
    }

    /// The link is a payload property now, and only the two payloads whose
    /// transcripts are of a spike-link image carry it.
    #[test]
    fn the_shipped_scenarios_speak_the_shipped_link() {
        for name in [
            "boot-idle",
            "usb-negative-control",
            "usb-detach-reattach",
            "usb-host-absent",
        ] {
            assert_eq!(
                find_payload(name).unwrap().link,
                Link::UsbSerialJtag,
                "{name}"
            );
        }
        assert_eq!(
            find_payload("shader-compile-stress").unwrap().link,
            Link::Uart0Spike,
            "its three committed transcripts are of that image"
        );
    }

    #[test]
    fn lookup_reports_the_known_set() {
        assert!(find_payload("shader-compile-stress").is_ok());
        let err = find_payload("nope").unwrap_err().to_string();
        assert!(err.contains("shader-compile-stress"), "{err}");
    }
}
