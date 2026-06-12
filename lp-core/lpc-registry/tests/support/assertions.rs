use lpc_model::{AssetContentType, NodeDefState, NodeKind};
use lpc_registry::ProjectRegistry;

use super::{artifact_asset, root_def};

pub fn assert_loaded_def_kinds(registry: &ProjectRegistry, expected: &[(&str, NodeKind)]) {
    assert_eq!(
        registry.inventory().defs.len(),
        expected.len(),
        "unexpected def inventory: {:#?}",
        registry.inventory().defs
    );

    for (path, kind) in expected {
        let location = root_def(path);
        let entry = registry
            .def(&location)
            .unwrap_or_else(|| panic!("missing def {path}"));
        let NodeDefState::Loaded(def) = &entry.state else {
            panic!("def {path} was not loaded: {:?}", entry.state);
        };
        assert_eq!(def.kind(), *kind, "wrong kind for {path}");
    }
}

pub fn assert_artifact_asset_content_types(
    registry: &ProjectRegistry,
    expected: &[(&str, AssetContentType)],
) {
    assert_eq!(
        registry.inventory().assets.len(),
        expected.len(),
        "unexpected asset inventory: {:#?}",
        registry.inventory().assets
    );

    for (path, content_type) in expected {
        let source = artifact_asset(path);
        let entry = registry
            .asset(&source)
            .unwrap_or_else(|| panic!("missing asset {path}"));
        assert_eq!(
            entry.content_type, *content_type,
            "wrong asset content type for {path}"
        );
        assert!(entry.state.is_available(), "asset {path} is not available");
    }
}
