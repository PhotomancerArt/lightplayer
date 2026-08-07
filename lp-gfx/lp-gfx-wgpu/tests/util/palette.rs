//! Palette strips for corpus shaders that declare a `sampler2D` uniform.
//!
//! `examples/fyeah-sign/idle.glsl` reads its colors from an authored palette
//! (palette roadmap M5), so the conformance corpus is no longer
//! sampler-free. The strip a corpus shader sees is baked here by the engine's
//! own [`bake_gradient_into`](lpc_engine::color::bake_gradient_into) from the
//! very stops the example's `shader.json` carries — reimplementing the color
//! math in a test helper would let the two drift and prove nothing.
//!
//! **The strip is held, not cycled.** A `GradientConfig::Cycle`'s position is
//! a function of the timebase, and this corpus renders at bare timestamps
//! with no engine around it; it measures whether the two numeric tiers agree
//! on identical inputs, and one gradient held is an identical input. Cycle
//! evolution and the cross-fade bake are `lpc-engine`'s to prove
//! (`nodes::shader::palette_eval`, `engine::shader_palette_tests`).

use lp_gfx::{LpGraphics, TextureHandle};
use lp_shader::{LpsEngine, LpsTextureBuf, TextureBuffer};
use lpc_engine::color::{PALETTE_BAKE_BYTES, PALETTE_BAKE_FORMAT, PALETTE_BAKE_WIDTH};
use lpc_model::{Colorspace, Gradient, InterpMethod};
use lps_shared::{LpsValueF32, TextureFilter, TextureWrap};
use lpvm_wasm::rt_wasmtime::WasmLpvmEngine;

use super::corpus::{CorpusPalette, CorpusShader};

/// The texture binding specs a corpus shader's palette uniforms declare.
///
/// Height-one `Rgba16Unorm`, `Linear` filter, `Repeat` wrap — the single
/// palette spec `lpc_engine::nodes::shader::palette_texture_specs` gives
/// every palette slot, restated here because that function is engine-private
/// and this harness has no engine.
pub fn texture_specs(shader: &CorpusShader) -> lp_shader::TextureBindingSpecs {
    let mut specs = lp_shader::TextureBindingSpecs::new();
    for palette in shader.palettes {
        specs.insert(
            String::from(palette.name),
            lp_shader::texture_binding::height_one(
                PALETTE_BAKE_FORMAT,
                TextureFilter::Linear,
                TextureWrap::Repeat,
            ),
        );
    }
    specs
}

/// Bake one authored palette into its strip texels.
pub fn strip_texels(palette: &CorpusPalette) -> Vec<u8> {
    let gradient = Gradient {
        space: Colorspace::parse(palette.space)
            .unwrap_or_else(|| panic!("{}: unknown space {}", palette.name, palette.space)),
        method: InterpMethod::parse(palette.method)
            .unwrap_or_else(|| panic!("{}: unknown method {}", palette.name, palette.method)),
        stops: lpc_model::parse_stops(palette.stops)
            .unwrap_or_else(|e| panic!("{}: stops: {e}", palette.name)),
    };
    gradient
        .validate()
        .unwrap_or_else(|e| panic!("{}: gradient: {e}", palette.name));
    let mut texels = vec![0u8; PALETTE_BAKE_BYTES];
    lpc_engine::color::bake_gradient_into(&gradient, &mut texels);
    texels
}

/// Create this shader's palette textures on `graphics` and return the
/// uniform fields that bind them, alongside the handles the caller must keep
/// alive for the render.
pub fn bind_on_graphics<G: LpGraphics + ?Sized>(
    graphics: &G,
    shader: &CorpusShader,
) -> Result<(Vec<(String, LpsValueF32)>, Vec<TextureHandle>), String> {
    let mut fields = Vec::new();
    let mut handles = Vec::new();
    for palette in shader.palettes {
        let texture = graphics
            .create_texture(
                PALETTE_BAKE_WIDTH,
                1,
                PALETTE_BAKE_FORMAT,
                &strip_texels(palette),
            )
            .map_err(|e| format!("{}: create palette texture: {e}", palette.name))?;
        fields.push((
            String::from(palette.name),
            graphics
                .texture_uniform_value(&texture)
                .map_err(|e| format!("{}: palette uniform value: {e}", palette.name))?,
        ));
        handles.push(texture);
    }
    Ok((fields, handles))
}

/// The same, for the wasm reference tier, which owns its textures through
/// [`LpsEngine`] rather than through [`LpGraphics`].
pub fn bind_on_engine(
    engine: &LpsEngine<WasmLpvmEngine>,
    shader: &CorpusShader,
) -> Result<(Vec<(String, LpsValueF32)>, Vec<LpsTextureBuf>), String> {
    let mut fields = Vec::new();
    let mut buffers = Vec::new();
    for palette in shader.palettes {
        let mut buffer = engine
            .alloc_texture(PALETTE_BAKE_WIDTH, 1, PALETTE_BAKE_FORMAT)
            .map_err(|e| format!("{}: alloc palette texture: {e:?}", palette.name))?;
        buffer.data_mut().copy_from_slice(&strip_texels(palette));
        fields.push((
            String::from(palette.name),
            LpsValueF32::Texture2D(buffer.to_texture2d_value()),
        ));
        buffers.push(buffer);
    }
    Ok((fields, buffers))
}
