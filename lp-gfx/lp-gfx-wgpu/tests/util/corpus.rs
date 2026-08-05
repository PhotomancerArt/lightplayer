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
    /// Phasor uniforms the shader declares, as (name, `period_seconds`) taken
    /// from its authored `shader.json`. The harness evaluates each as
    /// `fract(time / period)` per timestamp — the engine's phasor slot
    /// integrates instead of dividing, but at a fixed timestamp both land on
    /// the same cycle position, which is all a parity render needs.
    pub phasors: &'static [(&'static str, f32)],
}

/// Corpus of generative example shaders (M3 spike set).
pub const CORPUS: &[CorpusShader] = &[
    CorpusShader {
        name: "basic",
        path: "examples/basic/shader.glsl",
        source: include_str!("../../../../examples/basic/shader.glsl"),
        extra_uniforms: &[],
        phasors: &[
            ("palettePhase01", 25.0),
            ("panPhase", 20.9439516),
            ("scalePhase", 8.9759793),
        ],
    },
    CorpusShader {
        name: "basic2",
        path: "examples/basic2/shader.glsl",
        source: include_str!("../../../../examples/basic2/shader.glsl"),
        extra_uniforms: &[],
        phasors: &[
            ("panPhase", 20.9439516),
            ("scalePhase", 8.9759793),
            ("huePhase", 6.2831855),
        ],
    },
    CorpusShader {
        name: "fyeah_idle",
        path: "examples/fyeah-sign/idle.glsl",
        source: include_str!("../../../../examples/fyeah-sign/idle.glsl"),
        extra_uniforms: &[("glow", 0.5)],
        phasors: &[
            ("zoomPhase", 19.6349546),
            ("driftPhase", 34.906586),
            ("bandPhase", 7.3919829),
            ("breathPhase", 8.3775806),
            ("paletteCycle", 18.0),
        ],
    },
    CorpusShader {
        name: "fyeah_blast",
        path: "examples/fyeah-sign/blast.glsl",
        source: include_str!("../../../../examples/fyeah-sign/blast.glsl"),
        extra_uniforms: &[("progress", 0.35)],
        // Entry-relative: `blast` binds `node:..#entry_time`, which the
        // TimeProduct break deliberately left as an f32 `time` uniform.
        phasors: &[],
    },
    CorpusShader {
        name: "rocaille",
        path: "examples/rocaille/shader.glsl",
        source: include_str!("../../../../examples/rocaille/shader.glsl"),
        extra_uniforms: &[],
        phasors: &[("cycle", 20.0)],
    },
];
