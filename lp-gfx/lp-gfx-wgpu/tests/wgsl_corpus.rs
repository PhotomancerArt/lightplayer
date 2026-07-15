//! P2 done-when: every spike-corpus shader assembles → validates → produces
//! WGSL. No GPU required (pure naga translation).

mod util;

use lp_gfx_wgpu::wgsl_compile::compile_wgsl;
use lp_shader::TextureBindingSpecs;
use util::corpus::CORPUS;

#[test]
fn whole_corpus_translates_to_wgsl() {
    for shader in CORPUS {
        let translated = compile_wgsl(shader.source, &TextureBindingSpecs::new())
            .unwrap_or_else(|e| panic!("{}: {e}", shader.name));
        assert!(
            translated.wgsl.contains("fn main"),
            "{}: WGSL contains the fragment entry point",
            shader.name
        );
    }
}

#[test]
fn rocaille_tanh_is_bounded() {
    let rocaille = CORPUS
        .iter()
        .find(|s| s.name == "rocaille")
        .expect("corpus shader");
    let translated =
        compile_wgsl(rocaille.source, &TextureBindingSpecs::new()).expect("rocaille translates");
    assert!(
        translated.wgsl.contains("clamp"),
        "bounded-tanh pass applied:\n{}",
        &translated.wgsl[..translated.wgsl.len().min(2000)]
    );
}
