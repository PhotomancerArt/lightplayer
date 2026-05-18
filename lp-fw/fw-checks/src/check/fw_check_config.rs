use super::{FwCheck, FwCheckTarget};

pub const FW_CHECK_JSON_PREFIX: &str = "[fw-check-json] ";

#[derive(Clone, Copy, Debug)]
pub struct FwCheckConfig {
    pub check: FwCheck,
    pub display_name: &'static str,
    pub firmware_features: &'static [&'static str],
    pub done_marker: Option<&'static str>,
    pub trace_slug: &'static str,
    pub supported_targets: &'static [FwCheckTarget],
    pub emits_records: bool,
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
    },
    FwCheckConfig {
        check: FwCheck::JitMathPerf,
        display_name: "JIT Q32 math perf",
        firmware_features: &["test_jit_math_perf"],
        done_marker: Some("[jit-math-perf] === DONE ==="),
        trace_slug: "jit-math-perf",
        supported_targets: ESP32_ONLY,
        emits_records: false,
    },
    FwCheckConfig {
        check: FwCheck::Json,
        display_name: "JSON serial validation",
        firmware_features: &["test_json"],
        done_marker: None,
        trace_slug: "json",
        supported_targets: ESP32_ONLY,
        emits_records: false,
    },
    FwCheckConfig {
        check: FwCheck::Oom,
        display_name: "OOM recovery",
        firmware_features: &["test_oom"],
        done_marker: None,
        trace_slug: "oom",
        supported_targets: ESP32_ONLY,
        emits_records: false,
    },
    FwCheckConfig {
        check: FwCheck::Rmt,
        display_name: "RMT output",
        firmware_features: &["test_rmt"],
        done_marker: None,
        trace_slug: "rmt",
        supported_targets: ESP32_ONLY,
        emits_records: false,
    },
    FwCheckConfig {
        check: FwCheck::Dither,
        display_name: "Display dithering",
        firmware_features: &["test_dither"],
        done_marker: None,
        trace_slug: "dither",
        supported_targets: ESP32_ONLY,
        emits_records: false,
    },
    FwCheckConfig {
        check: FwCheck::FluidDemo,
        display_name: "Fluid demo",
        firmware_features: &["test_fluid_demo"],
        done_marker: None,
        trace_slug: "fluid-demo",
        supported_targets: ESP32_ONLY,
        emits_records: false,
    },
    FwCheckConfig {
        check: FwCheck::MsaFluid,
        display_name: "MSAFluid solver",
        firmware_features: &["test_msafluid"],
        done_marker: None,
        trace_slug: "msafluid",
        supported_targets: ESP32_ONLY,
        emits_records: false,
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
