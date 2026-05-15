use crate::error::SlotShapeCodegenError;
use crate::model::{StaticSlotRecord, StaticSlotRecordField};
use std::fs;
use std::path::Path;

pub(crate) fn discover_static_slot_records(
    src_dir: &Path,
) -> Result<Vec<StaticSlotRecord>, SlotShapeCodegenError> {
    if !src_dir.is_dir() {
        return Err(SlotShapeCodegenError::MissingSrcDir(src_dir.to_path_buf()));
    }

    let mut files = Vec::new();
    super::rust_files::collect_rust_files(src_dir, &mut files)?;
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
            if !super::derive::has_derive(&item.attrs, "SlotRecord") {
                continue;
            }
            let type_name = item.ident.to_string();
            records.push(StaticSlotRecord {
                type_path: super::type_path::infer_type_path(src_dir, &path, &type_name)?,
                fields: static_slot_record_fields(&item),
                type_name,
            });
        }
    }

    records.sort_by(|a, b| a.type_path.cmp(&b.type_path));
    Ok(records)
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
