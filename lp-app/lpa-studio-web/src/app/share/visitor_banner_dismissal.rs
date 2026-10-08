//! Remembering that this device dismissed a project's visitor strip.
//!
//! Pure view state, not a user verb: dismissing only hides the strip on
//! this browser, so it is no `UiAction` and not in the offer tree. The
//! answer lives in `localStorage` under one key per project; every access
//! tolerates a blocked or throwing store (private mode, Safari quirks) by
//! reading as "not dismissed" and writing nothing — the strip just shows.

/// The `localStorage` key for one project's dismissal.
pub(crate) fn dismissal_key(project_name: &str) -> String {
    format!("lp:visitor-banner-dismissed:{project_name}")
}

/// Whether this device dismissed the project's strip. Any storage failure
/// reads as `false`.
pub(crate) fn is_dismissed(project_name: &str) -> bool {
    storage()
        .and_then(|storage| {
            storage
                .get_item(&dismissal_key(project_name))
                .ok()
                .flatten()
        })
        .is_some()
}

/// Remember the dismissal. A failed write is harmless: the strip stays
/// hidden for this session (the caller's own signal) and returns next load.
pub(crate) fn remember_dismissed(project_name: &str) {
    if let Some(storage) = storage() {
        let _ = storage.set_item(&dismissal_key(project_name), "1");
    }
}

#[cfg(target_arch = "wasm32")]
fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

/// Host builds (tests, tooling) have no browser storage.
#[cfg(not(target_arch = "wasm32"))]
fn storage() -> Option<web_sys::Storage> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_project_has_its_own_key() {
        assert_ne!(dismissal_key("radiance-dome"), dismissal_key("choker"));
        assert_eq!(
            dismissal_key("choker"),
            "lp:visitor-banner-dismissed:choker"
        );
    }

    /// Where storage is unavailable the strip is never hidden.
    #[test]
    fn without_storage_nothing_is_dismissed() {
        remember_dismissed("choker");
        assert!(!is_dismissed("choker"));
    }
}
