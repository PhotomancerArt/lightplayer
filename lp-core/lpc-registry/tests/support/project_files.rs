use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

pub fn read_project_files(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    read_project_files_recursive(root, root, &mut files);
    files
}

fn read_project_files_recursive(root: &Path, dir: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
    let mut entries = std::fs::read_dir(dir)
        .unwrap_or_else(|err| panic!("read {}: {err}", dir.display()))
        .map(|entry| entry.expect("directory entry").path())
        .collect::<Vec<_>>();
    entries.sort();

    for path in entries {
        if path.is_dir() {
            read_project_files_recursive(root, &path, files);
            continue;
        }

        let relative = path.strip_prefix(root).expect("project-relative path");
        let project_path = format!("/{}", relative.to_string_lossy());
        let bytes =
            std::fs::read(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
        files.insert(project_path, bytes);
    }
}
