//! **The update driver**: one state machine that runs a whole update, or a
//! heal, over one board's links — the piece Studio (M7) and `lp-cli` (Part
//! B) drive.
//!
//! ```text
//! link up → Q → M → decide
//!   ├─ Heal            → engine source → offer → serve until the board resets and comes back
//!   ├─ ContinueUpdate  → offer the host's build → serve
//!   ├─ OfferUpdate (once the caller says go) →
//!   │     backup (engine source for the CURRENT engine, read-back last) →
//!   │     offer → the board resets into core-only → (new link) Q → M updating →
//!   │     log in on N/A → serve the core → the board resets on trial →
//!   │     (new link) M on-trial → offer again: now an engine install → serve →
//!   │     reset → M running
//!   └─ anything else   → report and stop
//! ```
//!
//! **It resumes from the manifest, never from its own memory** (DM9): every
//! reset is a link down and a new link up, and every new link starts with
//! `Q`. What it keeps across links is only what makes it cheaper — a
//! backup already held, a heal's engine already found — never what decides.
//!
//! Inputs: link up/down, board messages (with the credentials the caller
//! holds, passed each time: nothing here stores them), engine-source
//! results, `go`, and `tick(now_ms)` for a login backoff. Outputs are
//! [`DriverEffect`]s: messages to send, engine-source effects, progress by
//! stage, the decision, and the end. Stages, not copy (DM31).

use alloc::vec::Vec;

use lpc_access::Tier;
use lpc_update::{BoardManifest, BoardMessage, BoardState, PieceKind, encode_query};

use crate::backup::{BackupSession, BackupStep};
use crate::board_view::BoardView;
use crate::decide::decision::{Decision, HostFacts, decide};
use crate::decide::engine_source::{EngineSource, SourceEffect, SourceResult, SourceStep};
use crate::host_build::HostBuild;
use crate::host_refusal::HostRefusal;
use crate::login::{Credential, LoginClient, LoginEvent};
use crate::serve::{ServeConfig, ServeCounters, ServeEvent, ServeSession};

/// What the driver is doing, in the roadmap's words (the caller words them).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Reading the board's current engine back.
    BackingUp,
    /// Moving the new core.
    Updating,
    /// Putting the board's own engine back (a heal).
    Restoring,
    /// The new core fetching its engine.
    Finishing,
}

/// Why the driver stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StopReason {
    /// A decision with nothing for this host to do (needs USB, newer,
    /// crashing, busy, another target, play only, a refused build).
    Decision(Decision),
    /// The board refused, and retrying would not change it.
    Refused(HostRefusal),
    /// The board is older than a message this host sent.
    BoardLacksMessage(u8),
    /// The board asked for a login and the caller holds no credentials.
    NoCredentials,
    /// The running engine refused: log in to its server (channel 1) first,
    /// then drive again.
    NeedsEngineLogin,
    /// Every credential was refused.
    LoginRefused,
    /// No source had the engine (E13); `offline` when the store was not
    /// reachable.
    MissingEngine { offline: bool },
    /// The read-back did not hash to the engine the board reports.
    BackupFailed,
    /// A piece failed its hash more times than allowed.
    TooManyRetries,
}

/// How the driver ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Finish {
    /// The board runs the host's build (or already did).
    UpToDate,
    Stopped(StopReason),
}

/// What the caller does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DriverEffect {
    /// Send on the board's link.
    Send(Vec<u8>),
    /// The board wants a login and no credentials were passed: call
    /// [`UpdateDriver::login_with`] when they are at hand.
    NeedCredentials,
    /// Perform this engine-source effect (`LookUpCache`, `FetchFromStore`,
    /// `KeepInCache`) and, for the first two, pass the result to
    /// [`UpdateDriver::source_result`].
    Source(SourceEffect),
    Progress {
        stage: Stage,
        done: u32,
        total: u32,
    },
    /// What the driver decided on the board's latest manifest.
    Decided(Decision),
    Done(Finish),
}

/// The driver's settings.
#[derive(Clone, Copy, Debug)]
pub struct DriverConfig {
    pub serve: ServeConfig,
    pub user_tier: Option<Tier>,
    pub allow_downgrade: bool,
    /// Hash mismatches (`N`/`H`) tolerated before giving up.
    pub max_piece_retries: u8,
}

impl Default for DriverConfig {
    fn default() -> Self {
        Self {
            serve: ServeConfig::USB,
            user_tier: None,
            allow_downgrade: false,
            max_piece_retries: 3,
        }
    }
}

/// Which build is on offer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Serving {
    /// The host's own build.
    Own,
    /// A heal's build, found by the engine source.
    Heal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Purpose {
    Backup,
    Heal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// No link.
    Down,
    /// `Q` sent; waiting for `M`.
    Asked,
    /// `OfferUpdate` decided; waiting for `go`.
    WaitingGo,
    /// An engine-source effect is out.
    Sourcing,
    /// Reading back.
    BackingUp,
    /// An offer is out; serving.
    Offered(Serving),
    /// A login is out (then the offer goes again).
    LoggingIn(Serving),
    /// Waiting for credentials (then the login).
    NeedsCredentials(Serving),
    Done,
}

/// The update driver. See the module docs.
pub struct UpdateDriver {
    build: HostBuild,
    config: DriverConfig,
    phase: Phase,
    board: BoardView,
    go: bool,
    serve: Option<ServeSession>,
    totals: ServeCounters,
    heal_build: Option<HostBuild>,
    backup_held: bool,
    backup: Option<BackupSession>,
    source: Option<(EngineSource, Purpose)>,
    login: LoginClient,
    login_retry_at: Option<u64>,
    retries: u8,
    /// This host served core chunks (only names the engine's stage).
    moved_core: bool,
    effects: Vec<DriverEffect>,
}

impl UpdateDriver {
    /// A driver that would put `build` on the board.
    #[must_use]
    pub fn new(build: HostBuild, config: DriverConfig) -> Self {
        Self {
            build,
            config,
            phase: Phase::Down,
            board: BoardView::absent(),
            go: false,
            serve: None,
            totals: ServeCounters::default(),
            heal_build: None,
            backup_held: false,
            backup: None,
            source: None,
            login: LoginClient::new(),
            login_retry_at: None,
            retries: 0,
            moved_core: false,
            effects: Vec::new(),
        }
    }

    /// Effects to perform, in order.
    pub fn take_effects(&mut self) -> Vec<DriverEffect> {
        core::mem::take(&mut self.effects)
    }

    /// Whether the driver has finished.
    #[must_use]
    pub fn done(&self) -> bool {
        self.phase == Phase::Done
    }

    /// The board's latest manifest.
    #[must_use]
    pub fn board(&self) -> &BoardView {
        &self.board
    }

    /// Bytes served so far, across links.
    #[must_use]
    pub fn served(&self) -> ServeCounters {
        let mut t = self.totals;
        if let Some(s) = &self.serve {
            add(&mut t, s.counters());
        }
        t
    }

    /// The user said go: an offered update may proceed.
    pub fn go(&mut self) {
        self.go = true;
        if self.phase == Phase::WaitingGo {
            self.act();
        }
    }

    /// A link to the board came up: ask who it is.
    pub fn link_up(&mut self, _now_ms: u64) {
        if self.phase == Phase::Done {
            return;
        }
        self.drop_serve();
        self.login = LoginClient::new();
        self.phase = Phase::Asked;
        self.send(encode_query(lpc_update::PROTO_V1));
    }

    /// The link went down (or the board reset).
    pub fn link_down(&mut self, _now_ms: u64) {
        if self.phase == Phase::Done {
            return;
        }
        self.drop_serve();
        self.phase = Phase::Down;
    }

    /// Time passed: a login backoff may be over.
    pub fn tick(&mut self, now_ms: u64) {
        if let (Some(at), Phase::LoggingIn(_)) = (self.login_retry_at, self.phase)
            && now_ms >= at
        {
            self.login_retry_at = None;
            let begin = self.login.begin();
            self.send(begin);
        }
    }

    /// The caller has credentials now: log in.
    pub fn login_with(&mut self, credentials: &[Credential]) {
        if let Phase::NeedsCredentials(which) = self.phase {
            if credentials.is_empty() {
                return self.finish(Finish::Stopped(StopReason::NoCredentials));
            }
            self.phase = Phase::LoggingIn(which);
            let begin = self.login.begin();
            self.send(begin);
        }
    }

    /// The result of a `LookUpCache` or `FetchFromStore`.
    pub fn source_result(&mut self, result: SourceResult) {
        let Some((source, _)) = &mut self.source else {
            return;
        };
        let step = source.on_result(result);
        self.on_source_step(step);
    }

    /// One board message. `credentials` are what the caller holds, for a
    /// login challenge.
    pub fn on_board(&mut self, now_ms: u64, bytes: &[u8], credentials: &[Credential]) {
        if self.phase == Phase::Done {
            return;
        }
        match BoardMessage::decode(bytes) {
            Ok(BoardMessage::Manifest(json)) => {
                if let Ok(m) = BoardManifest::from_json(json) {
                    self.on_manifest(m);
                }
            }
            Ok(BoardMessage::Login(_)) => self.on_login(now_ms, bytes, credentials),
            Ok(BoardMessage::Data(_)) => self.on_read_back(bytes),
            Ok(BoardMessage::Request(r)) => {
                let Phase::Offered(which) = self.phase else {
                    return;
                };
                self.moved_core |= r.kind == PieceKind::Core;
                let stage = self.stage_of(which, r.kind);
                let total = self.served_build(which).piece(r.kind).len;
                self.effects.push(DriverEffect::Progress {
                    stage,
                    done: r.off,
                    total,
                });
                self.serve_message(which, bytes);
            }
            Ok(BoardMessage::Refusal(_)) => {
                if let Phase::Offered(which) | Phase::LoggingIn(which) = self.phase {
                    self.serve_message(which, bytes);
                }
            }
            // Unknown board messages are ignored: hosts are the newer side.
            _ => {}
        }
    }

    // ---- Deciding --------------------------------------------------------------

    fn on_manifest(&mut self, m: BoardManifest) {
        self.board = BoardView::from_manifest(m);
        if matches!(self.phase, Phase::BackingUp | Phase::Sourcing) {
            return; // the manifest only refreshes the view mid-source
        }
        self.act();
    }

    /// Decide on the latest manifest and do it.
    fn act(&mut self) {
        let facts = HostFacts {
            build: &self.build,
            user_tier: self.config.user_tier,
            allow_downgrade: self.config.allow_downgrade,
        };
        let decision = decide(&self.board, &facts);
        self.effects.push(DriverEffect::Decided(decision.clone()));
        match decision {
            Decision::Nothing => self.finish(Finish::UpToDate),
            Decision::Heal { engine_sha, .. } => {
                if self.board.core_sha256() == Some(self.build.core.sha256) {
                    self.offer(Serving::Own);
                } else if self.heal_build.as_ref().is_some_and(|b| {
                    b.engine.sha256 == engine_sha && Some(b.core.sha256) == self.board.core_sha256()
                }) {
                    self.offer(Serving::Heal);
                } else {
                    self.start_source(Purpose::Heal);
                }
            }
            Decision::ContinueUpdate { .. } => self.offer(Serving::Own),
            Decision::OfferUpdate { .. } => {
                if !self.go {
                    self.phase = Phase::WaitingGo;
                } else if self.backup_held {
                    self.offer(Serving::Own);
                } else if let Some(b) = &mut self.backup {
                    // A read-back cut short by a dropped link: ask again
                    // for what is missing.
                    let gs = b.resume();
                    self.phase = Phase::BackingUp;
                    self.send_all(gs);
                } else {
                    self.start_source(Purpose::Backup);
                }
            }
            other => self.finish(Finish::Stopped(StopReason::Decision(other))),
        }
    }

    fn start_source(&mut self, purpose: Purpose) {
        let Some(m) = &self.board.manifest else {
            return;
        };
        let Some(sha) = self.board.engine_sha256() else {
            return self.finish(Finish::Stopped(StopReason::Decision(Decision::NeedsUsb {
                why: crate::decide::decision::NeedsUsbWhy::UnknownBoard,
            })));
        };
        let read_back = match purpose {
            Purpose::Backup => m.engine_len,
            Purpose::Heal => None,
        };
        let source = EngineSource::new(sha, m.target.clone(), m.build_id.clone(), read_back);
        let step = source.start();
        self.source = Some((source, purpose));
        self.on_source_step(step);
    }

    fn on_source_step(&mut self, step: SourceStep) {
        let Some((_, purpose)) = self.source.as_ref().map(|(s, p)| (s.clone(), *p)) else {
            return;
        };
        match step {
            SourceStep::Ask(SourceEffect::ReadBack { sha, len }) => {
                let mut backup = BackupSession::new(sha, len, self.config.serve.ahead);
                let gs = backup.start();
                self.backup = Some(backup);
                self.phase = Phase::BackingUp;
                self.send_all(gs);
            }
            SourceStep::Ask(effect) => {
                self.phase = Phase::Sourcing;
                self.effects.push(DriverEffect::Source(effect));
            }
            SourceStep::Held { bytes, keep } => {
                self.source = None;
                if let Some(keep) = keep {
                    self.effects.push(DriverEffect::Source(keep));
                }
                match purpose {
                    Purpose::Backup => {
                        self.backup_held = true;
                        self.backup = None;
                        self.offer(Serving::Own);
                    }
                    Purpose::Heal => {
                        let built = self
                            .board
                            .manifest
                            .as_ref()
                            .and_then(|m| HostBuild::for_heal(m, bytes).ok());
                        match built {
                            Some(b) => {
                                self.heal_build = Some(b);
                                self.offer(Serving::Heal);
                            }
                            None => self.finish(Finish::Stopped(StopReason::MissingEngine {
                                offline: false,
                            })),
                        }
                    }
                }
            }
            SourceStep::Missing { offline } => {
                self.source = None;
                let reason = if purpose == Purpose::Backup && self.backup.is_some() {
                    StopReason::BackupFailed
                } else {
                    StopReason::MissingEngine { offline }
                };
                self.backup = None;
                self.finish(Finish::Stopped(reason));
            }
        }
    }

    fn on_read_back(&mut self, bytes: &[u8]) {
        let Some(backup) = &mut self.backup else {
            return;
        };
        match backup.on_board(bytes) {
            BackupStep::Send(gs) => {
                if let Some(total) = self.board.engine_len() {
                    let done = backup.contiguous();
                    self.effects.push(DriverEffect::Progress {
                        stage: Stage::BackingUp,
                        done,
                        total,
                    });
                }
                self.send_all(gs);
            }
            BackupStep::Done(engine) => self.source_result(SourceResult::ReadBack(Some(engine))),
            BackupStep::Failed(_) => self.source_result(SourceResult::ReadBack(None)),
        }
    }

    // ---- Offering and serving ----------------------------------------------------

    fn offer(&mut self, which: Serving) {
        if self.phase == Phase::Offered(which) && self.serve.is_some() {
            return; // already offered on this link
        }
        self.drop_serve();
        self.serve = Some(ServeSession::new(self.config.serve));
        self.phase = Phase::Offered(which);
        let offer = ServeSession::offer(self.served_build(which));
        self.send(offer);
    }

    fn serve_message(&mut self, which: Serving, bytes: &[u8]) {
        let build = match which {
            Serving::Own => &self.build,
            Serving::Heal => match &self.heal_build {
                Some(b) => b,
                None => return,
            },
        };
        let Some(serve) = &mut self.serve else {
            return;
        };
        let out = serve.on_board(build, bytes);
        self.send_all(out.send);
        for event in out.events {
            if let ServeEvent::Refused(r) = event {
                self.on_refused(which, r);
            }
        }
    }

    fn on_refused(&mut self, which: Serving, r: HostRefusal) {
        match r {
            // A running engine's channel 3 takes no `L`: its server's own
            // login (channel 1) holds the link's tier.
            HostRefusal::NeedsLogin if self.board.state() == Some(BoardState::Running) => {
                self.finish(Finish::Stopped(StopReason::NeedsEngineLogin));
            }
            HostRefusal::NeedsLogin => {
                self.drop_serve();
                self.phase = Phase::NeedsCredentials(which);
                self.effects.push(DriverEffect::NeedCredentials);
            }
            HostRefusal::HashMismatch => {
                self.retries += 1;
                if self.retries > self.config.max_piece_retries {
                    self.finish(Finish::Stopped(StopReason::TooManyRetries));
                } else {
                    self.drop_serve();
                    self.offer(which);
                }
            }
            HostRefusal::BoardLacksMessage(ty) => {
                self.finish(Finish::Stopped(StopReason::BoardLacksMessage(ty)));
            }
            HostRefusal::FailedBuild(build) => {
                self.finish(Finish::Stopped(StopReason::Decision(
                    Decision::RefusedBuild { build },
                )));
            }
            other => self.finish(Finish::Stopped(StopReason::Refused(other))),
        }
    }

    fn on_login(&mut self, now_ms: u64, bytes: &[u8], credentials: &[Credential]) {
        let Phase::LoggingIn(which) = self.phase else {
            return;
        };
        match self.login.on_board(bytes, credentials) {
            Some(LoginEvent::Send(answer)) => self.send(answer),
            Some(LoginEvent::Granted(_)) => self.offer(which),
            Some(LoginEvent::Refused {
                exhausted: true, ..
            })
            | Some(LoginEvent::NothingToAnswer) => {
                self.finish(Finish::Stopped(StopReason::LoginRefused));
            }
            Some(LoginEvent::Refused { retry_after_ms, .. }) => {
                if retry_after_ms == 0 {
                    let begin = self.login.begin();
                    self.send(begin);
                } else {
                    self.login_retry_at = Some(now_ms + u64::from(retry_after_ms));
                }
            }
            None => {}
        }
    }

    // ---- Helpers ---------------------------------------------------------------

    fn served_build(&self, which: Serving) -> &HostBuild {
        match (which, &self.heal_build) {
            (Serving::Heal, Some(b)) => b,
            _ => &self.build,
        }
    }

    fn stage_of(&self, which: Serving, kind: PieceKind) -> Stage {
        match (which, kind) {
            (_, PieceKind::Core) => Stage::Updating,
            (Serving::Heal, PieceKind::Engine) => Stage::Restoring,
            (Serving::Own, PieceKind::Engine) => {
                // The host's own engine: finishing an update when this host
                // moved the core; otherwise putting the engine back.
                if self.moved_core {
                    Stage::Finishing
                } else {
                    Stage::Restoring
                }
            }
        }
    }

    fn drop_serve(&mut self) {
        if let Some(s) = self.serve.take() {
            add(&mut self.totals, s.counters());
        }
    }

    fn finish(&mut self, finish: Finish) {
        self.drop_serve();
        self.phase = Phase::Done;
        self.effects.push(DriverEffect::Done(finish));
    }

    fn send(&mut self, bytes: Vec<u8>) {
        self.effects.push(DriverEffect::Send(bytes));
    }

    fn send_all(&mut self, all: Vec<Vec<u8>>) {
        for bytes in all {
            self.send(bytes);
        }
    }
}

fn add(t: &mut ServeCounters, c: ServeCounters) {
    t.requests += c.requests;
    t.chunks_raw += c.chunks_raw;
    t.chunks_encoded += c.chunks_encoded;
    t.bytes_raw += c.bytes_raw;
    t.bytes_encoded += c.bytes_encoded;
    t.chunks_duplicate += c.chunks_duplicate;
    t.bytes_duplicate += c.bytes_duplicate;
}
