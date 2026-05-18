use crate::error::SlotShapeCodegenError;
use crate::model::StaticRegisteredShape;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub(crate) fn discover_static_registered_shapes(
    src_dir: &Path,
) -> Result<Vec<StaticRegisteredShape>, SlotShapeCodegenError> {
    if !src_dir.is_dir() {
        return Err(SlotShapeCodegenError::MissingSrcDir(src_dir.to_path_buf()));
    }

    let mut files = Vec::new();
    super::rust_files::collect_rust_files(src_dir, &mut files)?;
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
            let has_record = super::derive::has_derive(&item.attrs, "Slotted");
            let has_value = super::derive::has_derive(&item.attrs, "SlotValue");
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
            shapes.push(StaticRegisteredShape {
                type_path: super::type_path::infer_type_path(
                    src_dir,
                    &path,
                    &item.ident.to_string(),
                )?,
                has_default_factory: has_record,
            });
        }
    }

    shapes.sort_by(|a, b| a.type_path.cmp(&b.type_path));
    Ok(shapes)
}
