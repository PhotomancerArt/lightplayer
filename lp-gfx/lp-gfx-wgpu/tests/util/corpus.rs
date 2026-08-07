//! The conformance corpus: real authored example shaders, carried over from
//! `spikes/wgpu-preview-poc` (M3). The production assembler generates
//! prototypes for authored functions, so the spike's per-shader
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
    /// `sampler2D` palette uniforms the shader declares, baked by
    /// [`util::palette`](super::palette) into the height-one strips both
    /// tiers read.
    pub palettes: &'static [CorpusPalette],
}

/// One authored palette a corpus shader consumes, copied from the `gradient`
/// its `shader.json` carries — same three tokens, same stops literal.
#[derive(Debug, Clone, Copy)]
pub struct CorpusPalette {
    /// Uniform name, matching the `consumed` slot.
    pub name: &'static str,
    /// `space` token (`lpc_model::Colorspace::parse`).
    pub space: &'static str,
    /// `method` token (`lpc_model::InterpMethod::parse`).
    pub method: &'static str,
    /// The stops literal (`docs/design/color.md` §5).
    pub stops: &'static str,
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
        palettes: &[],
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
        palettes: &[],
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
        ],
        // The first entry of idle.json's three-palette cycle ("cool"), held.
        // See `util::palette` for why the corpus holds rather than cycles.
        palettes: &[CorpusPalette {
            name: "palette",
            space: "oklab",
            method: "linear",
            stops: "(0.7593,0.0349,0.0439) (0.658,0.0528,0.0059) (0.574,0.0512,-0.0599) \
                    (0.5599,0.0064,-0.1104) (0.628,-0.0345,-0.1155) (0.7287,-0.0454,-0.0941) \
                    (0.8222,-0.0425,-0.0643) (0.8917,-0.0346,-0.0332) (0.9304,-0.0242,-0.0036) \
                    (0.9356,-0.0122,0.0225) (0.9071,0.0013,0.0425) (0.8466,0.0168,0.0522) \
                    (0.7593,0.0349,0.0439)",
        }],
    },
    CorpusShader {
        name: "fyeah_blast",
        path: "examples/fyeah-sign/blast.glsl",
        source: include_str!("../../../../examples/fyeah-sign/blast.glsl"),
        extra_uniforms: &[("progress", 0.35)],
        // Entry-relative: `blast` binds `node:..#entry_time`, which the
        // TimeProduct break deliberately left as an f32 `time` uniform.
        phasors: &[],
        palettes: &[],
    },
    CorpusShader {
        name: "rocaille",
        path: "examples/rocaille/shader.glsl",
        source: include_str!("../../../../examples/rocaille/shader.glsl"),
        extra_uniforms: &[],
        phasors: &[("cycle", 20.0)],
        palettes: &[],
    },
];
