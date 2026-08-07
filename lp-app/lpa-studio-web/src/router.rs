//! The Studio web router: one owned route model over the URL PATH.
//!
//! Routes are the app's navigation vocabulary, and they are **history
//! entries**: opening a runtime pushes state, the back button returns to
//! the gallery, forward reopens. The shell is route-framed and
//! actor-filled — the route picks which frame renders (gallery, opening
//! frame, story book); the studio actor's emitted view fills it. The core
//! stays route-free: reconciliation lives in `web_app.rs` against
//! [`UiStudioView`](lpa_studio_core::UiStudioView).
//!
//! Route table (real paths — the service SPA-falls-back on every unknown
//! path, so each of these is a loadable, linkable address):
//!
//! ```text
//! /                     the Home landing (via the logo, not a nav tab);
//!                       also the empty/unknown path. `/home` still parses
//!                       as an alias, and is never emitted.
//! /devices              the devices section
//! /projects             the projects library section
//! /explore              the explore section (placeholder until modpacks)
//! /sim/<project-key>    the editor as a lens on THE sim session running
//!                       that project (slug — the user-facing identifier —
//!                       or a `prj…` uid as fallback). A sim runtime's
//!                       identity is its project (D37).
//! /sim/<key>/play       the SAME session, rendered as play mode (panel.md
//!                       P12: the root module's panel, nothing else).
//! /device/<dev-uid>     the editor as a lens on that device's session;
//!                       the project comes from the device.
//! /device/<uid>/play    likewise.
//! /p/<slug>-prj<uid>    a SHARED project link (D24): the uid is the whole
//!                       of the identity (80 bits of it — the link IS the
//!                       token) and the slug in front is cosmetic, so
//!                       renaming never breaks a link already in somebody's
//!                       chat history. A bare `/p/prj<uid>` resolves too.
//! /stories[/<story-id>] the story book (dev)
//! /mapping              the standalone 2D mapping editor
//! /boards[/<vendor>/<product>], /boards/edit
//! /docs[/<article-slug>]
//! ```
//!
//! **Why the hash died.** Hash routing bought one thing: any dumb static
//! host (GitHub Pages) served the app from `index.html` whatever the route
//! said. The cloud service pays that cost properly now — it serves the SPA
//! on unknown paths (cloud-folders-sync P07/P10) — and the fragment's price
//! came due with share links: a `/p/…` link is handed to people who do NOT
//! have the app open, and a fragment is never sent to a server, so a
//! `#/p/…` share link could never be served, canonicalized, or unfurled by
//! anything. Paths are also what every other web surface (docs pages,
//! boards catalog) already reads like. See the plan
//! `2026-08-05-1642-cloud-folders-sync/p09-router-paths.md` (D24 the share
//! path, D25 the path re-encoding); the cloud-hosting ADR lands with the
//! service phases, beside the sibling
//! `docs/adr/2026-08-05-project-history-dag-joins.md`.
//!
//! **Old hashes keep working forever.** [`install_legacy_hash_shim`] runs
//! at boot, before anything reads the URL, and `replaceState`s a `#/…`
//! location to its path equivalent. It is five lines and it is
//! remove-never: bookmarks and pasted links outlive re-encodings (the
//! story-capture harness still drives the book by hash, too).
//!
//! **Play is a lens ZOOM, not a different document.** `/sim/x` and
//! `/sim/x/play` address the same runtime session, so every
//! route-equivalence question ("is the view already showing this route?",
//! "is the lens already bound here?") is asked through
//! [`StudioRoute::same_session`], which ignores the flag. Toggling play must
//! never re-open, re-attach, or reload anything.
//!
//! **The URL is the focused document** (the runtime-pool ADR's SDI
//! record): the model is multi-document — N runtime sessions in the pool —
//! but the interface is single-document, one editor lens at a time, and
//! the URL addresses the RUNTIME the lens is on, never a library project.
//! `/project/<key>` is deleted outright (no users, no redirect — Yona
//! 2026-07-16).
//!
//! Reconciliation rules (implemented in `web_app.rs`):
//! - the editor is showing → the route follows the LENS via
//!   [`lens_route`]: lens on the sim + open project → `Sim(slug)` (a
//!   **push** when coming from a page — a gallery open, a new history
//!   entry — a **replace** when already on a lens route); lens on the
//!   device → `Device(uid)`.
//!   A not-yet-identified device has no honest address; the URL stays put.
//! - the editor went away → `replace(Devices)` — the gallery the cards
//!   live on, not the `/` landing — once an open had actually started
//!   (`saw_opening`); the boot-time home flash never rewrites the URL, or
//!   a startup reopen would erase the very route that requested it.
//! - browser navigation (back/forward/in-app link/manual URL edit) →
//!   dispatch: to a gallery route (`Devices`/`Projects`, the sections
//!   that render the shell) while the editor is open = lens detach
//!   (runtime-pool P3: the editor closes, every runtime session keeps
//!   running); to `Sim` = the open-on-sim path (create/reuse the sim
//!   session and push the head — D19 — or re-attach when that project is
//!   already the sim's loaded project); to `Device` = attach the existing
//!   session for that uid, or granted-port connect (M1) + attach.
//!   Connecting/failed device states render honestly on the gallery's cards
//!   (their connect evidence) — the device route never shows the opening
//!   frame. `SharedProject` lands on Home carrying a pending intent; the
//!   open/pull flow itself is a later round.
//! - reload = re-derivation by the same rules: the pool dies with the
//!   page, and the route rebuilds its runtime (`Sim` respawns + loads;
//!   `Device` reconnects the granted port + attaches).
//!
//! `navigate`/`replace` update the URL via the History API, which fires
//! **no** events — the caller updates the route signal itself, so a
//! `popstate` always means real user navigation and needs no echo guard.
//! In-app links stay plain `<a href="/…">` anchors (cmd/middle-click opens
//! a real new tab, as it should); [`install_route_listener`] intercepts the
//! plain-click case so a link is a history push and not a page load, which
//! is what a fragment used to give us for free.
//!
//! The story *capture* harness's `?story-png=1&story=…` query params are
//! deliberately not routing (see `story_book.rs`) — they are a harness
//! seam, frozen so `scripts/studio-story-pngs.mjs` keeps working.

use lpa_studio_core::{UiLensRuntime, UiStudioView};
use lpc_history::{PrefixedUid, UID_BODY_LEN, UidPrefix};

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
    /// The landing page — the `/` (root) landing. Reached through the
    /// logo, not a nav tab (vision D1/D11). `/home` still parses as an
    /// alias so old links keep working, but only `/` is ever emitted.
    /// Unknown/malformed paths land here too — the URL is user input.
    /// Placeholder content until M3.
    ///
    /// The root used to be [`StudioRoute::Devices`] (vision Q2's lean:
    /// "are my devices up?" is a returning user's first question), marked
    /// revisit-when-Home-is-real; Yona ruled for Home at the root
    /// 2026-08-06.
    Home,
    /// The devices section (`/devices`).
    Devices,
    /// The projects library section (`/projects`). Renders the same
    /// gallery as Devices until the P09 page split.
    Projects,
    /// The explore section (`/explore`) — community/example content.
    /// Placeholder until modpack scaffolding gives it real material.
    Explore,
    /// The editor as a lens on THE sim session running this project. The
    /// key is the slug (preferred) or a `prj…` uid (machine-stable
    /// fallback). Reload respawns the sim and loads the project.
    /// `play` renders that same session as play mode (`/play` suffix).
    Sim { key: String, play: bool },
    /// The editor as a lens on this device's runtime session (`dev…`
    /// uid). Reload connects the granted port (M1) and attaches.
    /// `play` renders that same session as play mode (`/play` suffix).
    Device { uid: String, play: bool },
    /// A shared-project link (`/p/<slug>-prj…`, D24). The uid is the
    /// identity AND the link token; the slug that decorates it is cosmetic
    /// and is dropped at parse. Round one lands the app on `Home` with this
    /// uid held as a pending intent — the open/pull flow is a later round.
    SharedProject { uid: String },
    /// The story book; `None` selects the book's default story.
    Stories { story_id: Option<String> },
    /// The standalone 2D mapping editor (project-free; edits
    /// `.map2d.json` documents with localStorage autosave).
    MappingEditor,
    /// The public boards catalog (project-free, renders the checked-in
    /// board display metadata). `board` deep-links one board's detail view
    /// (`vendor/product`).
    Boards { board: Option<String> },
    /// The standalone board display-def editor (project-free; edits
    /// `.display.json` sidecars with localStorage autosave).
    BoardEditor,
    /// The in-app docs section (compiled-in `docs/user-guide/` articles).
    /// `page` deep-links one article by slug; `None` (and any unknown
    /// slug — the page's concern, not the router's) lands on the guide's
    /// landing article. `anchor` deep-links a heading inside the article
    /// (`#/docs/<slug>#<anchor>` — the whole string is `location.hash`,
    /// so the anchor rides INSIDE the routed hash and the docs page does
    /// the scrolling; the browser's native fragment scroll never sees it).
    Docs {
        page: Option<String>,
        anchor: Option<String>,
    },
}

#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "driven by the wasm URL plumbing; host builds only run the unit tests"
    )
)]
impl StudioRoute {
    /// Parse a `location.pathname`. Unknown or malformed paths read as
    /// `Home` — the root landing; the URL is user input (this is also
    /// where the deleted `/project/<key>` lands: as `Home`, no redirect).
    /// A query (the story book's `?viewport=`) is not part of the route
    /// and is stripped; its owner parses it from `location.search`.
    ///
    /// A legacy `#/…` string parses identically, so the shim and its tests
    /// can speak either dialect — but the shim still rewrites the address
    /// bar, because the rest of the app (links, history, the server) reads
    /// paths.
    pub(crate) fn parse(path: &str) -> Self {
        let path = path.strip_prefix('#').unwrap_or(path);
        // The fragment is captured, not discarded: the docs route reads it
        // as an in-article anchor. (Callers passing `location.pathname`
        // never carry one; the legacy `#/…` dialect and docs help links
        // do.)
        let (path, fragment) = path.split_once('#').unwrap_or((path, ""));
        let (path, _query) = path.split_once('?').unwrap_or((path, ""));
        let mut segments = path.split('/').filter(|s| !s.is_empty());
        match segments.next() {
            Some("sim") => match (segments.next(), segments.next(), segments.next()) {
                (Some(key), None, _) => StudioRoute::Sim {
                    key: key.to_string(),
                    play: false,
                },
                (Some(key), Some("play"), None) => StudioRoute::Sim {
                    key: key.to_string(),
                    play: true,
                },
                _ => StudioRoute::Home,
            },
            Some("device") => match (segments.next(), segments.next(), segments.next()) {
                (Some(uid), None, _) => StudioRoute::Device {
                    uid: uid.to_string(),
                    play: false,
                },
                (Some(uid), Some("play"), None) => StudioRoute::Device {
                    uid: uid.to_string(),
                    play: true,
                },
                _ => StudioRoute::Home,
            },
            // A share link: only the LAST segment is examined, so
            // `/p/<slug>-prjx` and `/p/anything/else/prjx` both resolve
            // and a path with no uid in it resolves to the landing rather
            // than to a guess. Mirrors `lp-cloud-server`'s
            // `page::share_path`.
            Some("p") => segments
                .next_back()
                .and_then(share_uid_from_segment)
                .map_or(StudioRoute::Home, |uid| StudioRoute::SharedProject { uid }),
            // `/home` is a kept alias for the root — old links stay
            // loadable; `path()` only ever emits `/`.
            Some("home") if segments.next().is_none() => StudioRoute::Home,
            Some("devices") if segments.next().is_none() => StudioRoute::Devices,
            Some("projects") if segments.next().is_none() => StudioRoute::Projects,
            Some("explore") if segments.next().is_none() => StudioRoute::Explore,
            Some("mapping") if segments.next().is_none() => StudioRoute::MappingEditor,
            Some("boards") => {
                let rest: Vec<&str> = segments.collect();
                if rest == ["edit"] {
                    StudioRoute::BoardEditor
                } else {
                    StudioRoute::Boards {
                        board: (rest.len() == 2).then(|| rest.join("/")),
                    }
                }
            }
            Some("docs") => {
                let rest: Vec<&str> = segments.collect();
                match rest.as_slice() {
                    [] => StudioRoute::Docs {
                        page: None,
                        anchor: None,
                    },
                    // One segment; the anchor (help links) was split off in
                    // the prologue as the URL fragment. An empty anchor
                    // (`slug#`) reads as no anchor.
                    [only] => {
                        let (slug, anchor) = (*only, Some(fragment));
                        let page = (!slug.is_empty()).then(|| slug.to_string());
                        // An anchor without a page has nothing to scroll.
                        let anchor = page
                            .is_some()
                            .then_some(anchor)
                            .flatten()
                            .filter(|anchor| !anchor.is_empty())
                            .map(str::to_string);
                        StudioRoute::Docs { page, anchor }
                    }
                    _ => StudioRoute::Docs {
                        page: None,
                        anchor: None,
                    },
                }
            }
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

    /// The canonical path for this route (always `/`-prefixed).
    ///
    /// `/` IS the Home path — the root landing is Home (see the variant
    /// docs); `/home` parses in as an alias but is never emitted. A
    /// `SharedProject` renders as the bare `/p/<uid>`:
    /// the pretty slug is decoration the router does not know, and is put
    /// back by [`canonical_share_path`] once the project's meta is in
    /// hand.
    pub(crate) fn path(&self) -> String {
        match self {
            StudioRoute::Home => "/".to_string(),
            StudioRoute::Devices => "/devices".to_string(),
            StudioRoute::Projects => "/projects".to_string(),
            StudioRoute::Explore => "/explore".to_string(),
            StudioRoute::Sim { key, play: false } => format!("/sim/{key}"),
            StudioRoute::Sim { key, play: true } => format!("/sim/{key}/play"),
            StudioRoute::Device { uid, play: false } => format!("/device/{uid}"),
            StudioRoute::Device { uid, play: true } => format!("/device/{uid}/play"),
            StudioRoute::SharedProject { uid } => format!("/p/{uid}"),
            StudioRoute::Stories { story_id: None } => "/stories".to_string(),
            StudioRoute::Stories { story_id: Some(id) } => format!("/stories/{id}"),
            StudioRoute::MappingEditor => "/mapping".to_string(),
            StudioRoute::Boards { board: None } => "/boards".to_string(),
            StudioRoute::Boards { board: Some(board) } => format!("/boards/{board}"),
            StudioRoute::BoardEditor => "/boards/edit".to_string(),
            StudioRoute::Docs { page: None, .. } => "/docs".to_string(),
            StudioRoute::Docs {
                page: Some(page),
                anchor: None,
            } => format!("/docs/{page}"),
            // In the path world the anchor is a REAL url fragment — the
            // browser scrolls to it natively; the docs page also reads it.
            StudioRoute::Docs {
                page: Some(page),
                anchor: Some(anchor),
            } => format!("/docs/{page}#{anchor}"),
        }
    }

    /// Whether the emitted view already shows this SIM route's project
    /// (the key may be either the slug or the uid). Drives the opening
    /// frame — which only sim routes render; a device route's connecting
    /// window renders honestly on the gallery's cards instead.
    pub(crate) fn sim_matches_view(&self, view: &UiStudioView) -> bool {
        match self {
            StudioRoute::Sim { key, play: _ } => {
                view.open_project_uid.as_deref() == Some(key)
                    || view.open_project_slug.as_deref() == Some(key)
            }
            _ => false,
        }
    }

    /// Whether two routes address the same runtime SESSION — play and
    /// non-play are the same document at different zoom (panel.md P12), so
    /// every "already here?" question uses this instead of `==`. Without it
    /// the view→URL sync would see `#/sim/x/play` as a different route from
    /// the lens's `#/sim/x` and rewrite the user straight back out of play.
    pub(crate) fn same_session(&self, other: &StudioRoute) -> bool {
        match (self, other) {
            (StudioRoute::Sim { key: a, .. }, StudioRoute::Sim { key: b, .. }) => a == b,
            (StudioRoute::Device { uid: a, .. }, StudioRoute::Device { uid: b, .. }) => a == b,
            _ => self == other,
        }
    }

    /// This route with play mode on/off; anything but a lens route is
    /// returned unchanged (nothing else has a play zoom).
    pub(crate) fn with_play(&self, play: bool) -> StudioRoute {
        match self {
            StudioRoute::Sim { key, .. } => StudioRoute::Sim {
                key: key.clone(),
                play,
            },
            StudioRoute::Device { uid, .. } => StudioRoute::Device {
                uid: uid.clone(),
                play,
            },
            other => other.clone(),
        }
    }

    /// Whether this route renders play mode.
    pub(crate) fn is_play(&self) -> bool {
        matches!(
            self,
            StudioRoute::Sim { play: true, .. } | StudioRoute::Device { play: true, .. }
        )
    }

    /// Whether this route is a lens on a runtime session (the routes that
    /// have a play variant at all).
    pub(crate) fn is_lens(&self) -> bool {
        matches!(self, StudioRoute::Sim { .. } | StudioRoute::Device { .. })
    }
}

/// A shared project the user arrived on (`/p/<slug>-prjx`), held as an
/// intent until something can act on it. Provided as a context by
/// `web_app.rs` and, this round, deliberately consumed by nobody: the route
/// lands the app on `Home` and the open/pull flow (fetch it, offer it, copy
/// it into the library) is the post-chrome frontend round. Until then the
/// address bar keeps the pretty link — a bare `/p/<uid>` rewrite would
/// throw away the slug the sender chose, and the canonicalization that puts
/// it back ([`canonical_share_path`]) needs project meta nobody has yet.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PendingSharedProject(pub(crate) Option<String>);

/// The project uid inside one share-path segment, if there is one.
///
/// The slug may itself contain `-` — and `prj` can even occur inside a
/// base-32 uid body — but the uid's length is FIXED, so the split point is
/// simply the last `"prj".len() + UID_BODY_LEN` characters; strict parsing
/// of that tail is what makes trailing junk a miss rather than a
/// truncation. Same rule as the server's `page::share_path` — the two
/// halves must agree about what a link means.
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "reached through parse from the wasm URL plumbing; host builds only run the unit tests"
    )
)]
fn share_uid_from_segment(segment: &str) -> Option<String> {
    let start = segment.len().checked_sub("prj".len() + UID_BODY_LEN)?;
    let uid: PrefixedUid = segment.get(start..)?.parse().ok()?;
    (uid.prefix() == UidPrefix::Project).then(|| uid.to_string())
}

/// The canonical share path for a project: cosmetic slug, load-bearing uid.
/// Callers hand it the slug once the project's meta is known (the pending
/// shared-project intent starts life with the uid alone).
#[allow(
    dead_code,
    reason = "the share UI that writes canonical links is the post-chrome round; the rule and its tests land with the parser they mirror"
)]
pub(crate) fn canonical_share_path(slug: &str, uid: &str) -> String {
    if slug.is_empty() {
        format!("/p/{uid}")
    } else {
        format!("/p/{slug}-{uid}")
    }
}

/// The path a legacy `#/…` location should become, or `None` when the hash
/// is not a legacy route (no hash at all, a plain anchor fragment, …).
///
/// The hash may carry its own query (`#/stories/x?viewport=md`, the story
/// book's dialect); those params are merged AFTER the real query so a
/// bookmarked hash query still wins over nothing, and the harness's
/// `?story-png=1` survives the trip.
#[allow(
    dead_code,
    reason = "called by the wasm boot shim; host builds only run the unit tests"
)]
fn legacy_hash_url(hash: &str, search: &str) -> Option<String> {
    let route = hash.strip_prefix("#/")?;
    let (path, hash_query) = route.split_once('?').unwrap_or((route, ""));
    let params: Vec<&str> = search
        .trim_start_matches('?')
        .split('&')
        .chain(hash_query.split('&'))
        .filter(|pair| !pair.is_empty())
        .collect();
    let query = if params.is_empty() {
        String::new()
    } else {
        format!("?{}", params.join("&"))
    };
    Some(format!("/{path}{query}"))
}

/// The legacy-hash boot shim: rewrite a `#/…` location to its path
/// equivalent BEFORE anything reads the URL (called from `main`, ahead of
/// the Dioxus launch — the story book and the preview lab read the URL in
/// their own early returns, before the router's hooks run).
///
/// Keep this forever. Bookmarks, pasted links and the story-capture harness
/// all still speak hash, and five lines is a cheap promise to keep.
#[cfg(target_arch = "wasm32")]
pub(crate) fn install_legacy_hash_shim() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let location = window.location();
    let hash = location.hash().unwrap_or_default();
    let search = location.search().unwrap_or_default();
    let Some(url) = legacy_hash_url(&hash, &search) else {
        return;
    };
    if let Ok(history) = window.history() {
        let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&url));
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn install_legacy_hash_shim() {}

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
///
/// The lens knows nothing about play mode, so the bound route always reads
/// `play: false`; the caller compares with [`StudioRoute::same_session`] and
/// leaves a play URL alone.
pub(crate) fn lens_route(view: &UiStudioView) -> Option<StudioRoute> {
    match view.lens.as_ref()? {
        UiLensRuntime::Sim { project_key } => project_key
            .clone()
            .map(|key| StudioRoute::Sim { key, play: false }),
        UiLensRuntime::Device { uid } => uid
            .clone()
            .map(|uid| StudioRoute::Device { uid, play: false }),
    }
}

/// The route at page boot: the path, verbatim (the legacy shim has already
/// turned a `#/…` location into one). The pre-router `?project=` query
/// kindness is gone with `/project/` itself — same no-users rationale;
/// [`replace`] still strips the stale params on first write.
#[cfg(target_arch = "wasm32")]
pub(crate) fn boot_route() -> StudioRoute {
    StudioRoute::parse(&current_path().unwrap_or_default())
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
    let Some(window) = web_sys::window() else {
        return;
    };
    let location = window.location();
    let current_path = location.pathname().unwrap_or_default();
    let target_path = route.path();
    let search = location.search().unwrap_or_default();
    let cleaned_search = strip_legacy_params(&search);
    if current_path == target_path && search == cleaned_search {
        return;
    }
    write_history(&window, mode, &format!("{target_path}{cleaned_search}"));
}

#[cfg(not(target_arch = "wasm32"))]
fn write_url(_route: &StudioRoute, _mode: HistoryWrite) {}

/// Rewrite the current entry to `route` carrying one explicit query
/// parameter, keeping every other param the URL already had (the story
/// book's `?viewport=` beside the capture harness's `?story-png=`).
#[allow(
    dead_code,
    reason = "the story book is the only caller, and it is wasm + `stories` glue"
)]
pub(crate) fn replace_with_query_param(route: &StudioRoute, key: &str, value: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(window) = web_sys::window() else {
            return;
        };
        let search = window.location().search().unwrap_or_default();
        let query = upsert_query_param(&strip_legacy_params(&search), key, value);
        write_history(
            &window,
            HistoryWrite::Replace,
            &format!("{}{query}", route.path()),
        );
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (route, key, value);
    }
}

#[cfg(target_arch = "wasm32")]
fn write_history(window: &web_sys::Window, mode: HistoryWrite, url: &str) {
    use wasm_bindgen::JsValue;

    if let Ok(history) = window.history() {
        let result = match mode {
            HistoryWrite::Push => history.push_state_with_url(&JsValue::NULL, "", Some(url)),
            HistoryWrite::Replace => history.replace_state_with_url(&JsValue::NULL, "", Some(url)),
        };
        let _ = result;
    }
}

/// `search` with `key` set to `value` — replacing the existing pair in
/// place (order matters to nobody, but a stable URL is easier to read) or
/// appending it. Returns a `?`-prefixed string, or the empty string.
#[allow(
    dead_code,
    reason = "called by the wasm URL writer; host builds only run the unit tests"
)]
fn upsert_query_param(search: &str, key: &str, value: &str) -> String {
    let mut replaced = false;
    let mut params: Vec<String> = search
        .trim_start_matches('?')
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            if pair.split_once('=').map_or(pair, |(name, _)| name) == key {
                replaced = true;
                format!("{key}={value}")
            } else {
                pair.to_string()
            }
        })
        .collect();
    if !replaced {
        params.push(format!("{key}={value}"));
    }
    format!("?{}", params.join("&"))
}

/// Drop the pre-router query params (`project`, `connect`) plus the OAuth
/// return leg's `code` (normally scrubbed at boot by the OpenRouter
/// interceptor; this is the second net); everything else (e.g. the story
/// capture harness's params) passes through.
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
            key != "project" && key != "connect" && key != "code"
        })
        .collect();
    if kept.is_empty() {
        String::new()
    } else {
        format!("?{}", kept.join("&"))
    }
}

/// Full page reload — the escape hatch for in-app navigations into the
/// story book (and the standalone editors), which only mount on a fresh
/// page load (their early returns in `App` run before any hooks; switching
/// modes live would change the hook order). The reload re-requests the new
/// PATH, so these routes are the ones that need the service's SPA fallback
/// to be in place (cloud-folders-sync P07/P10).
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
    StudioRoute::parse(&current_path().unwrap_or_default())
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn current_route() -> StudioRoute {
    StudioRoute::Home
}

#[cfg(target_arch = "wasm32")]
fn current_path() -> Option<String> {
    web_sys::window()
        .map(|window| window.location())
        .and_then(|location| location.pathname().ok())
}

/// Install the browser-navigation listener: `on_navigate` runs on every
/// `popstate` (back/forward, manual URL edits) and on every intercepted
/// in-app link click. Programmatic [`navigate`]/[`replace`] calls fire no
/// event, so this callback always means the user moved. Keep the returned
/// guard alive for the app's lifetime (a `use_hook`).
///
/// **The click interception** is what a fragment used to give us for free:
/// a plain click on `<a href="/sim/x">` would otherwise reload the whole
/// page (killing the runtime pool) even with a server fallback behind it.
/// It is deliberately narrow — same-origin, plain left click, no modifier,
/// no `target`/`download` — so cmd/middle-click still opens a real new tab
/// and every external link is left alone. It runs at the window (bubble
/// phase), i.e. AFTER Dioxus's own delegated handlers, so a component that
/// calls `prevent_default()` (the busy package card) still wins.
#[cfg(target_arch = "wasm32")]
pub(crate) fn install_route_listener(
    on_navigate: impl FnMut() + 'static,
) -> Option<std::rc::Rc<RouteListener>> {
    use core::cell::RefCell;
    use std::rc::Rc;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;

    let window = web_sys::window()?;
    let on_navigate = Rc::new(RefCell::new(on_navigate));

    let popstate = {
        let on_navigate = Rc::clone(&on_navigate);
        Closure::<dyn FnMut(web_sys::Event)>::wrap(Box::new(move |_| (on_navigate.borrow_mut())()))
    };
    window
        .add_event_listener_with_callback("popstate", popstate.as_ref().unchecked_ref())
        .ok()?;

    let click = {
        let on_navigate = Rc::clone(&on_navigate);
        let window = window.clone();
        Closure::<dyn FnMut(web_sys::MouseEvent)>::wrap(Box::new(move |event| {
            let Some(url) = in_app_link_url(&window, &event) else {
                return;
            };
            // Ours either way: even a click on the URL we already show must
            // not reload the page.
            event.prevent_default();
            let current = current_url(&window);
            if url != current {
                write_history(&window, HistoryWrite::Push, &url);
                (on_navigate.borrow_mut())();
            }
        }))
    };
    window
        .add_event_listener_with_callback("click", click.as_ref().unchecked_ref())
        .ok()?;

    Some(std::rc::Rc::new(RouteListener {
        window,
        popstate,
        click,
    }))
}

/// The path+query a click should navigate to in-app, or `None` when the
/// browser should handle the click itself (external link, new tab, a
/// handler that already claimed it, a non-link click).
#[cfg(target_arch = "wasm32")]
fn in_app_link_url(window: &web_sys::Window, event: &web_sys::MouseEvent) -> Option<String> {
    use wasm_bindgen::JsCast;

    if event.default_prevented()
        || event.button() != 0
        || event.meta_key()
        || event.ctrl_key()
        || event.shift_key()
        || event.alt_key()
    {
        return None;
    }
    let anchor = event
        .target()?
        .dyn_into::<web_sys::Element>()
        .ok()?
        .closest("a")
        .ok()??
        .dyn_into::<web_sys::HtmlAnchorElement>()
        .ok()?;
    // A tool card opening in its own tab, a download link, a foreign origin
    // (or a `mailto:`, whose origin is never ours) stay the browser's.
    let target = anchor.target();
    if !(target.is_empty() || target == "_self") || anchor.has_attribute("download") {
        return None;
    }
    if anchor.origin() != window.location().origin().ok()? {
        return None;
    }
    Some(format!("{}{}", anchor.pathname(), anchor.search()))
}

#[cfg(target_arch = "wasm32")]
fn current_url(window: &web_sys::Window) -> String {
    let location = window.location();
    format!(
        "{}{}",
        location.pathname().unwrap_or_default(),
        location.search().unwrap_or_default()
    )
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
    popstate: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::Event)>,
    #[cfg(target_arch = "wasm32")]
    click: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::MouseEvent)>,
}

#[cfg(target_arch = "wasm32")]
impl Drop for RouteListener {
    fn drop(&mut self) {
        use wasm_bindgen::JsCast;
        let _ = self.window.remove_event_listener_with_callback(
            "popstate",
            self.popstate.as_ref().unchecked_ref(),
        );
        let _ = self
            .window
            .remove_event_listener_with_callback("click", self.click.as_ref().unchecked_ref());
    }
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::{UiConsoleView, UiPaneView, UiStatus, UiViewContent};

    use super::*;

    /// Every route, once, so the round-trip and the legacy-hash tests below
    /// stay in step with the table in the module header.
    fn every_route() -> Vec<StudioRoute> {
        vec![
            StudioRoute::Home,
            StudioRoute::Devices,
            StudioRoute::Projects,
            StudioRoute::Explore,
            StudioRoute::Sim {
                key: "2026-07-09-1421-basic".to_string(),
                play: false,
            },
            StudioRoute::Sim {
                key: "2026-07-09-1421-basic".to_string(),
                play: true,
            },
            StudioRoute::Sim {
                key: "prjabc123".to_string(),
                play: false,
            },
            StudioRoute::Device {
                uid: "devaaaaaaaaaaaaaaaa".to_string(),
                play: false,
            },
            StudioRoute::Device {
                uid: "devaaaaaaaaaaaaaaaa".to_string(),
                play: true,
            },
            StudioRoute::SharedProject {
                uid: SHARE_UID.to_string(),
            },
            StudioRoute::Stories { story_id: None },
            StudioRoute::Stories {
                story_id: Some("base/detail-popover/open-sections".to_string()),
            },
            StudioRoute::MappingEditor,
            StudioRoute::Boards { board: None },
            StudioRoute::Boards {
                board: Some("domraem/dom-z-102".to_string()),
            },
            StudioRoute::BoardEditor,
            StudioRoute::Docs {
                page: None,
                anchor: None,
            },
            StudioRoute::Docs {
                page: Some("brightness-and-smooth-fades".to_string()),
                anchor: None,
            },
            StudioRoute::Docs {
                page: Some("what-is-a-shader".to_string()),
                anchor: Some("the-reveal".to_string()),
            },
        ]
    }

    /// A minted project uid’s shape: `prj` + 16 base-32 characters.
    const SHARE_UID: &str = "prjh7kq9xy2mq4tb8wz";

    #[test]
    fn routes_round_trip_through_their_path() {
        for route in every_route() {
            let path = route.path();
            assert!(path.starts_with('/'), "{path:?} is not a path");
            // No residual hash ROUTING — the one legitimate `#` is the
            // docs anchor, which is a real URL fragment the browser
            // scrolls to.
            let docs_anchor = matches!(
                &route,
                StudioRoute::Docs {
                    anchor: Some(_),
                    ..
                }
            );
            assert!(
                docs_anchor || !path.contains('#'),
                "{path:?} still carries a fragment"
            );
            assert_eq!(StudioRoute::parse(&path), route, "{route:?}");
        }
    }

    /// Old bookmarks outlive re-encodings: `#/x` reads as `/x`, and the boot
    /// shim rewrites the address bar to match.
    #[test]
    fn legacy_hashes_parse_and_shim_to_their_path_equivalents() {
        for route in every_route() {
            let path = route.path();
            let legacy = format!("#{path}");
            assert_eq!(StudioRoute::parse(&legacy), route, "{legacy:?}");
            assert_eq!(legacy_hash_url(&legacy, ""), Some(path), "{legacy:?}");
        }
    }

    /// The shim only claims `#/…`; a plain anchor fragment (or no hash at
    /// all) is somebody else's business. Hash-internal queries — the story
    /// book's dialect, and the capture harness's URLs — merge behind the
    /// real query rather than replacing it.
    #[test]
    fn the_shim_merges_queries_and_ignores_non_route_hashes() {
        assert_eq!(legacy_hash_url("", "?story-png=1"), None);
        assert_eq!(legacy_hash_url("#", ""), None);
        assert_eq!(legacy_hash_url("#section", ""), None);
        assert_eq!(legacy_hash_url("#/", ""), Some("/".to_string()));
        assert_eq!(
            legacy_hash_url(
                "#/stories/base/popover/overview?viewport=md",
                "?story-png=1"
            ),
            Some("/stories/base/popover/overview?story-png=1&viewport=md".to_string())
        );
        assert_eq!(
            legacy_hash_url("#/preview-lab?cards=10&autostart=1", "?r=17"),
            Some("/preview-lab?r=17&cards=10&autostart=1".to_string())
        );
    }

    #[test]
    fn unknown_and_malformed_paths_read_as_home() {
        for path in [
            "",
            "/",
            "/nope",
            "/sim",
            "/sim/prjx/extra",
            "/sim/prjx/play/extra",
            "/device",
            "/device/devx/extra",
            "/device/devx/play/extra",
            "/mapping/extra",
            // the same, in the legacy dialect
            "#",
            "#/",
            "#/nope",
            "#/sim",
            "#/device/devx/extra",
            "#/device/devx/play/extra",
            "#/mapping/extra",
            "#/home/extra",
            "#/explore/extra",
        ] {
            assert_eq!(StudioRoute::parse(path), StudioRoute::Home, "{path:?}");
        }
    }

    /// `/` and the kept `/home` alias both land on Home — the root IS the
    /// landing (Yona 2026-08-06, reversing vision Q2's devices-at-the-root
    /// lean), and only `/` is ever emitted back. The legacy `#/` dialect
    /// parses the same way, so an old `#/` bookmark follows the root's new
    /// meaning.
    #[test]
    fn the_root_path_is_the_home_landing() {
        assert_eq!(StudioRoute::parse("/"), StudioRoute::Home);
        assert_eq!(StudioRoute::parse(""), StudioRoute::Home);
        assert_eq!(StudioRoute::parse("/home"), StudioRoute::Home);
        assert_eq!(StudioRoute::parse("#/"), StudioRoute::Home);
        assert_eq!(StudioRoute::parse("#/home"), StudioRoute::Home);
        assert_eq!(StudioRoute::Home.path(), "/");
    }

    /// Devices is its own section now, at its own slug.
    #[test]
    fn the_devices_section_has_its_own_slug() {
        assert_eq!(StudioRoute::parse("/devices"), StudioRoute::Devices);
        assert_eq!(StudioRoute::parse("#/devices"), StudioRoute::Devices);
        assert_eq!(StudioRoute::Devices.path(), "/devices");
    }

    #[test]
    fn the_play_segment_parses_on_both_lens_routes() {
        assert_eq!(
            StudioRoute::parse("/sim/basic/play"),
            StudioRoute::Sim {
                key: "basic".to_string(),
                play: true
            }
        );
        assert_eq!(
            StudioRoute::parse("/device/deva/play"),
            StudioRoute::Device {
                uid: "deva".to_string(),
                play: true
            }
        );
        // `play` is a suffix, never a key
        assert_eq!(
            StudioRoute::parse("/sim/play"),
            StudioRoute::Sim {
                key: "play".to_string(),
                play: false
            }
        );
    }

    // -----------------------------------------------------------------
    // `/p/` — the share link (D24)
    // -----------------------------------------------------------------

    /// The uid is the identity and the link token; the slug in front of it
    /// is cosmetic, so two names for one uid are one project (which is what
    /// makes renaming safe for links already in somebody's chat history).
    #[test]
    fn share_paths_read_the_uid_whatever_decorates_it() {
        let shared = StudioRoute::SharedProject {
            uid: SHARE_UID.to_string(),
        };
        for path in [
            format!("/p/zook-dome-{SHARE_UID}"),
            format!("/p/{SHARE_UID}"),
            format!("/p/{SHARE_UID}/"),
            format!("/p/a-very-long-renamed-project-{SHARE_UID}"),
            // a stale slug for the same uid is the same project
            format!("/p/old-name-{SHARE_UID}"),
            // only the LAST segment is examined
            format!("/p/some/nested/thing-{SHARE_UID}"),
            // and the legacy dialect, for a link shared before the cutover
            format!("#/p/zook-dome-{SHARE_UID}"),
        ] {
            assert_eq!(StudioRoute::parse(&path), shared, "{path:?}");
        }
    }

    /// Strictness is the point: a truncated or padded body is a MISS, not a
    /// guess at some other project.
    #[test]
    fn share_paths_without_a_well_formed_uid_read_as_the_landing() {
        for path in [
            "/p",
            "/p/",
            "/p/zook-dome",
            "/p/prjtooshort",
            "/p/prjh7kq9xy2mq4tb8wzextra",
            "/p/prjh7kq9xy2mq4tb8w-",
            // right shape, wrong kind of thing
            "/p/devh7kq9xy2mq4tb8wz",
            "/p/usrh7Kq9xY2mQ4tB8Wz",
        ] {
            assert_eq!(StudioRoute::parse(path), StudioRoute::Home, "{path:?}");
        }
    }

    #[test]
    fn the_canonical_share_path_round_trips_and_survives_an_empty_slug() {
        let shared = StudioRoute::SharedProject {
            uid: SHARE_UID.to_string(),
        };
        let canonical = canonical_share_path("zook-dome", SHARE_UID);
        assert_eq!(canonical, format!("/p/zook-dome-{SHARE_UID}"));
        assert_eq!(StudioRoute::parse(&canonical), shared);
        assert_eq!(
            canonical_share_path("", SHARE_UID),
            format!("/p/{SHARE_UID}")
        );
        assert_eq!(
            StudioRoute::parse(&canonical_share_path("", SHARE_UID)),
            shared
        );
        // the router's own path is the bare uid — the slug comes back only
        // once the project's meta is known
        assert_eq!(shared.path(), format!("/p/{SHARE_UID}"));
    }

    /// A share link is not a lens: it has no play zoom and it is nobody's
    /// session (round one it lands on Home with a pending intent).
    #[test]
    fn a_share_route_is_not_a_lens() {
        let shared = StudioRoute::SharedProject {
            uid: SHARE_UID.to_string(),
        };
        assert!(!shared.is_lens());
        assert!(!shared.is_play());
        assert_eq!(shared.with_play(true), shared);
        assert!(!shared.same_session(&StudioRoute::Home));
        assert!(shared.same_session(&shared));
    }

    /// Play is a lens ZOOM: toggling it must never read as a different
    /// document, or the view→URL sync would bounce the user out of it.
    #[test]
    fn play_and_non_play_are_the_same_session() {
        let editing = StudioRoute::Sim {
            key: "basic".to_string(),
            play: false,
        };
        let playing = editing.with_play(true);
        assert_ne!(editing, playing);
        assert!(editing.same_session(&playing));
        assert!(playing.same_session(&editing));
        assert!(playing.is_play() && !editing.is_play());
        assert!(playing.is_lens() && editing.is_lens());
        // a different project is a different session, play or not
        assert!(!playing.same_session(&StudioRoute::Sim {
            key: "other".to_string(),
            play: true
        }));
        // and a device is never a sim
        assert!(!playing.same_session(&StudioRoute::Device {
            uid: "basic".to_string(),
            play: true
        }));
        // non-lens routes have no play zoom and compare by equality
        assert_eq!(StudioRoute::Home.with_play(true), StudioRoute::Home);
        assert!(!StudioRoute::Home.is_lens());
        assert!(StudioRoute::Home.same_session(&StudioRoute::Home));
        assert!(!StudioRoute::Home.same_session(&editing));
    }

    /// The opening frame follows the session, not the zoom: a play URL on a
    /// project the view has not reached yet still frames.
    #[test]
    fn sim_matches_view_ignores_play() {
        let view = editor_view(Some(UiLensRuntime::Sim {
            project_key: Some("basic".to_string()),
        }))
        .with_open_project(Some("prjabc".to_string()), Some("basic".to_string()));
        assert!(
            StudioRoute::Sim {
                key: "basic".to_string(),
                play: true
            }
            .sim_matches_view(&view)
        );
    }

    #[test]
    fn the_deleted_project_route_reads_as_the_landing_with_no_redirect() {
        // D37: `/project/<key>` is deleted outright (no users, no
        // redirect) — it parses as any other unknown path.
        assert_eq!(
            StudioRoute::parse("/project/2026-07-09-1421-basic"),
            StudioRoute::Home
        );
        assert_eq!(StudioRoute::parse("/project/prjabc"), StudioRoute::Home);
    }

    #[test]
    fn docs_junk_depth_reads_as_the_landing_page() {
        assert_eq!(
            StudioRoute::parse("/docs/a/b"),
            StudioRoute::Docs {
                page: None,
                anchor: None,
            }
        );
        assert_eq!(
            StudioRoute::parse("/docs/"),
            StudioRoute::Docs {
                page: None,
                anchor: None,
            }
        );
    }

    #[test]
    fn docs_anchor_splits_off_the_slug_and_empty_pieces_drop() {
        assert_eq!(
            StudioRoute::parse("/docs/guide#brightness"),
            StudioRoute::Docs {
                page: Some("guide".to_string()),
                anchor: Some("brightness".to_string()),
            }
        );
        assert_eq!(
            StudioRoute::parse("/docs/guide#"),
            StudioRoute::Docs {
                page: Some("guide".to_string()),
                anchor: None,
            }
        );
        assert_eq!(
            StudioRoute::parse("/docs/#lost"),
            StudioRoute::Docs {
                page: None,
                anchor: None,
            }
        );
        // the legacy dialect still parses (the shim's contract)
        assert_eq!(
            StudioRoute::parse("#/docs/guide#brightness"),
            StudioRoute::Docs {
                page: Some("guide".to_string()),
                anchor: Some("brightness".to_string()),
            }
        );
    }

    #[test]
    fn boards_edit_is_the_editor_not_a_board_id() {
        assert_eq!(StudioRoute::parse("/boards/edit"), StudioRoute::BoardEditor);
        // A two-segment id still reads as a board detail deep link.
        assert_eq!(
            StudioRoute::parse("/boards/vendor/edit"),
            StudioRoute::Boards {
                board: Some("vendor/edit".to_string())
            }
        );
    }

    #[test]
    fn story_ids_keep_their_slashes_and_drop_queries() {
        assert_eq!(
            StudioRoute::parse("/stories/studio/home/home-gallery/populated"),
            StudioRoute::Stories {
                story_id: Some("studio/home/home-gallery/populated".to_string())
            }
        );
        assert_eq!(
            StudioRoute::parse("/stories/base/popover/overview?viewport=md"),
            StudioRoute::Stories {
                story_id: Some("base/popover/overview".to_string())
            }
        );
        // and a real fragment on a real path is not part of the route
        assert_eq!(
            StudioRoute::parse("/stories/base/popover/overview#anchor"),
            StudioRoute::Stories {
                story_id: Some("base/popover/overview".to_string())
            }
        );
    }

    #[test]
    fn legacy_params_strip_and_harness_params_pass() {
        assert_eq!(
            strip_legacy_params("?project=prjabc&connect=simulator&story-png=1"),
            "?story-png=1"
        );
        assert_eq!(strip_legacy_params("?connect=usb"), "");
    }

    /// The story book's viewport switch rewrites ONE param and leaves the
    /// capture harness's own params where they are.
    #[test]
    fn a_query_param_is_replaced_in_place_or_appended() {
        assert_eq!(upsert_query_param("", "viewport", "md"), "?viewport=md");
        assert_eq!(
            upsert_query_param("?story-png=1", "viewport", "md"),
            "?story-png=1&viewport=md"
        );
        assert_eq!(
            upsert_query_param("?viewport=lg&story-png=1", "viewport", "sm"),
            "?viewport=sm&story-png=1"
        );
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
                key: "2026-07-09-1421-basic".to_string(),
                play: false
            })
        );
    }

    #[test]
    fn lens_on_a_device_binds_the_device_route_by_uid() {
        let view = editor_view(Some(UiLensRuntime::Device {
            uid: Some("devaaaaaaaaaaaaaaaa".to_string()),
        }));
        assert_eq!(
            lens_route(&view),
            Some(StudioRoute::Device {
                uid: "devaaaaaaaaaaaaaaaa".to_string(),
                play: false
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
            Some("prjabc".to_string()),
            Some("2026-07-09-1421-basic".to_string()),
        );
        for key in ["2026-07-09-1421-basic", "prjabc"] {
            assert!(
                StudioRoute::Sim {
                    key: key.to_string(),
                    play: false
                }
                .sim_matches_view(&view),
                "{key}"
            );
        }
        assert!(
            !StudioRoute::Sim {
                key: "other".to_string(),
                play: false
            }
            .sim_matches_view(&view)
        );
        // device routes render the gallery honestly, never the opening
        // frame — sim_matches_view is deliberately false for them
        assert!(
            !StudioRoute::Device {
                uid: "devaaaaaaaaaaaaaaaa".to_string(),
                play: false
            }
            .sim_matches_view(&view)
        );
    }
}
