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
        // The shader compile is the last thing the load produces, and the
        // last line before the walk would need M5's RMT model to go on.
        done_marker: Some("[shader-node] compilation succeeded"),
        trace_slug: "upload-walk",
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
        display_name: "RMT output",
        firmware_features: &["test_rmt"],
        done_marker: None,
        trace_slug: "rmt",
        supported_targets: ESP32_ONLY,
        emits_records: false,
        emits_header: false,
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
