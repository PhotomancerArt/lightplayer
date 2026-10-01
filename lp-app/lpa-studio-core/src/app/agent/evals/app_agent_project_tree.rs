//! [`ProjectTree`]: a project package as files, the unit the eval checks
//! read and stage B deploys.
//!
//! Checks never go through Studio or a server: they read the bytes a user
//! would save. The node walk follows the root module's `nodes` refs, module
//! sub-nodes, and playlist entries' `node` refs, so a check can ask "every
//! Output in this project" without knowing where the agent put it.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

/// A package's files, keyed by package-relative path (`project.json`,
/// `modules/spiral/shader.glsl`). No leading `./` or `/`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProjectTree {
    pub(crate) files: BTreeMap<String, Vec<u8>>,
}

/// One node found by the walk: where its def lives and the def itself.
#[derive(Clone, Debug)]
pub(crate) struct TreeNode {
    /// The node's name in its parent (`fixture`, `entry_1` for an unnamed
    /// playlist entry).
    pub(crate) name: String,
    /// The def file, package-relative; `None` for an inline def.
    pub(crate) file: Option<String>,
    /// The def's `kind` (`Fixture`, `Playlist`, …).
    pub(crate) kind: String,
    pub(crate) def: Value,
    /// Whether the walk reached it through a playlist entry (or below one).
    pub(crate) in_playlist: bool,
}

impl ProjectTree {
    /// Read every file under `dir`.
    pub(crate) fn from_dir(dir: &Path) -> std::io::Result<Self> {
        let mut files = BTreeMap::new();
        collect_dir(dir, dir, &mut files)?;
        Ok(Self { files })
    }

    /// Build from `(path, bytes)` pairs (a generated package, a server read).
    pub(crate) fn from_files(files: impl IntoIterator<Item = (String, Vec<u8>)>) -> Self {
        Self {
            files: files
                .into_iter()
                .map(|(path, bytes)| (normalize(&path), bytes))
                .collect(),
        }
    }

    /// Write every file under `dir` (created; existing files overwritten).
    pub(crate) fn write_to_dir(&self, dir: &Path) -> std::io::Result<()> {
        for (path, bytes) in &self.files {
            let target = dir.join(path);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(target, bytes)?;
        }
        Ok(())
    }

    pub(crate) fn text(&self, path: &str) -> Option<&str> {
        self.files
            .get(&normalize(path))
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
    }

    pub(crate) fn json(&self, path: &str) -> Option<Value> {
        self.text(path)
            .and_then(|text| serde_json::from_str(text).ok())
    }

    /// The parsed `project.json`.
    pub(crate) fn manifest(&self) -> Option<Value> {
        self.json("project.json")
    }

    /// Every node reachable from the root module, depth first.
    pub(crate) fn nodes(&self) -> Vec<TreeNode> {
        let mut out = Vec::new();
        if let Some(root) = self.json("module.json") {
            self.walk_module(&root, "module.json", false, &mut out, 0);
        }
        out
    }

    /// Every node of `kind` (case-insensitive).
    pub(crate) fn nodes_of_kind(&self, kind: &str) -> Vec<TreeNode> {
        self.nodes()
            .into_iter()
            .filter(|node| node.kind.eq_ignore_ascii_case(kind))
            .collect()
    }

    /// Resolve `reference` (`./fixture.map2d.json`) against the directory
    /// of `from` (a package-relative file path).
    pub(crate) fn resolve(from: &str, reference: &str) -> String {
        let mut parts: Vec<&str> = match from.rfind('/') {
            Some(at) => from[..at].split('/').collect(),
            None => Vec::new(),
        };
        for segment in reference.split('/') {
            match segment {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                other => parts.push(other),
            }
        }
        parts.join("/")
    }

    fn walk_module(
        &self,
        module: &Value,
        file: &str,
        in_playlist: bool,
        out: &mut Vec<TreeNode>,
        depth: usize,
    ) {
        if depth > 16 {
            return; // a ref cycle; the loader refuses those anyway
        }
        let Some(nodes) = module.get("nodes").and_then(Value::as_object) else {
            return;
        };
        for (name, slot) in nodes {
            self.visit(name, slot, file, in_playlist, out, depth + 1);
        }
    }

    /// Visit one node slot (`{"ref": …}` or an inline def).
    fn visit(
        &self,
        name: &str,
        slot: &Value,
        from: &str,
        in_playlist: bool,
        out: &mut Vec<TreeNode>,
        depth: usize,
    ) {
        let (file, def) = match slot.get("ref").and_then(Value::as_str) {
            Some(reference) => {
                let path = Self::resolve(from, reference);
                let Some(def) = self.json(&path) else {
                    return;
                };
                (Some(path), def)
            }
            None => (None, slot.clone()),
        };
        let kind = def
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let def_file = file.clone().unwrap_or_else(|| from.to_string());
        out.push(TreeNode {
            name: name.to_string(),
            file: file.clone(),
            kind: kind.clone(),
            def: def.clone(),
            in_playlist,
        });
        if kind.eq_ignore_ascii_case("module") {
            self.walk_module(&def, &def_file, in_playlist, out, depth);
        }
        if kind.eq_ignore_ascii_case("playlist")
            && let Some(entries) = def.get("entries").and_then(Value::as_object)
        {
            for (key, entry) in entries {
                if let Some(node) = entry.get("node") {
                    let name = entry
                        .get("name")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                        .unwrap_or_else(|| format!("entry_{key}"));
                    self.visit(&name, node, &def_file, true, out, depth + 1);
                }
            }
        }
    }
}

fn normalize(path: &str) -> String {
    path.trim_start_matches("./")
        .trim_start_matches('/')
        .to_string()
}

fn collect_dir(
    root: &Path,
    dir: &Path,
    files: &mut BTreeMap<String, Vec<u8>>,
) -> std::io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(std::fs::DirEntry::path);
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_dir(root, &path, files)?;
        } else {
            let relative = path
                .strip_prefix(root)
                .expect("walked under root")
                .to_string_lossy()
                .replace('\\', "/");
            files.insert(relative, std::fs::read(&path)?);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refs_resolve_against_the_referring_file() {
        assert_eq!(
            ProjectTree::resolve("module.json", "./fixture.json"),
            "fixture.json"
        );
        assert_eq!(
            ProjectTree::resolve("playlist.json", "./modules/spiral/module.json"),
            "modules/spiral/module.json"
        );
        assert_eq!(
            ProjectTree::resolve("modules/spiral/module.json", "./shader.json"),
            "modules/spiral/shader.json"
        );
        assert_eq!(
            ProjectTree::resolve("modules/spiral/module.json", "../x.json"),
            "modules/x.json"
        );
    }

    #[test]
    fn the_walk_reaches_root_nodes_modules_and_playlist_entries() {
        let tree = ProjectTree::from_files([
            (
                "module.json".to_string(),
                br#"{"kind":"Module","nodes":{"playlist":{"ref":"./playlist.json"},"clock":{"kind":"Clock"}}}"#.to_vec(),
            ),
            (
                "playlist.json".to_string(),
                br#"{"kind":"Playlist","entries":{"1":{"node":{"ref":"./modules/a/module.json"}}}}"#
                    .to_vec(),
            ),
            (
                "./modules/a/module.json".to_string(),
                br#"{"kind":"Module","nodes":{"shader":{"ref":"./shader.json"}}}"#.to_vec(),
            ),
            (
                "modules/a/shader.json".to_string(),
                br#"{"kind":"Shader"}"#.to_vec(),
            ),
        ]);
        let walked: Vec<(String, String, bool)> = tree
            .nodes()
            .into_iter()
            .map(|n| (n.name, n.kind, n.in_playlist))
            .collect();
        assert_eq!(
            walked,
            vec![
                ("clock".into(), "Clock".into(), false),
                ("playlist".into(), "Playlist".into(), false),
                ("entry_1".into(), "Module".into(), true),
                ("shader".into(), "Shader".into(), true),
            ]
        );
    }
}
