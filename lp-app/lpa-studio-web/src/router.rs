//! The Studio web router: one owned route model over the URL hash.
//!
//! Routes are the app's navigation vocabulary, and they are **history
//! entries**: opening a project pushes state, the back button returns to
//! the gallery, forward reopens. The shell is route-framed and
//! actor-filled — the route picks which frame renders (gallery, opening
//! frame, story book); the studio actor's emitted view fills it. The core
//! stays route-free: reconciliation lives in `web_app.rs` against
//! [`UiStudioView`](lpa_studio_core::UiStudioView).
//!
//! Route table (hash-based so any static host serves the app unmodified):
//!
//! ```text
//! #/                     home (the gallery) — also the empty/unknown hash
//! #/project/<key>        a library project (slug — the user-facing
//!                        identifier — or a `prj_…` uid as fallback)
//! #/stories[/<story-id>] the story book (dev)
//! ```
//!
//! Reconciliation rules (implemented in `web_app.rs`):
//! - view shows an open library project → route becomes
//!   `Project(uid)`: a **push** when coming from `Home` (a gallery open —
//!   new history entry), a **replace** otherwise (boot/forward
//!   resolution).
//! - view lost the project → `replace(Home)` once an open had actually
//!   started (`saw_opening`); the boot-time home flash never rewrites the
//!   URL, or a startup reopen would erase the very route that requested
//!   it.
//! - browser navigation (back/forward/manual hash edit) → dispatch: to
//!   `Home` while a project is open = lens detach (runtime-pool P3: the
//!   editor closes, every runtime session keeps running — the gallery
//!   renders live sim/device cards); to `Project` while home = open.
//!
//! `navigate`/`replace` update the URL via the History API, which fires
//! **no** events — the caller updates the route signal itself, so browser
//! events (`popstate`/`hashchange`) always mean real user navigation and
//! need no echo guards.
//!
//! The story *capture* harness's `?story-png=1&story=…` query params are
//! deliberately not routing (see `story_book.rs`) — they are a harness
//! seam, frozen so `scripts/studio-story-pngs.mjs` keeps working.

use lpa_studio_core::UiStudioView;

/// Where the user is (or is headed) in the Studio shell.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "constructed by the wasm route listeners and the stories feature; host builds only run the unit tests"
    )
)]
pub(crate) enum StudioRoute {
    /// The gallery.
    Home,
    /// A library project, open (or opening) in the simulator. The key is
    /// the slug (preferred) or a `prj_…` uid (machine-stable fallback).
    Project { key: String },
    /// The story book; `None` selects the book's default story.
    Stories { story_id: Option<String> },
}

#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "driven by the wasm URL plumbing; host builds only run the unit tests"
    )
)]
impl StudioRoute {
    /// Parse a `location.hash` value. Unknown or malformed hashes read as
    /// `Home` — the URL is user input. A hash-internal query (the story
    /// book's `?viewport=`) is not part of the route and is stripped; its
    /// owner parses it from the raw hash.
    pub(crate) fn parse(hash: &str) -> Self {
        let path = hash.trim_start_matches('#');
        let (path, _hash_query) = path.split_once('?').unwrap_or((path, ""));
        let mut segments = path.split('/').filter(|s| !s.is_empty());
        match segments.next() {
            Some("project") => match segments.next() {
                Some(key) if segments.next().is_none() => StudioRoute::Project {
                    key: key.to_string(),
                },
                _ => StudioRoute::Home,
            },
            Some("stories") => {
                let rest: Vec<&str> = segments.collect();
                StudioRoute::Stories {
                    story_id: (!rest.is_empty()).then(|| rest.join("/")),
                }
            }
            None => StudioRoute::Home,
            Some(_) => StudioRoute::Home,
        }
    }

    /// The canonical hash for this route (always `#/`-prefixed).
    pub(crate) fn hash(&self) -> String {
        match self {
            StudioRoute::Home => "#/".to_string(),
            StudioRoute::Project { key } => format!("#/project/{key}"),
            StudioRoute::Stories { story_id: None } => "#/stories".to_string(),
            StudioRoute::Stories { story_id: Some(id) } => format!("#/stories/{id}"),
        }
    }

    /// Whether the emitted view already shows this route's project (the
    /// key may be either the slug or the uid).
    pub(crate) fn project_matches_view(&self, view: &UiStudioView) -> bool {
        match self {
            StudioRoute::Project { key } => {
                view.open_project_uid.as_deref() == Some(key)
                    || view.open_project_slug.as_deref() == Some(key)
            }
            _ => false,
        }
    }
}

/// The route at page boot: the hash, with one kindness — a legacy
/// `?project=prj_…` query (the pre-router scheme) maps to its route so
/// old bookmarks keep reopening. The caller canonicalizes the URL with
/// [`replace`] afterwards.
#[cfg(target_arch = "wasm32")]
pub(crate) fn boot_route() -> StudioRoute {
    let hash = current_hash().unwrap_or_default();
    let route = StudioRoute::parse(&hash);
    if route != StudioRoute::Home {
        return route;
    }
    if let Some(search) = current_search() {
        if let Some(uid) = legacy_project_param(&search) {
            return StudioRoute::Project { key: uid };
        }
    }
    route
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn boot_route() -> StudioRoute {
    StudioRoute::Home
}

#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "called by the wasm boot path; host builds only run the unit tests"
    )
)]
fn legacy_project_param(search: &str) -> Option<String> {
    search
        .trim_start_matches('?')
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find_map(|(key, value)| {
            (key == "project" && value.starts_with("prj_")).then(|| value.to_string())
        })
}

/// Push a new history entry for `route` and update the URL. Fires no
/// events; the caller owns the route signal. No-ops when the URL already
/// shows the route (keeps history clean).
pub(crate) fn navigate(route: &StudioRoute) {
    write_url(route, HistoryWrite::Push);
}

/// Rewrite the current history entry to `route`. Fires no events.
pub(crate) fn replace(route: &StudioRoute) {
    write_url(route, HistoryWrite::Replace);
}

enum HistoryWrite {
    Push,
    Replace,
}

#[cfg(target_arch = "wasm32")]
fn write_url(route: &StudioRoute, mode: HistoryWrite) {
    use wasm_bindgen::JsValue;

    let Some(window) = web_sys::window() else {
        return;
    };
    let location = window.location();
    let current_hash = location.hash().unwrap_or_default();
    let target_hash = route.hash();
    let search = location.search().unwrap_or_default();
    let cleaned_search = strip_legacy_params(&search);
    if current_hash == target_hash && search == cleaned_search {
        return;
    }
    let pathname = location.pathname().unwrap_or_default();
    let next_url = format!("{pathname}{cleaned_search}{target_hash}");
    if let Ok(history) = window.history() {
        let result = match mode {
            HistoryWrite::Push => history.push_state_with_url(&JsValue::NULL, "", Some(&next_url)),
            HistoryWrite::Replace => {
                history.replace_state_with_url(&JsValue::NULL, "", Some(&next_url))
            }
        };
        let _ = result;
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn write_url(_route: &StudioRoute, _mode: HistoryWrite) {}

/// Drop the pre-router query params (`project`, `connect`); everything
/// else (e.g. the story capture harness's params) passes through.
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "called by the wasm URL writer; host builds only run the unit tests"
    )
)]
fn strip_legacy_params(search: &str) -> String {
    let kept: Vec<&str> = search
        .trim_start_matches('?')
        .split('&')
        .filter(|pair| !pair.is_empty())
        .filter(|pair| {
            let key = pair.split_once('=').map_or(*pair, |(key, _)| key);
            key != "project" && key != "connect"
        })
        .collect();
    if kept.is_empty() {
        String::new()
    } else {
        format!("?{}", kept.join("&"))
    }
}

/// Full page reload — the escape hatch for hash navigations into the
/// story book, which only mounts on a fresh page load (its early return
/// in `App` runs before any hooks; switching modes live would change the
/// hook order).
#[cfg(target_arch = "wasm32")]
pub(crate) fn hard_reload() {
    if let Some(window) = web_sys::window() {
        let _ = window.location().reload();
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn hard_reload() {}

/// The route the URL currently shows.
#[cfg(target_arch = "wasm32")]
pub(crate) fn current_route() -> StudioRoute {
    StudioRoute::parse(&current_hash().unwrap_or_default())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn current_route() -> StudioRoute {
    StudioRoute::Home
}

#[cfg(target_arch = "wasm32")]
fn current_hash() -> Option<String> {
    web_sys::window()
        .map(|window| window.location())
        .and_then(|location| location.hash().ok())
}

#[cfg(target_arch = "wasm32")]
fn current_search() -> Option<String> {
    web_sys::window()
        .map(|window| window.location())
        .and_then(|location| location.search().ok())
}

/// Install the browser-navigation listener: `on_navigate` runs on every
/// `popstate` and `hashchange` (back/forward, manual edits, plain hash
/// links). Programmatic [`navigate`]/[`replace`] calls fire neither event,
/// so this callback always means the browser moved on its own. Keep the
/// returned guard alive for the app's lifetime (a `use_hook`).
#[cfg(target_arch = "wasm32")]
pub(crate) fn install_route_listener(
    mut on_navigate: impl FnMut() + 'static,
) -> Option<std::rc::Rc<RouteListener>> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;

    let window = web_sys::window()?;
    let callback = Closure::<dyn FnMut(web_sys::Event)>::wrap(Box::new(move |_| on_navigate()));
    for event in ["popstate", "hashchange"] {
        window
            .add_event_listener_with_callback(event, callback.as_ref().unchecked_ref())
            .ok()?;
    }
    Some(std::rc::Rc::new(RouteListener { window, callback }))
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn install_route_listener(
    _on_navigate: impl FnMut() + 'static,
) -> Option<std::rc::Rc<RouteListener>> {
    None
}

pub(crate) struct RouteListener {
    #[cfg(target_arch = "wasm32")]
    window: web_sys::Window,
    #[cfg(target_arch = "wasm32")]
    callback: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::Event)>,
}

#[cfg(target_arch = "wasm32")]
impl Drop for RouteListener {
    fn drop(&mut self) {
        use wasm_bindgen::JsCast;
        for event in ["popstate", "hashchange"] {
            let _ = self
                .window
                .remove_event_listener_with_callback(event, self.callback.as_ref().unchecked_ref());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_round_trip_through_their_hash() {
        let routes = [
            StudioRoute::Home,
            StudioRoute::Project {
                key: "2026-07-09-1421-basic".to_string(),
            },
            StudioRoute::Project {
                key: "prj_abc123".to_string(),
            },
            StudioRoute::Stories { story_id: None },
            StudioRoute::Stories {
                story_id: Some("base/detail-popover/open-sections".to_string()),
            },
        ];
        for route in routes {
            assert_eq!(StudioRoute::parse(&route.hash()), route, "{route:?}");
        }
    }

    #[test]
    fn unknown_and_malformed_hashes_read_as_home() {
        for hash in [
            "",
            "#",
            "#/",
            "#/nope",
            "#/project",
            "#/project/prj_x/extra",
        ] {
            assert_eq!(StudioRoute::parse(hash), StudioRoute::Home, "{hash:?}");
        }
    }

    #[test]
    fn story_ids_keep_their_slashes_and_drop_hash_queries() {
        assert_eq!(
            StudioRoute::parse("#/stories/studio/home/home-gallery/populated"),
            StudioRoute::Stories {
                story_id: Some("studio/home/home-gallery/populated".to_string())
            }
        );
        assert_eq!(
            StudioRoute::parse("#/stories/base/popover/overview?viewport=md"),
            StudioRoute::Stories {
                story_id: Some("base/popover/overview".to_string())
            }
        );
    }

    #[test]
    fn legacy_project_param_maps_and_strips() {
        assert_eq!(
            legacy_project_param("?project=prj_abc"),
            Some("prj_abc".to_string())
        );
        assert_eq!(
            StudioRoute::parse("#/project/prj_abc"),
            StudioRoute::Project {
                key: "prj_abc".to_string()
            }
        );
        assert_eq!(legacy_project_param("?project=nope"), None);
        assert_eq!(
            strip_legacy_params("?project=prj_abc&connect=simulator&story-png=1"),
            "?story-png=1"
        );
        assert_eq!(strip_legacy_params("?connect=usb"), "");
    }
}
