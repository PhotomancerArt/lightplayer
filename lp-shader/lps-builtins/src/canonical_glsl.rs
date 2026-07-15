//! Canonical GLSL sources for the lpfn builtin library.
//!
//! GLSL is the canonical source of truth for builtin **float** semantics
//! (see `docs/adr/2026-07-08-glsl-canonical-builtins.md`). Each entry embeds
//! one `.glsl` file from the `glsl/lpfn/` tree, which mirrors the Rust
//! concept-per-file layout under `src/builtins/lpfn/`. The Q32 Rust
//! implementations remain the device execution path; they are conformance
//! approximations of these sources (harness: `lps-filetests`
//! `src/conformance/`).
//!
//! The sources define functions with the real user-facing `lpfn_*` names so
//! they can be spliced verbatim as a GPU shader prelude (M3 of the GPU
//! preview roadmap). Note that `lps-frontend` reserves the `lpfn_` prefix
//! for builtin imports, so a harness compiling these sources through the
//! normal frontend must first rename the prefix (see the conformance
//! harness's `oracle` module).

/// One canonical GLSL source file.
#[derive(Debug, Clone, Copy)]
pub struct CanonicalGlsl {
    /// Registry name (matches the Rust concept file stem, e.g. `snoise2`).
    pub name: &'static str,
    /// Path of the source inside `lp-shader/lps-builtins/` (greppable).
    pub path: &'static str,
    /// The GLSL source text.
    pub source: &'static str,
    /// Direct dependencies (registry names) that must be compiled before
    /// this source (GLSL requires declaration before use).
    pub deps: &'static [&'static str],
}

/// All canonical GLSL sources, dependency-ordered (dependencies never appear
/// after their dependents, so a prefix-preserving concatenation is valid).
pub const CANONICAL_GLSL: &[CanonicalGlsl] = &[
    CanonicalGlsl {
        name: "hash",
        path: "glsl/lpfn/hash.glsl",
        source: include_str!("../glsl/lpfn/hash.glsl"),
        deps: &[],
    },
    CanonicalGlsl {
        name: "saturate",
        path: "glsl/lpfn/math/saturate.glsl",
        source: include_str!("../glsl/lpfn/math/saturate.glsl"),
        deps: &[],
    },
    CanonicalGlsl {
        name: "hue2rgb",
        path: "glsl/lpfn/color/space/hue2rgb.glsl",
        source: include_str!("../glsl/lpfn/color/space/hue2rgb.glsl"),
        deps: &["saturate"],
    },
    CanonicalGlsl {
        name: "hsv2rgb",
        path: "glsl/lpfn/color/space/hsv2rgb.glsl",
        source: include_str!("../glsl/lpfn/color/space/hsv2rgb.glsl"),
        deps: &["hue2rgb"],
    },
    CanonicalGlsl {
        name: "rgb2hsv",
        path: "glsl/lpfn/color/space/rgb2hsv.glsl",
        source: include_str!("../glsl/lpfn/color/space/rgb2hsv.glsl"),
        deps: &[],
    },
    CanonicalGlsl {
        name: "random1",
        path: "glsl/lpfn/generative/random/random1.glsl",
        source: include_str!("../glsl/lpfn/generative/random/random1.glsl"),
        deps: &[],
    },
    CanonicalGlsl {
        name: "random2",
        path: "glsl/lpfn/generative/random/random2.glsl",
        source: include_str!("../glsl/lpfn/generative/random/random2.glsl"),
        deps: &[],
    },
    CanonicalGlsl {
        name: "random3",
        path: "glsl/lpfn/generative/random/random3.glsl",
        source: include_str!("../glsl/lpfn/generative/random/random3.glsl"),
        deps: &[],
    },
    CanonicalGlsl {
        name: "srandom1",
        path: "glsl/lpfn/generative/srandom/srandom1.glsl",
        source: include_str!("../glsl/lpfn/generative/srandom/srandom1.glsl"),
        deps: &["random1"],
    },
    CanonicalGlsl {
        name: "srandom2",
        path: "glsl/lpfn/generative/srandom/srandom2.glsl",
        source: include_str!("../glsl/lpfn/generative/srandom/srandom2.glsl"),
        deps: &["random2"],
    },
    CanonicalGlsl {
        name: "srandom3",
        path: "glsl/lpfn/generative/srandom/srandom3.glsl",
        source: include_str!("../glsl/lpfn/generative/srandom/srandom3.glsl"),
        deps: &["random3"],
    },
    CanonicalGlsl {
        name: "srandom3_vec",
        path: "glsl/lpfn/generative/srandom/srandom3_vec.glsl",
        source: include_str!("../glsl/lpfn/generative/srandom/srandom3_vec.glsl"),
        deps: &[],
    },
    CanonicalGlsl {
        name: "srandom3_tile",
        path: "glsl/lpfn/generative/srandom/srandom3_tile.glsl",
        source: include_str!("../glsl/lpfn/generative/srandom/srandom3_tile.glsl"),
        deps: &["srandom3_vec"],
    },
    CanonicalGlsl {
        name: "snoise1",
        path: "glsl/lpfn/generative/snoise/snoise1.glsl",
        source: include_str!("../glsl/lpfn/generative/snoise/snoise1.glsl"),
        deps: &["hash"],
    },
    CanonicalGlsl {
        name: "snoise2",
        path: "glsl/lpfn/generative/snoise/snoise2.glsl",
        source: include_str!("../glsl/lpfn/generative/snoise/snoise2.glsl"),
        deps: &["hash"],
    },
    CanonicalGlsl {
        name: "snoise3",
        path: "glsl/lpfn/generative/snoise/snoise3.glsl",
        source: include_str!("../glsl/lpfn/generative/snoise/snoise3.glsl"),
        deps: &["hash"],
    },
    CanonicalGlsl {
        name: "gnoise1",
        path: "glsl/lpfn/generative/gnoise/gnoise1.glsl",
        source: include_str!("../glsl/lpfn/generative/gnoise/gnoise1.glsl"),
        deps: &["random1"],
    },
    CanonicalGlsl {
        name: "gnoise2",
        path: "glsl/lpfn/generative/gnoise/gnoise2.glsl",
        source: include_str!("../glsl/lpfn/generative/gnoise/gnoise2.glsl"),
        deps: &["random2"],
    },
    CanonicalGlsl {
        name: "gnoise3",
        path: "glsl/lpfn/generative/gnoise/gnoise3.glsl",
        source: include_str!("../glsl/lpfn/generative/gnoise/gnoise3.glsl"),
        deps: &["random3"],
    },
    CanonicalGlsl {
        name: "gnoise3_tile",
        path: "glsl/lpfn/generative/gnoise/gnoise3_tile.glsl",
        source: include_str!("../glsl/lpfn/generative/gnoise/gnoise3_tile.glsl"),
        deps: &["gnoise3", "srandom3_tile"],
    },
    CanonicalGlsl {
        name: "fbm2",
        path: "glsl/lpfn/generative/fbm/fbm2.glsl",
        source: include_str!("../glsl/lpfn/generative/fbm/fbm2.glsl"),
        deps: &["snoise2"],
    },
    CanonicalGlsl {
        name: "fbm3",
        path: "glsl/lpfn/generative/fbm/fbm3.glsl",
        source: include_str!("../glsl/lpfn/generative/fbm/fbm3.glsl"),
        deps: &["snoise3"],
    },
    CanonicalGlsl {
        name: "fbm3_tile",
        path: "glsl/lpfn/generative/fbm/fbm3_tile.glsl",
        source: include_str!("../glsl/lpfn/generative/fbm/fbm3_tile.glsl"),
        deps: &["gnoise3_tile"],
    },
    CanonicalGlsl {
        name: "worley2",
        path: "glsl/lpfn/generative/worley/worley2.glsl",
        source: include_str!("../glsl/lpfn/generative/worley/worley2.glsl"),
        deps: &["hash"],
    },
    CanonicalGlsl {
        name: "worley2_value",
        path: "glsl/lpfn/generative/worley/worley2_value.glsl",
        source: include_str!("../glsl/lpfn/generative/worley/worley2_value.glsl"),
        deps: &["worley2"],
    },
    CanonicalGlsl {
        name: "worley3",
        path: "glsl/lpfn/generative/worley/worley3.glsl",
        source: include_str!("../glsl/lpfn/generative/worley/worley3.glsl"),
        deps: &["hash"],
    },
    CanonicalGlsl {
        name: "worley3_value",
        path: "glsl/lpfn/generative/worley/worley3_value.glsl",
        source: include_str!("../glsl/lpfn/generative/worley/worley3_value.glsl"),
        deps: &["worley3"],
    },
    CanonicalGlsl {
        name: "psrdnoise2",
        path: "glsl/lpfn/generative/psrdnoise/psrdnoise2.glsl",
        source: include_str!("../glsl/lpfn/generative/psrdnoise/psrdnoise2.glsl"),
        deps: &[],
    },
    CanonicalGlsl {
        name: "psrdnoise3",
        path: "glsl/lpfn/generative/psrdnoise/psrdnoise3.glsl",
        source: include_str!("../glsl/lpfn/generative/psrdnoise/psrdnoise3.glsl"),
        deps: &[],
    },
];

/// Look up a canonical source by registry name.
pub fn canonical_glsl(name: &str) -> Option<&'static CanonicalGlsl> {
    CANONICAL_GLSL.iter().find(|c| c.name == name)
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    #[test]
    fn names_unique_and_deps_resolve_backwards() {
        for (i, c) in CANONICAL_GLSL.iter().enumerate() {
            assert!(
                CANONICAL_GLSL[..i].iter().all(|p| p.name != c.name),
                "duplicate canonical name {}",
                c.name
            );
            for dep in c.deps {
                assert!(
                    CANONICAL_GLSL[..i].iter().any(|p| p.name == *dep),
                    "{}: dep {dep} must appear earlier in CANONICAL_GLSL",
                    c.name
                );
            }
        }
    }

    #[test]
    fn sources_define_lpfn_functions() {
        for c in CANONICAL_GLSL {
            assert!(
                c.source.contains("lpfn_"),
                "{}: source should define an lpfn_ function",
                c.name
            );
            assert!(!c.source.trim().is_empty(), "{}: empty source", c.name);
        }
    }

    #[test]
    fn lookup_finds_all() {
        for c in CANONICAL_GLSL {
            assert!(canonical_glsl(c.name).is_some());
        }
        assert!(canonical_glsl("nope").is_none());
    }
}
