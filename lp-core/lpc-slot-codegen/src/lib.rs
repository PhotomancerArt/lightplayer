//! Build-time slot shape bootstrap generator.
//!
//! This crate is host-only. A crate build script points it at that crate's
//! source tree, and it writes an `OUT_DIR` Rust module that can register every
//! static `SlotRecord` shape discovered in that crate. Runtime-owned dynamic
//! shapes are intentionally outside this discovery pass.

use std::{
    collections::BTreeMap,
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
    pub crate_root: PathBuf,
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

/// Generate `slot_views.rs` for every static `SlotRecord` in one crate.
pub fn generate_slot_views(config: SlotViewCodegenConfig) -> Result<(), SlotShapeCodegenError> {
    let src_dir = config.crate_root.join("src");
    let mut views = discover_static_slot_views(&src_dir)?;
    views.sort_by(|a, b| a.type_path.cmp(&b.type_path));

    if let Some(parent) = config.out_file.parent() {
        fs::create_dir_all(parent).map_err(SlotShapeCodegenError::Io)?;
    }
    fs::write(config.out_file, render_slot_views(&views)).map_err(SlotShapeCodegenError::Io)
}

/// Generate the mockup slot-codec module.
///
/// This remains mockup-specific while the codec model is being proven, but it
/// now constructs the slotted source records directly instead of depending on
/// codec-only constructors in the domain model.
pub fn generate_mockup_slot_codec(
    config: MockupSlotCodecCodegenConfig,
) -> Result<(), SlotShapeCodegenError> {
    let src_dir = config.crate_root.join("src");
    let records = discover_static_slot_records(&src_dir)?;
    if let Some(parent) = config.out_file.parent() {
        fs::create_dir_all(parent).map_err(SlotShapeCodegenError::Io)?;
    }
    fs::write(config.out_file, render_mockup_slot_codec(&records))
        .map_err(SlotShapeCodegenError::Io)
}

#[derive(Debug)]
pub enum SlotShapeCodegenError {
    Io(io::Error),
    Parse {
        path: PathBuf,
        source: syn::Error,
    },
    MissingSrcDir(PathBuf),
    NonUtf8Path(PathBuf),
    DuplicateShapeIdName {
        name: String,
        first: PathBuf,
        second: PathBuf,
    },
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
            Self::DuplicateShapeIdName {
                name,
                first,
                second,
            } => write!(
                f,
                "duplicate slot shape id name {name:?}: {} and {}",
                first.display(),
                second.display()
            ),
        }
    }
}

impl StdError for SlotShapeCodegenError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::Parse { source, .. } => Some(source),
            Self::MissingSrcDir(_) | Self::NonUtf8Path(_) | Self::DuplicateShapeIdName { .. } => {
                None
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StaticRegisteredShape {
    type_path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StaticSlotRecord {
    type_path: String,
    type_name: String,
    fields: Vec<StaticSlotRecordField>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StaticSlotRecordField {
    rust_name: String,
    slot_name: String,
    type_name: String,
    is_enum: bool,
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
    let mut id_names = BTreeMap::new();
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
            let has_record = has_derive(&item.attrs, "SlotRecord");
            let has_value = has_derive(&item.attrs, "SlotValue");
            if !has_record && !has_value {
                continue;
            }
            let id_name = item.ident.to_string();
            if let Some(first) = id_names.insert(id_name.clone(), path.clone()) {
                return Err(SlotShapeCodegenError::DuplicateShapeIdName {
                    name: id_name,
                    first,
                    second: path,
                });
            }
            if has_record {
                shapes.push(StaticRegisteredShape {
                    type_path: infer_type_path(src_dir, &path, &item.ident.to_string())?,
                });
            }
        }
    }

    shapes.sort_by(|a, b| a.type_path.cmp(&b.type_path));
    Ok(shapes)
}

fn discover_static_slot_records(
    src_dir: &Path,
) -> Result<Vec<StaticSlotRecord>, SlotShapeCodegenError> {
    if !src_dir.is_dir() {
        return Err(SlotShapeCodegenError::MissingSrcDir(src_dir.to_path_buf()));
    }

    let mut files = Vec::new();
    collect_rust_files(src_dir, &mut files)?;
    files.sort();

    let mut records = Vec::new();
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
            if !has_derive(&item.attrs, "SlotRecord") {
                continue;
            }
            let type_name = item.ident.to_string();
            records.push(StaticSlotRecord {
                type_path: infer_type_path(src_dir, &path, &type_name)?,
                fields: static_slot_record_fields(&item),
                type_name,
            });
        }
    }

    records.sort_by(|a, b| a.type_path.cmp(&b.type_path));
    Ok(records)
}

fn discover_static_slot_views(
    src_dir: &Path,
) -> Result<Vec<StaticSlotView>, SlotShapeCodegenError> {
    Ok(discover_static_slot_records(src_dir)?
        .into_iter()
        .map(|record| StaticSlotView {
            view_name: format!("{}View", record.type_name),
            type_path: record.type_path,
            fields: record
                .fields
                .into_iter()
                .map(|field| StaticSlotViewField {
                    accessor_name: format!("{}_accessor", field.rust_name),
                    some_accessor_name: (field.type_name == "OptionSlot")
                        .then(|| format!("{}_some_accessor", field.rust_name)),
                    method_name: field.rust_name,
                    slot_name: field.slot_name,
                })
                .collect(),
        })
        .collect())
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

fn has_derive(attrs: &[syn::Attribute], derive_name: &str) -> bool {
    attrs
        .iter()
        .filter(|attr| attr.path().is_ident("derive"))
        .any(|attr| {
            attr.meta.require_list().is_ok_and(|meta| {
                meta.tokens
                    .to_string()
                    .split(',')
                    .any(|derive| derive.trim().ends_with(derive_name))
            })
        })
}

fn static_slot_record_fields(item: &syn::ItemStruct) -> Vec<StaticSlotRecordField> {
    let syn::Fields::Named(fields) = &item.fields else {
        return Vec::new();
    };
    fields
        .named
        .iter()
        .filter_map(|field| {
            let ident = field.ident.as_ref()?;
            let rust_name = ident.to_string();
            let slot_name = slot_field_name(field).unwrap_or_else(|| rust_name.clone());
            Some(StaticSlotRecordField {
                rust_name,
                slot_name,
                type_name: field_type_name(&field.ty),
                is_enum: slot_field_is_enum(field),
            })
        })
        .collect()
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

fn slot_field_is_enum(field: &syn::Field) -> bool {
    field.attrs.iter().any(|attr| {
        attr.path().is_ident("slot") && {
            let mut is_enum = false;
            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("enum") {
                    is_enum = true;
                }
                Ok(())
            });
            is_enum
        }
    })
}

fn field_type_name(ty: &syn::Type) -> String {
    let syn::Type::Path(path) = ty else {
        return String::from("<unsupported>");
    };
    path.path
        .segments
        .last()
        .map(|segment| segment.ident.to_string())
        .unwrap_or_else(|| String::from("<unknown>"))
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

fn render_mockup_slot_codec(records: &[StaticSlotRecord]) -> String {
    let mut out = String::from("// @generated by lpc-slot-codegen. Do not edit.\n\n");
    out.push_str(MOCKUP_SLOT_CODEC_IMPORTS_AND_TYPES);
    out.push_str(MOCKUP_SLOT_CODEC_BUNDLE_READERS);
    out.push_str(&render_mockup_slot_codec_record_impls(records));
    out.push_str(&render_mockup_source_surfaces(&mockup_codec_surfaces()));
    out.push_str(MOCKUP_SLOT_CODEC_REAL_HELPERS);
    out.push_str(MOCKUP_SLOT_CODEC_WRITERS);
    out
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MockupCodecSurface {
    rust_type: &'static str,
    fn_stem: &'static str,
    kind_expr: &'static str,
}

fn mockup_codec_surfaces() -> Vec<MockupCodecSurface> {
    vec![
        MockupCodecSurface {
            rust_type: "crate::source::ProjectDef",
            fn_stem: "project_def",
            kind_expr: "ProjectDef::KIND",
        },
        MockupCodecSurface {
            rust_type: "crate::source::OutputDef",
            fn_stem: "output_def",
            kind_expr: "OutputDef::KIND",
        },
        MockupCodecSurface {
            rust_type: "crate::source::TextureDef",
            fn_stem: "texture_def",
            kind_expr: "TextureDef::KIND",
        },
        MockupCodecSurface {
            rust_type: "crate::source::FixtureDef",
            fn_stem: "fixture_def",
            kind_expr: "FixtureDef::KIND",
        },
        MockupCodecSurface {
            rust_type: "crate::source::ShaderDef",
            fn_stem: "shader_def",
            kind_expr: "ShaderDef::KIND",
        },
    ]
}

fn render_mockup_source_surfaces(surfaces: &[MockupCodecSurface]) -> String {
    let mut out = String::new();
    for surface in surfaces {
        render_mockup_source_surface(&mut out, surface);
    }
    out
}

fn render_mockup_source_surface(out: &mut String, surface: &MockupCodecSurface) {
    out.push_str("pub fn read_");
    out.push_str(surface.fn_stem);
    out.push_str("_json(json: &str) -> Result<");
    out.push_str(surface.rust_type);
    out.push_str(", SyntaxError> {\n");
    out.push_str("    let registry = SlotShapeRegistry::default();\n");
    out.push_str(
        "    let mut reader = SlotReader::new(JsonSyntaxSource::new(json)?, &registry);\n",
    );
    out.push_str("    read_");
    out.push_str(surface.fn_stem);
    out.push_str("(&mut reader)\n");
    out.push_str("}\n\n");

    out.push_str("pub fn read_");
    out.push_str(surface.fn_stem);
    out.push_str("_toml(value: &toml::Value) -> Result<");
    out.push_str(surface.rust_type);
    out.push_str(", SyntaxError> {\n");
    out.push_str("    let registry = SlotShapeRegistry::default();\n");
    out.push_str(
        "    let mut reader = SlotReader::new(TomlSyntaxSource::new(value)?, &registry);\n",
    );
    out.push_str("    read_");
    out.push_str(surface.fn_stem);
    out.push_str("(&mut reader)\n");
    out.push_str("}\n\n");

    out.push_str("pub fn read_");
    out.push_str(surface.fn_stem);
    out.push_str("<S>(reader: &mut SlotReader<'_, S>) -> Result<");
    out.push_str(surface.rust_type);
    out.push_str(", SyntaxError>\nwhere\n    S: SyntaxEventSource,\n{\n");
    out.push_str("    let mut object = reader.object()?;\n");
    out.push_str("    let _kind = object.expect_discriminator(\"kind\", &[");
    out.push_str(surface.kind_expr);
    out.push_str("])?;\n");
    out.push_str("    read_");
    out.push_str(&surface.fn_stem);
    out.push_str("_slot_body(object)\n");
    out.push_str("}\n\n");

    let arg_name = surface
        .fn_stem
        .strip_suffix("_def")
        .unwrap_or(surface.fn_stem);
    out.push_str("pub fn write_");
    out.push_str(surface.fn_stem);
    out.push_str("_json(");
    out.push_str(arg_name);
    out.push_str(": &");
    out.push_str(surface.rust_type);
    out.push_str(") -> Vec<u8> {\n");
    out.push_str("    let mut out = Vec::new();\n");
    out.push_str("    let mut writer = SlotWriter::new(&mut out);\n");
    out.push_str("    let mut object = writer.object().unwrap();\n");
    out.push_str("    object.prop(\"kind\").unwrap().string(");
    out.push_str(surface.kind_expr);
    out.push_str(").unwrap();\n");
    out.push_str("    write_");
    out.push_str(surface.fn_stem);
    out.push_str("_slot_body(&mut object, ");
    out.push_str(arg_name);
    out.push_str(").unwrap();\n");
    out.push_str("    object.finish().unwrap();\n");
    out.push_str("    out\n");
    out.push_str("}\n\n");
}

fn render_mockup_slot_codec_record_impls(records: &[StaticSlotRecord]) -> String {
    let mut out = String::new();
    for record in records {
        render_mockup_slot_codec_record_impl(&mut out, record);
    }
    out
}

fn render_mockup_slot_codec_record_impl(out: &mut String, record: &StaticSlotRecord) {
    let stem = slot_codec_record_stem(&record.type_name);
    render_mockup_slot_codec_record_read_body(out, record, &stem);
    render_mockup_slot_codec_record_write_body(out, record, &stem);

    out.push_str("impl SlotCodec for ");
    out.push_str(&record.type_path);
    out.push_str(" {\n");
    out.push_str(
        "    fn read_slot<S>(value: ValueReader<'_, '_, S>) -> Result<Self, SyntaxError>\n",
    );
    out.push_str("    where\n        S: SyntaxEventSource,\n    {\n");
    out.push_str("        read_");
    out.push_str(&stem);
    out.push_str("_slot_body(value.object()?)\n");
    out.push_str("    }\n\n");

    out.push_str(
        "    fn write_slot<W>(&self, value: SlotValueWriter<'_, W>) -> Result<(), SlotWriteError<W::Error>>\n",
    );
    out.push_str("    where\n        W: SlotWrite,\n    {\n");
    out.push_str("        let mut object = value.object()?;\n");
    out.push_str("        write_");
    out.push_str(&stem);
    out.push_str("_slot_body(&mut object, self)?;\n");
    out.push_str("        object.finish()\n");
    out.push_str("    }\n");
    out.push_str("}\n\n");
}

fn render_mockup_slot_codec_record_read_body(
    out: &mut String,
    record: &StaticSlotRecord,
    stem: &str,
) {
    out.push_str("fn read_");
    out.push_str(stem);
    out.push_str("_slot_body<S>(mut object: ObjectReader<'_, '_, S>) -> Result<");
    out.push_str(&record.type_path);
    out.push_str(", SyntaxError>\nwhere\n    S: SyntaxEventSource,\n{\n");
    render_mockup_record_fields_const(out, record, "    ");
    out.push_str("    let mut out = <");
    out.push_str(&record.type_path);
    out.push_str(" as Default>::default();\n");
    out.push_str("    while let Some(mut prop) = object.next_prop()? {\n");
    out.push_str("        match prop.name() {\n");
    for field in &record.fields {
        out.push_str("            \"");
        out.push_str(&field.slot_name);
        out.push_str("\" => ");
        if mockup_slot_codec_skip_field(record, field) {
            out.push_str("prop.value().skip_value()?,\n");
        } else {
            out.push_str("out.");
            out.push_str(&field.rust_name);
            out.push_str(" = SlotCodec::read_slot(prop.value())?,\n");
        }
    }
    out.push_str("            other => return Err(prop.unknown_field(other, FIELDS)),\n");
    out.push_str("        }\n");
    out.push_str("    }\n");
    out.push_str("    Ok(out)\n");
    out.push_str("}\n\n");
}

fn render_mockup_slot_codec_record_write_body(
    out: &mut String,
    record: &StaticSlotRecord,
    stem: &str,
) {
    out.push_str("fn write_");
    out.push_str(stem);
    out.push_str("_slot_body<W>(object: &mut SlotObjectWriter<'_, W>, value: &");
    out.push_str(&record.type_path);
    out.push_str(") -> Result<(), SlotWriteError<W::Error>>\nwhere\n    W: SlotWrite,\n{\n");
    for field in record
        .fields
        .iter()
        .filter(|field| !mockup_slot_codec_skip_field(record, field))
    {
        out.push_str("    if value.");
        out.push_str(&field.rust_name);
        out.push_str(".should_write_slot() {\n");
        out.push_str("        value.");
        out.push_str(&field.rust_name);
        out.push_str(".write_slot(object.prop(\"");
        out.push_str(&field.slot_name);
        out.push_str("\")?)?;\n");
        out.push_str("    }\n");
    }
    out.push_str("    Ok(())\n");
    out.push_str("}\n\n");
}

fn slot_codec_record_stem(type_name: &str) -> String {
    let mut stem = String::new();
    for (index, ch) in type_name.chars().enumerate() {
        if ch.is_uppercase() {
            if index != 0 {
                stem.push('_');
            }
            stem.extend(ch.to_lowercase());
        } else {
            stem.push(ch);
        }
    }
    stem
}

fn render_mockup_record_fields_const(out: &mut String, record: &StaticSlotRecord, indent: &str) {
    out.push_str(indent);
    out.push_str("const FIELDS: &[&str] = &[");
    for (index, field) in record.fields.iter().enumerate() {
        if index != 0 {
            out.push_str(", ");
        }
        out.push('"');
        out.push_str(&field.slot_name);
        out.push('"');
    }
    out.push_str("];\n");
}

fn mockup_slot_codec_skip_field(record: &StaticSlotRecord, field: &StaticSlotRecordField) -> bool {
    matches!(field.rust_name.as_str(), "bindings")
        || matches!(
            (record.type_name.as_str(), field.rust_name.as_str()),
            ("FixtureDef", "sampling")
        )
}

const MOCKUP_SLOT_CODEC_IMPORTS_AND_TYPES: &str = r#"
use std::collections::BTreeMap;

use crate::source::{
    FixtureDef, MappingConfig, OutputDef, PathSpec, ProjectDef, RingOrder, ShaderDef, TextureDef,
};
use lpc_model::{
    current_revision, MapSlot, SlotEnumAccess, ValueSlot, Xy, XySlot,
};
use lpc_model::SlotShapeRegistry;
use lpc_model::slot_codec::{
    JsonSyntaxSource, ObjectReader, SlotJsonValue, SlotJsonWrite, SlotJsonWriter,
    SlotObjectWriter, SlotReader, SlotCodec, SlotValueWriter, SlotWrite, SlotWriteError, SlotWriter,
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

const MOCKUP_SLOT_CODEC_REAL_HELPERS: &str = r#"impl SlotCodec for MappingConfig {
    fn read_slot<S>(value: ValueReader<'_, '_, S>) -> Result<Self, SyntaxError>
    where
        S: SyntaxEventSource,
    {
        read_mapping_config(value)
    }

    fn write_slot<W>(&self, value: SlotValueWriter<'_, W>) -> Result<(), SlotWriteError<W::Error>>
    where
        W: SlotWrite,
    {
        write_mapping_config(value, self)
    }
}

impl SlotCodec for PathSpec {
    fn read_slot<S>(value: ValueReader<'_, '_, S>) -> Result<Self, SyntaxError>
    where
        S: SyntaxEventSource,
    {
        read_path_spec(value)
    }

    fn write_slot<W>(&self, value: SlotValueWriter<'_, W>) -> Result<(), SlotWriteError<W::Error>>
    where
        W: SlotWrite,
    {
        write_path_spec(value, self)
    }
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
    Ok(MappingConfig::Square {
        variant_revision: current_revision(),
        origin: XySlot::new(Xy(
            origin.ok_or_else(|| object.missing_required_field("origin"))?,
        )),
        size: XySlot::new(Xy(
            size.ok_or_else(|| object.missing_required_field("size"))?,
        )),
    })
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

fn write_mapping_config<W>(
    value: SlotValueWriter<'_, W>,
    mapping: &MappingConfig,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    let mut object = value.object()?;
    match mapping.variant() {
        "disabled" => {
            object.prop("kind")?.string("disabled")?;
        }
        "square" => {
            let (origin, size) = mapping.square_fields().unwrap();
            object.prop("kind")?.string("square")?;
            object.prop("origin")?.f32_array(&origin)?;
            object.prop("size")?.f32_array(&size)?;
        }
        "path_points" => {
            let (paths, sample_diameter) = mapping.path_points_fields().unwrap();
            object.prop("kind")?.string("path_points")?;
            object
                .prop("paths")?
                .u32_key_map(&paths.entries, |value, path| write_path_spec(value, path))?;
            object.prop("sample_diameter")?.f32(sample_diameter)?;
        }
        _ => unreachable!("known mapping variant"),
    }
    object.finish()
}

fn write_path_spec<W>(
    value: SlotValueWriter<'_, W>,
    path: &PathSpec,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite,
{
    let mut object = value.object()?;
    match path.variant() {
        "manual" => object.prop("kind")?.string("manual")?,
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
            object.prop("kind")?.string("ring_array")?;
            object.prop("center")?.f32_array(&center)?;
            object.prop("diameter")?.f32(diameter)?;
            object.prop("start_ring_inclusive")?.u32(start_ring_inclusive)?;
            object.prop("end_ring_exclusive")?.u32(end_ring_exclusive)?;
            object
                .prop("ring_lamp_counts")?
                .u32_key_map(&ring_lamp_counts.entries, |value, count| value.u32(*count.value()))?;
            object.prop("offset_angle")?.f32(offset_angle)?;
            object.prop("order")?.string(order.as_str())?;
        }
        _ => unreachable!("known path variant"),
    }
    object.finish()
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
    fn discovers_static_slot_record_fields_and_enum_markers() {
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
    pub mapping: MappingConfig,
    #[slot(name = "gamma")]
    pub gamma_correction: ValueSlot<bool>,
}
"#,
        )
        .unwrap();

        let records = discover_static_slot_records(&src).unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].type_path, "crate::source::FixtureDef");
        assert_eq!(records[0].type_name, "FixtureDef");
        assert_eq!(
            records[0].fields,
            vec![
                StaticSlotRecordField {
                    rust_name: String::from("render_size"),
                    slot_name: String::from("render_size"),
                    type_name: String::from("Dim2uSlot"),
                    is_enum: false,
                },
                StaticSlotRecordField {
                    rust_name: String::from("mapping"),
                    slot_name: String::from("mapping"),
                    type_name: String::from("MappingConfig"),
                    is_enum: true,
                },
                StaticSlotRecordField {
                    rust_name: String::from("gamma_correction"),
                    slot_name: String::from("gamma"),
                    type_name: String::from("ValueSlot"),
                    is_enum: false,
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

    #[test]
    fn generated_mockup_codec_does_not_use_domain_codec_constructors() {
        let code = render_mockup_slot_codec(&[]);

        assert!(!code.contains(&format!("from_{}", "codec")));
    }

    #[test]
    fn generated_mockup_codec_contains_slot_codec_record_impls() {
        let records = vec![StaticSlotRecord {
            type_path: String::from("crate::source::OutputDriverOptionsConfig"),
            type_name: String::from("OutputDriverOptionsConfig"),
            fields: vec![StaticSlotRecordField {
                rust_name: String::from("brightness"),
                slot_name: String::from("brightness"),
                type_name: String::from("RatioSlot"),
                is_enum: false,
            }],
        }];

        let code = render_mockup_slot_codec(&records);

        assert!(code.contains("impl SlotCodec for crate::source::OutputDriverOptionsConfig"));
        assert!(code.contains("out.brightness = SlotCodec::read_slot(prop.value())?"));
        assert!(code.contains("value.brightness.write_slot(object.prop(\"brightness\")?)?"));
    }
}
