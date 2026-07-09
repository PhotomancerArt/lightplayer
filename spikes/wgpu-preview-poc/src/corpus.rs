//! The shader corpus: real authored example shaders (no `sampler2D`).
//!
//! Each entry records what it exercises and any forward declarations the
//! naga `glsl-in` frontend needs (naga resolves calls in source order, while
//! `lps-glsl` accepts out-of-order local functions).

/// One corpus shader, sourced verbatim from `examples/`.
#[derive(Debug, Clone, Copy)]
pub struct CorpusShader {
    /// Short name used for output files.
    pub name: &'static str,
    /// Repo-relative path of the authored source (documentation only).
    pub path: &'static str,
    /// Authored GLSL source, exactly what the device compiles.
    pub source: &'static str,
    /// Forward declarations for authored functions that are *called before
    /// they are defined* in the authored source. `lps-glsl` (and GLSL
    /// compilers with relaxed ordering) accept that; naga `glsl-in` does not,
    /// so the spike splices these prototypes ahead of the authored code.
    pub forward_decls: &'static str,
    /// What this shader exercises (for the report).
    pub exercises: &'static str,
    /// Extra authored uniforms beyond `outputSize`/`time`, as (name, value).
    pub extra_uniforms: &'static [(&'static str, f32)],
}

/// Corpus of generative example shaders (3–5 per the milestone; none use
/// `sampler2D`).
pub const CORPUS: &[CorpusShader] = &[
    CorpusShader {
        name: "basic",
        path: "examples/basic/shader.glsl",
        source: include_str!("../../../examples/basic/shader.glsl"),
        forward_decls: "",
        exercises: "lpfn_psrdnoise (2D, out gradient) live; lpfn_fbm + \
                    lpfn_worley compiled (dead demo fns); 5 procedural \
                    palettes, palette cycling, smoothstep/mix/mod/atan, \
                    if-chains, const bool branch",
        extra_uniforms: &[],
    },
    CorpusShader {
        name: "basic2",
        path: "examples/basic2/shader.glsl",
        source: include_str!("../../../examples/basic2/shader.glsl"),
        // `render` calls `worley_demo`, which is defined after it.
        forward_decls: "vec4 worley_demo(vec2 scaledCoord, float time);\n",
        exercises: "lpfn_worley (2D cellular) + lpfn_hsv2rgb; out-of-order \
                    local function (naga glsl-in ordering coverage)",
        extra_uniforms: &[],
    },
    CorpusShader {
        name: "fyeah_idle",
        path: "examples/fyeah-sign/idle.glsl",
        source: include_str!("../../../examples/fyeah-sign/idle.glsl"),
        forward_decls: "",
        exercises: "lpfn_psrdnoise (2D, out gradient); 3 palettes with \
                    crossfade, atan/fract/dot, banding + breathing terms",
        extra_uniforms: &[],
    },
    CorpusShader {
        name: "fyeah_blast",
        path: "examples/fyeah-sign/blast.glsl",
        source: include_str!("../../../examples/fyeah-sign/blast.glsl"),
        forward_decls: "",
        exercises: "lpfn_fbm (2D, 2 octaves → snoise2 + hash); third authored \
                    uniform (progress); pow/atan/smoothstep radial blast",
        extra_uniforms: &[("progress", 0.35)],
    },
    CorpusShader {
        name: "rocaille",
        path: "examples/rocaille/shader.glsl",
        source: include_str!("../../../examples/rocaille/shader.glsl"),
        forward_decls: "",
        exercises: "no lpfn builtins: nested counted loops (81 iterations), \
                    vec4 arithmetic, tanh/cos/sin/length — pure-math naga \
                    coverage + Q32 accumulation drift probe",
        extra_uniforms: &[],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corpus_has_three_to_five_shaders() {
        assert!(CORPUS.len() >= 3 && CORPUS.len() <= 5);
    }

    #[test]
    fn corpus_sources_have_no_sampler2d() {
        for shader in CORPUS {
            assert!(
                !shader.source.contains("sampler2D"),
                "{} uses sampler2D (out of scope for M3)",
                shader.name
            );
        }
    }
}
