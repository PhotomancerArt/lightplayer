//! Build-time slot shape bootstrap generator.
//!
//! This crate is host-only. A crate build script points it at that crate's
//! source tree, and it writes an `OUT_DIR` Rust module that can register every
//! static `#[slot(root)]` shape discovered in that crate. Runtime-owned dynamic
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
            let syn::Item::Struct(item) = item else {
                continue;
            };
            if !has_slot_record_derive(&item.attrs) || !has_slot_root_attr(&item.attrs) {
                continue;
            }
            roots.push(StaticSlotRoot {
                type_path: infer_type_path(src_dir, &path, &item.ident.to_string())?,
            });
        }
    }

    Ok(roots)
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
    attrs.iter().any(|attr| {
        attr.path().is_ident("slot")
            && attr.meta.require_list().is_ok_and(|meta| {
                meta.tokens
                    .to_string()
                    .split(',')
                    .any(|token| token.trim() == "root")
            })
    })
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
    fn empty_generated_code_avoids_unused_warnings() {
        let code = render_slot_shapes(&[]);

        assert!(code.contains("_registry"));
        assert!(code.contains("_id"));
        assert!(!code.contains("ensure_referenced_static_slot_shapes"));
    }
}
