//! Build-time slot shape bootstrap generator.
//!
//! This crate is host-only. A crate build script points it at that crate's
//! source tree, and it writes an `OUT_DIR` Rust module that can register every
//! static shape discovered in that crate.
//!
//! There are two static-shape sources:
//!
//! - `#[derive(SlotRecord)] #[slot(root)]` records, used by Rust-authored slot
//!   roots such as node definitions and runtime state.
//! - Manual `impl StaticSlotShape for Type` blocks, used by native value roots
//!   that are referenced by name from other shapes.
//!
//! Runtime-owned dynamic shapes are intentionally outside this discovery pass.

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

/// Generate `slot_shapes.rs` for one crate.
pub fn generate_slot_shapes(config: SlotShapeCodegenConfig) -> Result<(), SlotShapeCodegenError> {
    let src_dir = config.crate_root.join("src");
    let mut roots = discover_static_slot_roots(&src_dir)?;
    roots.sort_by(|a, b| a.type_path.cmp(&b.type_path));

    if let Some(parent) = config.out_file.parent() {
        fs::create_dir_all(parent).map_err(SlotShapeCodegenError::Io)?;
    }
    fs::write(config.out_file, render_slot_shapes(&roots)).map_err(SlotShapeCodegenError::Io)
}

/// Generate `slot_views.rs` for `#[slot(root, view)]` records in one crate.
pub fn generate_slot_views(config: SlotViewCodegenConfig) -> Result<(), SlotShapeCodegenError> {
    let src_dir = config.crate_root.join("src");
    let mut views = discover_static_slot_views(&src_dir)?;
    views.sort_by(|a, b| a.type_path.cmp(&b.type_path));

    if let Some(parent) = config.out_file.parent() {
        fs::create_dir_all(parent).map_err(SlotShapeCodegenError::Io)?;
    }
    fs::write(config.out_file, render_slot_views(&views)).map_err(SlotShapeCodegenError::Io)
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
struct StaticSlotRoot {
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

fn discover_static_slot_roots(
    src_dir: &Path,
) -> Result<Vec<StaticSlotRoot>, SlotShapeCodegenError> {
    if !src_dir.is_dir() {
        return Err(SlotShapeCodegenError::MissingSrcDir(src_dir.to_path_buf()));
    }

    let mut files = Vec::new();
    collect_rust_files(src_dir, &mut files)?;
    files.sort();

    let mut roots = Vec::new();
    for path in files {
        let source = fs::read_to_string(&path).map_err(SlotShapeCodegenError::Io)?;
        let syntax = syn::parse_file(&source).map_err(|source| SlotShapeCodegenError::Parse {
            path: path.clone(),
            source,
        })?;
        for item in syntax.items {
            match item {
                syn::Item::Struct(item)
                    if has_slot_record_derive(&item.attrs) && has_slot_root_attr(&item.attrs) =>
                {
                    push_unique_root(
                        &mut roots,
                        infer_type_path(src_dir, &path, &item.ident.to_string())?,
                    );
                }
                syn::Item::Impl(item) => {
                    if let Some(type_name) = static_slot_shape_impl_type_name(&item) {
                        push_unique_root(&mut roots, infer_type_path(src_dir, &path, &type_name)?);
                    }
                }
                _ => {}
            }
        }
    }

    Ok(roots)
}

fn push_unique_root(roots: &mut Vec<StaticSlotRoot>, type_path: String) {
    if !roots.iter().any(|root| root.type_path == type_path) {
        roots.push(StaticSlotRoot { type_path });
    }
}

fn static_slot_shape_impl_type_name(item: &syn::ItemImpl) -> Option<String> {
    let (_, trait_path, _) = item.trait_.as_ref()?;
    if !trait_path
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "StaticSlotShape")
    {
        return None;
    }

    let syn::Type::Path(self_ty) = item.self_ty.as_ref() else {
        return None;
    };
    self_ty
        .path
        .segments
        .last()
        .map(|segment| segment.ident.to_string())
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
            if !has_slot_record_derive(&item.attrs) || !has_slot_root_view_attr(&item.attrs) {
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

fn has_slot_root_attr(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .any(|attr| slot_attr_has_flags(attr, &["root"]))
}

fn has_slot_root_view_attr(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .any(|attr| attr.path().is_ident("slot") && slot_attr_has_flags(attr, &["root", "view"]))
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

fn render_slot_shapes(roots: &[StaticSlotRoot]) -> String {
    let mut out = String::from("// @generated by lpc-slot-codegen. Do not edit.\n\n");
    out.push_str("pub fn register_all_static_slot_shapes(\n");
    if roots.is_empty() {
        out.push_str("    _registry: &mut ::lpc_model::SlotShapeRegistry,\n");
    } else {
        out.push_str("    registry: &mut ::lpc_model::SlotShapeRegistry,\n");
    }
    out.push_str(") -> Result<(), ::lpc_model::SlotShapeRegistryError> {\n");
    for root in roots {
        out.push_str("    ensure_static_slot_shape(registry, <");
        out.push_str(&root.type_path);
        out.push_str(" as ::lpc_model::StaticSlotShape>::SHAPE_ID)?;\n");
    }
    out.push_str("    Ok(())\n");
    out.push_str("}\n\n");

    out.push_str("pub fn ensure_static_slot_shape(\n");
    if roots.is_empty() {
        out.push_str("    _registry: &mut ::lpc_model::SlotShapeRegistry,\n");
        out.push_str("    _id: ::lpc_model::SlotShapeId,\n");
    } else {
        out.push_str("    registry: &mut ::lpc_model::SlotShapeRegistry,\n");
        out.push_str("    id: ::lpc_model::SlotShapeId,\n");
    }
    out.push_str(") -> Result<bool, ::lpc_model::SlotShapeRegistryError> {\n");
    for (index, root) in roots.iter().enumerate() {
        if index == 0 {
            out.push_str("    if id == <");
        } else {
            out.push_str("    } else if id == <");
        }
        out.push_str(&root.type_path);
        out.push_str(" as ::lpc_model::StaticSlotShape>::SHAPE_ID {\n");
        out.push_str("        let inserted = <");
        out.push_str(&root.type_path);
        out.push_str(" as ::lpc_model::StaticSlotShape>::ensure_registered(registry)?;\n");
        out.push_str("        ensure_referenced_static_slot_shapes(registry, id)?;\n");
        out.push_str("        Ok(inserted)\n");
    }
    if roots.is_empty() {
        out.push_str("    Ok(false)\n");
    } else {
        out.push_str("    } else {\n");
        out.push_str("        Ok(false)\n");
        out.push_str("    }\n");
    }
    out.push_str("}\n");

    if !roots.is_empty() {
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
    fn discovers_root_slot_records_and_infers_parent_reexport_paths() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(src.join("source")).unwrap();
        fs::write(
            src.join("source").join("shader_def.rs"),
            r#"
#[derive(lpc_model::SlotRecord)]
#[slot(root)]
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

        let roots = discover_static_slot_roots(&src).unwrap();

        assert_eq!(
            roots,
            vec![StaticSlotRoot {
                type_path: String::from("crate::source::ShaderDef"),
            }]
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
#[slot(root)]
pub struct ProjectDef {}
"#,
        )
        .unwrap();

        let roots = discover_static_slot_roots(&src).unwrap();

        assert_eq!(
            roots,
            vec![StaticSlotRoot {
                type_path: String::from("crate::node::project::ProjectDef"),
            }]
        );
    }

    #[test]
    fn discovers_manual_static_slot_shape_impls() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src");
        fs::create_dir_all(src.join("nodes").join("fluid")).unwrap();
        fs::write(
            src.join("nodes").join("fluid").join("fluid_emitter.rs"),
            r#"
pub struct FluidEmitter;

impl StaticSlotShape for FluidEmitter {
    const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("lp::fluid::Emitter");

    fn slot_shape() -> SlotShape {
        todo!()
    }
}
"#,
        )
        .unwrap();

        let roots = discover_static_slot_roots(&src).unwrap();

        assert_eq!(
            roots,
            vec![StaticSlotRoot {
                type_path: String::from("crate::nodes::fluid::FluidEmitter"),
            }]
        );
    }

    #[test]
    fn generated_code_contains_bootstrap_functions_and_type_paths() {
        let roots = vec![StaticSlotRoot {
            type_path: String::from("crate::source::ShaderDef"),
        }];

        let code = render_slot_shapes(&roots);

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
