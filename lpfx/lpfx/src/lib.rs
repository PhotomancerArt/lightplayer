#![no_std]

//! Effect module manifests and `fx.toml` parsing (`no_std` + `alloc`).
//!
//! Loading files from disk is done by the caller; see [`FxModule::from_sources`].

extern crate alloc;

mod defaults;
pub mod engine;
mod error;
mod input;
mod manifest;
mod module;
mod parse;
mod render_inputs;
pub mod texture;

pub use defaults::defaults_from_manifest;
pub use engine::{FxEngine, FxInstance};
pub use error::FxError;
pub use input::{FxChoice, FxInputDef, FxInputType, FxPresentation, FxValue};
pub use manifest::{FxManifest, FxMeta, FxResolution};
pub use module::FxModule;
pub use parse::parse_manifest;
pub use render_inputs::FxRenderInputs;
pub use texture::TextureId;

#[cfg(test)]
mod tests {
    use super::*;

    const NOISE_FX_TOML: &str = include_str!("../../../examples/noise.fx/fx.toml");
    const NOISE_FX_GLSL: &str = include_str!("../../../examples/noise.fx/main.glsl");

    #[test]
    fn noise_fx_happy_path() {
        let toml = NOISE_FX_TOML;
        let m = parse_manifest(&toml).expect("parse");
        assert_eq!(m.meta.name, "Noise");
        assert_eq!(m.resolution.width, 512);
        assert_eq!(m.inputs.len(), 6);
        assert!(m.inputs.contains_key("speed"));
        let noise_fn = m.inputs.get("noise_fn").unwrap();
        assert!(matches!(noise_fn.input_type, FxInputType::I32));
        assert_eq!(noise_fn.presentation, Some(FxPresentation::Choice));
        assert_eq!(noise_fn.choices.as_ref().map(|c| c.len()), Some(3));
        let spd = m.inputs.get("speed").unwrap();
        assert_eq!(spd.default, Some(FxValue::F32(1.0)));
    }

    #[test]
    fn missing_meta_name() {
        let toml = "[meta]\nname = \"\"\n";
        let e = parse_manifest(toml).unwrap_err();
        assert!(matches!(
            e,
            FxError::MissingField {
                section: "meta",
                ..
            }
        ));
    }

    #[test]
    fn invalid_input_type() {
        let toml = "[meta]\nname = \"x\"\n\n[input.k]\ntype = \"nope\"\n";
        let e = parse_manifest(toml).unwrap_err();
        assert!(matches!(e, FxError::InvalidType { .. }));
    }

    #[test]
    fn default_type_mismatch() {
        let toml = "[meta]\nname = \"x\"\n\n[input.k]\ntype = \"f32\"\ndefault = true\n";
        let e = parse_manifest(toml).unwrap_err();
        assert!(matches!(e, FxError::DefaultTypeMismatch { .. }));
    }

    #[test]
    fn choice_ui_empty_choices() {
        let toml = "[meta]\nname = \"x\"\n\n[input.k]\ntype = \"i32\"\nui = { choices = [] }\n";
        let e = parse_manifest(toml).unwrap_err();
        match e {
            FxError::ValidationError(msg) => {
                assert!(msg.contains("choices"));
            }
            other => panic!("unexpected: {other}"),
        }
    }

    #[test]
    fn minimal_manifest() {
        let toml = "[meta]\nname = \"Bare\"\n";
        let m = parse_manifest(toml).expect("parse");
        assert_eq!(m.meta.name, "Bare");
        assert_eq!(m.resolution.width, 512);
        assert!(m.inputs.is_empty());
    }

    #[test]
    fn noise_fx_compiles_in_lps_frontend() {
        let module = FxModule::from_sources(NOISE_FX_TOML, NOISE_FX_GLSL).expect("fx module");
        assert_eq!(module.manifest.inputs.len(), 6);
        lps_frontend::compile(NOISE_FX_GLSL).expect("glsl should compile");
    }
}
