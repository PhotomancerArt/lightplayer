//! The live half of the P6 visitor surface: one coordinator per tab that
//! knows who is looking at the open `/p/` project and keeps the tracking
//! copy honest.
//!
//! Owns, for the project the route addresses:
//!
//! - **The mode fetch** — one anonymous-legal `GetProject` per open
//!   ([`visitor_mode::share_mode`] classifies the answer), refreshed by
//!   every pull (a pull carries the same meta + roster).
//! - **The pull loop** (Q11/CFS-D5) — on open, on window focus, and on a
//!   ~30 s timer. Pristine + behind fast-forwards **into the open editor**
//!   (apply to the mounted stores, then `ProjectOp::ReloadActiveProject`)
//!   with the ratified toast — but never over a dirty session overlay
//!   (D18: [`should_apply_fast_forward`]). The decide-half is the pure,
//!   host-tested logic in `visitor_banner`; this file is the IO glue, with
//!   the timer behind one local factory fn (the `make_pull_timer` idiom).
//! - **The refused-push consequence** — a view-visitor's save flips the
//!   banner to edited (the history genuinely diverged) and says the spike's
//!   line once, at save time; the engine's `Denied` latch (sync_queue)
//!   independently stops the retry churn, and a pull that observes an
//!   access/membership change lifts it (`sync_engine::clear_denied`).
//! - **Fork** — `LocalProject::fork_from` at the copy's head into a new
//!   library entry "<name> (fork)" (`InstallSyncedProject`, fork
//!   provenance → P4's engine auto-publishes it when signed in), then the
//!   ordinary open funnel navigates to the new address.
//! - **Discard** — re-adopt the service's line in place (the blessed
//!   re-`open_shared` alternative, run through the open stores): bank the
//!   working copy, replace the event log with the service's log verbatim,
//!   re-observe the frontier, check out the remote head, reload. Local
//!   divergence stays banked by hash — discard drops it from the line, not
//!   from the disk.
//!
//! Anonymous edit-visitors get their saves pushed from here (the P4 engine
//! deliberately no-ops signed out; an `Edit` link is the one anonymous
//! write the service accepts).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use dioxus::prelude::*;
use lpa_studio_core::app::studio::studio_view_channel::CommandSender;
use lpa_studio_core::{UiStudioView, has_unsaved_work};
use lpc_cloud_api::Access;
use lpc_history::{PrefixedUid, SyncRelation};

use crate::app::share::share_url::{ShareUrl, current_origin};
use crate::app::share::visitor_banner::{BannerState, VisitorBanner, VisitorBannerView};
use crate::app::share::visitor_mode::ShareMode;
use crate::app::share::visitor_popover::VisitorSharePopover;
use crate::base::Toasts;
use crate::cloud::CloudSession;
use crate::router::StudioRoute;

/// The spike's refused-push line, said once per save that stays local.
pub const REFUSED_PUSH_LINE: &str =
    "This is a shared project — your changes stay on this device. Fork to give them a home.";
/// CFS-D5's ratified auto-apply toast.
pub const UPDATED_LINE: &str = "Updated to the latest version";
/// The fork confirmation.
pub const FORKED_LINE: &str = "Forked — this one's yours.";
/// The discard confirmation.
pub const DISCARDED_LINE: &str = "Changes discarded — you're back on the shared version.";

/// How often the pull loop asks, between the open and focus triggers.
const PULL_INTERVAL_MS: u32 = 30_000;
/// The loop's wake-up granularity (focus-flag polling), not a pull rate.
const TICK_MS: u32 = 1_000;

/// What the coordinator currently knows about the open project's share
/// situation. `None` (in the surrounding signal) = no door and no banner.
#[derive(Clone, Debug, PartialEq)]
pub struct VisitorUx {
    pub mode: ShareMode,
    /// Display name (sidecar, slug fallback) — the banner's "<name>".
    pub name: String,
    /// The cloud slug, for the copied link.
    pub slug: String,
    pub access: Access,
    pub banner: BannerState,
}

/// The coordinator handle: signals plus the dispatch seam, cloneable into
/// any handler. Provided as context by [`use_visitor_session`].
#[derive(Clone)]
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(dead_code, reason = "the IO half that reads these is browser-only")
)]
pub struct VisitorSession {
    tx: CommandSender,
    view: Signal<UiStudioView>,
    toasts: Option<Toasts>,
    cloud: Option<Signal<CloudSession>>,
    /// The `/p/` uid the route currently addresses.
    uid: Signal<Option<PrefixedUid>>,
    /// The share situation, once the service has answered.
    ux: Signal<Option<VisitorUx>>,
    /// The last pull's service-side relation (the banner tie-breaker).
    relation: Signal<Option<SyncRelation>>,
    /// A fork or discard is running; their buttons go quiet.
    busy: Signal<bool>,
    /// One pull at a time.
    pulling: Rc<Cell<bool>>,
    /// Save-transition tracking (unsaved-work high-water mark).
    last_unsaved: Rc<Cell<bool>>,
    /// The uid last seen open, for open-landing detection.
    last_open: Rc<RefCell<Option<String>>>,
}

#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(dead_code, reason = "the IO half that calls these is browser-only")
)]
impl VisitorSession {
    /// The route's project uid, if any.
    pub fn uid(&self) -> Option<PrefixedUid> {
        (self.uid)()
    }

    /// The current share situation.
    pub fn ux(&self) -> Option<VisitorUx> {
        (self.ux)()
    }

    pub fn busy(&self) -> bool {
        (self.busy)()
    }

    /// The canonical link for the open project.
    pub fn share_url(&self) -> Option<ShareUrl> {
        let uid = self.uid()?;
        let ux = self.ux()?;
        Some(ShareUrl {
            origin: current_origin(),
            slug: lpc_cloud_api::share_link::slugify(&ux.name),
            uid,
        })
    }

    /// Copy the link and say so, phrased by what the link grants.
    pub fn copy_link(&self) {
        let Some(url) = self.share_url() else {
            return;
        };
        crate::clipboard::write_text(&url.absolute());
        let access = self.ux().map(|ux| ux.access);
        let mut this = self.clone();
        if let Some(toasts) = this.toasts.as_mut() {
            match access {
                Some(Access::Edit) => {
                    toasts.say("Link copied — anyone holding it can edit and save.")
                }
                _ => toasts.say("Link copied — opens running for anyone (no account)."),
            }
        }
    }

    /// Fork the tracking copy into a new project of the visitor's own.
    ///
    /// The fork is taken at the copy's last SAVE — a dirty session overlay
    /// is runtime-only and cannot ride along, so it gets the same confirm
    /// the open-another-project gate uses (the fork's open replaces the
    /// loaded project, and this dispatch bypasses `on_action`'s gate).
    pub fn fork(&self) {
        if self.overlay_dirty()
            && !crate::unsaved_gate::confirm_discarding_unsaved(
                "This project has unsaved changes — they won't be in the fork.\n\nSave first to keep them. Fork without them?",
            )
        {
            return;
        }
        let session = self.clone();
        spawn(async move { io::fork_flow(session).await });
    }

    /// Reset the tracking copy to the service's line. Unsaved overlay edits
    /// are part of what "discard" discards — but never silently.
    pub fn discard(&self) {
        if self.overlay_dirty()
            && !crate::unsaved_gate::confirm_discarding_unsaved(
                "Discard also drops the edits you haven't saved.\n\nDiscard everything and return to the shared version?",
            )
        {
            return;
        }
        let session = self.clone();
        spawn(async move { io::discard_flow(session).await });
    }

    /// Whether this browser session is anonymous (no account).
    fn anonymous(&self) -> bool {
        self.cloud
            .map(|session| session.read().me().is_none())
            .unwrap_or(true)
    }

    fn say(&self, line: impl Into<String>) {
        let mut this = self.clone();
        if let Some(toasts) = this.toasts.as_mut() {
            toasts.say(line);
        }
    }

    /// Whether the open editor currently shows this uid.
    fn is_open(&self, uid: PrefixedUid) -> bool {
        self.view
            .peek()
            .open_project_uid
            .as_deref()
            .is_some_and(|open| open == uid.to_string())
    }

    /// The session overlay's dirty flag, from the last emitted view.
    fn overlay_dirty(&self) -> bool {
        has_unsaved_work(&self.view.peek().dirty)
    }
}

/// Build the tab's one visitor coordinator and provide it as context.
/// Call once, in `App`, after the toast and cloud-session providers.
pub(crate) fn use_visitor_session(
    tx: CommandSender,
    view: Signal<UiStudioView>,
    route: Signal<StudioRoute>,
) -> VisitorSession {
    let toasts = try_consume_context::<Toasts>();
    let cloud = try_consume_context::<Signal<CloudSession>>();
    let uid = use_signal(|| None::<PrefixedUid>);
    let ux = use_signal(|| None::<VisitorUx>);
    let relation = use_signal(|| None::<SyncRelation>);
    let busy = use_signal(|| false);
    let focus_requested = use_hook(|| Rc::new(Cell::new(false)));
    let session = use_hook(|| VisitorSession {
        tx,
        view,
        toasts,
        cloud,
        uid,
        ux,
        relation,
        busy,
        pulling: Rc::new(Cell::new(false)),
        last_unsaved: Rc::new(Cell::new(false)),
        last_open: Rc::new(RefCell::new(None)),
    });
    use_context_provider(|| session.clone());

    // Route → uid: a new project resets everything and starts the mode
    // fetch; leaving the project routes clears the surface.
    let route_session = session.clone();
    use_effect(move || {
        let current = match route() {
            StudioRoute::Project { uid, .. } => Some(uid),
            _ => None,
        };
        let mut session = route_session.clone();
        if session.uid.peek().as_ref() == current.as_ref() {
            return;
        }
        session.uid.set(current);
        session.ux.set(None);
        session.relation.set(None);
        session.busy.set(false);
        if let Some(uid) = current {
            let session = session.clone();
            spawn(async move {
                io::fetch_mode(session.clone(), uid).await;
                io::pull_once(session, PullTrigger::Open).await;
            });
        }
    });

    // View → save transitions and open landings. A save is the moment the
    // history can diverge (record_save), so it is the banner's local
    // trigger — and the view-visitor's refused-push moment.
    let view_session = session.clone();
    use_effect(move || {
        let (open_uid, unsaved) = {
            let view = view.read();
            (view.open_project_uid.clone(), has_unsaved_work(&view.dirty))
        };
        let session = view_session.clone();
        let Some(uid) = session.uid.peek().as_ref().copied() else {
            session.last_unsaved.set(unsaved);
            *session.last_open.borrow_mut() = open_uid;
            return;
        };
        let matches = open_uid.as_deref() == Some(uid.to_string().as_str());

        let newly_open = matches && *session.last_open.borrow() != open_uid;
        let save_landed = matches && session.last_unsaved.get() && !unsaved;
        session.last_unsaved.set(unsaved);
        *session.last_open.borrow_mut() = open_uid;

        if newly_open {
            // The "pull on open" half of the loop: the route effect's pull
            // usually ran before the editor finished opening (its is-open
            // gate skipped it), so the landing is the real first pull. The
            // recompute first paints the banner from local state even when
            // the network has nothing to say.
            let session = session.clone();
            spawn(async move {
                io::recompute_banner(session.clone(), uid, false).await;
                io::pull_once(session, PullTrigger::Open).await;
            });
        }
        if save_landed {
            let announce = session
                .ux
                .peek()
                .as_ref()
                .is_some_and(|ux| ux.mode == ShareMode::ViewVisitor);
            let push_anon = session
                .ux
                .peek()
                .as_ref()
                .is_some_and(|ux| ux.mode == ShareMode::EditVisitor)
                && session.anonymous();
            let session = session.clone();
            spawn(async move {
                if push_anon {
                    io::push_saved_work(session.clone(), uid).await;
                }
                io::recompute_banner(session, uid, announce).await;
            });
        }
    });

    // The ~30 s timer and the focus trigger, one loop. The raw `focus`
    // listener only flips an `Rc<Cell<bool>>` (house rule: platform
    // listeners never touch Dioxus state directly — see `unsaved_gate`);
    // this loop, which lives inside the runtime, is what acts on it. The
    // 1 s tick is the flag-poll granularity, not a pull cadence.
    let timer_session = session.clone();
    let timer_focus = Rc::clone(&focus_requested);
    use_future(move || {
        let session = timer_session.clone();
        let focus_requested = Rc::clone(&timer_focus);
        async move {
            let mut elapsed_ms: u32 = 0;
            loop {
                make_pull_timer(TICK_MS).await;
                elapsed_ms += TICK_MS;
                let focused = focus_requested.replace(false);
                if focused || elapsed_ms >= PULL_INTERVAL_MS {
                    elapsed_ms = 0;
                    let trigger = if focused {
                        PullTrigger::Focus
                    } else {
                        PullTrigger::Timer
                    };
                    io::pull_once(session.clone(), trigger).await;
                }
            }
        }
    });

    // Window focus: the user came back; ask now rather than in ≤30 s.
    use_hook(move || install_focus_listener(focus_requested));

    session
}

/// Why a pull ran (logging only — the behavior is identical).
#[derive(Clone, Copy, Debug)]
pub enum PullTrigger {
    Open,
    Focus,
    Timer,
}

/// The pull loop's timer, as a factory fn so the loop body stays one
/// swap away from a sans-IO test harness (the `make_pull_timer` idiom —
/// the decide-half is already pure in `visitor_banner`).
fn make_pull_timer(delay_ms: u32) -> gloo_timers::future::TimeoutFuture {
    gloo_timers::future::TimeoutFuture::new(delay_ms)
}

#[cfg(target_arch = "wasm32")]
fn install_focus_listener(flag: Rc<Cell<bool>>) {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::Closure;
    let Some(window) = web_sys::window() else {
        return;
    };
    let on_focus = Closure::wrap(Box::new(move |_event: web_sys::Event| {
        flag.set(true);
    }) as Box<dyn FnMut(_)>);
    if let Err(e) =
        window.add_event_listener_with_callback("focus", on_focus.as_ref().unchecked_ref())
    {
        log::warn!("visitor pull: focus listener failed: {e:?}");
    }
    on_focus.forget();
}

#[cfg(not(target_arch = "wasm32"))]
fn install_focus_listener(_flag: Rc<Cell<bool>>) {}

/// The chrome's pill slot, visitor variant: renders the §2-D door when the
/// service said this viewer is a link-holder, and nothing otherwise (the
/// member pill is `ProjectShareControl`'s — each self-gates, so exactly
/// one door ever draws).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn VisitorShareSlot() -> Element {
    let Some(session) = try_consume_context::<VisitorSession>() else {
        return rsx! {};
    };
    let Some(ux) = session.ux() else {
        return rsx! {};
    };
    if !ux.mode.is_visitor() {
        return rsx! {};
    }
    let Some(url) = session.share_url() else {
        return rsx! {};
    };
    let copy_session = session.clone();
    let fork_session = session.clone();
    rsx! {
        VisitorSharePopover {
            name: ux.name.clone(),
            url,
            access: ux.access,
            on_copy: move |()| copy_session.copy_link(),
            on_fork: move |()| fork_session.fork(),
        }
    }
}

/// The strip under the chrome: mounted on project routes, draws only for
/// a visitor whose project is actually open in the editor.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn VisitorBannerHost() -> Element {
    let Some(session) = try_consume_context::<VisitorSession>() else {
        return rsx! {};
    };
    let Some(uid) = session.uid() else {
        return rsx! {};
    };
    let Some(ux) = session.ux() else {
        return rsx! {};
    };
    // The banner narrates the OPEN copy; while the open is still landing
    // there is nothing honest to say yet. Read reactively — the strip must
    // appear the moment the open lands.
    let open_matches = session
        .view
        .read()
        .open_project_uid
        .as_deref()
        .is_some_and(|open| open == uid.to_string());
    if !open_matches {
        return rsx! {};
    }
    let view = match (ux.mode, ux.banner) {
        (ShareMode::ViewVisitor, BannerState::Pristine) => VisitorBannerView::ViewPristine {
            name: ux.name.clone(),
        },
        (ShareMode::ViewVisitor, BannerState::Edited) => VisitorBannerView::ViewEdited,
        (ShareMode::EditVisitor, _) => VisitorBannerView::EditLive {
            name: ux.name.clone(),
        },
        (ShareMode::Member, _) => return rsx! {},
    };
    let copy_session = session.clone();
    let fork_session = session.clone();
    let discard_session = session.clone();
    rsx! {
        VisitorBanner {
            view,
            on_copy_link: move |()| copy_session.copy_link(),
            on_fork: move |()| fork_session.fork(),
            on_discard: move |()| discard_session.discard(),
        }
    }
}

/// The IO half: OPFS mounts, the cloud port, and the studio dispatch.
#[cfg(target_arch = "wasm32")]
mod io {
    #[allow(
        unused_imports,
        reason = "the signal read/write extension traits ride the prelude"
    )]
    use dioxus::prelude::*;
    use lpa_cloud_client::sync::apply_fast_forward;
    use lpa_cloud_client::{LocalProject, SyncError, call, pull, push};
    use lpa_studio_core::app::library::{CatalogOp, PackageProvenance};
    use lpa_studio_core::{ProjectController, ProjectOp, StudioCommand, UiAction};
    use lpc_cloud_api::CloudError;
    use lpc_cloud_api::request::GetProject;
    use lpc_cloud_api::response::ProjectInfo;
    use lpc_history::event::event_log::EVENT_LOG_PATH;
    use lpc_history::{EventLog, PrefixedUid, SyncRelation, UidPrefix};
    use lpfs::{LpFs, LpFsMemory, LpPath};

    use super::{PullTrigger, VisitorSession, VisitorUx};
    use crate::app::share::visitor_banner::{BannerState, banner_state, should_apply_fast_forward};
    use crate::app::share::visitor_mode::share_mode;
    use crate::cloud::FetchCloudPort;
    use crate::cloud::shared_open::all_files;
    use crate::cloud::sync::sync_engine;
    use crate::library_host_opfs::SyncMount;

    /// One `GetProject`: the mode, the name, and the access level.
    pub(super) async fn fetch_mode(session: VisitorSession, uid: PrefixedUid) {
        let mut session = session;
        match call(&FetchCloudPort::new(), GetProject { uid }).await {
            Ok(info) => apply_info(&mut session, uid, &info),
            Err(error) => {
                // No door and no banner: unpublished, restricted, archived,
                // or unreachable — the surface stays quiet.
                log::debug!("visitor session: no share surface at {uid}: {error}");
                if session.uid.peek().as_ref() == Some(&uid) {
                    session.ux.set(None);
                }
            }
        }
    }

    /// Fold a `GetProject`-shaped answer into the ux signal.
    fn apply_info(session: &mut VisitorSession, uid: PrefixedUid, info: &ProjectInfo) {
        if session.uid.peek().as_ref() != Some(&uid) {
            return; // the route moved on mid-flight
        }
        let Some(mode) = share_mode(info) else {
            session.ux.set(None);
            return;
        };
        let name = if info.sidecar.name.trim().is_empty() {
            info.meta.slug.clone()
        } else {
            info.sidecar.name.clone()
        };
        let banner = session
            .ux
            .peek()
            .as_ref()
            .map(|ux| ux.banner)
            .unwrap_or(BannerState::Pristine);
        session.ux.set(Some(VisitorUx {
            mode,
            name,
            slug: info.meta.slug.clone(),
            access: info.meta.access,
            banner,
        }));
        // Something that could change a denied push's answer? Lift the
        // engine's latch so the next save is offered again.
        if mode.can_write() {
            sync_engine::clear_denied(&uid.to_string());
        }
    }

    /// Borrow (or briefly lock) the project's stores for one operation.
    async fn mount(uid: PrefixedUid) -> Option<SyncMount> {
        let host = crate::local_store::opfs_library_host()?;
        match host.mount_for_sync(&uid.to_string(), None).await {
            Ok(Some(mount)) => Some(mount),
            Ok(None) => None, // another tab holds it; its loop owns this
            Err(error) => {
                log::debug!("visitor session: cannot mount {uid}: {error}");
                None
            }
        }
    }

    /// One pull: fetch, re-classify, apply when the D18 gate allows.
    pub(super) async fn pull_once(session: VisitorSession, trigger: PullTrigger) {
        let mut session = session;
        let Some(uid) = *session.uid.peek() else {
            return;
        };
        // Only visitor copies loop — a member's own copy is the push
        // engine's business, and mixing writers would invite races.
        if !session
            .ux
            .peek()
            .as_ref()
            .is_some_and(|ux| ux.mode.is_visitor())
        {
            return;
        }
        if session.pulling.replace(true) {
            return;
        }
        let result = pull_body(&mut session, uid, trigger).await;
        session.pulling.set(false);
        if let Err(error) = result {
            match error {
                SyncError::Cloud(CloudError::NotFound) => {
                    // Revoked or archived mid-visit: the copy stays usable
                    // and simply stops hearing anything (Q12).
                    log::debug!("visitor pull: {uid} no longer answers");
                }
                other => log::debug!("visitor pull ({trigger:?}): {uid}: {other}"),
            }
        }
    }

    async fn pull_body(
        session: &mut VisitorSession,
        uid: PrefixedUid,
        trigger: PullTrigger,
    ) -> Result<(), SyncError> {
        // Pull only what the editor is showing: the loop narrates and
        // updates the OPEN copy (borrowed stores, single writer).
        if !session.is_open(uid) {
            return Ok(());
        }
        let Some(mount) = mount(uid).await else {
            return Ok(());
        };
        let project = LocalProject::new(uid, mount.package(), mount.history());
        let report = match pull(&FetchCloudPort::new(), &project).await {
            Ok(report) => report,
            Err(error) => {
                mount.release().await;
                return Err(error);
            }
        };
        log::debug!(
            "visitor pull ({trigger:?}): {uid} relation {:?}",
            report.relation
        );
        session.relation.set(Some(report.relation));

        // The same answer refreshes the mode (meta + roster ride along).
        let info = ProjectInfo {
            meta: report.meta.clone(),
            heads: report.heads.clone(),
            sidecar: report.sidecar.clone(),
            members: report.members.clone(),
        };
        apply_info(session, uid, &info);

        // D18: apply a fast-forward only over a clean session overlay.
        if should_apply_fast_forward(report.can_fast_forward(), session.overlay_dirty()) {
            match apply_fast_forward(&project, &report) {
                Ok(applied) if applied.applied_events > 0 => {
                    session.relation.set(Some(SyncRelation::AtHead));
                    session.tx.send(StudioCommand::Action(UiAction::from_op(
                        ProjectController::NODE_ID,
                        ProjectOp::ReloadActiveProject,
                    )));
                    session.say(super::UPDATED_LINE);
                }
                Ok(_) => {}
                Err(error) => {
                    // A save racing the pull turns the fast-forward stale;
                    // banked content is untouched and the next tick re-asks.
                    log::debug!("visitor pull: apply on {uid} deferred: {error}");
                }
            }
        }
        mount.release().await;
        recompute_banner(session.clone(), uid, false).await;
        Ok(())
    }

    /// Re-classify the strip from the stores (and optionally announce the
    /// refused-push line — the save-landed, view-visitor moment).
    pub(super) async fn recompute_banner(
        session: VisitorSession,
        uid: PrefixedUid,
        announce: bool,
    ) {
        let mut session = session;
        if session.uid.peek().as_ref() != Some(&uid) {
            return;
        }
        let Some(mount) = mount(uid).await else {
            return;
        };
        let project = LocalProject::new(uid, mount.package(), mount.history());
        let state = classify_stores(&project, *session.relation.peek());
        mount.release().await;
        let Some(state) = state else {
            return;
        };
        let previous = session.ux.peek().as_ref().map(|ux| ux.banner);
        if let Some(ux) = session.ux.write().as_mut() {
            ux.banner = state;
        }
        if announce && state == BannerState::Edited && previous != Some(BannerState::Edited) {
            session.say(super::REFUSED_PUSH_LINE);
        }
    }

    /// The banner classification over mounted stores — the pure
    /// `banner_state` fed by the binding and the replayed history.
    fn classify_stores(
        project: &LocalProject<'_>,
        relation: Option<SyncRelation>,
    ) -> Option<BannerState> {
        let binding = project.binding().ok().flatten()?;
        let history = project.history().ok()?;
        Some(banner_state(&binding.last_seen_heads, &history, relation))
    }

    /// An anonymous edit-visitor's save, pushed from here: the P4 engine
    /// no-ops signed out, and an `Edit` link is the one anonymous write
    /// the service accepts.
    pub(super) async fn push_saved_work(session: VisitorSession, uid: PrefixedUid) {
        let mut session = session;
        let Some(mount) = mount(uid).await else {
            return;
        };
        let fallback = session
            .ux
            .peek()
            .as_ref()
            .map(|ux| ux.name.clone())
            .unwrap_or_else(|| uid.to_string());
        let identity =
            match crate::cloud::sync::sidecar_producer::read_identity(mount.package(), &fallback) {
                Ok(identity) => identity,
                Err(error) => {
                    log::warn!("visitor push: {uid} identity: {error}");
                    mount.release().await;
                    return;
                }
            };
        let project = LocalProject::new(uid, mount.package(), mount.history());
        let result = push(&FetchCloudPort::new(), &project, &identity.sidecar()).await;
        mount.release().await;
        match result {
            Ok(_) => {
                session.relation.set(Some(SyncRelation::AtHead));
                recompute_banner(session, uid, false).await;
            }
            Err(SyncError::Cloud(CloudError::NotAuthorized | CloudError::NotAuthenticated)) => {
                // The link stopped granting writes mid-session: the same
                // honest consequence as the view-visitor's save.
                recompute_banner(session, uid, true).await;
            }
            Err(error) => log::debug!("visitor push: {uid}: {error}"),
        }
    }

    /// Fork the tracking copy at its head into a new project.
    pub(super) async fn fork_flow(session: VisitorSession) {
        let mut session = session;
        let Some(uid) = *session.uid.peek() else {
            return;
        };
        let Some(ux) = session.ux.peek().as_ref().cloned() else {
            return;
        };
        if *session.busy.peek() {
            return;
        }
        session.busy.set(true);
        let result = fork_body(&session, uid, &ux).await;
        session.busy.set(false);
        match result {
            Ok(new_uid) => {
                session.say(super::FORKED_LINE);
                session.tx.send(StudioCommand::Action(UiAction::from_op(
                    lpa_studio_core::HOME_NODE_ID,
                    lpa_studio_core::HomeOp::OpenPackage {
                        key: new_uid.to_string(),
                    },
                )));
            }
            Err(message) => {
                log::warn!("fork of {uid} failed: {message}");
                let mut toasts = session.toasts;
                if let Some(toasts) = toasts.as_mut() {
                    toasts.warn("Could not fork — nothing was created.");
                }
            }
        }
    }

    async fn fork_body(
        _session: &VisitorSession,
        uid: PrefixedUid,
        ux: &VisitorUx,
    ) -> Result<PrefixedUid, String> {
        let host = crate::local_store::library_host().ok_or("no library host")?;
        let mount = mount(uid).await.ok_or("project unavailable")?;
        let parent = LocalProject::new(uid, mount.package(), mount.history());
        let head = parent
            .head()
            .map_err(|e| e.to_string())?
            .ok_or("the copy has no saved version to fork")?;

        let new_uid = PrefixedUid::mint(
            UidPrefix::Project,
            &crate::library_host_opfs::random_bytes(),
        );
        let fork_name = format!("{} (fork)", ux.name);
        let package = LpFsMemory::new();
        let history = LpFsMemory::new();
        let fork = LocalProject::fork_from(
            &parent,
            head,
            new_uid,
            &package,
            &history,
            crate::web_app::now_secs(),
        )
        .map_err(|e| e.to_string())?;
        mount.release().await;

        // The forked content carries the PARENT's manifest; the fork gets
        // its own identity — uid and "<name> (fork)" — and records that as
        // its own first save on top of the fork origin.
        patch_manifest(&package, new_uid, &fork_name)?;
        fork.save(crate::web_app::now_secs())
            .map_err(|e| e.to_string())?;

        let package_files = all_files(&package).map_err(|e| e.to_string())?;
        let history_files = all_files(&history).map_err(|e| e.to_string())?;
        let outcome = host
            .catalog(CatalogOp::InstallSyncedProject {
                name: fork_name,
                package_files,
                history_files,
                provenance: PackageProvenance::ForkedFrom {
                    parent_project: uid.to_string(),
                    parent_version: head.to_string(),
                },
            })
            .await
            .map_err(|e| e.to_string())?;
        outcome
            .summary
            .map(|summary| summary.uid)
            .ok_or_else(|| "install produced no package".to_string())
    }

    /// Give the fork its own manifest identity.
    fn patch_manifest(package: &dyn LpFs, uid: PrefixedUid, name: &str) -> Result<(), String> {
        let path = LpPath::new("/project.json");
        let bytes = package.read_file(path).map_err(|e| e.to_string())?;
        let text = core::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
        let mut manifest =
            lpc_model::ProjectManifest::read_json(text).map_err(|e| e.to_string())?;
        manifest.uid = Some(uid.to_string());
        manifest.name = Some(name.to_string());
        package
            .write_file(path, manifest.write_json().as_bytes())
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Discard local divergence: re-adopt the service's line in place.
    pub(super) async fn discard_flow(session: VisitorSession) {
        let mut session = session;
        let Some(uid) = *session.uid.peek() else {
            return;
        };
        if *session.busy.peek() {
            return;
        }
        session.busy.set(true);
        let result = discard_body(&mut session, uid).await;
        session.busy.set(false);
        match result {
            Ok(()) => {
                session.relation.set(Some(SyncRelation::AtHead));
                session.tx.send(StudioCommand::Action(UiAction::from_op(
                    ProjectController::NODE_ID,
                    ProjectOp::ReloadActiveProject,
                )));
                session.say(super::DISCARDED_LINE);
                recompute_banner(session, uid, false).await;
            }
            Err(message) => {
                log::warn!("discard on {uid} failed: {message}");
                let mut toasts = session.toasts;
                if let Some(toasts) = toasts.as_mut() {
                    toasts.warn("Could not reach the shared version — nothing was changed.");
                }
            }
        }
    }

    /// Bank, replace the log with the service's log, re-observe, check out.
    ///
    /// This is the blessed re-`open_shared` shape run through the open
    /// stores: `lpc-history` has no reset-to-head verb (the log is
    /// append-only by design), so the honest discard adopts the service's
    /// event log verbatim — exactly what a fresh tracking copy would hold —
    /// while the banked local versions stay reachable by hash.
    async fn discard_body(_session: &mut VisitorSession, uid: PrefixedUid) -> Result<(), String> {
        let mount = mount(uid).await.ok_or("project unavailable")?;
        let project = LocalProject::new(uid, mount.package(), mount.history());
        let result = async {
            let report = pull(&FetchCloudPort::new(), &project)
                .await
                .map_err(|e| e.to_string())?;
            let remote_head = report.remote_head.ok_or("the service holds no version")?;
            // Nothing is ever lost: the working copy is banked by hash
            // before the line is rewritten.
            project.bank_working_copy().map_err(|e| e.to_string())?;
            let history_fs = project.history_fs();
            match history_fs.delete_file(LpPath::new(EVENT_LOG_PATH)) {
                Ok(()) => {}
                Err(lpfs::FsError::NotFound(_)) => {}
                Err(e) => return Err(e.to_string()),
            }
            let log = EventLog::new(history_fs);
            for event in &report.remote_events {
                log.append(event).map_err(|e| e.to_string())?;
            }
            let mut binding = project
                .binding()
                .map_err(|e| e.to_string())?
                .unwrap_or_else(|| lpa_cloud_client::CloudBinding::new(uid));
            binding.observe_heads(&report.heads);
            binding.last_event_seq = report.next_since;
            project.put_binding(&binding).map_err(|e| e.to_string())?;
            project.checkout(remote_head).map_err(|e| e.to_string())?;
            Ok(())
        }
        .await;
        mount.release().await;
        result
    }
}

/// Host builds compile the components and the pure logic; the IO half is
/// browser-only (OPFS, fetch, dispatch), so these stubs keep the hook
/// callable in host unit tests without dragging the platform in.
#[cfg(not(target_arch = "wasm32"))]
mod io {
    use lpc_history::PrefixedUid;

    use super::{PullTrigger, VisitorSession};

    pub(super) async fn fetch_mode(_session: VisitorSession, _uid: PrefixedUid) {}
    pub(super) async fn pull_once(_session: VisitorSession, _trigger: PullTrigger) {}
    pub(super) async fn recompute_banner(
        _session: VisitorSession,
        _uid: PrefixedUid,
        _announce: bool,
    ) {
    }
    pub(super) async fn push_saved_work(_session: VisitorSession, _uid: PrefixedUid) {}
    pub(super) async fn fork_flow(_session: VisitorSession) {}
    pub(super) async fn discard_flow(_session: VisitorSession) {}
}
