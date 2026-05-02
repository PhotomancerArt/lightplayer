//! Map tree path leaf type names to legacy [`lpc_source::legacy::nodes::NodeKind`].

use alloc::string::ToString;

use lpc_model::TreePath;
use lpc_source::legacy::nodes::NodeKind;

pub(crate) fn legacy_node_kind_from_tree_path(path: &TreePath) -> Option<NodeKind> {
    let ty = path.0.last()?.ty.to_string();
    legacy_node_kind_from_ty(ty.as_str())
}

pub(crate) fn legacy_node_kind_from_ty(ty: &str) -> Option<NodeKind> {
    match ty {
        "texture" => Some(NodeKind::Texture),
        "shader" => Some(NodeKind::Shader),
        "output" => Some(NodeKind::Output),
        "fixture" => Some(NodeKind::Fixture),
        _ => None,
    }
}
