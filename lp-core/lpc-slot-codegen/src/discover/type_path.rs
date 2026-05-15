use crate::error::SlotShapeCodegenError;
use std::path::Path;

pub(crate) fn infer_type_path(
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
