//! Descriptor for serial compute shader compilation.

use alloc::string::String;

use lpir::{CompilerConfig, FloatMode};
use lps_shared::LpsType;

use crate::compute_abi::{ComputeAbi, ComputeOutputAbi};

/// GLSL source, compiler settings, and expected ABI for a serial compute shader.
///
/// The ABI describes which shader-visible uniforms are consumed inputs and
/// which private globals are produced outputs. Compilation validates this
/// contract against lowered shader metadata before producing an executable
/// [`crate::LpsComputeShader`].
pub struct CompileComputeDesc<'a> {
    /// Full GLSL source, including any generated header.
    pub glsl: &'a str,
    /// LPIR/backend compiler settings.
    pub compiler_config: CompilerConfig,
    /// Expected consumed/produced shader interface.
    pub abi: ComputeAbi,
    /// Numeric mode this shader compiles in — the compute node's authored
    /// `float_mode` slot. See [`crate::CompilePxDesc::float_mode`].
    pub float_mode: FloatMode,
}

impl<'a> CompileComputeDesc<'a> {
    /// New compute descriptor with no consumed or produced slots, in the
    /// shipped numeric mode ([`FloatMode::Q32`]).
    #[must_use]
    pub fn new(glsl: &'a str, compiler_config: CompilerConfig) -> Self {
        Self {
            glsl,
            compiler_config,
            abi: ComputeAbi::default(),
            float_mode: FloatMode::Q32,
        }
    }

    /// Same descriptor, compiled in `float_mode`.
    #[must_use]
    pub fn with_float_mode(mut self, float_mode: FloatMode) -> Self {
        self.float_mode = float_mode;
        self
    }

    /// Add one uniform-backed consumed value.
    #[must_use]
    pub fn with_consumed(mut self, name: impl Into<String>, ty: LpsType) -> Self {
        self.abi.consumed.insert(name.into(), ty);
        self
    }

    /// Add one private-global-backed produced value.
    #[must_use]
    pub fn with_produced(mut self, name: impl Into<String>, ty: LpsType) -> Self {
        self.abi
            .produced
            .insert(name.into(), ComputeOutputAbi::Value(ty));
        self
    }

    /// Add a fixed private-global array that is interpreted as a map by key.
    #[must_use]
    pub fn with_sentinel_array_output(
        mut self,
        name: impl Into<String>,
        element: LpsType,
        len: u32,
        key: impl Into<String>,
    ) -> Self {
        self.abi.produced.insert(
            name.into(),
            ComputeOutputAbi::SentinelArray {
                element,
                len,
                key: key.into(),
            },
        );
        self
    }
}
