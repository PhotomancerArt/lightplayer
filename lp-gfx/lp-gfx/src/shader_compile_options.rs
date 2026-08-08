//! Backend-agnostic shader compile options.

use crate::shader_semantics::ShaderSemantics;

/// Backend-agnostic compile options understood by every [`crate::LpGraphics`].
pub struct ShaderCompileOptions {
    /// Numeric semantics tier this shader must be compiled with.
    ///
    /// Explicit so no backend can silently pick its own: a backend that does
    /// not implement the requested tier must fail compilation.
    pub semantics: ShaderSemantics,
    /// Maximum semantic errors from the GLSL → LPIR front-end.
    pub max_errors: Option<usize>,
    /// GLSL frontend used before LPIR lowering.
    pub frontend: lp_shader::ShaderFrontend,
    /// Compile-time texture binding contract: one [`lps_shared::TextureBindingSpec`]
    /// per `sampler2D` uniform leaf, keyed by canonical dotted uniform path
    /// (`docs/design/lp-shader-texture-access.md`). Every backend validates
    /// this map against the shader's declared samplers and fails compilation
    /// on a mismatch — missing or extra specs are compile errors on the CPU
    /// and GPU tiers alike.
    pub textures: lp_shader::TextureBindingSpecs,
    /// Space the shader declares it renders in — the entry contract every
    /// backend must honour: `TwoD` means the source defines
    /// `vec4 render_2d(vec2 pos)`, `OneD` means `vec4 render_1d(float pos)`
    /// (dimensionality plan D19).
    ///
    /// Explicit for the same reason as [`Self::semantics`]: it is an
    /// authored decision (`ShaderDef::space`) that travels with the compile
    /// request, never inferred from the source text. Backends that fork at
    /// the GLSL (the GPU tier) splice the matching entry call; the CPU tier
    /// validates and synthesises against it.
    pub space: lp_shader::ShaderEntrySpace,
}

impl ShaderCompileOptions {
    /// Build options from the two per-backend product decisions — semantics
    /// tier and GLSL frontend — with neutral defaults for the rest (20 max
    /// errors, no texture bindings, the default 2D declared space).
    ///
    /// There is deliberately no `Default`: `frontend` used to fall back to a
    /// `cfg!(feature = "naga")` default, which let Cargo feature unification
    /// silently change compile behavior with the build graph. Render paths
    /// take both values from the backend the host constructed
    /// ([`crate::LpGraphics::native_semantics`] /
    /// [`crate::LpGraphics::glsl_frontend`]).
    #[must_use]
    pub fn new(semantics: ShaderSemantics, frontend: lp_shader::ShaderFrontend) -> Self {
        Self {
            semantics,
            max_errors: Some(20),
            frontend,
            textures: lp_shader::TextureBindingSpecs::new(),
            space: lp_shader::ShaderEntrySpace::TwoD,
        }
    }

    /// Same options, for a shader declaring `space`.
    #[must_use]
    pub fn with_space(mut self, space: lp_shader::ShaderEntrySpace) -> Self {
        self.space = space;
        self
    }

    /// LPIR compiler configuration for this compile.
    ///
    /// The Q32 arithmetic configuration is no longer selectable per shader —
    /// the compiler hard-codes the shader-speed expansion (wrapping add/sub/mul,
    /// reciprocal divide). See `docs/design/q32.md`.
    pub fn to_compiler_config(&self) -> lpir::CompilerConfig {
        lpir::CompilerConfig::default()
    }
}
