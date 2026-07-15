//! The conformance corpus: real authored example shaders (no `sampler2D`),
//! carried over from `spikes/wgpu-preview-poc` (M3). The production assembler
//! generates prototypes for authored functions, so the spike's per-shader
//! `forward_decls` field is gone.

/// One corpus shader, sourced verbatim from `examples/`.
#[derive(Debug, Clone, Copy)]
pub struct CorpusShader {
    /// Short name used for output files and test labels.
    pub name: &'static str,
    /// Repo-relative path of the authored source (documentation only).
    pub path: &'static str,
    /// Authored GLSL source, exactly what the device compiles.
    pub source: &'static str,
    /// Extra authored uniforms beyond `outputSize`/`time`, as (name, value).
    pub extra_uniforms: &'static [(&'static str, f32)],
}

/// Corpus of generative example shaders (M3 spike set).
pub const CORPUS: &[CorpusShader] = &[
    CorpusShader {
        name: "basic",
        path: "examples/basic/shader.glsl",
        source: include_str!("../../../../examples/basic/shader.glsl"),
        extra_uniforms: &[],
    },
    CorpusShader {
        name: "basic2",
        path: "examples/basic2/shader.glsl",
        source: include_str!("../../../../examples/basic2/shader.glsl"),
        extra_uniforms: &[],
    },
    CorpusShader {
        name: "fyeah_idle",
        path: "examples/fyeah-sign/idle.glsl",
        source: include_str!("../../../../examples/fyeah-sign/idle.glsl"),
        extra_uniforms: &[],
    },
    CorpusShader {
        name: "fyeah_blast",
        path: "examples/fyeah-sign/blast.glsl",
        source: include_str!("../../../../examples/fyeah-sign/blast.glsl"),
        extra_uniforms: &[("progress", 0.35)],
    },
    CorpusShader {
        name: "rocaille",
        path: "examples/rocaille/shader.glsl",
        source: include_str!("../../../../examples/rocaille/shader.glsl"),
        extra_uniforms: &[],
    },
];
