//! The Studio web router: one owned route model over the URL hash.
//!
//! Routes are the app's navigation vocabulary, and they are **history
//! entries**: opening a runtime pushes state, the back button returns to
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
//! #/sim/<project-key>    the editor as a lens on THE sim session running
//!                        that project (slug — the user-facing identifier —
//!                        or a `prj_…` uid as fallback). A sim runtime's
//!                        identity is its project (D37).
//! #/device/<dev-uid>     the editor as a lens on that device's session;
//!                        the project comes from the device.
//! #/stories[/<story-id>] the story book (dev)
//! ```
//!
//! **The URL is the focused document** (the runtime-pool ADR's SDI
//! record): the model is multi-document — N runtime sessions in the pool —
//! but the interface is single-document, one editor lens at a time, and
//! the URL addresses the RUNTIME the lens is on, never a library project.
//! `#/project/<key>` is deleted outright (no users, no redirect — Yona
//! 2026-07-16).
//!
//! Reconciliation rules (implemented in `web_app.rs`):
//! - the editor is showing → the route follows the LENS via
//!   [`lens_route`]: lens on the sim + open project → `Sim(slug)` (a
//!   **push** when coming from `Home` — a gallery open, a new history
//!   entry — a **replace** otherwise); lens on the device → `Device(uid)`.
//!   A not-yet-identified device has no honest address; the URL stays put.
//! - the editor went away → `replace(Home)` once an open had actually
//!   started (`saw_opening`); the boot-time home flash never rewrites the
//!   URL, or a startup reopen would erase the very route that requested
//!   it.
//! - browser navigation (back/forward/manual hash edit) → dispatch: to
//!   `Home` while the editor is open = lens detach (runtime-pool P3: the
//!   editor closes, every runtime session keeps running); to `Sim` = the
//!   open-on-sim path (create/reuse the sim session and push the head —
//!   D19 — or re-attach when that project is already the sim's loaded
//!   project); to `Device` = attach the existing session for that uid, or
//!   granted-port connect (M1) + attach. Connecting/failed device states
//!   render honestly on the gallery's cards (their connect evidence) — the
//!   device route never shows the opening frame.
//! - reload = re-derivation by the same rules: the pool dies with the
//!   page, and the route rebuilds its runtime (`Sim` respawns + loads;
//!   `Device` reconnects the granted port + attaches).
//!
//! `navigate`/`replace` update the URL via the History API, which fires
//! **no** events — the caller updates the route signal itself, so browser
//! events (`popstate`/`hashchange`) always mean real user navigation and
//! need no echo guards.
//!
//! The story *capture* harness's `?story-png=1&story=…` query params are
//! deliberately not routing (see `story_book.rs`) — they are a harness
//! seam, frozen so `scripts/studio-story-pngs.mjs` keeps working.

use lpa_studio_core::{UiLensRuntime, UiStudioView};

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
    /// The editor as a lens on THE sim session running this project. The
    /// key is the slug (preferred) or a `prj_…` uid (machine-stable
    /// fallback). Reload respawns the sim and loads the project.
    Sim { key: String },
    /// The editor as a lens on this device's runtime session (`dev_…`
    /// uid). Reload connects the granted port (M1) and attaches.
    Device { uid: String },
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
    /// `Home` — the URL is user input (this is also where the deleted
    /// `#/project/<key>` lands: as `Home`, no redirect). A hash-internal
    /// query (the story book's `?viewport=`) is not part of the route and
    /// is stripped; its owner parses it from the raw hash.
    pub(crate) fn parse(hash: &str) -> Self {
        let path = hash.trim_start_matches('#');
        let (path, _hash_query) = path.split_once('?').unwrap_or((path, ""));
        let mut segments = path.split('/').filter(|s| !s.is_empty());
        match segments.next() {
            Some("sim") => match segments.next() {
                Some(key) if segments.next().is_none() => StudioRoute::Sim {
                    key: key.to_string(),
                },
                _ => StudioRoute::Home,
            },
            Some("device") => match segments.next() {
                Some(uid) if segments.next().is_none() => StudioRoute::Device {
                    uid: uid.to_string(),
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
            StudioRoute::Sim { key } => format!("#/sim/{key}"),
            StudioRoute::Device { uid } => format!("#/device/{uid}"),
            StudioRoute::Stories { story_id: None } => "#/stories".to_string(),
            StudioRoute::Stories { story_id: Some(id) } => format!("#/stories/{id}"),
        }
    }

    /// Whether the emitted view already shows this SIM route's project
    /// (the key may be either the slug or the uid). Drives the opening
    /// frame — which only sim routes render; a device route's connecting
    /// window renders honestly on the gallery's cards instead.
    pub(crate) fn sim_matches_view(&self, view: &UiStudioView) -> bool {
        match self {
            StudioRoute::Sim { key } => {
                view.open_project_uid.as_deref() == Some(key)
                    || view.open_project_slug.as_deref() == Some(key)
            }
            _ => false,
        }
    }
}

/// The route the LENS binds, when the editor has an addressable one (SDI:
/// the URL is the focused document). The sim's key is the session's
/// loaded-project slug (it survives detach, so re-attach flows address
/// the same document). `None` while the lens is detached, while the sim
/// runs nothing library-backed (the storeless demo path), and for a
/// device whose identity has not landed — in each case the caller leaves
/// the URL alone.
///
/// The caller gates on "the editor is showing" (`!view.panes.is_empty()`):
/// mid-open views (lens claimed, mirror not yet built) must not rewrite
/// the URL that requested them.
pub(crate) fn lens_route(view: &UiStudioView) -> Option<StudioRoute> {
    match view.lens.as_ref()? {
        UiLensRuntime::Sim { project_key } => {
            project_key.clone().map(|key| StudioRoute::Sim { key })
        }
        UiLensRuntime::Device { uid } => uid.clone().map(|uid| StudioRoute::Device { uid }),
    }
}

/// The route at page boot: the hash, verbatim. (The pre-router `?project=`
/// query kindness is gone with `#/project/` itself — same no-users
/// rationale; [`replace`] still strips the stale params on first write.)
#[cfg(target_arch = "wasm32")]
pub(crate) fn boot_route() -> StudioRoute {
    StudioRoute::parse(&current_hash().unwrap_or_default())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn boot_route() -> StudioRoute {
    StudioRoute::Home
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
    use lpa_studio_core::{UiConsoleView, UiPaneView, UiStatus, UiViewContent};

    use super::*;

    #[test]
    fn routes_round_trip_through_their_hash() {
        let routes = [
            StudioRoute::Home,
            StudioRoute::Sim {
                key: "2026-07-09-1421-basic".to_string(),
            },
            StudioRoute::Sim {
                key: "prj_abc123".to_string(),
            },
            StudioRoute::Device {
                uid: "dev_aaaaaaaaaaaaaaaa".to_string(),
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
            "#/sim",
            "#/sim/prj_x/extra",
            "#/device",
            "#/device/dev_x/extra",
        ] {
            assert_eq!(StudioRoute::parse(hash), StudioRoute::Home, "{hash:?}");
        }
    }

    #[test]
    fn the_deleted_project_route_reads_as_home_with_no_redirect() {
        // D37: `#/project/<key>` is deleted outright (no users, no
        // redirect) — it parses as any other unknown hash.
        assert_eq!(
            StudioRoute::parse("#/project/2026-07-09-1421-basic"),
            StudioRoute::Home
        );
        assert_eq!(StudioRoute::parse("#/project/prj_abc"), StudioRoute::Home);
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
    fn legacy_params_strip_and_harness_params_pass() {
        assert_eq!(
            strip_legacy_params("?project=prj_abc&connect=simulator&story-png=1"),
            "?story-png=1"
        );
        assert_eq!(strip_legacy_params("?connect=usb"), "");
    }

    // -----------------------------------------------------------------
    // lens_route: the URL is the focused document (SDI)
    // -----------------------------------------------------------------

    fn editor_view(lens: Option<UiLensRuntime>) -> UiStudioView {
        let pane = UiPaneView::new(
            "project",
            "Project",
            UiStatus::neutral("Ready"),
            UiViewContent::Text(String::new()),
            Vec::new(),
        );
        UiStudioView::new(vec![pane], UiConsoleView::empty()).with_lens(lens)
    }

    #[test]
    fn lens_on_the_sim_binds_the_sim_route_by_slug() {
        let view = editor_view(Some(UiLensRuntime::Sim {
            project_key: Some("2026-07-09-1421-basic".to_string()),
        }));
        assert_eq!(
            lens_route(&view),
            Some(StudioRoute::Sim {
                key: "2026-07-09-1421-basic".to_string()
            })
        );
    }

    #[test]
    fn lens_on_a_device_binds_the_device_route_by_uid() {
        let view = editor_view(Some(UiLensRuntime::Device {
            uid: Some("dev_aaaaaaaaaaaaaaaa".to_string()),
        }));
        assert_eq!(
            lens_route(&view),
            Some(StudioRoute::Device {
                uid: "dev_aaaaaaaaaaaaaaaa".to_string()
            })
        );
    }

    #[test]
    fn unaddressable_lenses_bind_nothing() {
        // detached editor: no lens, no route
        assert_eq!(lens_route(&editor_view(None)), None);
        // a device whose identity has not landed has no honest address
        assert_eq!(
            lens_route(&editor_view(Some(UiLensRuntime::Device { uid: None }))),
            None
        );
        // a sim-run project with no library slug (the storeless demo path)
        assert_eq!(
            lens_route(&editor_view(Some(UiLensRuntime::Sim { project_key: None }))),
            None
        );
    }

    #[test]
    fn sim_route_matches_the_view_by_slug_or_uid_and_device_routes_never_frame() {
        let view = editor_view(Some(UiLensRuntime::Sim {
            project_key: Some("2026-07-09-1421-basic".to_string()),
        }))
        .with_open_project(
            Some("prj_abc".to_string()),
            Some("2026-07-09-1421-basic".to_string()),
        );
        for key in ["2026-07-09-1421-basic", "prj_abc"] {
            assert!(
                StudioRoute::Sim {
                    key: key.to_string()
                }
                .sim_matches_view(&view),
                "{key}"
            );
        }
        assert!(
            !StudioRoute::Sim {
                key: "other".to_string()
            }
            .sim_matches_view(&view)
        );
        // device routes render the gallery honestly, never the opening
        // frame — sim_matches_view is deliberately false for them
        assert!(
            !StudioRoute::Device {
                uid: "dev_aaaaaaaaaaaaaaaa".to_string()
            }
            .sim_matches_view(&view)
        );
    }
}
