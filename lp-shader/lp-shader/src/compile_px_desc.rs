//! Descriptor for pixel shader compilation (GLSL + output format + texture binding specs).

use alloc::string::String;
use lp_collection::VecMap;

use lpir::{CompilerConfig, FloatMode};
use lps_shared::{TextureBindingSpec, TextureStorageFormat};

use crate::entry_space::ShaderEntrySpace;

pub type TextureBindingSpecs = VecMap<String, TextureBindingSpec>;

/// Frontend used for GLSL source before LPIR lowering.
///
/// Deliberately has no `Default`: which frontend compiles a shader is a
/// product decision of the host that constructs the engine/backend. A
/// `cfg!(feature = "naga")` default used to live here, which meant Cargo
/// feature unification silently changed compile behavior depending on which
/// packages shared the build graph.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShaderFrontend {
    /// Existing Naga GLSL frontend.
    Naga,
    /// LightPlayer-native GLSL frontend (`lps-glsl`).
    LpsGlsl,
}

impl ShaderFrontend {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Naga => "naga",
            Self::LpsGlsl => "lps-glsl",
        }
    }

    /// Whether this frontend is compiled into the current binary.
    ///
    /// `Naga` is behind the `naga` feature; selecting it without the feature
    /// fails every compile at runtime with "naga frontend was not built into
    /// this binary". Hosts that pin a frontend as a const can const-assert
    /// this so a build whose feature graph dropped the frontend fails at
    /// compile time instead.
    #[must_use]
    pub const fn built_in(self) -> bool {
        match self {
            Self::Naga => cfg!(feature = "naga"),
            Self::LpsGlsl => true,
        }
    }
}

/// GLSL source, output [`TextureStorageFormat`], compiler settings, and optional per-sampler
/// [`TextureBindingSpec`] entries consumed by [`crate::LpsEngine::compile_px_desc`].
pub struct CompilePxDesc<'a> {
    pub glsl: &'a str,
    pub output_format: TextureStorageFormat,
    pub compiler_config: CompilerConfig,
    pub textures: TextureBindingSpecs,
    pub frontend: ShaderFrontend,
    /// Numeric mode this shader compiles in — the authored `float_mode` slot,
    /// carried per compile because it lives per shader node.
    ///
    /// [`crate::LpsEngine`] passes it to the backend through
    /// [`lpvm::LpvmCompileParams`]; an engine that cannot honour it says so
    /// via [`lpvm::LpvmEngine::supports_float_mode`] and the caller errors.
    /// See `docs/adr/2026-08-01-float-mode-as-a-compiler-parameter.md`.
    pub float_mode: FloatMode,
    /// Space this shader declares it renders in, which *is* the entry
    /// contract: `TwoD` requires `vec4 render_2d(vec2 pos)`, `OneD`
    /// requires `vec4 render_1d(float pos)`.
    ///
    /// Carried per compile for the same reason as [`Self::float_mode`]:
    /// it is an authored slot on the shader node (`ShaderDef::space`), and
    /// the compiler never infers it from the source. Defaults to
    /// [`ShaderEntrySpace::TwoD`].
    pub space: ShaderEntrySpace,
}

impl<'a> CompilePxDesc<'a> {
    /// Build a descriptor with no texture binding specs, in the shipped
    /// numeric mode ([`FloatMode::Q32`]) and the default declared space
    /// ([`ShaderEntrySpace::TwoD`]). Set [`Self::float_mode`] /
    /// [`Self::space`] after construction to compile Float or a 1D shader.
    #[must_use]
    pub fn new(
        glsl: &'a str,
        output_format: TextureStorageFormat,
        compiler_config: CompilerConfig,
        frontend: ShaderFrontend,
    ) -> Self {
        Self {
            glsl,
            output_format,
            compiler_config,
            textures: TextureBindingSpecs::new(),
            frontend,
            float_mode: FloatMode::Q32,
            space: ShaderEntrySpace::TwoD,
        }
    }

    /// Same descriptor, compiled in `float_mode`.
    #[must_use]
    pub fn with_float_mode(mut self, float_mode: FloatMode) -> Self {
        self.float_mode = float_mode;
        self
    }

    /// Same descriptor, compiled for a shader declaring `space`.
    #[must_use]
    pub fn with_space(mut self, space: ShaderEntrySpace) -> Self {
        self.space = space;
        self
    }

    /// Adds or replaces the compile-time [`TextureBindingSpec`] for uniform `name`.
    ///
    /// Callers supply specs that match how they populate buffers at runtime (format, dimensions via
    /// [`crate::TextureBuffer::height`], etc.). Palette strips baked elsewhere must use a compatible
    /// hint—for example [`texture_binding::height_one`] when the backing texture is one row tall.
    #[must_use]
    pub fn with_texture_spec(mut self, name: impl Into<String>, spec: TextureBindingSpec) -> Self {
        self.textures.insert(name.into(), spec);
        self
    }
}

/// Convenience constructors for [`TextureBindingSpec`]: general 2D vs a height-one palette/gradient strip.
///
/// These functions set `TextureShapeHint` for lowering (`texture()` → 2D vs 1D-style builtins).
/// [`texture_binding::height_one`] fixes `TextureWrap` on the unused vertical axis to
/// `TextureWrap::ClampToEdge`; sampling ignores `uv.y` when the hint is `TextureShapeHint::HeightOne`.
///
/// Baking palette bytes into [`crate::LpsTextureBuf`] (or another [`crate::TextureBuffer`]) and
/// choosing filter/wrap remains the caller’s responsibility; this module only builds matching specs.
pub mod texture_binding {
    use super::TextureBindingSpec;
    use lps_shared::{TextureFilter, TextureShapeHint, TextureStorageFormat, TextureWrap};

    /// [`TextureShapeHint::General2D`] with explicit horizontal and vertical wrap.
    #[must_use]
    pub fn texture2d(
        format: TextureStorageFormat,
        filter: TextureFilter,
        wrap_x: TextureWrap,
        wrap_y: TextureWrap,
    ) -> TextureBindingSpec {
        TextureBindingSpec {
            format,
            filter,
            wrap_x,
            wrap_y,
            shape_hint: TextureShapeHint::General2D,
        }
    }

    /// Palette or gradient strip: [`TextureShapeHint::HeightOne`], single-row sampling semantics.
    #[must_use]
    pub fn height_one(
        format: TextureStorageFormat,
        filter: TextureFilter,
        wrap_x: TextureWrap,
    ) -> TextureBindingSpec {
        TextureBindingSpec {
            format,
            filter,
            wrap_x,
            wrap_y: TextureWrap::ClampToEdge,
            shape_hint: TextureShapeHint::HeightOne,
        }
    }
}
