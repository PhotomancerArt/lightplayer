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
//! /account              the signed-in account's profile page (identity,
//!                       account, sessions); signed out it asks you in
//! /p/<slug>-prj<uid>    THE project route (identity vision D1/D9): the
//!                       editor as a lens on the sim session running that
//!                       project, and the share link, at ONE address — the
//!                       address bar IS the link you paste. The uid is the
//!                       whole of the identity (80 bits of it — the link IS
//!                       the token); the slug in front is cosmetic, so
//!                       renaming never breaks a link already in somebody's
//!                       chat history. A bare `/p/prj<uid>` resolves too.
//! /p/<link>/play        the SAME session, rendered as play mode (panel.md
//!                       P12: the root module's panel, nothing else).
//! /p/<slug>             an EMBEDDED EXAMPLE's canonical address (examples
//!                       vision D4): a bare segment with no uid tail that
//!                       matches an embedded example slug (`fyeah-sign`).
//!                       Resolves client-side — offline, dev, no network —
//!                       and opens a TRANSIENT view session (D2: viewing
//!                       creates nothing; the URL never heals until an
//!                       explicit save forks the project and navigates to
//!                       its fresh `/p/<slug>-<uid>`). View suffixes ride
//!                       it like any lens route. An unknown bare segment
//!                       stays the landing, never a guess.
//! /device/<dev-uid>     the editor as a lens on that device's session;
//!                       the project comes from the device. Devices keep
//!                       their own `dev…` identity (vision D13).
//! /device/<uid>/play    likewise.
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
//! **`/sim/` is gone.** The project route absorbed it (identity vision
//! D1/D9, P3): one project had two addresses — `/sim/<slug>` for the
//! owner and `/p/<slug>-<uid>` for everybody else — which is one address
//! too many for a product whose share gesture is "copy what the address
//! bar already says". Old `/sim/` links get no shim and no redirect: they
//! are pre-1.0, and the hash-era links they descend from already died
//! once (cloud-folders-sync P09).
//!
//! **Play is a lens ZOOM, not a different document.** `/p/x` and
//! `/p/x/play` address the same runtime session, so every
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
//!   [`lens_route`]: lens on the sim + open project → `Project(uid)` (a
//!   **push** when coming from a page — a gallery open, a new history
//!   entry — a **replace** when already on a lens route); lens on the
//!   device → `Device(uid)`.
//!   A not-yet-identified device has no honest address; the URL stays put.
//! - the open project's display name is known (or changed) → the address
//!   bar HEALS to `/p/<slugify(name)>-<uid>` via `replaceState` (D10), so
//!   a stale slug, a case-mangled paste and a bare uid all straighten out
//!   without a navigation or a history entry. One place: `web_app.rs`.
//! - the editor went away → `replace(Devices)` — the gallery the cards
//!   live on, not the `/` landing — once an open had actually started
//!   (`saw_opening`); the boot-time home flash never rewrites the URL, or
//!   a startup reopen would erase the very route that requested it.
//! - browser navigation (back/forward/in-app link/manual URL edit) →
//!   dispatch: to a gallery route (`Devices`/`Projects`, the sections
//!   that render the shell) while the editor is open = lens detach
//!   (runtime-pool P3: the editor closes, every runtime session keeps
//!   running); to `Project` = the open-on-sim path when the LIBRARY HAS
//!   that uid (create/reuse the sim session and push the head — D19 — or
//!   re-attach when that project is already the sim's loaded project), and
//!   otherwise somebody else's share link: the uid is held as a
//!   [`PendingSharedProject`] intent and the app lands on Home with the
//!   URL untouched (the visitor pull is a later round); to `Device` =
//!   attach the existing session for that uid, or granted-port connect
//!   (M1) + attach.
//!   Connecting/failed device states render honestly on the gallery's cards
//!   (their connect evidence) — the device route never shows the opening
//!   frame.
//! - reload = re-derivation by the same rules: the pool dies with the
//!   page, and the route rebuilds its runtime (`Project` respawns the sim
//!   + loads; `Device` reconnects the granted port + attaches).
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
use lpc_cloud_api::share_link;
use lpc_history::PrefixedUid;

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
    /// The account's own profile page (`/account`): identity, account
    /// facts, and the sessions open on it. Lights no nav tab — it is
    /// reached from the identity dropdown, not the tab row — and renders
    /// an invitation to sign in rather than a 404 when nobody is.
    Account,
    /// **The** project address (`/p/<slug>-prj…`, identity vision
    /// D1/D9/D10): the editor as a lens on THE sim session running this
    /// project when the library has it, and the share link somebody else
    /// opens when it does not. One route, because the address bar IS the
    /// link — there is no second, owner-only URL to keep in step.
    ///
    /// Reload respawns the sim and loads the project; `play` renders that
    /// same session as play mode (`/play` suffix).
    Project {
        /// The whole of the identity: 80 bits that are both the project's
        /// name in the system and (for a shared project) the access token.
        /// Routing reads nothing else.
        uid: PrefixedUid,
        /// The slug the address carried, kept so [`StudioRoute::path`] can
        /// re-emit the link the user is looking at rather than collapsing
        /// it to a bare uid. Cosmetic and never compared: the authoritative
        /// slug is recomputed from the project's display name and healed
        /// into the address bar by `web_app.rs` (D10). `None` for a bare
        /// `/p/prj…`.
        slug: Option<String>,
        /// Which surface the route renders — the `/play`, `/patch`, and
        /// `/mapping` suffixes. Same-session zooms/views, never a
        /// different document (`same_session` ignores it).
        view: ProjectView,
    },
    /// An embedded example's canonical address (`/p/<slug>`, examples
    /// vision D4): a bare `/p/` segment with no uid tail, resolved against
    /// the compiled-in example table at parse time — so an `Example` route
    /// always names a real example, and an unknown bare segment reads as
    /// `Home`. Opens a TRANSIENT view session (D2): nothing is installed,
    /// and the address never heals — an explicit save forks the project
    /// and navigates to the fork's own `Project` route.
    Example {
        /// The example's bare slug — the id tail
        /// (`examples/fyeah-sign` → `fyeah-sign`).
        slug: String,
        /// Same view suffixes as [`StudioRoute::Project`] — one session,
        /// mutually exclusive zooms.
        view: ProjectView,
    },
    /// The editor as a lens on this device's runtime session (`dev…`
    /// uid). Reload connects the granted port (M1) and attaches.
    /// `play` renders that same session as play mode (`/play` suffix).
    Device { uid: String, play: bool },
    /// The story book; `None` selects the book's default story.
    Stories { story_id: Option<String> },
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

/// Which surface a [`StudioRoute::Project`] renders. The workbench's view
/// tabs (Nodes · Mapping · Patch) and the play zoom are all suffixes on the
/// ONE project address — mutually exclusive, and never a session boundary.
///
/// `Workspace` is the suffix-less default: the Nodes view (today's node
/// workspace). `Mapping` and `Patch` are workbench views (`/mapping`,
/// `/patch`; `/map` and `/patching` parse as aliases). `Play` is the
/// full-page zoom.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProjectView {
    #[default]
    Workspace,
    Play,
    Patch,
    Mapping,
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
            // The project route. Exactly one link segment, optionally
            // followed by `play` — depth junk is not a guess at some
            // project, it is an unknown path. The segment itself is as
            // forgiving as the shared grammar allows (case mangling,
            // sentence punctuation, a stale or missing slug); a segment
            // with no well-formed `prj…` uid in it reads as the landing,
            // never as a guess.
            Some("p") => match (segments.next(), segments.next(), segments.next()) {
                (Some(link), None, _) => project_route(link, ProjectView::Workspace),
                (Some(link), Some("play"), None) => project_route(link, ProjectView::Play),
                // Both spellings parse; `path()` emits the canonical one
                // (`patch`, `mapping` — RULED at the patching-view G1:
                // "keep it short and simple") and navigation heals an
                // aliased address.
                (Some(link), Some("patch" | "patching"), None) => {
                    project_route(link, ProjectView::Patch)
                }
                (Some(link), Some("mapping" | "map"), None) => {
                    project_route(link, ProjectView::Mapping)
                }
                _ => StudioRoute::Home,
            },
            // `/home` is a kept alias for the root — old links stay
            // loadable; `path()` only ever emits `/`.
            Some("home") if segments.next().is_none() => StudioRoute::Home,
            Some("devices") if segments.next().is_none() => StudioRoute::Devices,
            Some("projects") if segments.next().is_none() => StudioRoute::Projects,
            Some("explore") if segments.next().is_none() => StudioRoute::Explore,
            Some("account") if segments.next().is_none() => StudioRoute::Account,
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
    /// `Project` re-emits whatever slug its route state carries (a bare
    /// `/p/<uid>` when it has none); the authoritative slug comes from the
    /// project's display name, and `web_app.rs` heals the address bar to
    /// it (D10) rather than the router guessing here.
    pub(crate) fn path(&self) -> String {
        match self {
            StudioRoute::Home => "/".to_string(),
            StudioRoute::Devices => "/devices".to_string(),
            StudioRoute::Projects => "/projects".to_string(),
            StudioRoute::Explore => "/explore".to_string(),
            StudioRoute::Account => "/account".to_string(),
            StudioRoute::Project { uid, slug, view } => {
                let path = share_link::canonical_path(slug.as_deref().unwrap_or_default(), *uid);
                match view {
                    ProjectView::Workspace => path,
                    ProjectView::Play => format!("{path}/play"),
                    ProjectView::Patch => format!("{path}/patch"),
                    ProjectView::Mapping => format!("{path}/mapping"),
                }
            }
            StudioRoute::Example { slug, view } => {
                let path = format!("/p/{slug}");
                match view {
                    ProjectView::Workspace => path,
                    ProjectView::Play => format!("{path}/play"),
                    ProjectView::Patch => format!("{path}/patch"),
                    ProjectView::Mapping => format!("{path}/mapping"),
                }
            }
            StudioRoute::Device { uid, play: false } => format!("/device/{uid}"),
            StudioRoute::Device { uid, play: true } => format!("/device/{uid}/play"),
            StudioRoute::Stories { story_id: None } => "/stories".to_string(),
            StudioRoute::Stories { story_id: Some(id) } => format!("/stories/{id}"),
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

    /// Whether the emitted view already shows this PROJECT route's project.
    /// The uid is the whole comparison — the slug in the address is
    /// cosmetic and may be stale, so matching on it would frame a project
    /// that is already open. Drives the opening frame, which only project
    /// routes render; a device route's connecting window renders honestly
    /// on the gallery's cards instead.
    pub(crate) fn project_matches_view(&self, view: &UiStudioView) -> bool {
        match self {
            StudioRoute::Project { uid, .. } => {
                view.open_project_uid.as_deref() == Some(uid.to_string().as_str())
            }
            // An example route is showing when the open session is the
            // TRANSIENT view of that example — the ephemeral uid is
            // deliberately not the comparison (it never appears in a URL).
            StudioRoute::Example { slug, .. } => view
                .open_project_transient
                .as_deref()
                .and_then(lpa_studio_core::app::home::embedded_example)
                .is_some_and(|example| example.slug() == slug),
            _ => false,
        }
    }

    /// Whether two routes address the same runtime SESSION — play and
    /// non-play are the same document at different zoom (panel.md P12), so
    /// every "already here?" question uses this instead of `==`. Without it
    /// the view→URL sync would see `/p/x/play` as a different route from
    /// the lens's `/p/x` and rewrite the user straight back out of play.
    ///
    /// A project's slug is ignored for the same reason: the canonicalizer
    /// (D10) rewrites a stale slug in place, and an `==` comparison would
    /// read that rewrite as a navigation to somewhere else.
    pub(crate) fn same_session(&self, other: &StudioRoute) -> bool {
        match (self, other) {
            (StudioRoute::Project { uid: a, .. }, StudioRoute::Project { uid: b, .. }) => a == b,
            (StudioRoute::Example { slug: a, .. }, StudioRoute::Example { slug: b, .. }) => a == b,
            (StudioRoute::Device { uid: a, .. }, StudioRoute::Device { uid: b, .. }) => a == b,
            _ => self == other,
        }
    }

    /// This route rendering the given project view; the views are
    /// mutually exclusive suffixes on one address, so setting any view
    /// leaves every other one. Non-project routes return unchanged.
    pub(crate) fn with_view(&self, view: ProjectView) -> StudioRoute {
        match self {
            StudioRoute::Project { uid, slug, .. } => StudioRoute::Project {
                uid: *uid,
                slug: slug.clone(),
                view,
            },
            StudioRoute::Example { slug, .. } => StudioRoute::Example {
                slug: slug.clone(),
                view,
            },
            other => other.clone(),
        }
    }

    /// This route with play mode on/off; anything but a lens route is
    /// returned unchanged (nothing else has a play zoom).
    pub(crate) fn with_play(&self, play: bool) -> StudioRoute {
        match self {
            // Entering or leaving play exits every other view — they are
            // exclusive zooms on one session.
            StudioRoute::Project { .. } | StudioRoute::Example { .. } => self.with_view(if play {
                ProjectView::Play
            } else {
                ProjectView::Workspace
            }),
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
            StudioRoute::Project {
                view: ProjectView::Play,
                ..
            } | StudioRoute::Example {
                view: ProjectView::Play,
                ..
            } | StudioRoute::Device { play: true, .. }
        )
    }

    /// The project view this route renders; non-project routes read as
    /// the workspace (they have no view suffix to speak of).
    pub(crate) fn project_view(&self) -> ProjectView {
        match self {
            StudioRoute::Project { view, .. } | StudioRoute::Example { view, .. } => *view,
            _ => ProjectView::Workspace,
        }
    }

    /// Whether this route is a lens on a runtime session (the routes that
    /// have a play variant at all).
    pub(crate) fn is_lens(&self) -> bool {
        matches!(
            self,
            StudioRoute::Project { .. } | StudioRoute::Example { .. } | StudioRoute::Device { .. }
        )
    }
}

/// One `/p/` link segment as a route: the shared grammar's `(slug, uid)`
/// split, with an empty slug read as no slug at all. A segment with no
/// well-formed uid tail gets one more chance as an embedded example's
/// bare slug (examples vision D4 — the grammar `split_segment` refuses is
/// exactly the free real estate; the split rule itself is untouched, one
/// copy, pre-#384 lesson). A segment that names neither is the landing —
/// never a guess.
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "reached through parse from the wasm URL plumbing; host builds only run the unit tests"
    )
)]
fn project_route(link: &str, view: ProjectView) -> StudioRoute {
    match share_link::split_segment(link) {
        Some((slug, uid)) => StudioRoute::Project {
            uid,
            slug: (!slug.is_empty()).then_some(slug),
            view,
        },
        None => match lpa_studio_core::app::home::embedded_example_by_slug(link) {
            Some(example) => StudioRoute::Example {
                slug: example.slug().to_string(),
                view,
            },
            None => StudioRoute::Home,
        },
    }
}

/// A shared project the user arrived on (`/p/<slug>-prjx`) that this
/// library does NOT have, held as an intent until something can act on it.
/// Provided as a context by `web_app.rs` and, this round, deliberately
/// consumed by nobody: the route lands the app on `Home` and the visitor
/// flow (fetch it, offer it, copy it into the library) is a later round.
/// Until then the address bar keeps the pretty link exactly as the sender
/// wrote it — the D10 canonicalization only heals a project we can name,
/// and nobody here can name this one yet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PendingSharedProject(pub(crate) Option<PrefixedUid>);

/// The canonical share path for a project: cosmetic slug, load-bearing uid.
/// Callers hand it the slug once the project's meta is known.
///
/// `uid` is a `&str` here because its caller (the gallery card's `<a
/// href>`) carries uids the way its view model does — as a string straight
/// off a card. Every one of them is an
/// already-valid project uid, so this delegates to
/// [`share_link::canonical_path`] and falls back to the bare shape only
/// for a malformed uid, which never happens in practice.
pub(crate) fn canonical_share_path(slug: &str, uid: &str) -> String {
    match uid.parse::<PrefixedUid>() {
        Ok(uid) => share_link::canonical_path(slug, uid),
        Err(_) => {
            if slug.is_empty() {
                format!("/p/{uid}")
            } else {
                format!("/p/{slug}-{uid}")
            }
        }
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
/// the URL is the focused document). The sim's identity is the session's
/// loaded-project `prj…` uid (it survives detach, so re-attach flows
/// address the same document); the slug that decorates it is cosmetic and
/// comes from the live library binding, so a rename shows up without
/// waiting for a reload. `None` while the lens is detached, while the sim
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
        UiLensRuntime::Sim { project_uid } => {
            // A TRANSIENT view session binds its example's bare address
            // (examples vision D2/D4) — checked BEFORE the loaded-project
            // uid, which for a transient session is the ephemeral RAM uid
            // and must never reach the URL.
            if let Some(example) = view
                .open_project_transient
                .as_deref()
                .and_then(lpa_studio_core::app::home::embedded_example)
            {
                return Some(StudioRoute::Example {
                    slug: example.slug().to_string(),
                    view: ProjectView::Workspace,
                });
            }
            let uid: PrefixedUid = project_uid.as_deref()?.parse().ok()?;
            Some(StudioRoute::Project {
                uid,
                slug: view.open_project_name.clone(),
                view: ProjectView::Workspace,
            })
        }
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

/// Programmatic same-session navigation: push the route and wake the
/// route listener (a manual `push_state` fires no `popstate`, so one is
/// dispatched by hand — the listener treats it like any back/forward).
#[cfg(target_arch = "wasm32")]
pub(crate) fn navigate_push(route: &StudioRoute) {
    let Some(window) = web_sys::window() else {
        return;
    };
    write_history(&window, HistoryWrite::Push, &route.path());
    if let Ok(event) = web_sys::Event::new("popstate") {
        let _ = window.dispatch_event(&event);
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn navigate_push(route: &StudioRoute) {
    let _ = route;
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

/// Why the navigation callback is running — and, for a click, whether the
/// app permits the move at all.
///
/// The two cases are genuinely different, and the difference is history:
/// a `popstate` arrives with the URL **already** moved (the entry is
/// spent, and refusing means putting the old one back), while a click is
/// caught BEFORE the entry is written, so a refusal there costs nothing
/// and leaves no trace. The single-session nav guard needs both halves,
/// so the interceptor asks first and reports second.
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(dead_code, reason = "constructed by the wasm listener only")
)]
pub(crate) enum NavEvent {
    /// A same-origin link click, before any history is written: the
    /// callback returns whether the move may happen. `false` keeps the
    /// URL exactly where it is (the callback has already said why).
    ClickIntent(StudioRoute),
    /// The URL has moved — back/forward, a manual edit, an accepted
    /// click, or a programmatic [`navigate_push`]. The return value is
    /// ignored. Act on the new route.
    Moved,
}

/// Install the browser-navigation listener: `on_navigate` runs on every
/// `popstate` (back/forward, manual URL edits) and on every intercepted
/// in-app link click — see [`NavEvent`] for the two shapes and what the
/// return value means. Programmatic [`navigate`]/[`replace`] calls fire no
/// event, so this callback always means the user moved. Keep the returned
/// guard alive for the app's lifetime (a `use_hook`).
///
/// **The click interception** is what a fragment used to give us for free:
/// a plain click on `<a href="/p/x-prjy">` would otherwise reload the whole
/// page (killing the runtime pool) even with a server fallback behind it.
/// It is deliberately narrow — same-origin, plain left click, no modifier,
/// no `target`/`download` — so cmd/middle-click still opens a real new tab
/// and every external link is left alone. It runs at the window (bubble
/// phase), i.e. AFTER Dioxus's own delegated handlers, so a component that
/// calls `prevent_default()` (the busy package card) still wins.
#[cfg(target_arch = "wasm32")]
pub(crate) fn install_route_listener(
    on_navigate: impl FnMut(NavEvent) -> bool + 'static,
) -> Option<std::rc::Rc<RouteListener>> {
    use core::cell::RefCell;
    use std::rc::Rc;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;

    let window = web_sys::window()?;
    let on_navigate = Rc::new(RefCell::new(on_navigate));

    let popstate = {
        let on_navigate = Rc::clone(&on_navigate);
        Closure::<dyn FnMut(web_sys::Event)>::wrap(Box::new(move |_| {
            (on_navigate.borrow_mut())(NavEvent::Moved);
        }))
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
            if url == current {
                return;
            }
            // Ask BEFORE writing history. A refusal here is invisible —
            // no entry is spent, no forward stack is truncated, and the
            // URL never flickers through the place we would not go.
            let intent = StudioRoute::parse(url.split('?').next().unwrap_or(&url));
            if !(on_navigate.borrow_mut())(NavEvent::ClickIntent(intent)) {
                return;
            }
            write_history(&window, HistoryWrite::Push, &url);
            (on_navigate.borrow_mut())(NavEvent::Moved);
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
    _on_navigate: impl FnMut(NavEvent) -> bool + 'static,
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
            StudioRoute::Account,
            StudioRoute::Project {
                uid: share_uid(),
                slug: Some("2026-07-09-1421-basic".to_string()),
                view: ProjectView::Workspace,
            },
            StudioRoute::Project {
                uid: share_uid(),
                slug: Some("2026-07-09-1421-basic".to_string()),
                view: ProjectView::Play,
            },
            // the bare-uid form: no slug to re-emit
            StudioRoute::Project {
                uid: share_uid(),
                slug: None,
                view: ProjectView::Workspace,
            },
            StudioRoute::Example {
                slug: "fyeah-sign".to_string(),
                view: ProjectView::Workspace,
            },
            StudioRoute::Example {
                slug: "fyeah-sign".to_string(),
                view: ProjectView::Play,
            },
            StudioRoute::Device {
                uid: "devaaaaaaaaaaaaaaaa".to_string(),
                play: false,
            },
            StudioRoute::Device {
                uid: "devaaaaaaaaaaaaaaaa".to_string(),
                play: true,
            },
            StudioRoute::Stories { story_id: None },
            StudioRoute::Stories {
                story_id: Some("base/detail-popover/open-sections".to_string()),
            },
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

    fn share_uid() -> PrefixedUid {
        SHARE_UID.parse().expect("a well-formed project uid")
    }

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
            // `/sim/` retired with P3 — no shim, no redirect
            "/sim",
            "/sim/2026-07-09-1421-basic",
            "/sim/prjh7kq9xy2mq4tb8wz",
            "/sim/prjh7kq9xy2mq4tb8wz/play",
            "/device",
            "/device/devx/extra",
            "/device/devx/play/extra",
            "/mapping/extra",
            // the same, in the legacy dialect
            "#",
            "#/",
            "#/nope",
            "#/sim/2026-07-09-1421-basic",
            "#/device/devx/extra",
            "#/device/devx/play/extra",
            "#/mapping/extra",
            "#/home/extra",
            "#/explore/extra",
            "#/account/extra",
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
            StudioRoute::parse(&format!("/p/zook-dome-{SHARE_UID}/play")),
            StudioRoute::Project {
                uid: share_uid(),
                slug: Some("zook-dome".to_string()),
                view: ProjectView::Play
            }
        );
        // …and on the bare-uid form, which a chip link emits before the
        // canonicalizer has a name to slugify
        assert_eq!(
            StudioRoute::parse(&format!("/p/{SHARE_UID}/play")),
            StudioRoute::Project {
                uid: share_uid(),
                slug: None,
                view: ProjectView::Play
            }
        );
        assert_eq!(
            StudioRoute::parse("/device/deva/play"),
            StudioRoute::Device {
                uid: "deva".to_string(),
                play: true
            }
        );
        // `play` is a suffix, never a link: it names no project, so it is
        // the landing rather than a guess
        assert_eq!(StudioRoute::parse("/p/play"), StudioRoute::Home);
    }

    /// The workbench's Mapping view is a route suffix like play/patch:
    /// it parses, re-emits, and never reads as a different session.
    #[test]
    fn the_mapping_segment_parses_and_round_trips() {
        let route = StudioRoute::parse(&format!("/p/zook-dome-{SHARE_UID}/mapping"));
        assert_eq!(
            route,
            StudioRoute::Project {
                uid: share_uid(),
                slug: Some("zook-dome".to_string()),
                view: ProjectView::Mapping
            }
        );
        assert_eq!(route.path(), format!("/p/zook-dome-{SHARE_UID}/mapping"));
        // a same-session view, exactly like the play/patch zooms
        assert!(route.same_session(&route.with_view(ProjectView::Workspace)));
        assert!(!route.is_play());
        // the views are mutually exclusive suffixes on one address
        assert_eq!(
            route.with_play(true).project_view(),
            ProjectView::Play,
            "entering play leaves the mapping view"
        );
        // the standalone editor is deleted (R1): a bare `/mapping` (no
        // project segment) is user input like any unknown path — Home,
        // never a project guess
        assert_eq!(StudioRoute::parse("/mapping"), StudioRoute::Home);
    }

    /// Both view-suffix spellings parse — `/map` for `/mapping`, `/patching`
    /// for `/patch` — and heal to the canonical emission on the next
    /// `path()`. The canonical naming call is open (patching-view plan G1);
    /// only the emit side moves when it's ruled.
    #[test]
    fn view_suffix_aliases_parse_and_heal_to_canonical() {
        let map_alias = StudioRoute::parse(&format!("/p/zook-dome-{SHARE_UID}/map"));
        assert_eq!(map_alias.project_view(), ProjectView::Mapping);
        assert_eq!(
            map_alias.path(),
            format!("/p/zook-dome-{SHARE_UID}/mapping"),
            "aliased address heals to the canonical spelling"
        );

        let patching_alias = StudioRoute::parse(&format!("/p/zook-dome-{SHARE_UID}/patching"));
        assert_eq!(patching_alias.project_view(), ProjectView::Patch);
        assert_eq!(
            patching_alias.path(),
            format!("/p/zook-dome-{SHARE_UID}/patch")
        );

        // Aliases are view suffixes only — never standalone routes.
        assert_eq!(StudioRoute::parse("/map"), StudioRoute::Home);
        assert_eq!(StudioRoute::parse("/patching"), StudioRoute::Home);
    }

    // -----------------------------------------------------------------
    // `/p/<slug>` — embedded example addresses (examples vision D4)
    // -----------------------------------------------------------------

    /// A bare segment matching an embedded example slug is that example's
    /// canonical address; an unknown bare segment stays the landing —
    /// never a guess — and uid-bearing segments are untouched grammar.
    #[test]
    fn bare_example_slugs_parse_and_unknown_ones_stay_home() {
        assert_eq!(
            StudioRoute::parse("/p/fyeah-sign"),
            StudioRoute::Example {
                slug: "fyeah-sign".to_string(),
                view: ProjectView::Workspace,
            }
        );
        assert_eq!(
            StudioRoute::parse("/p/fyeah-sign/play"),
            StudioRoute::Example {
                slug: "fyeah-sign".to_string(),
                view: ProjectView::Play,
            }
        );
        assert_eq!(
            StudioRoute::parse("/p/fyeah-sign/mapping").project_view(),
            ProjectView::Mapping
        );
        // unknown bare slug → the landing, exactly as before this grammar
        assert_eq!(StudioRoute::parse("/p/no-such-example"), StudioRoute::Home);
        // junk decoration around a real uid keeps reading as that project
        assert_eq!(
            StudioRoute::parse(&format!("/p/fyeah-sign-{SHARE_UID}")),
            StudioRoute::Project {
                uid: share_uid(),
                slug: Some("fyeah-sign".to_string()),
                view: ProjectView::Workspace,
            }
        );
        // depth junk under a slug is an unknown path, not a guess
        assert_eq!(
            StudioRoute::parse("/p/fyeah-sign/play/extra"),
            StudioRoute::Home
        );
    }

    /// Example routes are lens routes: play/view zooms are same-session,
    /// and two different examples are different sessions.
    #[test]
    fn example_routes_are_same_session_across_views() {
        let route = StudioRoute::parse("/p/fyeah-sign");
        assert!(route.is_lens());
        assert!(route.same_session(&route.with_play(true)));
        assert!(route.with_play(true).is_play());
        assert!(!route.same_session(&StudioRoute::parse("/p/plasma")));
    }

    /// The transient marker — not the (ephemeral) open uid — is what says
    /// an example route is showing; an ordinary project view never
    /// matches an example route.
    #[test]
    fn example_route_matches_only_the_transient_view() {
        let route = StudioRoute::parse("/p/fyeah-sign");
        let mut view = UiStudioView::new(Vec::new(), UiConsoleView::empty())
            .with_open_project(Some(SHARE_UID.to_string()), Some("Fyeah Sign".to_string()));
        assert!(!route.project_matches_view(&view));
        view.open_project_transient = Some("examples/fyeah-sign".to_string());
        assert!(route.project_matches_view(&view));
    }

    /// A transient session's lens binds the BARE example address — the
    /// ephemeral uid the session runs under must never reach the URL.
    #[test]
    fn a_transient_lens_binds_the_bare_example_address() {
        let mut view = editor_view(Some(UiLensRuntime::Sim {
            project_uid: Some(SHARE_UID.to_string()),
        }))
        .with_open_project(Some(SHARE_UID.to_string()), Some("Fyeah Sign".to_string()));
        assert_eq!(
            lens_route(&view),
            Some(StudioRoute::Project {
                uid: share_uid(),
                slug: Some("Fyeah Sign".to_string()),
                view: ProjectView::Workspace,
            }),
            "an ordinary library session binds its project address"
        );
        view.open_project_transient = Some("examples/fyeah-sign".to_string());
        assert_eq!(
            lens_route(&view),
            Some(StudioRoute::Example {
                slug: "fyeah-sign".to_string(),
                view: ProjectView::Workspace,
            }),
            "the transient marker wins over the loaded-project uid"
        );
    }

    // -----------------------------------------------------------------
    // `/p/` — THE project route (identity vision D1/D9/D10)
    // -----------------------------------------------------------------

    /// The uid is the identity and the link token; the slug in front of it
    /// is cosmetic, so two names for one uid are one project (which is what
    /// makes renaming safe for links already in somebody's chat history).
    #[test]
    fn project_paths_read_the_uid_whatever_decorates_it() {
        for path in [
            format!("/p/zook-dome-{SHARE_UID}"),
            format!("/p/{SHARE_UID}"),
            format!("/p/{SHARE_UID}/"),
            format!("/p/a-very-long-renamed-project-{SHARE_UID}"),
            // a stale slug for the same uid is the same project
            format!("/p/old-name-{SHARE_UID}"),
            // and the legacy dialect, for a link shared before the cutover
            format!("#/p/zook-dome-{SHARE_UID}"),
            // D10: the whole segment survives case mangling (a link
            // through a shouting chat client, a mail app that title-cases)
            format!("/p/{}", format!("zook-dome-{SHARE_UID}").to_uppercase()),
            // trailing sentence punctuation is junk, not part of the link
            format!("/p/zook-dome-{SHARE_UID})."),
            // …and the play zoom addresses the same project
            format!("/p/zook-dome-{SHARE_UID}/play"),
        ] {
            let route = StudioRoute::parse(&path);
            assert!(
                matches!(&route, StudioRoute::Project { uid, .. } if *uid == share_uid()),
                "{path:?} parsed as {route:?}"
            );
        }
    }

    /// The slug rides along for display only — a stale or shouted one is
    /// preserved verbatim so `path()` re-emits the link the user is looking
    /// at; healing it is the canonicalizer's job, not the parser's.
    #[test]
    fn a_project_path_keeps_its_slug_verbatim_for_display() {
        assert_eq!(
            StudioRoute::parse(&format!("/p/Old-Name-{SHARE_UID}")),
            StudioRoute::Project {
                uid: share_uid(),
                slug: Some("Old-Name".to_string()),
                view: ProjectView::Workspace,
            }
        );
        // a bare uid carries no slug at all (not an empty one)
        assert_eq!(
            StudioRoute::parse(&format!("/p/{SHARE_UID}")),
            StudioRoute::Project {
                uid: share_uid(),
                slug: None,
                view: ProjectView::Workspace,
            }
        );
    }

    /// Strictness is the point: a truncated or padded body is a MISS, not a
    /// guess at some other project. Depth junk is a miss too — `/p/` takes
    /// exactly one link segment, plus an optional `play`.
    #[test]
    fn project_paths_without_a_well_formed_uid_read_as_the_landing() {
        for path in [
            "/p",
            "/p/",
            // NB not an embedded example slug — a bare segment matching one
            // parses as that example's address now (examples vision D4;
            // `bare_example_slugs_parse_and_unknown_ones_stay_home`)
            "/p/zook-dome-renamed",
            "/p/prjtooshort",
            "/p/prjh7kq9xy2mq4tb8wzextra",
            "/p/prjh7kq9xy2mq4tb8w-",
            // right shape, wrong kind of thing
            "/p/devh7kq9xy2mq4tb8wz",
            "/p/usrh7Kq9xY2mQ4tB8Wz",
            // depth junk: a nested path is not a link (it used to resolve
            // off its last segment; `/play` needs the position to mean
            // something, so the shape is now exact)
            "/p/some/nested/thing-prjh7kq9xy2mq4tb8wz",
            "/p/zook-dome-prjh7kq9xy2mq4tb8wz/play/extra",
            "/p/zook-dome-prjh7kq9xy2mq4tb8wz/edit",
        ] {
            assert_eq!(StudioRoute::parse(path), StudioRoute::Home, "{path:?}");
        }
    }

    #[test]
    fn the_canonical_share_path_round_trips_and_survives_an_empty_slug() {
        let canonical = canonical_share_path("zook-dome", SHARE_UID);
        assert_eq!(canonical, format!("/p/zook-dome-{SHARE_UID}"));
        assert_eq!(
            StudioRoute::parse(&canonical),
            StudioRoute::Project {
                uid: share_uid(),
                slug: Some("zook-dome".to_string()),
                view: ProjectView::Workspace,
            }
        );
        assert_eq!(
            canonical_share_path("", SHARE_UID),
            format!("/p/{SHARE_UID}")
        );
        assert_eq!(
            StudioRoute::parse(&canonical_share_path("", SHARE_UID)),
            StudioRoute::Project {
                uid: share_uid(),
                slug: None,
                view: ProjectView::Workspace,
            }
        );
    }

    /// A stale slug and the canonical one are the SAME session, so the
    /// canonicalizer's `replaceState` never reads as a navigation and the
    /// view→URL sync never fights it.
    #[test]
    fn a_stale_slug_addresses_the_same_session_as_the_canonical_one() {
        let stale = StudioRoute::parse(&format!("/p/old-name-{SHARE_UID}"));
        let canonical = StudioRoute::parse(&format!("/p/zook-dome-{SHARE_UID}"));
        let bare = StudioRoute::parse(&format!("/p/{SHARE_UID}"));
        assert_ne!(stale, canonical);
        assert!(stale.same_session(&canonical));
        assert!(bare.same_session(&canonical));
        assert!(canonical.same_session(&canonical.with_play(true)));
    }

    /// Play is a lens ZOOM: toggling it must never read as a different
    /// document, or the view→URL sync would bounce the user out of it.
    #[test]
    fn play_and_non_play_are_the_same_session() {
        let editing = StudioRoute::Project {
            uid: share_uid(),
            slug: Some("basic".to_string()),
            view: ProjectView::Workspace,
        };
        let playing = editing.with_play(true);
        assert_ne!(editing, playing);
        assert!(editing.same_session(&playing));
        assert!(playing.same_session(&editing));
        assert!(playing.is_play() && !editing.is_play());
        assert!(playing.is_lens() && editing.is_lens());
        // a different project is a different session, play or not
        assert!(!playing.same_session(&StudioRoute::Project {
            uid: "prj0000000000000000".parse().expect("a project uid"),
            slug: Some("basic".to_string()),
            view: ProjectView::Play
        }));
        // and a device is never a project
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
    fn project_matches_view_ignores_play() {
        let view = editor_view(Some(UiLensRuntime::Sim {
            project_uid: Some(SHARE_UID.to_string()),
        }))
        .with_open_project(Some(SHARE_UID.to_string()), Some("basic".to_string()));
        assert!(
            StudioRoute::Project {
                uid: share_uid(),
                slug: Some("basic".to_string()),
                view: ProjectView::Play
            }
            .project_matches_view(&view)
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

    /// The lens binds the project route by UID; the open package's slug
    /// rides along as the address's cosmetic half.
    #[test]
    fn lens_on_the_sim_binds_the_project_route_by_uid() {
        let view = editor_view(Some(UiLensRuntime::Sim {
            project_uid: Some(SHARE_UID.to_string()),
        }))
        .with_open_project(
            Some(SHARE_UID.to_string()),
            Some("2026-07-09-1421-basic".to_string()),
        );
        assert_eq!(
            lens_route(&view),
            Some(StudioRoute::Project {
                uid: share_uid(),
                slug: Some("2026-07-09-1421-basic".to_string()),
                view: ProjectView::Workspace
            })
        );
        assert_eq!(
            lens_route(&view).map(|route| route.path()),
            Some(format!("/p/2026-07-09-1421-basic-{SHARE_UID}"))
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
        // a sim-run project with no library identity (the storeless demo
        // path) has no honest address either
        assert_eq!(
            lens_route(&editor_view(Some(UiLensRuntime::Sim { project_uid: None }))),
            None
        );
    }

    #[test]
    fn a_project_route_matches_the_view_by_uid_and_device_routes_never_frame() {
        let view = editor_view(Some(UiLensRuntime::Sim {
            project_uid: Some(SHARE_UID.to_string()),
        }))
        .with_open_project(
            Some(SHARE_UID.to_string()),
            Some("2026-07-09-1421-basic".to_string()),
        );
        // whatever slug the address carries — canonical, stale or absent —
        // the uid is what decides
        for slug in [Some("2026-07-09-1421-basic"), Some("old-name"), None] {
            let route = StudioRoute::Project {
                uid: share_uid(),
                slug: slug.map(str::to_string),
                view: ProjectView::Workspace,
            };
            assert!(route.project_matches_view(&view), "{slug:?}");
        }
        assert!(
            !StudioRoute::Project {
                uid: "prj0000000000000000".parse().expect("a project uid"),
                slug: Some("2026-07-09-1421-basic".to_string()),
                view: ProjectView::Workspace
            }
            .project_matches_view(&view)
        );
        // device routes render the gallery honestly, never the opening
        // frame — project_matches_view is deliberately false for them
        assert!(
            !StudioRoute::Device {
                uid: "devaaaaaaaaaaaaaaaa".to_string(),
                play: false
            }
            .project_matches_view(&view)
        );
    }
}
