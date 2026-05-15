//! Build-time slot shape bootstrap generator.
//!
//! This crate is host-only. A crate build script points it at that crate's
//! source tree, and it writes an `OUT_DIR` Rust module that can register every
//! static `SlotRecord` shape discovered in that crate. Runtime-owned dynamic
//! shapes are intentionally outside this discovery pass.

use std::fs;

mod config;
mod discover;
mod error;
mod model;
mod render;

pub use config::{SlotShapeCodegenConfig, SlotViewCodegenConfig};
pub use error::SlotShapeCodegenError;

/// Generate `slot_shapes.rs` for one crate.
pub fn generate_slot_shapes(config: SlotShapeCodegenConfig) -> Result<(), SlotShapeCodegenError> {
    let src_dir = config.crate_root.join("src");
    let mut shapes = discover::discover_static_registered_shapes(&src_dir)?;
    shapes.sort_by(|a, b| a.type_path.cmp(&b.type_path));

    if let Some(parent) = config.out_file.parent() {
        fs::create_dir_all(parent).map_err(SlotShapeCodegenError::Io)?;
    }
    fs::write(config.out_file, render::render_slot_shapes(&shapes))
        .map_err(SlotShapeCodegenError::Io)
}

/// Generate `slot_views.rs` for every static `SlotRecord` in one crate.
pub fn generate_slot_views(config: SlotViewCodegenConfig) -> Result<(), SlotShapeCodegenError> {
    let src_dir = config.crate_root.join("src");
    let mut views = discover::discover_static_slot_views(&src_dir)?;
    views.sort_by(|a, b| a.type_path.cmp(&b.type_path));

    if let Some(parent) = config.out_file.parent() {
        fs::create_dir_all(parent).map_err(SlotShapeCodegenError::Io)?;
    }
    fs::write(config.out_file, render::render_slot_views(&views)).map_err(SlotShapeCodegenError::Io)
}

#[cfg(test)]
mod tests {
    use crate::discover::{discover_static_registered_shapes, discover_static_slot_views};
    use crate::model::{StaticRegisteredShape, StaticSlotView, StaticSlotViewField};
    use crate::render::{render_slot_shapes, render_slot_views};
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn discovers_static_slot_records_and_infers_parent_reexport_paths() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(src.join("source")).unwrap();
        fs::write(
            src.join("source").join("shader_def.rs"),
            r#"
use lpc_model::SlotRecord;

#[derive(SlotRecord)]
pub struct ShaderDef {
    value: ValueSlot<bool>,
}

#[derive(SlotRecord)]
pub struct Nested {
    value: ValueSlot<bool>,
}
"#,
        )
        .unwrap();

        let shapes = discover_static_registered_shapes(&src).unwrap();

        assert_eq!(
            shapes,
            vec![
                StaticRegisteredShape {
                    type_path: String::from("crate::source::Nested"),
                },
                StaticRegisteredShape {
                    type_path: String::from("crate::source::ShaderDef"),
                }
            ]
        );
    }

    #[test]
    fn infers_mod_file_paths() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(src.join("node").join("project")).unwrap();
        fs::write(
            src.join("node").join("project").join("mod.rs"),
            r#"
#[derive(SlotRecord)]
pub struct ProjectDef {}
"#,
        )
        .unwrap();

        let shapes = discover_static_registered_shapes(&src).unwrap();

        assert_eq!(
            shapes,
            vec![StaticRegisteredShape {
                type_path: String::from("crate::node::project::ProjectDef"),
            }]
        );
    }

    #[test]
    fn discovers_static_slot_view_fields() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(src.join("source")).unwrap();
        fs::write(
            src.join("source").join("fixture_def.rs"),
            r#"
use lpc_model::{SlotRecord, ValueSlot};

#[derive(SlotRecord)]
pub struct FixtureDef {
    pub render_size: Dim2uSlot,
    #[slot(enum)]
    pub mode: SomeEnum,
    #[slot(name = "gamma")]
    pub gamma_correction: ValueSlot<bool>,
}
"#,
        )
        .unwrap();

        let views = discover_static_slot_views(&src).unwrap();

        assert_eq!(views.len(), 1);
        assert_eq!(views[0].type_path, "crate::source::FixtureDef");
        assert_eq!(views[0].view_name, "FixtureDefView");
        assert_eq!(
            views[0].fields,
            vec![
                StaticSlotViewField {
                    method_name: String::from("render_size"),
                    slot_name: String::from("render_size"),
                    accessor_name: String::from("render_size_accessor"),
                    some_accessor_name: None,
                },
                StaticSlotViewField {
                    method_name: String::from("mode"),
                    slot_name: String::from("mode"),
                    accessor_name: String::from("mode_accessor"),
                    some_accessor_name: None,
                },
                StaticSlotViewField {
                    method_name: String::from("gamma_correction"),
                    slot_name: String::from("gamma"),
                    accessor_name: String::from("gamma_correction_accessor"),
                    some_accessor_name: None,
                },
            ]
        );
    }

    #[test]
    fn generated_code_contains_bootstrap_functions_and_type_paths() {
        let shapes = vec![StaticRegisteredShape {
            type_path: String::from("crate::source::ShaderDef"),
        }];

        let code = render_slot_shapes(&shapes);

        assert!(code.contains("register_all_static_slot_shapes"));
        assert!(code.contains("ensure_static_slot_shape"));
        assert!(code.contains("<crate::source::ShaderDef as ::lpc_model::StaticSlotShape>"));
        assert!(code.contains("MissingReferencedShape"));
    }

    #[test]
    fn generated_view_code_contains_named_view_and_accessors() {
        let views = vec![StaticSlotView {
            type_path: String::from("crate::nodes::texture::TextureDef"),
            view_name: String::from("TextureDefView"),
            fields: vec![
                StaticSlotViewField {
                    method_name: String::from("size"),
                    slot_name: String::from("size"),
                    accessor_name: String::from("size_accessor"),
                    some_accessor_name: None,
                },
                StaticSlotViewField {
                    method_name: String::from("bindings"),
                    slot_name: String::from("bindings"),
                    accessor_name: String::from("bindings_accessor"),
                    some_accessor_name: None,
                },
                StaticSlotViewField {
                    method_name: String::from("brightness"),
                    slot_name: String::from("brightness"),
                    accessor_name: String::from("brightness_accessor"),
                    some_accessor_name: Some(String::from("brightness_some_accessor")),
                },
            ],
        }];

        let code = render_slot_views(&views);

        assert!(code.contains("pub struct TextureDefView"));
        assert!(code.contains("pub fn get_or_compile"));
        assert!(code.contains("pub fn size(&self) -> ::lpc_model::SlotFieldReader<'_>"));
        assert!(code.contains("pub fn brightness(&self) -> ::lpc_model::SlotOptionReader<'_>"));
        assert!(code.contains("SlotPath::parse(\"brightness.some\")"));
        assert!(
            code.contains("<crate::nodes::texture::TextureDef as ::lpc_model::StaticSlotShape>")
        );
    }

    #[test]
    fn empty_generated_code_avoids_unused_warnings() {
        let code = render_slot_shapes(&[]);

        assert!(code.contains("_registry"));
        assert!(code.contains("_id"));
        assert!(!code.contains("ensure_referenced_static_slot_shapes"));
    }
}
