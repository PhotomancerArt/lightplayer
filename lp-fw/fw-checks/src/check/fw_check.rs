use core::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FwCheck {
    ShaderCompileStress,
    JitMathPerf,
    Json,
    Oom,
    Rmt,
    Dither,
    FluidDemo,
    MsaFluid,
}

impl FwCheck {
    pub const fn slug(self) -> &'static str {
        match self {
            Self::ShaderCompileStress => "shader-compile-stress",
            Self::JitMathPerf => "jit-math-perf",
            Self::Json => "json",
            Self::Oom => "oom",
            Self::Rmt => "rmt",
            Self::Dither => "dither",
            Self::FluidDemo => "fluid-demo",
            Self::MsaFluid => "msafluid",
        }
    }
}

impl fmt::Display for FwCheck {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.slug())
    }
}
