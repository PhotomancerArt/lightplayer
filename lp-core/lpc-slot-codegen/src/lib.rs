//! Build-time slot shape bootstrap generator.
//!
//! This crate is host-only. A crate build script points it at that crate's
//! source tree, and it writes an `OUT_DIR` Rust module that can register every
//! static `SlotRecord` shape discovered in that crate. Runtime-owned dynamic
//! shapes are intentionally outside this discovery pass.

use std::{
    error::Error as StdError,
    fmt, fs, io,
    path::{Path, PathBuf},
};

/// Configuration for generating a static slot-shape bootstrap module.
pub struct SlotShapeCodegenConfig {
    pub crate_root: PathBuf,
    pub out_file: PathBuf,
}

/// Configuration for generating typed slot-view helpers.
pub struct SlotViewCodegenConfig {
    pub crate_root: PathBuf,
    pub out_file: PathBuf,
}

/// Configuration for the M2 mockup slot-codec generation experiment.
pub struct MockupSlotCodecCodegenConfig {
    pub out_file: PathBuf,
}

/// Generate `slot_shapes.rs` for one crate.
pub fn generate_slot_shapes(config: SlotShapeCodegenConfig) -> Result<(), SlotShapeCodegenError> {
    let src_dir = config.crate_root.join("src");
    let mut shapes = discover_static_registered_shapes(&src_dir)?;
    shapes.sort_by(|a, b| a.type_path.cmp(&b.type_path));

    if let Some(parent) = config.out_file.parent() {
        fs::create_dir_all(parent).map_err(SlotShapeCodegenError::Io)?;
    }
    fs::write(config.out_file, render_slot_shapes(&shapes)).map_err(SlotShapeCodegenError::Io)
}

/// Generate `slot_views.rs` for `#[slot(view)]` records in one crate.
pub fn generate_slot_views(config: SlotViewCodegenConfig) -> Result<(), SlotShapeCodegenError> {
    let src_dir = config.crate_root.join("src");
    let mut views = discover_static_slot_views(&src_dir)?;
    views.sort_by(|a, b| a.type_path.cmp(&b.type_path));

    if let Some(parent) = config.out_file.parent() {
        fs::create_dir_all(parent).map_err(SlotShapeCodegenError::Io)?;
    }
    fs::write(config.out_file, render_slot_views(&views)).map_err(SlotShapeCodegenError::Io)
}

/// Generate the first compact slot-codec mockup module.
///
/// This is intentionally narrow: M2 uses it to validate the generated-code
/// shape and build-script plumbing before broadening to discovered model types.
pub fn generate_mockup_slot_codec(
    config: MockupSlotCodecCodegenConfig,
) -> Result<(), SlotShapeCodegenError> {
    if let Some(parent) = config.out_file.parent() {
        fs::create_dir_all(parent).map_err(SlotShapeCodegenError::Io)?;
    }
    fs::write(config.out_file, render_mockup_slot_codec()).map_err(SlotShapeCodegenError::Io)
}

#[derive(Debug)]
pub enum SlotShapeCodegenError {
    Io(io::Error),
    Parse { path: PathBuf, source: syn::Error },
    MissingSrcDir(PathBuf),
    NonUtf8Path(PathBuf),
}

impl fmt::Display for SlotShapeCodegenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::Parse { path, source } => {
                write!(f, "failed to parse {}: {source}", path.display())
            }
            Self::MissingSrcDir(path) => write!(
                f,
                "crate source directory does not exist: {}",
                path.display()
            ),
            Self::NonUtf8Path(path) => write!(f, "source path is not UTF-8: {}", path.display()),
        }
    }
}

impl StdError for SlotShapeCodegenError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::Parse { source, .. } => Some(source),
            Self::MissingSrcDir(_) | Self::NonUtf8Path(_) => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StaticRegisteredShape {
    type_path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StaticSlotView {
    type_path: String,
    view_name: String,
    fields: Vec<StaticSlotViewField>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StaticSlotViewField {
    method_name: String,
    slot_name: String,
    accessor_name: String,
    some_accessor_name: Option<String>,
}

fn discover_static_registered_shapes(
    src_dir: &Path,
) -> Result<Vec<StaticRegisteredShape>, SlotShapeCodegenError> {
    if !src_dir.is_dir() {
        return Err(SlotShapeCodegenError::MissingSrcDir(src_dir.to_path_buf()));
    }

    let mut files = Vec::new();
    collect_rust_files(src_dir, &mut files)?;
    files.sort();

    let mut shapes = Vec::new();
    for path in files {
        let source = fs::read_to_string(&path).map_err(SlotShapeCodegenError::Io)?;
        let syntax = syn::parse_file(&source).map_err(|source| SlotShapeCodegenError::Parse {
            path: path.clone(),
            source,
        })?;
        for item in syntax.items {
            let syn::Item::Struct(item) = item else {
                continue;
            };
            if !has_slot_record_derive(&item.attrs) {
                continue;
            }
            shapes.push(StaticRegisteredShape {
                type_path: infer_type_path(src_dir, &path, &item.ident.to_string())?,
            });
        }
    }

    shapes.sort_by(|a, b| a.type_path.cmp(&b.type_path));
    Ok(shapes)
}

fn discover_static_slot_views(
    src_dir: &Path,
) -> Result<Vec<StaticSlotView>, SlotShapeCodegenError> {
    if !src_dir.is_dir() {
        return Err(SlotShapeCodegenError::MissingSrcDir(src_dir.to_path_buf()));
    }

    let mut files = Vec::new();
    collect_rust_files(src_dir, &mut files)?;
    files.sort();

    let mut views = Vec::new();
    for path in files {
        let source = fs::read_to_string(&path).map_err(SlotShapeCodegenError::Io)?;
        let syntax = syn::parse_file(&source).map_err(|source| SlotShapeCodegenError::Parse {
            path: path.clone(),
            source,
        })?;
        for item in syntax.items {
            let syn::Item::Struct(item) = item else {
                continue;
            };
            if !has_slot_record_derive(&item.attrs) || !has_slot_view_attr(&item.attrs) {
                continue;
            }
            let type_name = item.ident.to_string();
            views.push(StaticSlotView {
                type_path: infer_type_path(src_dir, &path, &type_name)?,
                view_name: format!("{type_name}View"),
                fields: slot_view_fields(&item),
            });
        }
    }

    Ok(views)
}

fn collect_rust_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), SlotShapeCodegenError> {
    for entry in fs::read_dir(dir).map_err(SlotShapeCodegenError::Io)? {
        let entry = entry.map_err(SlotShapeCodegenError::Io)?;
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
    Ok(())
}

fn has_slot_record_derive(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .filter(|attr| attr.path().is_ident("derive"))
        .any(|attr| {
            attr.meta.require_list().is_ok_and(|meta| {
                meta.tokens
                    .to_string()
                    .split(',')
                    .any(|derive| derive.trim().ends_with("SlotRecord"))
            })
        })
}

fn has_slot_view_attr(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .any(|attr| attr.path().is_ident("slot") && slot_attr_has_flags(attr, &["view"]))
}

fn slot_attr_has_flags(attr: &syn::Attribute, required: &[&str]) -> bool {
    if !attr.path().is_ident("slot") {
        return false;
    }
    let mut found = vec![false; required.len()];
    let _ = attr.parse_nested_meta(|meta| {
        for (index, required) in required.iter().enumerate() {
            if meta.path.is_ident(required) {
                found[index] = true;
            }
        }
        Ok(())
    });
    found.into_iter().all(|flag| flag)
}

fn slot_view_fields(item: &syn::ItemStruct) -> Vec<StaticSlotViewField> {
    let syn::Fields::Named(fields) = &item.fields else {
        return Vec::new();
    };
    fields
        .named
        .iter()
        .filter(|field| !has_slot_skip_attr(&field.attrs))
        .filter_map(|field| {
            let ident = field.ident.as_ref()?;
            let method_name = ident.to_string();
            let slot_name = slot_field_name(field).unwrap_or_else(|| method_name.clone());
            Some(StaticSlotViewField {
                accessor_name: format!("{ident}_accessor"),
                some_accessor_name: type_is_option_slot(&field.ty)
                    .then(|| format!("{ident}_some_accessor")),
                method_name,
                slot_name,
            })
        })
        .collect()
}

fn type_is_option_slot(ty: &syn::Type) -> bool {
    let syn::Type::Path(path) = ty else {
        return false;
    };
    path.path
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "OptionSlot")
}

fn has_slot_skip_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("slot")
            && attr.meta.require_list().is_ok_and(|meta| {
                meta.tokens
                    .to_string()
                    .split(',')
                    .any(|token| token.trim() == "skip")
            })
    })
}

fn slot_field_name(field: &syn::Field) -> Option<String> {
    for attr in &field.attrs {
        if !attr.path().is_ident("slot") {
            continue;
        }
        let mut name = None;
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("name") {
                let value = meta.value()?;
                let lit: syn::LitStr = value.parse()?;
                name = Some(lit.value());
            }
            Ok(())
        });
        if name.is_some() {
            return name;
        }
    }
    None
}

fn infer_type_path(
    src_dir: &Path,
    source_path: &Path,
    type_name: &str,
) -> Result<String, SlotShapeCodegenError> {
    let relative = source_path
        .strip_prefix(src_dir)
        .expect("source path came from source dir");
    let mut components = relative
        .components()
        .map(|component| {
            component
                .as_os_str()
                .to_str()
                .ok_or_else(|| SlotShapeCodegenError::NonUtf8Path(source_path.to_path_buf()))
        })
        .collect::<Result<Vec<_>, _>>()?;

    // Concept files are expected to re-export their headline type from the
    // parent module, so `source/project_def.rs` becomes
    // `crate::source::ProjectDef`. `mod.rs` files naturally use their parent
    // directory path as the module path.
    components.pop().expect("rust file has a filename");

    let modules = components
        .into_iter()
        .filter(|component| !component.is_empty())
        .collect::<Vec<_>>();
    let mut path = String::from("crate");
    for module in modules {
        path.push_str("::");
        path.push_str(module);
    }
    path.push_str("::");
    path.push_str(type_name);
    Ok(path)
}

fn render_slot_shapes(shapes: &[StaticRegisteredShape]) -> String {
    let mut out = String::from("// @generated by lpc-slot-codegen. Do not edit.\n\n");
    out.push_str("pub fn register_all_static_slot_shapes(\n");
    if shapes.is_empty() {
        out.push_str("    _registry: &mut ::lpc_model::SlotShapeRegistry,\n");
    } else {
        out.push_str("    registry: &mut ::lpc_model::SlotShapeRegistry,\n");
    }
    out.push_str(") -> Result<(), ::lpc_model::SlotShapeRegistryError> {\n");
    for shape in shapes {
        out.push_str("    ensure_static_slot_shape(registry, <");
        out.push_str(&shape.type_path);
        out.push_str(" as ::lpc_model::StaticSlotShape>::SHAPE_ID)?;\n");
    }
    out.push_str("    Ok(())\n");
    out.push_str("}\n\n");

    out.push_str("pub fn ensure_static_slot_shape(\n");
    if shapes.is_empty() {
        out.push_str("    _registry: &mut ::lpc_model::SlotShapeRegistry,\n");
        out.push_str("    _id: ::lpc_model::SlotShapeId,\n");
    } else {
        out.push_str("    registry: &mut ::lpc_model::SlotShapeRegistry,\n");
        out.push_str("    id: ::lpc_model::SlotShapeId,\n");
    }
    out.push_str(") -> Result<bool, ::lpc_model::SlotShapeRegistryError> {\n");
    for (index, shape) in shapes.iter().enumerate() {
        if index == 0 {
            out.push_str("    if id == <");
        } else {
            out.push_str("    } else if id == <");
        }
        out.push_str(&shape.type_path);
        out.push_str(" as ::lpc_model::StaticSlotShape>::SHAPE_ID {\n");
        out.push_str("        let inserted = <");
        out.push_str(&shape.type_path);
        out.push_str(" as ::lpc_model::StaticSlotShape>::ensure_registered(registry)?;\n");
        out.push_str("        ensure_referenced_static_slot_shapes(registry, id)?;\n");
        out.push_str("        Ok(inserted)\n");
    }
    if shapes.is_empty() {
        out.push_str("    Ok(false)\n");
    } else {
        out.push_str("    } else {\n");
        out.push_str("        Ok(false)\n");
        out.push_str("    }\n");
    }
    out.push_str("}\n");

    if !shapes.is_empty() {
        out.push_str("\nfn ensure_referenced_static_slot_shapes(\n");
        out.push_str("    registry: &mut ::lpc_model::SlotShapeRegistry,\n");
        out.push_str("    id: ::lpc_model::SlotShapeId,\n");
        out.push_str(") -> Result<(), ::lpc_model::SlotShapeRegistryError> {\n");
        out.push_str("    let refs = registry\n");
        out.push_str("        .get(&id)\n");
        out.push_str("        .map(::lpc_model::SlotShape::referenced_shape_ids)\n");
        out.push_str("        .unwrap_or_default();\n");
        out.push_str("    for ref_id in refs {\n");
        out.push_str("        if registry.contains(&ref_id) {\n");
        out.push_str("            continue;\n");
        out.push_str("        }\n");
        out.push_str("        if !ensure_static_slot_shape(registry, ref_id)? {\n");
        out.push_str("            return Err(::lpc_model::SlotShapeRegistryError::MissingReferencedShape(ref_id));\n");
        out.push_str("        }\n");
        out.push_str("    }\n");
        out.push_str("    Ok(())\n");
        out.push_str("}\n");
    }

    out
}

fn render_slot_views(views: &[StaticSlotView]) -> String {
    let mut out = String::from("// @generated by lpc-slot-codegen. Do not edit.\n\n");
    for view in views {
        render_one_slot_view(&mut out, view);
    }
    out
}

fn render_mockup_slot_codec() -> String {
    let mut out = String::from("// @generated by lpc-slot-codegen. Do not edit.\n\n");
    out.push_str(MOCKUP_SLOT_CODEC_IMPORTS_AND_TYPES);
    out.push_str(MOCKUP_SLOT_CODEC_BUNDLE_READERS);
    out.push_str(&render_mockup_source_types(&mockup_source_codec_module()));
    out.push_str(MOCKUP_SLOT_CODEC_REAL_HELPERS);
    out.push_str(MOCKUP_SLOT_CODEC_WRITERS);
    out
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SlotCodecModule {
    types: Vec<SlotCodecType>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SlotCodecType {
    rust_type: &'static str,
    fn_stem: &'static str,
    kind_expr: &'static str,
    default_expr: Option<&'static str>,
    constructor: SlotCodecConstructor,
    fields: Vec<SlotCodecField>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SlotCodecConstructor {
    expression: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SlotCodecField {
    wire_name: &'static str,
    local_name: &'static str,
    init_expr: &'static str,
    read_expr: &'static str,
    write_expr: Option<&'static str>,
    skip_read: bool,
}

fn mockup_source_codec_module() -> SlotCodecModule {
    SlotCodecModule {
        types: vec![
            SlotCodecType {
                rust_type: "ProjectDef",
                fn_stem: "project_def",
                kind_expr: "ProjectDef::KIND",
                default_expr: None,
                constructor: SlotCodecConstructor {
                    expression: "ProjectDef {\n        kind: ProjectDef::KIND.to_string(),\n        name,\n        nodes: MapSlot::new(nodes),\n    }",
                },
                fields: vec![
                    SlotCodecField {
                        wire_name: "name",
                        local_name: "name",
                        init_expr: "OptionSlot::none()",
                        read_expr: "OptionSlot::some(ValueSlot::new(prop.value().string()?))",
                        write_expr: Some(
                            "if let Some(name) = &project.name.data {\n        object.prop(\"name\").unwrap().string(name.value()).unwrap();\n    }",
                        ),
                        skip_read: false,
                    },
                    SlotCodecField {
                        wire_name: "nodes",
                        local_name: "nodes",
                        init_expr: "BTreeMap::new()",
                        read_expr: "prop.value().string_key_map(read_project_invocation)?",
                        write_expr: Some(
                            "if !project.nodes.is_empty() {\n        object\n            .prop(\"nodes\")\n            .unwrap()\n            .string_key_map(&project.nodes.entries, |value, invocation| {\n                let mut object = value.object()?;\n                object.prop(\"artifact\")?.string(invocation.artifact())?;\n                object.finish()\n            })\n            .unwrap();\n    }",
                        ),
                        skip_read: false,
                    },
                ],
            },
            SlotCodecType {
                rust_type: "OutputDef",
                fn_stem: "output_def",
                kind_expr: "OutputDef::KIND",
                default_expr: Some("OutputDef::default()"),
                constructor: SlotCodecConstructor {
                    expression: "OutputDef::from_codec(pin, options)",
                },
                fields: vec![
                    SlotCodecField {
                        wire_name: "pin",
                        local_name: "pin",
                        init_expr: "defaults.pin()",
                        read_expr: "prop.value().u32()?",
                        write_expr: Some(
                            "object.prop(\"pin\").unwrap().u32(output.pin()).unwrap();",
                        ),
                        skip_read: false,
                    },
                    SlotCodecField {
                        wire_name: "bindings",
                        local_name: "bindings",
                        init_expr: "",
                        read_expr: "",
                        write_expr: None,
                        skip_read: true,
                    },
                    SlotCodecField {
                        wire_name: "options",
                        local_name: "options",
                        init_expr: "defaults.options().cloned()",
                        read_expr: "Some(read_output_driver_options(prop.value())?)",
                        write_expr: Some(
                            "if let Some(options) = output.options() {\n        write_output_driver_options(object.prop(\"options\").unwrap(), options);\n    }",
                        ),
                        skip_read: false,
                    },
                ],
            },
            SlotCodecType {
                rust_type: "TextureDef",
                fn_stem: "texture_def",
                kind_expr: "TextureDef::KIND",
                default_expr: Some("TextureDef::default()"),
                constructor: SlotCodecConstructor {
                    expression: "TextureDef::from_codec(size)",
                },
                fields: vec![
                    SlotCodecField {
                        wire_name: "size",
                        local_name: "size",
                        init_expr: "defaults.size()",
                        read_expr: "read_dim2u(prop.value())?",
                        write_expr: Some(
                            "write_dim2u(object.prop(\"size\").unwrap(), texture.size());",
                        ),
                        skip_read: false,
                    },
                    SlotCodecField {
                        wire_name: "bindings",
                        local_name: "bindings",
                        init_expr: "",
                        read_expr: "",
                        write_expr: None,
                        skip_read: true,
                    },
                ],
            },
            SlotCodecType {
                rust_type: "FixtureDef",
                fn_stem: "fixture_def",
                kind_expr: "FixtureDef::KIND",
                default_expr: Some("FixtureDef::default()"),
                constructor: SlotCodecConstructor {
                    expression: "FixtureDef::from_codec(\n        render_size,\n        mapping,\n        color_order,\n        transform,\n        brightness,\n        gamma_correction,\n    )",
                },
                fields: vec![
                    SlotCodecField {
                        wire_name: "render_size",
                        local_name: "render_size",
                        init_expr: "defaults.render_size()",
                        read_expr: "read_dim2u(prop.value())?",
                        write_expr: Some(
                            "write_dim2u(object.prop(\"render_size\").unwrap(), fixture.render_size());",
                        ),
                        skip_read: false,
                    },
                    SlotCodecField {
                        wire_name: "bindings",
                        local_name: "bindings",
                        init_expr: "",
                        read_expr: "",
                        write_expr: None,
                        skip_read: true,
                    },
                    SlotCodecField {
                        wire_name: "sampling",
                        local_name: "sampling",
                        init_expr: "",
                        read_expr: "",
                        write_expr: None,
                        skip_read: true,
                    },
                    SlotCodecField {
                        wire_name: "mapping",
                        local_name: "mapping",
                        init_expr: "defaults.mapping().clone()",
                        read_expr: "read_mapping_config(prop.value())?",
                        write_expr: Some(
                            "write_mapping_config(object.prop(\"mapping\").unwrap(), fixture.mapping());",
                        ),
                        skip_read: false,
                    },
                    SlotCodecField {
                        wire_name: "color_order",
                        local_name: "color_order",
                        init_expr: "defaults.color_order()",
                        read_expr: "{\n                let text = prop.value().string()?;\n                ColorOrderValue::parse(&text).unwrap_or(color_order)\n            }",
                        write_expr: Some(
                            "object\n        .prop(\"color_order\")\n        .unwrap()\n        .string(fixture.color_order().as_str())\n        .unwrap();",
                        ),
                        skip_read: false,
                    },
                    SlotCodecField {
                        wire_name: "transform",
                        local_name: "transform",
                        init_expr: "defaults.transform()",
                        read_expr: "read_affine2d(prop.value())?",
                        write_expr: Some(
                            "write_affine2d(object.prop(\"transform\").unwrap(), fixture.transform());",
                        ),
                        skip_read: false,
                    },
                    SlotCodecField {
                        wire_name: "brightness",
                        local_name: "brightness",
                        init_expr: "defaults.brightness().cloned()",
                        read_expr: "Some(read_scalar_hint(prop.value())?)",
                        write_expr: Some(
                            "if let Some(brightness) = fixture.brightness() {\n        write_scalar_hint(object.prop(\"brightness\").unwrap(), brightness);\n    }",
                        ),
                        skip_read: false,
                    },
                    SlotCodecField {
                        wire_name: "gamma_correction",
                        local_name: "gamma_correction",
                        init_expr: "defaults.gamma_correction()",
                        read_expr: "Some(prop.value().bool()?)",
                        write_expr: Some(
                            "if let Some(gamma_correction) = fixture.gamma_correction() {\n        object\n            .prop(\"gamma_correction\")\n            .unwrap()\n            .bool(gamma_correction)\n            .unwrap();\n    }",
                        ),
                        skip_read: false,
                    },
                ],
            },
            SlotCodecType {
                rust_type: "ShaderDef",
                fn_stem: "shader_def",
                kind_expr: "ShaderDef::KIND",
                default_expr: Some("ShaderDef::default()"),
                constructor: SlotCodecConstructor {
                    expression: "ShaderDef::from_codec(\n        glsl_path,\n        render_order,\n        glsl_opts,\n        param_defs,\n    )",
                },
                fields: vec![
                    SlotCodecField {
                        wire_name: "glsl_path",
                        local_name: "glsl_path",
                        init_expr: "defaults.glsl_path().to_string()",
                        read_expr: "prop.value().string()?",
                        write_expr: Some(
                            "object.prop(\"glsl_path\").unwrap().string(shader.glsl_path()).unwrap();",
                        ),
                        skip_read: false,
                    },
                    SlotCodecField {
                        wire_name: "render_order",
                        local_name: "render_order",
                        init_expr: "defaults.render_order()",
                        read_expr: "prop.value().i32()?",
                        write_expr: Some(
                            "object\n        .prop(\"render_order\")\n        .unwrap()\n        .i32(shader.render_order())\n        .unwrap();",
                        ),
                        skip_read: false,
                    },
                    SlotCodecField {
                        wire_name: "bindings",
                        local_name: "bindings",
                        init_expr: "",
                        read_expr: "",
                        write_expr: None,
                        skip_read: true,
                    },
                    SlotCodecField {
                        wire_name: "glsl_opts",
                        local_name: "glsl_opts",
                        init_expr: "defaults.glsl_opts().clone()",
                        read_expr: "read_glsl_opts(prop.value())?",
                        write_expr: Some(
                            "write_glsl_opts(object.prop(\"glsl_opts\").unwrap(), shader.glsl_opts());",
                        ),
                        skip_read: false,
                    },
                    SlotCodecField {
                        wire_name: "param_defs",
                        local_name: "param_defs",
                        init_expr: "defaults.param_defs.entries.clone()",
                        read_expr: "prop.value().string_key_map(read_shader_param_def)?",
                        write_expr: Some(
                            "object\n        .prop(\"param_defs\")\n        .unwrap()\n        .string_key_map(&shader.param_defs.entries, |value, param| {\n            write_shader_param_def(value, param);\n            Ok(())\n        })\n        .unwrap();",
                        ),
                        skip_read: false,
                    },
                ],
            },
        ],
    }
}

fn render_mockup_source_types(module: &SlotCodecModule) -> String {
    let mut out = String::new();
    for codec_type in &module.types {
        render_slot_codec_type(&mut out, codec_type);
    }
    out
}

fn render_slot_codec_type(out: &mut String, codec_type: &SlotCodecType) {
    render_slot_codec_type_read_wrappers(out, codec_type);
    render_slot_codec_type_reader(out, codec_type);
    render_slot_codec_type_writer(out, codec_type);
}

fn render_slot_codec_type_read_wrappers(out: &mut String, codec_type: &SlotCodecType) {
    out.push_str("pub fn read_");
    out.push_str(codec_type.fn_stem);
    out.push_str("_json(json: &str) -> Result<");
    out.push_str(codec_type.rust_type);
    out.push_str(", SyntaxError> {\n");
    out.push_str("    let registry = SlotShapeRegistry::default();\n");
    out.push_str(
        "    let mut reader = SlotReader::new(JsonSyntaxSource::new(json)?, &registry);\n",
    );
    out.push_str("    read_");
    out.push_str(codec_type.fn_stem);
    out.push_str("(&mut reader)\n");
    out.push_str("}\n\n");

    out.push_str("pub fn read_");
    out.push_str(codec_type.fn_stem);
    out.push_str("_toml(value: &toml::Value) -> Result<");
    out.push_str(codec_type.rust_type);
    out.push_str(", SyntaxError> {\n");
    out.push_str("    let registry = SlotShapeRegistry::default();\n");
    out.push_str(
        "    let mut reader = SlotReader::new(TomlSyntaxSource::new(value)?, &registry);\n",
    );
    out.push_str("    read_");
    out.push_str(codec_type.fn_stem);
    out.push_str("(&mut reader)\n");
    out.push_str("}\n\n");
}

fn render_slot_codec_type_reader(out: &mut String, codec_type: &SlotCodecType) {
    out.push_str("pub fn read_");
    out.push_str(codec_type.fn_stem);
    out.push_str("<S>(reader: &mut SlotReader<'_, S>) -> Result<");
    out.push_str(codec_type.rust_type);
    out.push_str(", SyntaxError>\nwhere\n    S: SyntaxEventSource,\n{\n");
    render_slot_codec_type_fields_const(out, codec_type);
    if let Some(default_expr) = codec_type.default_expr {
        out.push_str("    let defaults = ");
        out.push_str(default_expr);
        out.push_str(";\n");
    }
    for field in codec_type.fields.iter().filter(|field| !field.skip_read) {
        out.push_str("    let mut ");
        out.push_str(field.local_name);
        out.push_str(" = ");
        out.push_str(field.init_expr);
        out.push_str(";\n");
    }
    out.push_str("    let mut object = reader.object()?;\n");
    out.push_str("    let _kind = object.expect_discriminator(\"kind\", &[");
    out.push_str(codec_type.kind_expr);
    out.push_str("])?;\n");
    out.push_str("    while let Some(mut prop) = object.next_prop()? {\n");
    out.push_str("        match prop.name() {\n");
    for field in &codec_type.fields {
        out.push_str("            \"");
        out.push_str(field.wire_name);
        out.push_str("\" => ");
        if field.skip_read {
            out.push_str("prop.value().skip_value()?,\n");
        } else {
            out.push_str(field.local_name);
            out.push_str(" = ");
            out.push_str(field.read_expr);
            out.push_str(",\n");
        }
    }
    out.push_str("            other => return Err(prop.unknown_field(other, FIELDS)),\n");
    out.push_str("        }\n");
    out.push_str("    }\n");
    out.push_str("    Ok(");
    out.push_str(codec_type.constructor.expression);
    out.push_str(")\n");
    out.push_str("}\n\n");
}

fn render_slot_codec_type_writer(out: &mut String, codec_type: &SlotCodecType) {
    let arg_name = codec_type
        .fn_stem
        .strip_suffix("_def")
        .unwrap_or(codec_type.fn_stem);
    out.push_str("pub fn write_");
    out.push_str(codec_type.fn_stem);
    out.push_str("_json(");
    out.push_str(arg_name);
    out.push_str(": &");
    out.push_str(codec_type.rust_type);
    out.push_str(") -> Vec<u8> {\n");
    out.push_str("    let mut out = Vec::new();\n");
    out.push_str("    let mut writer = SlotJsonWriter::new(&mut out);\n");
    out.push_str("    let mut object = writer.object().unwrap();\n");
    out.push_str("    object.prop(\"kind\").unwrap().string(");
    out.push_str(codec_type.kind_expr);
    out.push_str(").unwrap();\n");
    for field in codec_type
        .fields
        .iter()
        .filter_map(|field| field.write_expr)
    {
        push_indented_block(out, field, "    ");
    }
    out.push_str("    object.finish().unwrap();\n");
    out.push_str("    out\n");
    out.push_str("}\n\n");
}

fn render_slot_codec_type_fields_const(out: &mut String, codec_type: &SlotCodecType) {
    out.push_str("    const FIELDS: &[&str] = &[\"kind\"");
    for field in &codec_type.fields {
        out.push_str(", \"");
        out.push_str(field.wire_name);
        out.push('"');
    }
    out.push_str("];\n");
}

fn push_indented_block(out: &mut String, block: &str, indent: &str) {
    for line in block.lines() {
        out.push_str(indent);
        out.push_str(line);
        out.push('\n');
    }
}

const MOCKUP_SLOT_CODEC_IMPORTS_AND_TYPES: &str = r#"
use std::collections::BTreeMap;

use crate::source::{
    FixtureDef, MappingConfig, NodeInvocationDef, OutputDef, OutputDriverOptionsConfig, PathSpec,
    ProjectDef, RingOrder, ScalarHint, ShaderDef, ShaderParamDef, TextureDef,
};
use lpc_model::{
    AddSubMode, Affine2d, ColorOrderValue, Dim2u, DivMode, GlslOpts, MapSlot, MulMode,
    OptionSlot, SlotEnumAccess, ValueSlot,
};
use lpc_model::SlotShapeRegistry;
use lpc_model::slot_codec::{
    JsonSyntaxSource, ObjectReader, SlotJsonValue, SlotJsonWrite, SlotJsonWriter, SlotReader,
    SyntaxError, SyntaxEventSource, TomlSyntaxSource, ValueReader,
};

#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedBundle {
    pub project: GeneratedProject,
    pub nodes: Vec<GeneratedNodeDef>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedProject {
    pub name: Option<String>,
    pub nodes: BTreeMap<String, GeneratedInvocation>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedInvocation {
    pub artifact: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum GeneratedNodeDef {
    Output(GeneratedOutputDef),
    Fixture(GeneratedFixtureDef),
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedOutputDef {
    pub pin: u32,
    pub bindings: BTreeMap<String, GeneratedBindingDef>,
    pub options: Option<GeneratedOutputOptions>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedOutputOptions {
    pub white_point: [f32; 3],
    pub brightness: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedFixtureDef {
    pub mapping: GeneratedMapping,
}

#[derive(Clone, Debug, PartialEq)]
pub enum GeneratedMapping {
    Disabled,
    Square { origin: [f32; 2], size: [f32; 2] },
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedBindingDef {
    pub source: Option<GeneratedEndpoint>,
    pub target: Option<GeneratedEndpoint>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum GeneratedEndpoint {
    Ref(String),
    Value(f32),
}
"#;

const MOCKUP_SLOT_CODEC_BUNDLE_READERS: &str = r#"
pub fn read_bundle_json(json: &str) -> Result<GeneratedBundle, SyntaxError> {
    let registry = SlotShapeRegistry::default();
    let mut reader = SlotReader::new(JsonSyntaxSource::new(json)?, &registry);
    read_bundle(&mut reader)
}

pub fn read_bundle_toml(value: &toml::Value) -> Result<GeneratedBundle, SyntaxError> {
    let registry = SlotShapeRegistry::default();
    let mut reader = SlotReader::new(TomlSyntaxSource::new(value)?, &registry);
    read_bundle(&mut reader)
}

pub fn read_bundle<S>(reader: &mut SlotReader<'_, S>) -> Result<GeneratedBundle, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["project", "node_defs"];
    let mut project = None;
    let mut nodes = None;
    let mut object = reader.object()?;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "project" => project = Some(read_project(prop.value())?),
            "node_defs" => nodes = Some(read_node_defs(prop.value())?),
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }
    Ok(GeneratedBundle {
        project: project.ok_or_else(|| object.missing_required_field("project"))?,
        nodes: nodes.ok_or_else(|| object.missing_required_field("node_defs"))?,
    })
}

fn read_project<S>(value: ValueReader<'_, '_, S>) -> Result<GeneratedProject, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["kind", "name", "nodes"];
    let mut object = value.object()?;
    let _kind = object.expect_discriminator("kind", &["ProjectDef"])?;
    let mut name = None;
    let mut nodes = None;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "name" => name = Some(prop.value().string()?),
            "nodes" => nodes = Some(prop.value().string_key_map(read_invocation)?),
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }
    Ok(GeneratedProject {
        name,
        nodes: nodes.ok_or_else(|| object.missing_required_field("nodes"))?,
    })
}

fn read_invocation<S>(value: ValueReader<'_, '_, S>) -> Result<GeneratedInvocation, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["artifact"];
    let mut artifact = None;
    let mut object = value.object()?;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "artifact" => artifact = Some(prop.value().string()?),
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }
    Ok(GeneratedInvocation {
        artifact: artifact.ok_or_else(|| object.missing_required_field("artifact"))?,
    })
}

fn read_node_defs<S>(value: ValueReader<'_, '_, S>) -> Result<Vec<GeneratedNodeDef>, SyntaxError>
where
    S: SyntaxEventSource,
{
    let mut nodes = Vec::new();
    let mut array = value.array()?;
    while let Some(item) = array.next_item()? {
        nodes.push(read_node_def(item)?);
    }
    Ok(nodes)
}

fn read_node_def<S>(value: ValueReader<'_, '_, S>) -> Result<GeneratedNodeDef, SyntaxError>
where
    S: SyntaxEventSource,
{
    let mut object = value.object()?;
    let kind = object.expect_discriminator("kind", &["OutputDef", "FixtureDef"])?;
    match kind.as_str() {
        "OutputDef" => read_output_body(object).map(GeneratedNodeDef::Output),
        "FixtureDef" => read_fixture_body(object).map(GeneratedNodeDef::Fixture),
        _ => unreachable!("expect_discriminator validated variants"),
    }
}

fn read_output_body<S>(mut object: ObjectReader<'_, '_, S>) -> Result<GeneratedOutputDef, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["kind", "pin", "bindings", "options"];
    let mut pin = None;
    let mut bindings = BTreeMap::new();
    let mut options = None;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "pin" => pin = Some(prop.value().u32()?),
            "bindings" => bindings = prop.value().string_key_map(read_binding_def)?,
            "options" => options = Some(read_output_options(prop.value())?),
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }
    Ok(GeneratedOutputDef {
        pin: pin.ok_or_else(|| object.missing_required_field("pin"))?,
        bindings,
        options,
    })
}

fn read_output_options<S>(
    value: ValueReader<'_, '_, S>,
) -> Result<GeneratedOutputOptions, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["white_point", "brightness"];
    let mut white_point = None;
    let mut brightness = None;
    let mut object = value.object()?;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "white_point" => white_point = Some(prop.value().f32_array()?),
            "brightness" => brightness = Some(prop.value().f32()?),
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }
    Ok(GeneratedOutputOptions {
        white_point: white_point.ok_or_else(|| object.missing_required_field("white_point"))?,
        brightness: brightness.ok_or_else(|| object.missing_required_field("brightness"))?,
    })
}

fn read_fixture_body<S>(
    mut object: ObjectReader<'_, '_, S>,
) -> Result<GeneratedFixtureDef, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["kind", "mapping"];
    let mut mapping = None;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "mapping" => mapping = Some(read_mapping(prop.value())?),
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }
    Ok(GeneratedFixtureDef {
        mapping: mapping.ok_or_else(|| object.missing_required_field("mapping"))?,
    })
}

fn read_mapping<S>(value: ValueReader<'_, '_, S>) -> Result<GeneratedMapping, SyntaxError>
where
    S: SyntaxEventSource,
{
    let mut object = value.object()?;
    let kind = object.expect_discriminator("kind", &["Disabled", "Square"])?;
    match kind.as_str() {
        "Disabled" => {
            object.finish()?;
            Ok(GeneratedMapping::Disabled)
        }
        "Square" => read_square_body(object),
        _ => unreachable!("expect_discriminator validated variants"),
    }
}

fn read_square_body<S>(mut object: ObjectReader<'_, '_, S>) -> Result<GeneratedMapping, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["kind", "origin", "size"];
    let mut origin = None;
    let mut size = None;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "origin" => origin = Some(prop.value().f32_array()?),
            "size" => size = Some(prop.value().f32_array()?),
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }
    Ok(GeneratedMapping::Square {
        origin: origin.ok_or_else(|| object.missing_required_field("origin"))?,
        size: size.ok_or_else(|| object.missing_required_field("size"))?,
    })
}

fn read_binding_def<S>(value: ValueReader<'_, '_, S>) -> Result<GeneratedBindingDef, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["source", "target"];
    let mut source = None;
    let mut target = None;
    let mut object = value.object()?;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "source" => source = Some(read_endpoint(prop.value())?),
            "target" => target = Some(read_endpoint(prop.value())?),
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }
    Ok(GeneratedBindingDef { source, target })
}

fn read_endpoint<S>(value: ValueReader<'_, '_, S>) -> Result<GeneratedEndpoint, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["ref", "value"];
    let mut reference = None;
    let mut value_endpoint = None;
    let mut object = value.object()?;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "ref" => reference = Some(prop.value().string()?),
            "value" => value_endpoint = Some(prop.value().f32()?),
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }
    match (reference, value_endpoint) {
        (Some(reference), None) => Ok(GeneratedEndpoint::Ref(reference)),
        (None, Some(value)) => Ok(GeneratedEndpoint::Value(value)),
        _ => Err(object.missing_required_field("ref or value")),
    }
}
"#;

const MOCKUP_SLOT_CODEC_REAL_HELPERS: &str = r#"fn read_project_invocation<S>(
    value: ValueReader<'_, '_, S>,
) -> Result<NodeInvocationDef, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["artifact"];
    let mut artifact = None;
    let mut object = value.object()?;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "artifact" => artifact = Some(prop.value().string()?),
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }
    Ok(NodeInvocationDef::new(
        &artifact.ok_or_else(|| object.missing_required_field("artifact"))?,
    ))
}

fn read_output_driver_options<S>(
    value: ValueReader<'_, '_, S>,
) -> Result<OutputDriverOptionsConfig, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &[
        "lum_power",
        "white_point",
        "brightness",
        "interpolation_enabled",
        "dithering_enabled",
        "lut_enabled",
    ];
    let defaults = OutputDriverOptionsConfig::default();
    let mut lum_power = defaults.lum_power();
    let mut white_point = defaults.white_point();
    let mut brightness = defaults.brightness();
    let mut interpolation_enabled = defaults.interpolation_enabled();
    let mut dithering_enabled = defaults.dithering_enabled();
    let mut lut_enabled = defaults.lut_enabled();
    let mut object = value.object()?;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "lum_power" => lum_power = prop.value().f32()?,
            "white_point" => white_point = prop.value().f32_array()?,
            "brightness" => brightness = prop.value().f32()?,
            "interpolation_enabled" => interpolation_enabled = prop.value().bool()?,
            "dithering_enabled" => dithering_enabled = prop.value().bool()?,
            "lut_enabled" => lut_enabled = prop.value().bool()?,
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }
    Ok(OutputDriverOptionsConfig::from_codec(
        lum_power,
        white_point,
        brightness,
        interpolation_enabled,
        dithering_enabled,
        lut_enabled,
    ))
}

fn write_output_driver_options<W>(
    value: SlotJsonValue<'_, W>,
    options: &OutputDriverOptionsConfig,
)
where
    W: SlotJsonWrite,
    W::Error: core::fmt::Debug,
{
    let mut object = value.object().unwrap();
    object.prop("lum_power").unwrap().f32(options.lum_power()).unwrap();
    object
        .prop("white_point")
        .unwrap()
        .f32_array(&options.white_point())
        .unwrap();
    object
        .prop("brightness")
        .unwrap()
        .f32(options.brightness())
        .unwrap();
    object
        .prop("interpolation_enabled")
        .unwrap()
        .bool(options.interpolation_enabled())
        .unwrap();
    object
        .prop("dithering_enabled")
        .unwrap()
        .bool(options.dithering_enabled())
        .unwrap();
    object.prop("lut_enabled").unwrap().bool(options.lut_enabled()).unwrap();
    object.finish().unwrap();
}

fn read_dim2u<S>(value: ValueReader<'_, '_, S>) -> Result<Dim2u, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["width", "height"];
    let mut width = None;
    let mut height = None;
    let mut object = value.object()?;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "width" => width = Some(prop.value().u32()?),
            "height" => height = Some(prop.value().u32()?),
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }
    Ok(Dim2u {
        width: width.ok_or_else(|| object.missing_required_field("width"))?,
        height: height.ok_or_else(|| object.missing_required_field("height"))?,
    })
}

fn write_dim2u<W>(value: SlotJsonValue<'_, W>, size: Dim2u)
where
    W: SlotJsonWrite,
    W::Error: core::fmt::Debug,
{
    let mut object = value.object().unwrap();
    object.prop("width").unwrap().u32(size.width).unwrap();
    object.prop("height").unwrap().u32(size.height).unwrap();
    object.finish().unwrap();
}

fn read_affine2d<S>(value: ValueReader<'_, '_, S>) -> Result<Affine2d, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["m00", "m01", "m10", "m11", "tx", "ty"];
    let mut transform = Affine2d::identity();
    let mut object = value.object()?;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "m00" => transform.m00 = prop.value().f32()?,
            "m01" => transform.m01 = prop.value().f32()?,
            "m10" => transform.m10 = prop.value().f32()?,
            "m11" => transform.m11 = prop.value().f32()?,
            "tx" => transform.tx = prop.value().f32()?,
            "ty" => transform.ty = prop.value().f32()?,
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }
    Ok(transform)
}

fn read_scalar_hint<S>(value: ValueReader<'_, '_, S>) -> Result<ScalarHint, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["value"];
    let mut scalar = None;
    let mut object = value.object()?;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "value" => scalar = Some(prop.value().f32()?),
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }
    Ok(ScalarHint::new(
        scalar.ok_or_else(|| object.missing_required_field("value"))?,
    ))
}

fn read_mapping_config<S>(value: ValueReader<'_, '_, S>) -> Result<MappingConfig, SyntaxError>
where
    S: SyntaxEventSource,
{
    let mut object = value.object()?;
    let kind = object.expect_discriminator("kind", &["disabled", "square", "path_points"])?;
    match kind.as_str() {
        "disabled" => {
            object.finish()?;
            Ok(MappingConfig::disabled())
        }
        "square" => read_mapping_square_body(object),
        "path_points" => read_mapping_path_points_body(object),
        _ => unreachable!("expect_discriminator validated variants"),
    }
}

fn read_mapping_square_body<S>(
    mut object: ObjectReader<'_, '_, S>,
) -> Result<MappingConfig, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["kind", "origin", "size"];
    let mut origin = None;
    let mut size = None;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "origin" => origin = Some(prop.value().f32_array()?),
            "size" => size = Some(prop.value().f32_array()?),
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }
    Ok(MappingConfig::square_from_codec(
        origin.ok_or_else(|| object.missing_required_field("origin"))?,
        size.ok_or_else(|| object.missing_required_field("size"))?,
    ))
}

fn read_mapping_path_points_body<S>(
    mut object: ObjectReader<'_, '_, S>,
) -> Result<MappingConfig, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["kind", "paths", "sample_diameter"];
    let mut paths = None;
    let mut sample_diameter = None;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "paths" => paths = Some(prop.value().u32_key_map(read_path_spec)?),
            "sample_diameter" => sample_diameter = Some(prop.value().f32()?),
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }
    Ok(MappingConfig::path_points(
        MapSlot::new(paths.ok_or_else(|| object.missing_required_field("paths"))?),
        sample_diameter.ok_or_else(|| object.missing_required_field("sample_diameter"))?,
    ))
}

fn read_path_spec<S>(value: ValueReader<'_, '_, S>) -> Result<PathSpec, SyntaxError>
where
    S: SyntaxEventSource,
{
    let mut object = value.object()?;
    let kind = object.expect_discriminator("kind", &["ring_array", "manual"])?;
    match kind.as_str() {
        "manual" => {
            object.finish()?;
            Ok(PathSpec::manual())
        }
        "ring_array" => read_ring_array_body(object),
        _ => unreachable!("expect_discriminator validated variants"),
    }
}

fn read_ring_array_body<S>(mut object: ObjectReader<'_, '_, S>) -> Result<PathSpec, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &[
        "kind",
        "center",
        "diameter",
        "start_ring_inclusive",
        "end_ring_exclusive",
        "ring_lamp_counts",
        "offset_angle",
        "order",
    ];
    let mut center = None;
    let mut diameter = None;
    let mut start_ring_inclusive = None;
    let mut end_ring_exclusive = None;
    let mut ring_lamp_counts = None;
    let mut offset_angle = None;
    let mut order = None;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "center" => center = Some(prop.value().f32_array()?),
            "diameter" => diameter = Some(prop.value().f32()?),
            "start_ring_inclusive" => start_ring_inclusive = Some(prop.value().u32()?),
            "end_ring_exclusive" => end_ring_exclusive = Some(prop.value().u32()?),
            "ring_lamp_counts" => {
                ring_lamp_counts = Some(
                    prop.value()
                        .u32_key_map(|value| value.u32().map(ValueSlot::new))?,
                )
            }
            "offset_angle" => offset_angle = Some(prop.value().f32()?),
            "order" => {
                let text = prop.value().string()?;
                order = Some(RingOrder::parse(&text).unwrap_or_default());
            }
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }
    Ok(PathSpec::ring_array(
        center.ok_or_else(|| object.missing_required_field("center"))?,
        diameter.ok_or_else(|| object.missing_required_field("diameter"))?,
        start_ring_inclusive
            .ok_or_else(|| object.missing_required_field("start_ring_inclusive"))?,
        end_ring_exclusive.ok_or_else(|| object.missing_required_field("end_ring_exclusive"))?,
        MapSlot::new(
            ring_lamp_counts.ok_or_else(|| object.missing_required_field("ring_lamp_counts"))?,
        ),
        offset_angle.ok_or_else(|| object.missing_required_field("offset_angle"))?,
        order.ok_or_else(|| object.missing_required_field("order"))?,
    ))
}

fn write_affine2d<W>(value: SlotJsonValue<'_, W>, transform: Affine2d)
where
    W: SlotJsonWrite,
    W::Error: core::fmt::Debug,
{
    let mut object = value.object().unwrap();
    object.prop("m00").unwrap().f32(transform.m00).unwrap();
    object.prop("m01").unwrap().f32(transform.m01).unwrap();
    object.prop("m10").unwrap().f32(transform.m10).unwrap();
    object.prop("m11").unwrap().f32(transform.m11).unwrap();
    object.prop("tx").unwrap().f32(transform.tx).unwrap();
    object.prop("ty").unwrap().f32(transform.ty).unwrap();
    object.finish().unwrap();
}

fn write_scalar_hint<W>(value: SlotJsonValue<'_, W>, hint: &ScalarHint)
where
    W: SlotJsonWrite,
    W::Error: core::fmt::Debug,
{
    let mut object = value.object().unwrap();
    object.prop("value").unwrap().f32(hint.value()).unwrap();
    object.finish().unwrap();
}

fn write_mapping_config<W>(value: SlotJsonValue<'_, W>, mapping: &MappingConfig)
where
    W: SlotJsonWrite,
    W::Error: core::fmt::Debug,
{
    let mut object = value.object().unwrap();
    match mapping.variant() {
        "disabled" => {
            object.prop("kind").unwrap().string("disabled").unwrap();
        }
        "square" => {
            let (origin, size) = mapping.square_fields().unwrap();
            object.prop("kind").unwrap().string("square").unwrap();
            object.prop("origin").unwrap().f32_array(&origin).unwrap();
            object.prop("size").unwrap().f32_array(&size).unwrap();
        }
        "path_points" => {
            let (paths, sample_diameter) = mapping.path_points_fields().unwrap();
            object.prop("kind").unwrap().string("path_points").unwrap();
            object
                .prop("paths")
                .unwrap()
                .u32_key_map(&paths.entries, |value, path| {
                    write_path_spec(value, path);
                    Ok(())
                })
                .unwrap();
            object
                .prop("sample_diameter")
                .unwrap()
                .f32(sample_diameter)
                .unwrap();
        }
        _ => unreachable!("known mapping variant"),
    }
    object.finish().unwrap();
}

fn write_path_spec<W>(value: SlotJsonValue<'_, W>, path: &PathSpec)
where
    W: SlotJsonWrite,
    W::Error: core::fmt::Debug,
{
    let mut object = value.object().unwrap();
    match path.variant() {
        "manual" => object.prop("kind").unwrap().string("manual").unwrap(),
        "ring_array" => {
            let (
                center,
                diameter,
                start_ring_inclusive,
                end_ring_exclusive,
                ring_lamp_counts,
                offset_angle,
                order,
            ) = path.ring_array_fields().unwrap();
            object.prop("kind").unwrap().string("ring_array").unwrap();
            object.prop("center").unwrap().f32_array(&center).unwrap();
            object.prop("diameter").unwrap().f32(diameter).unwrap();
            object
                .prop("start_ring_inclusive")
                .unwrap()
                .u32(start_ring_inclusive)
                .unwrap();
            object
                .prop("end_ring_exclusive")
                .unwrap()
                .u32(end_ring_exclusive)
                .unwrap();
            object
                .prop("ring_lamp_counts")
                .unwrap()
                .u32_key_map(&ring_lamp_counts.entries, |value, count| value.u32(*count.value()))
                .unwrap();
            object.prop("offset_angle").unwrap().f32(offset_angle).unwrap();
            object.prop("order").unwrap().string(order.as_str()).unwrap();
        }
        _ => unreachable!("known path variant"),
    }
    object.finish().unwrap();
}

fn read_glsl_opts<S>(value: ValueReader<'_, '_, S>) -> Result<GlslOpts, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["add_sub", "mul", "div"];
    let defaults = GlslOpts::default();
    let mut add_sub = *defaults.add_sub.value();
    let mut mul = *defaults.mul.value();
    let mut div = *defaults.div.value();
    let mut object = value.object()?;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "add_sub" => {
                let text = prop.value().string()?;
                add_sub = AddSubMode::parse(&text).unwrap_or(add_sub);
            }
            "mul" => {
                let text = prop.value().string()?;
                mul = MulMode::parse(&text).unwrap_or(mul);
            }
            "div" => {
                let text = prop.value().string()?;
                div = DivMode::parse(&text).unwrap_or(div);
            }
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }
    Ok(GlslOpts {
        add_sub: ValueSlot::new(add_sub),
        mul: ValueSlot::new(mul),
        div: ValueSlot::new(div),
    })
}

fn read_shader_param_def<S>(value: ValueReader<'_, '_, S>) -> Result<ShaderParamDef, SyntaxError>
where
    S: SyntaxEventSource,
{
    const FIELDS: &[&str] = &["label", "description", "value_type", "default", "min"];
    let mut label = None;
    let mut description = None;
    let mut value_type = String::from("f32");
    let mut default = None;
    let mut min = None;
    let mut object = value.object()?;
    while let Some(mut prop) = object.next_prop()? {
        match prop.name() {
            "label" => label = Some(prop.value().string()?),
            "description" => description = Some(prop.value().string()?),
            "value_type" => value_type = prop.value().string()?,
            "default" => default = Some(prop.value().f32()?),
            "min" => min = Some(read_scalar_hint(prop.value())?.value()),
            other => return Err(prop.unknown_field(other, FIELDS)),
        }
    }
    let mut param = ShaderParamDef::new(
        &label.ok_or_else(|| object.missing_required_field("label"))?,
        &description.ok_or_else(|| object.missing_required_field("description"))?,
        default.ok_or_else(|| object.missing_required_field("default"))?,
        min,
    );
    param.set_value_type_for_codec(&value_type);
    Ok(param)
}

fn write_glsl_opts<W>(value: SlotJsonValue<'_, W>, opts: &GlslOpts)
where
    W: SlotJsonWrite,
    W::Error: core::fmt::Debug,
{
    let mut object = value.object().unwrap();
    object.prop("add_sub").unwrap().string(opts.add_sub.value().as_str()).unwrap();
    object.prop("mul").unwrap().string(opts.mul.value().as_str()).unwrap();
    object.prop("div").unwrap().string(opts.div.value().as_str()).unwrap();
    object.finish().unwrap();
}

fn write_shader_param_def<W>(value: SlotJsonValue<'_, W>, param: &ShaderParamDef)
where
    W: SlotJsonWrite,
    W::Error: core::fmt::Debug,
{
    let mut object = value.object().unwrap();
    object.prop("label").unwrap().string(param.label()).unwrap();
    object
        .prop("description")
        .unwrap()
        .string(param.description())
        .unwrap();
    object
        .prop("value_type")
        .unwrap()
        .string(param.value_type())
        .unwrap();
    object.prop("default").unwrap().f32(param.default_scalar()).unwrap();
    if let Some(min) = param.min() {
        write_scalar_hint(object.prop("min").unwrap(), min);
    }
    object.finish().unwrap();
}
"#;

const MOCKUP_SLOT_CODEC_WRITERS: &str = r#"
pub fn write_bundle_json(bundle: &GeneratedBundle) -> Vec<u8> {
    let mut out = Vec::new();
    let mut writer = SlotJsonWriter::new(&mut out);
    let mut object = writer.object().unwrap();
    write_project(object.prop("project").unwrap(), &bundle.project);
    let mut nodes = object.prop("node_defs").unwrap().array().unwrap();
    for node in &bundle.nodes {
        write_node(nodes.item().unwrap(), node);
    }
    nodes.finish().unwrap();
    object.finish().unwrap();
    out
}

fn write_project<W>(value: SlotJsonValue<'_, W>, project: &GeneratedProject)
where
    W: SlotJsonWrite,
    W::Error: core::fmt::Debug,
{
    let mut object = value.object().unwrap();
    object.prop("kind").unwrap().string("ProjectDef").unwrap();
    if let Some(name) = &project.name {
        object.prop("name").unwrap().string(name).unwrap();
    }
    object
        .prop("nodes")
        .unwrap()
        .string_key_map(&project.nodes, |value, invocation| {
            let mut object = value.object()?;
            object.prop("artifact")?.string(&invocation.artifact)?;
            object.finish()
        })
        .unwrap();
    object.finish().unwrap();
}

fn write_node<W>(value: SlotJsonValue<'_, W>, node: &GeneratedNodeDef)
where
    W: SlotJsonWrite,
    W::Error: core::fmt::Debug,
{
    match node {
        GeneratedNodeDef::Output(output) => write_output(value, output),
        GeneratedNodeDef::Fixture(fixture) => write_fixture(value, fixture),
    }
}

fn write_output<W>(value: SlotJsonValue<'_, W>, output: &GeneratedOutputDef)
where
    W: SlotJsonWrite,
    W::Error: core::fmt::Debug,
{
    let mut object = value.object().unwrap();
    object.prop("kind").unwrap().string("OutputDef").unwrap();
    object.prop("pin").unwrap().u32(output.pin).unwrap();
    object
        .prop("bindings")
        .unwrap()
        .string_key_map(&output.bindings, |value, binding| {
            write_binding(value, binding);
            Ok(())
        })
        .unwrap();
    if let Some(options) = &output.options {
        let mut options_object = object.prop("options").unwrap().object().unwrap();
        options_object
            .prop("white_point")
            .unwrap()
            .f32_array(&options.white_point)
            .unwrap();
        options_object.prop("brightness").unwrap().f32(options.brightness).unwrap();
        options_object.finish().unwrap();
    }
    object.finish().unwrap();
}

fn write_fixture<W>(value: SlotJsonValue<'_, W>, fixture: &GeneratedFixtureDef)
where
    W: SlotJsonWrite,
    W::Error: core::fmt::Debug,
{
    let mut object = value.object().unwrap();
    object.prop("kind").unwrap().string("FixtureDef").unwrap();
    write_mapping(object.prop("mapping").unwrap(), &fixture.mapping);
    object.finish().unwrap();
}

fn write_mapping<W>(value: SlotJsonValue<'_, W>, mapping: &GeneratedMapping)
where
    W: SlotJsonWrite,
    W::Error: core::fmt::Debug,
{
    let mut object = value.object().unwrap();
    match mapping {
        GeneratedMapping::Disabled => {
            object.prop("kind").unwrap().string("Disabled").unwrap();
        }
        GeneratedMapping::Square { origin, size } => {
            object.prop("kind").unwrap().string("Square").unwrap();
            object.prop("origin").unwrap().f32_array(origin).unwrap();
            object.prop("size").unwrap().f32_array(size).unwrap();
        }
    }
    object.finish().unwrap();
}

fn write_binding<W>(value: SlotJsonValue<'_, W>, binding: &GeneratedBindingDef)
where
    W: SlotJsonWrite,
    W::Error: core::fmt::Debug,
{
    let mut object = value.object().unwrap();
    if let Some(source) = &binding.source {
        write_endpoint(object.prop("source").unwrap(), source);
    }
    if let Some(target) = &binding.target {
        write_endpoint(object.prop("target").unwrap(), target);
    }
    object.finish().unwrap();
}

fn write_endpoint<W>(value: SlotJsonValue<'_, W>, endpoint: &GeneratedEndpoint)
where
    W: SlotJsonWrite,
    W::Error: core::fmt::Debug,
{
    let mut object = value.object().unwrap();
    match endpoint {
        GeneratedEndpoint::Ref(reference) => object.prop("ref").unwrap().string(reference).unwrap(),
        GeneratedEndpoint::Value(value) => object.prop("value").unwrap().f32(*value).unwrap(),
    }
    object.finish().unwrap();
}
"#;

fn render_one_slot_view(out: &mut String, view: &StaticSlotView) {
    out.push_str("pub struct ");
    out.push_str(&view.view_name);
    out.push_str(" {\n");
    out.push_str("    registry_revision: ::lpc_model::Revision,\n");
    for field in &view.fields {
        out.push_str("    ");
        out.push_str(&field.accessor_name);
        out.push_str(": ::lpc_model::SlotAccessor,\n");
        if let Some(some_accessor_name) = &field.some_accessor_name {
            out.push_str("    ");
            out.push_str(some_accessor_name);
            out.push_str(": ::lpc_model::SlotAccessor,\n");
        }
    }
    out.push_str("}\n\n");

    out.push_str("impl ");
    out.push_str(&view.view_name);
    out.push_str(" {\n");
    out.push_str("    pub fn compile(\n");
    out.push_str("        registry: &::lpc_model::SlotShapeRegistry,\n");
    out.push_str("    ) -> Result<Self, ::lpc_model::SlotAccessorError> {\n");
    out.push_str("        Ok(Self {\n");
    out.push_str("            registry_revision: registry.revision(),\n");
    for field in &view.fields {
        out.push_str("            ");
        out.push_str(&field.accessor_name);
        out.push_str(": ::lpc_model::SlotAccessor::compile(\n");
        out.push_str("                <");
        out.push_str(&view.type_path);
        out.push_str(" as ::lpc_model::StaticSlotShape>::SHAPE_ID,\n");
        out.push_str("                ::lpc_model::SlotPath::parse(\"");
        out.push_str(&escape_rust_string(&field.slot_name));
        out.push_str("\").expect(\"generated slot field path is valid\"),\n");
        out.push_str("                registry,\n");
        out.push_str("            )?,\n");
        if let Some(some_accessor_name) = &field.some_accessor_name {
            out.push_str("            ");
            out.push_str(some_accessor_name);
            out.push_str(": ::lpc_model::SlotAccessor::compile(\n");
            out.push_str("                <");
            out.push_str(&view.type_path);
            out.push_str(" as ::lpc_model::StaticSlotShape>::SHAPE_ID,\n");
            out.push_str("                ::lpc_model::SlotPath::parse(\"");
            out.push_str(&escape_rust_string(&field.slot_name));
            out.push_str(".some\").expect(\"generated option slot payload path is valid\"),\n");
            out.push_str("                registry,\n");
            out.push_str("            )?,\n");
        }
    }
    out.push_str("        })\n");
    out.push_str("    }\n\n");

    out.push_str("    pub fn get_or_compile<'a>(\n");
    out.push_str("        cache: &'a mut Option<Self>,\n");
    out.push_str("        registry: &::lpc_model::SlotShapeRegistry,\n");
    out.push_str("    ) -> Result<&'a Self, ::lpc_model::SlotAccessorError> {\n");
    out.push_str("        let needs_compile = cache\n");
    out.push_str("            .as_ref()\n");
    out.push_str("            .is_none_or(|view| !view.is_valid_for(registry));\n");
    out.push_str("        if needs_compile {\n");
    out.push_str("            *cache = Some(Self::compile(registry)?);\n");
    out.push_str("        }\n");
    out.push_str("        Ok(cache.as_ref().expect(\"slot view cache was just compiled\"))\n");
    out.push_str("    }\n\n");

    out.push_str("    pub fn registry_revision(&self) -> ::lpc_model::Revision {\n");
    out.push_str("        self.registry_revision\n");
    out.push_str("    }\n\n");

    out.push_str(
        "    pub fn is_valid_for(&self, registry: &::lpc_model::SlotShapeRegistry) -> bool {\n",
    );
    out.push_str("        self.registry_revision == registry.revision()\n");
    out.push_str("    }\n\n");

    for field in &view.fields {
        out.push_str("    pub fn ");
        out.push_str(&field.method_name);
        if let Some(some_accessor_name) = &field.some_accessor_name {
            out.push_str("(&self) -> ::lpc_model::SlotOptionReader<'_> {\n");
            out.push_str("        ::lpc_model::SlotOptionReader::new(&self.");
            out.push_str(&field.accessor_name);
            out.push_str(", &self.");
            out.push_str(some_accessor_name);
            out.push_str(")\n");
        } else {
            out.push_str("(&self) -> ::lpc_model::SlotFieldReader<'_> {\n");
            out.push_str("        ::lpc_model::SlotFieldReader::new(&self.");
            out.push_str(&field.accessor_name);
            out.push_str(")\n");
        }
        out.push_str("    }\n\n");
    }

    out.push_str("}\n\n");
}

fn escape_rust_string(input: &str) -> String {
    input.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn discovers_static_slot_records_and_infers_parent_reexport_paths() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(src.join("source")).unwrap();
        fs::write(
            src.join("source").join("shader_def.rs"),
            r#"
#[derive(lpc_model::SlotRecord)]
pub struct ShaderDef {
    value: ValueSlot<bool>,
}

#[derive(lpc_model::SlotRecord)]
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

    #[test]
    fn mockup_source_codec_module_contains_expected_types() {
        let module = mockup_source_codec_module();
        let type_names = module
            .types
            .iter()
            .map(|codec_type| codec_type.rust_type)
            .collect::<Vec<_>>();

        assert_eq!(
            type_names,
            vec![
                "ProjectDef",
                "OutputDef",
                "TextureDef",
                "FixtureDef",
                "ShaderDef"
            ]
        );
    }

    #[test]
    fn mockup_source_codec_module_contains_representative_fields() {
        let module = mockup_source_codec_module();

        assert_codec_type_has_fields(&module, "ProjectDef", &["name", "nodes"]);
        assert_codec_type_has_fields(&module, "OutputDef", &["pin", "options"]);
        assert_codec_type_has_fields(&module, "FixtureDef", &["mapping"]);
        assert_codec_type_has_fields(&module, "ShaderDef", &["param_defs"]);
    }

    fn assert_codec_type_has_fields(module: &SlotCodecModule, rust_type: &str, expected: &[&str]) {
        let codec_type = module
            .types
            .iter()
            .find(|codec_type| codec_type.rust_type == rust_type)
            .unwrap_or_else(|| panic!("missing codec type {rust_type}"));
        for expected_field in expected {
            assert!(
                codec_type
                    .fields
                    .iter()
                    .any(|field| field.wire_name == *expected_field),
                "missing field {rust_type}.{expected_field}"
            );
        }
    }
}
