use super::{FwCheck, FwCheckTarget};

pub const FW_CHECK_JSON_PREFIX: &str = "[fw-check-json] ";

#[derive(Clone, Copy, Debug)]
pub struct FwCheckConfig {
    pub check: FwCheck,
    pub display_name: &'static str,
    /// The cargo features on `fw-esp32c6` that build this check, on top of
    /// the crate's defaults. Usually one `test_*` feature; `boot-idle` is the
    /// exception — it is the shipped image itself, so its list is the image's.
    pub firmware_features: &'static [&'static str],
    pub done_marker: Option<&'static str>,
    pub trace_slug: &'static str,
    pub supported_targets: &'static [FwCheckTarget],
    pub emits_records: bool,
    /// Does the image print the in-band `[fw-checks-header] ` line?
    ///
    /// Every check with a module in this crate does, through
    /// [`crate::write_header`]. `boot-idle` does not: there is no check module
    /// to print it — the payload *is* the product image — so its provenance
    /// comes from the transcript's sidecar alone.
    pub emits_header: bool,
}

impl FwCheckConfig {
    pub const fn slug(self) -> &'static str {
        self.check.slug()
    }

    pub fn supports_target(self, target: FwCheckTarget) -> bool {
        self.supported_targets.contains(&target)
    }
}

const ESP32_ONLY: &[FwCheckTarget] = &[FwCheckTarget::Esp32C6];
const ESP32_AND_EMU: &[FwCheckTarget] = &[FwCheckTarget::Esp32C6, FwCheckTarget::FwEmu];

pub const ALL_CHECKS: &[FwCheckConfig] = &[
    FwCheckConfig {
        check: FwCheck::ShaderCompileStress,
        display_name: "Incremental shader compile stress",
        firmware_features: &["test_shader_compile_incremental"],
        done_marker: Some("[inc-shader-compile] === DONE ==="),
        trace_slug: "inc-shader-compile-stress",
        supported_targets: ESP32_AND_EMU,
        emits_records: true,
        emits_header: true,
    },
    FwCheckConfig {
        check: FwCheck::GpioCalibrate,
        display_name: "Host-driven GPIO square-wave calibration",
        firmware_features: &["test_gpio_calibrate"],
        // The payload serves until the host stops asking; it never finishes.
        // Its sentinel is the readiness line `CAL READY target=`, which
        // lp-emu-validate's payload registry carries as `Sentinel::Ready`.
        done_marker: None,
        trace_slug: "gpio-calibrate",
        supported_targets: ESP32_ONLY,
        emits_records: false,
        emits_header: true,
    },
    FwCheckConfig {
        check: FwCheck::UartBridge,
        display_name: "Transparent USB-Serial-JTAG <-> UART0 bridge",
        firmware_features: &["test_uart_bridge"],
        // The bridge serves until it is unplugged; it never finishes. Its
        // sentinel is the readiness line `UART-BRIDGE READY `, which
        // lp-emu-validate's payload registry carries as `Sentinel::Ready`.
        done_marker: None,
        trace_slug: "uart-bridge",
        supported_targets: ESP32_ONLY,
        emits_records: false,
        emits_header: true,
    },
    FwCheckConfig {
        check: FwCheck::JitMathPerf,
        display_name: "JIT Q32 math perf",
        firmware_features: &["test_jit_math_perf"],
        done_marker: Some("[jit-math-perf] === DONE ==="),
        trace_slug: "jit-math-perf",
        supported_targets: ESP32_ONLY,
        emits_records: true,
        emits_header: true,
    },
    FwCheckConfig {
        check: FwCheck::BootIdle,
        display_name: "Shipped image to the idle loop",
        // The shipped-image walk as a payload (vision Q1): no check module,
        // no `test_*` feature — the product image itself, built with the
        // firmware's own "no flash" switch on top of the defaults
        // (`esp32c6,server,radio`). `memory_fs` is there because the
        // flash-backed boot mounts `lpfs` through the SPI1 controller, which
        // is M4 of the esp-emulator plan (DD23): on `lp-emu:esp32c6:*` the
        // flash-backed image stops at `SPIN SPI1+0x000 cmd` at 11 ms, and the
        // §5.1 figures it would print (`freeBytes 265392`,
        // `[stack] high-water 11844 B of 71328 B`, the `[FS]` mount pair) are
        // recorded as expected at M4 rather than compared now.
        //
        // An emulated configuration adds `spike_uart0_link` on top of this
        // list, because neither emulator has a USB host to serve the shipped
        // link over (`RunRequest::features`, spike report §4, §5.1).
        firmware_features: &["server", "radio", "memory_fs"],
        // The first stack heartbeat, at 5 s of uptime. It is the last line of
        // the boot the payload exists to observe, so a run must be at least
        // 5.5 s long to reach it.
        done_marker: Some("[stack] heartbeat: high-water"),
        trace_slug: "boot-idle",
        supported_targets: ESP32_ONLY,
        emits_records: false,
        // No check module runs in this image, so nothing prints the in-band
        // header; the transcript's sidecar is the whole provenance.
        emits_header: false,
    },
    FwCheckConfig {
        check: FwCheck::BootIdleFlash,
        display_name: "Shipped image to the idle loop, from flash",
        // The same product image as `boot-idle` **without** `memory_fs`: the
        // one that mounts `lpfs` from the flash chip through the mask ROM's
        // `esp_rom_spiflash_*` path. That is what M4 built, and what its
        // first gate compares — spike report §5.1's `freeBytes 265392`,
        // `largestFreeBlock 199173` and `[stack] high-water 11844 B of
        // 71328 B`, plus the `[FS] Mount failed … Formatted and mounted`
        // pair `boot-idle` cannot produce.
        firmware_features: &["server", "radio"],
        done_marker: Some("[stack] heartbeat: high-water"),
        trace_slug: "boot-idle-flash",
        supported_targets: ESP32_ONLY,
        emits_records: false,
        emits_header: false,
    },
    FwCheckConfig {
        check: FwCheck::UploadWalk,
        display_name: "Project upload walk (examples/basic)",
        // The same flash-backed product image as `boot-idle-flash`, with a
        // host on the other end: the thirteen wire frames `lp-cli upload
        // examples/basic` sends. On an emulated configuration the host is a
        // committed `--uart0-script`
        // (`lp-emu/esp/lp-emu-esp32c6/walks/examples-basic.script`); on
        // silicon it is the client itself, over a port.
        firmware_features: &["server", "radio"],
        // The last frame of the `projectRead` answer. The shader compile
        // used to be the marker, because it was the last line before the
        // walk needed M5's RMT model to go on — `Ws281xOutput::write` waited
        // for an interrupt an accept block never raised. With the channel
        // model the frame completes, request 12 is answered, and the walk's
        // end is where the conversation ends (M5 P3, DD40).
        done_marker: Some("\"id\":12,\"seq\":2,"),
        trace_slug: "upload-walk",
        supported_targets: ESP32_ONLY,
        emits_records: false,
        emits_header: false,
    },
    FwCheckConfig {
        check: FwCheck::UploadWalkUsb,
        display_name: "Project upload walk (examples/basic), over the USB link",
        // The same image and the same conversation as `upload-walk`, on the
        // link the product ships instead of the spike's UART0 workaround
        // (M6 P5). Two payloads rather than one with a flag, because
        // `upload-walk`'s committed transcript is of the `spike_uart0_link`
        // image and a transcript is never re-baselined to suit a later idea
        // — the pair is the comparison. Silicon's own §11.3 walk went over
        // USB-Serial-JTAG, so this is the like-for-like one.
        firmware_features: &["server", "radio"],
        done_marker: Some("[shader-node] compilation succeeded"),
        trace_slug: "upload-walk-usb",
        supported_targets: ESP32_ONLY,
        emits_records: false,
        emits_header: false,
    },
    FwCheckConfig {
        check: FwCheck::MeteorWalkUsb,
        display_name: "Project upload walk (examples/meteor), over the USB link",
        // The spike report §11.2 ledger: the desk's only heap comparison on
        // a device with a project loaded and running, rather than idle.
        //
        // `done_marker` is `None` because this payload does not stop when it
        // arrives — §11.2's "steady" figures are read off a heartbeat well
        // after the load, so the run has to keep going. The host registry
        // spells that `Sentinel::Ready`.
        firmware_features: &["server", "radio"],
        done_marker: None,
        trace_slug: "meteor-walk-usb",
        supported_targets: ESP32_ONLY,
        emits_records: false,
        emits_header: false,
    },
    FwCheckConfig {
        check: FwCheck::UsbNegativeControl,
        display_name: "Shipped image with the port closed from boot",
        // The shipped image itself, flash-backed — `boot-idle` minus
        // `memory_fs`. No check module and no `test_*` feature: what is under
        // test is the product's own USB-Serial-JTAG link, and swapping the
        // filesystem out would change the image for a reason that has nothing
        // to do with the question.
        //
        // What makes it a different payload from `boot-idle` is not the image
        // at all — it is when the host side opens the port. `boot-idle` is
        // watched from its first byte; this one is flashed with no monitor,
        // left alone for several seconds, and then read by a non-resetting
        // reader. The registry that carries that difference is the host's
        // (`lp-emu-validate`'s `Payload::capture`), because it is a fact
        // about the operator, not about the firmware.
        firmware_features: &["server", "radio"],
        // The recovery stamp, in the first heartbeat delivered after the
        // reader attaches. NOT a stack heartbeat: `stack_probe` reports only
        // when the high-water mark has grown since the last report, so the
        // one report this payload could have seen went into a closed port
        // and there may never be another (M6 P4, measured on the emulator
        // twin: one `[stack]` line in a twenty-second run, at five seconds,
        // into the dark).
        done_marker: Some("\"hostDrainingAgainMs\""),
        trace_slug: "usb-negative-control",
        supported_targets: ESP32_ONLY,
        emits_records: false,
        emits_header: false,
    },
    FwCheckConfig {
        check: FwCheck::UsbDetachReattach,
        display_name: "The cable out mid-session and back in",
        // The shipped image minus flash, like every M6 emulator scenario
        // until M4 lands the flash controller. What is under test is the
        // link; the filesystem has nothing to say about it.
        firmware_features: &["server", "radio", "memory_fs"],
        // A whole heartbeat arriving after the port is re-opened. The stack
        // heartbeat at 5 s crosses before the unplug and the next is at 15 s,
        // long after the scenario is over; "a frame crossed the recovered
        // session" is the claim, and this is the line that carries it.
        done_marker: Some("\"uptime_ms\":10000"),
        trace_slug: "usb-detach-reattach",
        supported_targets: ESP32_ONLY,
        emits_records: false,
        emits_header: false,
    },
    FwCheckConfig {
        check: FwCheck::UsbHostAbsent,
        display_name: "The shipped image with no cable at all",
        firmware_features: &["server", "radio", "memory_fs"],
        // None, and not because the payload never finishes: with no host the
        // device says nothing at all, which is the finding. What the run has
        // to report is read out of its memory rather than off a wire, so the
        // host registry carries `Sentinel::State` and this side has no marker
        // to declare.
        done_marker: None,
        trace_slug: "usb-host-absent",
        supported_targets: ESP32_ONLY,
        emits_records: false,
        emits_header: false,
    },
    FwCheckConfig {
        check: FwCheck::Json,
        display_name: "JSON serial validation",
        firmware_features: &["test_json"],
        done_marker: None,
        trace_slug: "json",
        supported_targets: ESP32_ONLY,
        emits_records: false,
        emits_header: false,
    },
    FwCheckConfig {
        check: FwCheck::Rmt,
        display_name: "RMT chase (256 LEDs, three passes)",
        // `ws281x_telemetry` beside the harness switch, because the payload's
        // subject is not only the frames: the `[WS281X]` line is what the
        // driver believes about its own refill race, and a capture without it
        // can say the frames arrived but not at what cost. Three chases is
        // 13.9 s, which is what puts one such line in the transcript
        // (`checks::rmt_chase::CHASES`).
        firmware_features: &["test_rmt", "ws281x_telemetry"],
        // The literal, not `checks::rmt_chase::DONE_MARKER`: this table is a
        // `const` that exists whether or not `check-rmt` compiles the module,
        // so it cannot name a constant that may not be there. The two are
        // pinned equal by `the_rmt_chase_marker_is_the_module's` below.
        done_marker: Some("[rmt-chase] === DONE ==="),
        trace_slug: "rmt",
        supported_targets: ESP32_ONLY,
        emits_records: true,
        emits_header: true,
    },
    FwCheckConfig {
        check: FwCheck::Dither,
        display_name: "Display dithering",
        firmware_features: &["test_dither"],
        done_marker: None,
        trace_slug: "dither",
        supported_targets: ESP32_ONLY,
        emits_records: false,
        emits_header: false,
    },
    FwCheckConfig {
        check: FwCheck::FluidDemo,
        display_name: "Fluid demo",
        firmware_features: &["test_fluid_demo"],
        done_marker: None,
        trace_slug: "fluid-demo",
        supported_targets: ESP32_ONLY,
        emits_records: false,
        emits_header: false,
    },
    FwCheckConfig {
        check: FwCheck::MsaFluid,
        display_name: "MSAFluid solver",
        firmware_features: &["test_msafluid"],
        done_marker: None,
        trace_slug: "msafluid",
        supported_targets: ESP32_ONLY,
        emits_records: false,
        emits_header: false,
    },
];

pub const fn all_checks() -> &'static [FwCheckConfig] {
    ALL_CHECKS
}

pub fn find_check(slug: &str) -> Option<FwCheckConfig> {
    ALL_CHECKS
        .iter()
        .copied()
        .find(|check| check.slug() == slug)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one entry whose done marker is written twice — once as a literal
    /// in the table above, once as the module's own constant, because a
    /// `const` table cannot name a feature-gated item.
    #[test]
    fn the_rmt_chase_marker_is_the_modules() {
        let check = find_check("rmt-chase").expect("registered");
        assert_eq!(
            check.done_marker,
            Some(crate::checks::rmt_chase::DONE_MARKER)
        );
    }
}
