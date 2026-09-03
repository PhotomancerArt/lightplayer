//! One device across time: intent, evidence, and at most one supervised
//! activity.
//!
//! One entry point ([`Device::handle`]), two arms with different rights:
//!
//! ```text
//! Input::Action (user)  → journal it; may write `intent`, spawn/cancel an
//!                         activity, emit commands. NEVER writes evidence.
//! Input::Event  (world) → journal it; fold into `evidence`; forward to the
//!                         active activity. NEVER writes intent.
//! ```
//!
//! Supervision lives here too, and it is the answer to "zero foreground
//! cancellation": a cancel is *requested*, the activity gets a bounded grace
//! to wind down, and then it is **evicted** — the device journals the
//! eviction, emits link-rebuild commands, and re-derives from fresh
//! evidence. Cancellation is bounded by removal, not by politeness.
//!
//! There is no `link` field. The link a device is on is
//! [`Evidence::presence`](crate::Evidence::presence); the roster's route map
//! is the other half. Storing it a third time on the device is how the
//! shipped system ended up with two cards for one board.

use serde::{Deserialize, Serialize};

use crate::activity::activity_cell::Reducer;
use crate::activity::erase::EraseActivity;
use crate::activity::flash::FlashActivity;
use crate::activity::identify::IdentifyActivity;
use crate::activity::push::PushActivity;
use crate::activity::remove_project::RemoveProjectActivity;
use crate::activity::{
    ActivityCell, ActivityCtx, ActivityKind, ActivityOutcome, ActivityProgress, ActivityStep,
};
use crate::event::{Action, ActivityMarker, Command, EffectId, Event, Input};
use crate::evidence::{Classification, Evidence};
use crate::identity::{DeviceId, IdentityBinding, IdentityChain};
use crate::intent::{ConnectionIntent, Intent};
use crate::journal::{EvictionReason, JournalNote, Scope};
use crate::link::{LinkCommand, LinkId};
use crate::record::DeviceRecord;
use crate::roster::ModelCtx;
use crate::time::Millis;
use crate::wire::ClientFrame;

/// Correlation ids for the two frames [`Action::ClearFaults`] sends.
///
/// Constants rather than a counter because nothing correlates them: the fold
/// reads a frame's BODY, and every activity already restarts its own ids at
/// 1 (see `IdentifyActivity::ask`). A number that only ever appears in a
/// journal line earns no state on the device.
const CLEAR_FAULTS_REQUEST_ID: u32 = 1;
const CLEAR_FAULTS_REREAD_REQUEST_ID: u32 = 2;

/// One known device.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Device {
    pub id: DeviceId,
    /// The persisted snapshot, once there is one.
    pub record: Option<DeviceRecord>,
    /// Bindings learned from evidence. Hoisted out of the record on purpose:
    /// an anonymous board has a chain long before it earns a record.
    pub identity: IdentityChain,
    pub intent: Intent,
    pub evidence: Evidence,
    pub activity: Option<ActivityCell>,
    /// Routing bookkeeping: the generation of the single timer this device
    /// has armed, and when it is due.
    armed_timer: Option<(u64, Millis)>,
    /// Consecutive silent identifies on the current link generation, for
    /// the auto-retry cap. Reset by a fresh link, a verdict, or a user
    /// gesture that asks again.
    #[serde(default)]
    identify_retries: u32,
    /// Generations handed to coarse effects on this device. Monotonic and
    /// never reset: a marker from an effect this device started two
    /// activities ago must not be able to look current.
    #[serde(default)]
    next_effect_id: u64,
}

impl Device {
    pub fn new(id: DeviceId, identity: IdentityChain) -> Self {
        Self {
            id,
            record: None,
            identity,
            intent: Intent::default(),
            evidence: Evidence::default(),
            activity: None,
            armed_timer: None,
            identify_retries: 0,
            next_effect_id: 0,
        }
    }

    pub fn from_record(record: DeviceRecord) -> Self {
        let mut device = Self::new(record.device, record.identity.clone());
        device.intent.name = record.name.clone();
        device.intent.autoconnect = record.autoconnect;
        device.record = Some(record);
        device
    }

    /// The link this device is on, derived from the fold — never stored
    /// twice.
    pub fn link(&self) -> Option<LinkId> {
        self.evidence.link()
    }

    /// At most one activity per device (invariant I5).
    pub fn is_busy(&self) -> bool {
        self.activity.is_some()
    }

    /// What to call this device: the user's name, else the provisioned name,
    /// else the strongest identity binding, else an honest placeholder.
    pub fn title(&self) -> String {
        if let Some(name) = &self.intent.name {
            return name.clone();
        }
        if let Some(name) = &self.identity.name {
            return name.clone();
        }
        if let Some(label) = self.identity.strongest_label() {
            return label;
        }
        "New device".to_string()
    }

    /// The one entry point.
    pub(crate) fn handle(
        &mut self,
        now: Millis,
        input: &Input,
        ctx: &mut ModelCtx<'_>,
    ) -> Vec<Command> {
        if let Input::Event(Event::TimerFired { timer }) = input {
            // A superseded generation: the scope has re-armed since. Drop it
            // without journaling, so stale fires cannot churn the timeline.
            if self.armed_timer.map(|(seq, _)| seq) != Some(timer.seq) {
                return Vec::new();
            }
            self.armed_timer = None;
        }

        ctx.journal.record_input(now, self.scope(), input);
        let mut commands = match input {
            Input::Action(action) => self.handle_action(now, action, ctx),
            Input::Event(event) => self.handle_event(now, event, ctx),
        };
        commands.extend(self.rearm(now, ctx));
        commands
    }

    /// Remove the running activity, journal it, and emit recovery. Used by
    /// supervision, by link loss, and by Forget — which is why Forget works
    /// mid-activity.
    pub(crate) fn evict(
        &mut self,
        now: Millis,
        reason: EvictionReason,
        ctx: &mut ModelCtx<'_>,
    ) -> Vec<Command> {
        let Some(cell) = self.activity.take() else {
            return Vec::new();
        };
        let mut abandoned = self.abandon_commands(&cell);
        ctx.journal.note(
            now,
            self.scope(),
            JournalNote::ActivityEvicted {
                kind: cell.kind,
                reason,
            },
        );
        let outcome = match reason {
            EvictionReason::CancelGraceExpired => ActivityOutcome::Interrupted {
                reason: "cancel grace expired".to_string(),
            },
            EvictionReason::DeadlineExpired => ActivityOutcome::TimedOut,
            EvictionReason::LinkLost => ActivityOutcome::Interrupted {
                reason: "link lost".to_string(),
            },
            EvictionReason::UserDisconnected => ActivityOutcome::Interrupted {
                reason: "disconnected".to_string(),
            },
            EvictionReason::DeviceForgotten => ActivityOutcome::Interrupted {
                reason: "device forgotten".to_string(),
            },
        };
        self.raise_marker(
            now,
            ActivityMarker::Ended {
                kind: cell.kind,
                outcome,
            },
            ctx,
        );
        // The abandon goes FIRST: the recovery it precedes reopens the port,
        // and a pump still paused behind an orphaned borrow would eat every
        // line the reopened port carries.
        abandoned.extend(self.recovery_commands(reason, ctx));
        abandoned
    }

    /// Let go of the coarse effect a cell that is coming down still owns.
    ///
    /// The effect itself cannot be stopped — it runs in a spawned future the
    /// model has no handle on — but its exclusive wire borrow can be given
    /// back, and it must be: an orphaned borrow held the fold deaf for the
    /// rest of the effect's own budget (G1 bench, 2026-08-31). Late markers
    /// are already dropped by the generation stamp.
    fn abandon_commands(&self, cell: &ActivityCell) -> Vec<Command> {
        let (Some(effect_id), Some(link)) = (cell.current_effect, self.link()) else {
            return Vec::new();
        };
        vec![Command::AbandonEffect {
            device: self.id,
            link,
            effect_id,
        }]
    }

    /// Persist-worthy snapshot of identity + preferences.
    pub(crate) fn record_snapshot(&mut self) -> DeviceRecord {
        let mut record = self
            .record
            .take()
            .unwrap_or_else(|| DeviceRecord::new(self.id, self.identity.clone()));
        record.identity = self.identity.clone();
        record.name = self.intent.name.clone();
        record.autoconnect = self.intent.autoconnect;
        record.last_seen = self.evidence.freshness.last_heard.or(record.last_seen);
        self.record = Some(record.clone());
        record
    }

    /// Fold an event without journaling the input itself. The roster uses
    /// this when it re-derives a device after a routing change.
    pub(crate) fn fold_only(
        &mut self,
        now: Millis,
        event: &Event,
        ctx: &mut ModelCtx<'_>,
    ) -> Vec<Command> {
        if matches!(event, Event::LinkAttached { .. }) {
            // A fresh link generation gets a fresh auto-retry budget.
            self.identify_retries = 0;
        }
        let notes = self
            .evidence
            .fold(now, event, &mut self.identity, ctx.config);
        self.journal_notes(now, notes, ctx);
        self.rearm(now, ctx)
    }

    /// Spawn identification, unless the device is already busy (I5) or has
    /// no link to talk over.
    pub(crate) fn spawn_identify(&mut self, now: Millis, ctx: &mut ModelCtx<'_>) -> Vec<Command> {
        if self.activity.is_some() || self.link().is_none() {
            return Vec::new();
        }
        let settle_at = now.plus_ms(ctx.config.identify_deadline_ms);
        let effect_id = self.mint_effect_id();
        let mut reducer =
            IdentifyActivity::new(now, settle_at, ctx.config.hello_request_interval_ms);
        let commands = {
            let mut activity_ctx = ActivityCtx {
                link: self.evidence.link(),
                evidence: &self.evidence,
                config: ctx.config,
                effect_id,
            };
            reducer.spawn_commands(&mut activity_ctx)
        };
        let cell = ActivityCell::new(
            now,
            settle_at.plus_ms(ctx.config.supervision_slack_ms),
            Reducer::Identify(reducer),
        );
        self.install_activity(now, cell, effect_id, &commands, ctx);
        commands
    }

    /// Spawn the Flash activity, unless the device is already busy (I5) or
    /// has no link to flash over. The build id was resolved by the app from
    /// (board, detected chip); the model treats both as opaque.
    pub(crate) fn spawn_flash(
        &mut self,
        now: Millis,
        board_id: &str,
        build_id: &str,
        park_first: bool,
        ctx: &mut ModelCtx<'_>,
    ) -> Vec<Command> {
        if self.activity.is_some() || self.link().is_none() {
            return Vec::new();
        }
        let effect_id = self.mint_effect_id();
        let mut reducer = FlashActivity::new(
            self.id,
            board_id.to_string(),
            build_id.to_string(),
            park_first,
        );
        let commands = {
            let activity_ctx = ActivityCtx {
                link: self.evidence.link(),
                evidence: &self.evidence,
                config: ctx.config,
                effect_id,
            };
            reducer.spawn_commands(now, &activity_ctx)
        };
        let cell = ActivityCell::new(
            now,
            now.plus_ms(ctx.config.flash_deadline_ms),
            Reducer::Flash(reducer),
        );
        self.install_activity(now, cell, effect_id, &commands, ctx);
        commands
    }

    /// Spawn the Erase activity — Factory reset — unless the device is
    /// already busy (I5) or has no link to erase over.
    pub(crate) fn spawn_erase(&mut self, now: Millis, ctx: &mut ModelCtx<'_>) -> Vec<Command> {
        if self.activity.is_some() || self.link().is_none() {
            return Vec::new();
        }
        let effect_id = self.mint_effect_id();
        let reducer = EraseActivity::new(self.id);
        let commands = {
            let activity_ctx = ActivityCtx {
                link: self.evidence.link(),
                evidence: &self.evidence,
                config: ctx.config,
                effect_id,
            };
            reducer.spawn_commands(&activity_ctx)
        };
        let cell = ActivityCell::new(
            now,
            now.plus_ms(ctx.config.flash_deadline_ms),
            Reducer::Erase(reducer),
        );
        self.install_activity(now, cell, effect_id, &commands, ctx);
        commands
    }

    /// Spawn the Remove-project activity, unless the device is already busy
    /// (I5) or has no link to talk over. WHICH project is removed is the
    /// board's own report, read inside the conversation — see
    /// [`Action::RemoveProject`](crate::Action::RemoveProject).
    pub(crate) fn spawn_remove_project(
        &mut self,
        now: Millis,
        ctx: &mut ModelCtx<'_>,
    ) -> Vec<Command> {
        if self.activity.is_some() || self.link().is_none() {
            return Vec::new();
        }
        let effect_id = self.mint_effect_id();
        let reducer = RemoveProjectActivity::new(self.id);
        let commands = {
            let activity_ctx = ActivityCtx {
                link: self.evidence.link(),
                evidence: &self.evidence,
                config: ctx.config,
                effect_id,
            };
            reducer.spawn_commands(&activity_ctx)
        };
        // The same conversation shape as a push, over the same wire: its
        // backstop is the same one.
        let cell = ActivityCell::new(
            now,
            now.plus_ms(ctx.config.push_deadline_ms),
            Reducer::RemoveProject(reducer),
        );
        self.install_activity(now, cell, effect_id, &commands, ctx);
        commands
    }

    /// Spawn the Push activity, unless the device is already busy (I5) or
    /// has no link to push over. The payload was staged with the effects
    /// layer before this gesture was folded — see
    /// [`Action::Push`](crate::Action::Push) for why the model does not
    /// carry it.
    pub(crate) fn spawn_push(&mut self, now: Millis, ctx: &mut ModelCtx<'_>) -> Vec<Command> {
        if self.activity.is_some() || self.link().is_none() {
            return Vec::new();
        }
        let effect_id = self.mint_effect_id();
        let reducer = PushActivity::new(self.id);
        let commands = {
            let activity_ctx = ActivityCtx {
                link: self.evidence.link(),
                evidence: &self.evidence,
                config: ctx.config,
                effect_id,
            };
            reducer.spawn_commands(&activity_ctx)
        };
        let cell = ActivityCell::new(
            now,
            now.plus_ms(ctx.config.push_deadline_ms),
            Reducer::Push(reducer),
        );
        self.install_activity(now, cell, effect_id, &commands, ctx);
        commands
    }

    /// Seat a freshly spawned activity: journal the bracket, raise the
    /// `Started` marker, and remember which coarse effect (if any) the
    /// spawn started, so that effect's markers can be told apart from a
    /// predecessor's.
    fn install_activity(
        &mut self,
        now: Millis,
        mut cell: ActivityCell,
        effect_id: EffectId,
        commands: &[Command],
        ctx: &mut ModelCtx<'_>,
    ) {
        if starts_effect(commands, effect_id) {
            cell.current_effect = Some(effect_id);
        }
        let kind = cell.kind;
        self.activity = Some(cell);
        ctx.journal
            .note(now, self.scope(), JournalNote::ActivityStarted { kind });
        self.raise_marker(now, ActivityMarker::Started { kind }, ctx);
    }

    /// The stamp the next coarse effect this device starts will wear.
    fn mint_effect_id(&mut self) -> EffectId {
        self.next_effect_id += 1;
        EffectId(self.next_effect_id)
    }

    pub(crate) fn scope(&self) -> Scope {
        Scope::Device(self.id)
    }

    fn handle_action(
        &mut self,
        now: Millis,
        action: &Action,
        ctx: &mut ModelCtx<'_>,
    ) -> Vec<Command> {
        match action {
            Action::Connect { .. } => {
                self.intent.connection = ConnectionIntent::Connected;
                self.identify_retries = 0;
                let mut commands = vec![Command::PersistRecord(self.record_snapshot())];
                commands.extend(self.spawn_identify(now, ctx));
                commands
            }
            Action::Disconnect { .. } => {
                self.intent.connection = ConnectionIntent::Disconnected;
                // No link rebuild: the user asked for the port back, so
                // recovery would be fighting them.
                let mut commands = self.evict(now, EvictionReason::UserDisconnected, ctx);
                if let Some(link) = self.link() {
                    commands.push(Command::Link {
                        link,
                        command: LinkCommand::Close,
                    });
                }
                commands
            }
            Action::CancelActivity { .. } => self.request_cancel(now, action, ctx),
            Action::Identify { .. } => {
                self.identify_retries = 0;
                self.spawn_identify(now, ctx)
            }
            Action::Flash {
                board_id,
                build_id,
                park_first,
                ..
            } => {
                // Flashing implies wanting the board connected afterwards.
                self.intent.connection = ConnectionIntent::Connected;
                self.spawn_flash(now, board_id, build_id, *park_first, ctx)
            }
            Action::Push { .. } => {
                // Sending a project implies wanting the board connected.
                self.intent.connection = ConnectionIntent::Connected;
                self.spawn_push(now, ctx)
            }
            Action::Erase { .. } => {
                // A wipe implies staying connected to re-flash afterwards.
                self.intent.connection = ConnectionIntent::Connected;
                self.spawn_erase(now, ctx)
            }
            Action::ResetBoard { .. } => {
                // A hardware reset is a direct gesture, not an activity: one
                // command, then identify reads whatever boots. Refused while
                // an activity runs (I5 — a reset under a flash would wreck
                // it) and without a link there is nothing to pulse.
                if self.activity.is_some() {
                    return Vec::new();
                }
                let Some(link) = self.link() else {
                    return Vec::new();
                };
                let mut commands = vec![Command::Link {
                    link,
                    command: crate::link::LinkCommand::RunReset(crate::link::ResetKind::Normal),
                }];
                commands.extend(self.spawn_identify(now, ctx));
                commands
            }
            Action::ClearFaults { .. } => {
                // A direct gesture like ResetBoard, and for the same reason:
                // there is nothing to supervise. The device answers and then
                // does NOTHING — no reboot, no re-load — because the cleared
                // ledger takes effect on its own next tick. An activity that
                // owns the port would have its correlation walked over, so
                // this is refused while one runs (I5), and with no link there
                // is nobody to ask.
                if self.activity.is_some() {
                    return Vec::new();
                }
                let Some(link) = self.link() else {
                    return Vec::new();
                };
                // Asked for, not waited for: the loaded-project report is
                // what carries each project's fault verdict, and a card that
                // kept saying Degraded for a whole heartbeat period after
                // the user cleared it would read as a verb that did nothing.
                // The answer is honest either way — a failure that is still
                // there faults again on the next tick and the following
                // heartbeat re-degrades the card.
                vec![
                    Command::Link {
                        link,
                        command: LinkCommand::SendFrame(ClientFrame::clear_faults(
                            CLEAR_FAULTS_REQUEST_ID,
                        )),
                    },
                    Command::Link {
                        link,
                        command: LinkCommand::SendFrame(ClientFrame::list_loaded(
                            CLEAR_FAULTS_REREAD_REQUEST_ID,
                        )),
                    },
                ]
            }
            Action::RemoveProject { .. } => {
                // Clearing a board implies staying connected to put
                // something else on it — the empty face is the next stop.
                self.intent.connection = ConnectionIntent::Connected;
                self.spawn_remove_project(now, ctx)
            }
            Action::SetName { name, .. } => {
                self.intent.name = Some(name.clone());
                vec![Command::PersistRecord(self.record_snapshot())]
            }
            Action::SetAutoconnect { enabled, .. } => {
                self.intent.autoconnect = *enabled;
                vec![Command::PersistRecord(self.record_snapshot())]
            }
            // Roster-level verbs never reach a device: `Forget` has to
            // remove the entry, and the link verbs address links.
            Action::Forget { .. }
            | Action::AddFromUsb
            | Action::Reconnect { .. }
            | Action::AdoptLink { .. }
            | Action::DismissLink { .. } => Vec::new(),
        }
    }

    fn handle_event(&mut self, now: Millis, event: &Event, ctx: &mut ModelCtx<'_>) -> Vec<Command> {
        // A coarse effect runs in a spawned future nothing can cancel. When
        // its activity is evicted mid-run — a cancel grace that expired, a
        // deadline, a Forget — the effect keeps going and eventually reports.
        // Without this guard that straggler's `Ended` would fold as the END
        // of whatever activity is running NOW, retiring a live push because
        // an old flash finally finished. It is dropped before the fold, and
        // journaled so the timeline still explains the silence.
        if let Event::ActivityMarker {
            effect: Some(effect),
            marker,
            ..
        } = event
            && self.activity.as_ref().and_then(|cell| cell.current_effect) != Some(*effect)
        {
            ctx.journal.note(
                now,
                self.scope(),
                JournalNote::StaleEffectMarker {
                    effect: *effect,
                    ended: matches!(marker, ActivityMarker::Ended { .. }),
                },
            );
            return Vec::new();
        }
        if matches!(event, Event::LinkAttached { .. }) {
            // A fresh link generation gets a fresh auto-retry budget.
            self.identify_retries = 0;
        }
        let mut commands = Vec::new();

        // 1. Fold first, so the activity always reads fresh evidence.
        let notes = self
            .evidence
            .fold(now, event, &mut self.identity, ctx.config);
        let promoted_identity = notes.iter().any(|note| {
            matches!(
                note,
                JournalNote::IdentityPromoted {
                    binding: IdentityBinding::Uid | IdentityBinding::Mac | IdentityBinding::Name,
                    ..
                }
            )
        });
        self.journal_notes(now, notes, ctx);

        // 2. Coarse-effect progress lands on the cell (display only).
        if let Event::ActivityMarker {
            marker: ActivityMarker::Progress { label, percent },
            ..
        } = event
        {
            if let Some(cell) = &mut self.activity {
                cell.progress = Some(ActivityProgress {
                    label: label.clone(),
                    percent: *percent,
                });
            }
            ctx.journal.note(
                now,
                self.scope(),
                JournalNote::ActivityProgress {
                    label: label.clone(),
                    percent: *percent,
                },
            );
        }

        // 3. Forward to the activity.
        if let Some(step) = self.forward(now, &Input::Event(event.clone()), ctx) {
            commands.extend(self.apply_step(now, step, ctx));
        }

        // 4. A vanished link removes the ground under any activity.
        if matches!(event, Event::LinkDetached { .. }) {
            commands.extend(self.evict(now, EvictionReason::LinkLost, ctx));
        }

        // 5. Supervision looks at the clock last.
        commands.extend(self.supervise(now, ctx));

        if promoted_identity {
            commands.push(Command::PersistRecord(self.record_snapshot()));
        }
        commands
    }

    fn request_cancel(
        &mut self,
        now: Millis,
        action: &Action,
        ctx: &mut ModelCtx<'_>,
    ) -> Vec<Command> {
        let Some(cell) = &mut self.activity else {
            return Vec::new();
        };
        if cell.is_cancel_requested() {
            return Vec::new();
        }
        let kind = cell.kind;
        cell.cancel = crate::activity::CancelPhase::CancelRequested { since: now };
        ctx.journal.note(
            now,
            self.scope(),
            JournalNote::ActivityCancelRequested { kind },
        );
        let mut commands = Vec::new();
        if let Some(step) = self.forward(now, &Input::Action(action.clone()), ctx) {
            commands.extend(self.apply_step(now, step, ctx));
        }
        commands
    }

    fn forward(
        &mut self,
        now: Millis,
        input: &Input,
        ctx: &mut ModelCtx<'_>,
    ) -> Option<ActivityStep> {
        let effect_id = self.mint_effect_id();
        let Self {
            activity, evidence, ..
        } = self;
        let cell = activity.as_mut()?;
        let mut activity_ctx = ActivityCtx {
            link: evidence.link(),
            evidence,
            config: ctx.config,
            effect_id,
        };
        let step = cell.handle(now, input, &mut activity_ctx);
        if starts_effect(step_commands(&step), effect_id) {
            cell.current_effect = Some(effect_id);
        }
        Some(step)
    }

    fn apply_step(
        &mut self,
        now: Millis,
        step: ActivityStep,
        ctx: &mut ModelCtx<'_>,
    ) -> Vec<Command> {
        match step {
            ActivityStep::Continue(commands) => commands,
            ActivityStep::Done {
                outcome,
                mut commands,
            } => {
                let Some(cell) = self.activity.take() else {
                    return commands;
                };
                // A reducer can settle while its effect is still running —
                // the stamp's own deadline is exactly that case. Give the
                // wire back rather than leaving the pump paused behind an
                // effect nothing is listening to any more.
                commands.extend(self.abandon_commands(&cell));
                ctx.journal.note(
                    now,
                    self.scope(),
                    JournalNote::ActivityEnded {
                        kind: cell.kind,
                        outcome: outcome.clone(),
                    },
                );
                let failed = matches!(outcome, ActivityOutcome::Failed { .. });
                self.raise_marker(
                    now,
                    ActivityMarker::Ended {
                        kind: cell.kind,
                        outcome,
                    },
                    ctx,
                );
                commands.push(Command::PersistRecord(self.record_snapshot()));
                // Silence is the one failure identify can end in, and one
                // flaky boot window must not strand the card at "no
                // response" while the intent is still Connected: re-ask on
                // a fresh window, up to the configured cap. Any verdict —
                // welcome or not — resets the budget.
                if cell.kind == ActivityKind::Identify {
                    if failed
                        && self.intent.connection == ConnectionIntent::Connected
                        && self.link().is_some()
                        && self.identify_retries < ctx.config.identify_auto_retries
                    {
                        self.identify_retries += 1;
                        commands.extend(self.spawn_identify(now, ctx));
                    } else if !failed {
                        self.identify_retries = 0;
                    }
                }
                commands
            }
        }
    }

    fn supervise(&mut self, now: Millis, ctx: &mut ModelCtx<'_>) -> Vec<Command> {
        let Some(cell) = &self.activity else {
            return Vec::new();
        };
        if cell.cancel_grace_expired(now, cell.kind.cancel_grace_ms(ctx.config)) {
            return self.evict(now, EvictionReason::CancelGraceExpired, ctx);
        }
        if now >= cell.deadline {
            return self.evict(now, EvictionReason::DeadlineExpired, ctx);
        }
        Vec::new()
    }

    /// Rebuild the link so the device can re-derive from fresh evidence. A
    /// lost link has nothing to rebuild; everything else gets close+open.
    fn recovery_commands(&self, reason: EvictionReason, ctx: &ModelCtx<'_>) -> Vec<Command> {
        let Some(link) = self.link() else {
            return Vec::new();
        };
        match reason {
            EvictionReason::LinkLost
            | EvictionReason::UserDisconnected
            | EvictionReason::DeviceForgotten => Vec::new(),
            EvictionReason::CancelGraceExpired | EvictionReason::DeadlineExpired => vec![
                Command::Link {
                    link,
                    command: LinkCommand::Close,
                },
                Command::Link {
                    link,
                    command: LinkCommand::Open {
                        baud: ctx.config.open_baud,
                    },
                },
            ],
        }
    }

    fn raise_marker(&mut self, now: Millis, marker: ActivityMarker, ctx: &mut ModelCtx<'_>) {
        // The model's own bracket: no effect stamp, and therefore never
        // stale — it is raised by the same fold that would judge it.
        let event = Event::ActivityMarker {
            device: self.id,
            effect: None,
            marker,
        };
        let notes = self
            .evidence
            .fold(now, &event, &mut self.identity, ctx.config);
        self.journal_notes(now, notes, ctx);
    }

    fn journal_notes(&self, now: Millis, notes: Vec<JournalNote>, ctx: &mut ModelCtx<'_>) {
        for note in notes {
            ctx.journal.note(now, self.scope(), note);
        }
    }

    /// One timer per device, armed for the nearest thing it is waiting on.
    fn rearm(&mut self, now: Millis, ctx: &mut ModelCtx<'_>) -> Vec<Command> {
        let mut soonest: Option<Millis> = None;
        if let Some(cell) = &self.activity {
            soonest = Some(cell.next_deadline(cell.kind.cancel_grace_ms(ctx.config)));
        }
        // `Evidence`'s own answer, not the raw freshness one: a borrowed
        // wire has no silence to time, so a flash does not keep re-arming a
        // quiet timer that could only fire a lie.
        if let Some(quiet_at) = self.evidence.quiet_deadline(ctx.config.quiet_after_ms) {
            soonest = Some(match soonest {
                Some(existing) => existing.min(quiet_at),
                None => quiet_at,
            });
        }
        let Some(at) = soonest else {
            self.armed_timer = None;
            return Vec::new();
        };
        if let Some((_, armed_at)) = self.armed_timer {
            if armed_at == at {
                return Vec::new();
            }
        }
        let timer = ctx.timers.next(self.scope());
        self.armed_timer = Some((timer.seq, at));
        vec![Command::StartTimer {
            timer,
            after_ms: at.since(now),
        }]
    }
}

/// Whether a batch of commands starts the coarse effect stamped `effect_id`.
fn starts_effect(commands: &[Command], effect_id: EffectId) -> bool {
    commands.iter().any(|command| {
        matches!(
            command,
            Command::RunEffect { effect_id: started, .. } if *started == effect_id
        )
    })
}

/// The commands a step is carrying, whichever shape it took.
fn step_commands(step: &ActivityStep) -> &[Command] {
    match step {
        ActivityStep::Continue(commands) | ActivityStep::Done { commands, .. } => commands,
    }
}

/// A device's headline state, for the projection and for tests. Derived —
/// never stored.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum DeviceStatus {
    /// Not on the bus.
    Offline,
    /// On the bus, nothing being asked of it.
    Attached,
    /// An activity is running.
    Busy,
    /// Identified as a usable LightPlayer.
    Ready,
    /// A usable LightPlayer that is running something BROKEN: a node in
    /// fault, or a crash-recovery state it reported as not green.
    ///
    /// Still a running board — the loaded-project face stays `Running` —
    /// but "Ready" over a show that is painting the fault pattern is the
    /// lie this status exists to stop (2026-09-01 bench: two days of
    /// "Running" over a black strip).
    Degraded,
    /// Identified as something else: blank, bootloader, foreign,
    /// incompatible.
    NeedsAttention,
    /// Attached and open, but saying nothing.
    NotResponding,
}

impl Device {
    /// Derive the headline state. Total by construction: every combination
    /// of presence, classification and activity lands somewhere.
    pub fn status(&self) -> DeviceStatus {
        if self.activity.is_some() {
            return DeviceStatus::Busy;
        }
        if !self.evidence.presence.is_attached() {
            return DeviceStatus::Offline;
        }
        // Statuses that CLAIM we are listening (Ready, NotResponding) are
        // only honest on an open port: a surviving verdict on a closed one
        // (the window outlives a close — a close is our action, not
        // evidence) renders as Attached instead, because "Ready" from a
        // port nobody holds is the same lie "port closed" was. Verdicts
        // that ask for action (needs-firmware family) keep their face:
        // they are actionable exactly as stored, listening or not.
        match &self.evidence.classification {
            Classification::LightPlayer { .. } => {
                if !self.evidence.presence.is_open() {
                    return DeviceStatus::Attached;
                }
                // Degradation is a refinement of Ready, never of a verdict
                // that already asks for action: a board we are not
                // listening to has nothing current to say about its own
                // health, and a blank chip has no project to fault.
                match self.evidence.is_degraded() {
                    true => DeviceStatus::Degraded,
                    false => DeviceStatus::Ready,
                }
            }
            Classification::Incompatible { .. }
            | Classification::Blank
            | Classification::Bootloader
            | Classification::Foreign { .. } => DeviceStatus::NeedsAttention,
            Classification::Quiet { .. } | Classification::Unknown => {
                if self.evidence.presence.is_open() {
                    DeviceStatus::NotResponding
                } else {
                    DeviceStatus::Attached
                }
            }
        }
    }

    /// The activity kind currently running, if any.
    pub fn activity_kind(&self) -> Option<ActivityKind> {
        self.activity.as_ref().map(|cell| cell.kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::Journal;
    use crate::link::{LinkEvent, LinkInfo};
    use crate::roster::RosterConfig;
    use crate::time::TimerAllocator;
    use crate::wire::{HelloFacts, ServerFrame};

    #[test]
    fn actions_never_touch_evidence() {
        // Fold discipline (I6) as an executable rule: run every device-level
        // action against a device with real evidence and assert the fold's
        // output is untouched.
        let mut harness = Harness::new();
        let mut device = harness.attached_device();
        harness.feed(
            &mut device,
            Millis(10),
            Input::link(
                LinkId(1),
                LinkEvent::Frame(ServerFrame::hello(1, harness.hello())),
            ),
        );
        let before = device.evidence.clone();

        let device_id = device.id;
        for action in [
            Action::Connect { device: device_id },
            Action::SetName {
                device: device_id,
                name: "Kitchen".to_string(),
            },
            Action::SetAutoconnect {
                device: device_id,
                enabled: true,
            },
            Action::Identify { device: device_id },
            Action::CancelActivity { device: device_id },
        ] {
            let evidence_before = device.evidence.clone();
            harness.feed(&mut device, Millis(20), Input::Action(action.clone()));
            assert_eq!(
                device.evidence.presence, evidence_before.presence,
                "{action:?} moved presence"
            );
            assert_eq!(
                device.evidence.classification, evidence_before.classification,
                "{action:?} moved classification"
            );
            assert_eq!(
                device.evidence.freshness, evidence_before.freshness,
                "{action:?} moved freshness"
            );
        }

        assert_eq!(before.classification, device.evidence.classification);
    }

    #[test]
    fn a_cancel_inside_the_grace_ends_the_activity_without_eviction() {
        let mut harness = Harness::new();
        let mut device = harness.attached_device();
        let id = device.id;
        harness.feed(
            &mut device,
            Millis(0),
            Input::Action(Action::Connect { device: id }),
        );
        harness.feed(
            &mut device,
            Millis(10),
            Input::link(
                LinkId(1),
                LinkEvent::Opened {
                    info: LinkInfo::default(),
                },
            ),
        );
        assert!(device.is_busy());

        harness.feed(
            &mut device,
            Millis(100),
            Input::Action(Action::CancelActivity { device: id }),
        );
        assert!(device.is_busy(), "still winding down");

        harness.feed(
            &mut device,
            Millis(150),
            Input::link(
                LinkId(1),
                LinkEvent::Closed {
                    reason: "cancelled".to_string(),
                },
            ),
        );

        assert!(!device.is_busy());
        assert_eq!(
            device.evidence.last_outcome,
            Some(ActivityOutcome::Cancelled)
        );
        assert!(
            !harness.has_note(|note| matches!(note, JournalNote::ActivityEvicted { .. })),
            "a polite wind-down is not an eviction"
        );
    }

    /// G1 follow-up (2026-08-31): one flaky boot window left a card
    /// stranded at "no response". A silent identify re-asks on its own up
    /// to the configured cap, and the settled card then wears the Retry
    /// escape so a human re-ask never needs a replug.
    #[test]
    fn a_silent_identify_retries_itself_then_settles_with_a_retry_escape() {
        let mut harness = Harness::new();
        let mut device = harness.attached_device();
        let id = device.id;
        harness.feed(
            &mut device,
            Millis(0),
            Input::Action(Action::Connect { device: id }),
        );
        harness.feed(
            &mut device,
            Millis(10),
            Input::link(
                LinkId(1),
                LinkEvent::Opened {
                    info: LinkInfo::default(),
                },
            ),
        );

        // Silence through every window: each deadline settles Failed and
        // re-spawns, until the cap. now advances past each settle time.
        let deadline = harness.config.identify_deadline_ms;
        let mut now = Millis(10);
        for round in 0..=harness.config.identify_auto_retries {
            assert!(device.is_busy(), "identify round {round} should be running");
            let armed = device.armed_timer.expect("a timer must be armed");
            let timer = crate::time::TimerId {
                scope: device.scope(),
                seq: armed.0,
            };
            now = now.plus_ms(deadline + 1);
            harness.feed(&mut device, now, Input::Event(Event::TimerFired { timer }));
        }

        assert!(!device.is_busy(), "the cap ends the auto-retry loop");
        assert_eq!(
            device.evidence.last_outcome,
            Some(ActivityOutcome::Failed {
                message: "no response from the device".to_string()
            })
        );
        let view = crate::view::device_view(&device, now);
        assert!(
            view.escapes.contains(&crate::view::Escape::Retry),
            "the settled-silent card offers the re-ask: {:?}",
            view.escapes
        );

        // The human retry resets the budget and asks again.
        let commands = harness.feed(
            &mut device,
            now.plus_ms(10),
            Input::Action(Action::Identify { device: id }),
        );
        assert!(device.is_busy());
        assert!(commands.iter().any(|command| matches!(
            command,
            Command::Link {
                command: LinkCommand::SendFrame(_),
                ..
            }
        )));
    }

    #[test]
    fn a_cancel_that_hangs_past_the_grace_is_evicted_with_recovery() {
        let mut harness = Harness::new();
        let mut device = harness.attached_device();
        let id = device.id;
        harness.feed(
            &mut device,
            Millis(0),
            Input::Action(Action::Connect { device: id }),
        );
        harness.feed(
            &mut device,
            Millis(10),
            Input::link(
                LinkId(1),
                LinkEvent::Opened {
                    info: LinkInfo::default(),
                },
            ),
        );
        harness.feed(
            &mut device,
            Millis(100),
            Input::Action(Action::CancelActivity { device: id }),
        );

        // The port never reports the close. Supervision takes over.
        let grace = harness.config.cancel_grace_ms;
        let armed = device.armed_timer.expect("a timer must be armed");
        let commands = device.handle(
            Millis(100 + grace + 1),
            &Input::Event(Event::TimerFired {
                timer: crate::time::TimerId {
                    scope: device.scope(),
                    seq: armed.0,
                },
            }),
            &mut harness.ctx(),
        );

        assert!(!device.is_busy(), "eviction is bounded by removal");
        assert!(harness.has_note(|note| matches!(
            note,
            JournalNote::ActivityEvicted {
                reason: EvictionReason::CancelGraceExpired,
                ..
            }
        )));
        assert!(
            commands.iter().any(|command| matches!(
                command,
                Command::Link {
                    command: LinkCommand::Open { .. },
                    ..
                }
            )),
            "recovery rebuilds the link: {commands:?}"
        );
    }

    #[test]
    fn losing_the_link_mid_activity_evicts_and_refolds() {
        let mut harness = Harness::new();
        let mut device = harness.attached_device();
        let id = device.id;
        harness.feed(
            &mut device,
            Millis(0),
            Input::Action(Action::Connect { device: id }),
        );
        assert!(device.is_busy());

        harness.feed(
            &mut device,
            Millis(50),
            Input::Event(Event::LinkDetached { link: LinkId(1) }),
        );

        assert!(!device.is_busy());
        assert!(device.link().is_none());
        assert_eq!(device.status(), DeviceStatus::Offline);
        assert!(matches!(
            device.evidence.last_outcome,
            Some(ActivityOutcome::Interrupted { .. })
        ));
    }

    #[test]
    fn a_stale_timer_generation_is_dropped_without_a_journal_line() {
        let mut harness = Harness::new();
        let mut device = harness.attached_device();
        let id = device.id;
        harness.feed(
            &mut device,
            Millis(0),
            Input::Action(Action::Connect { device: id }),
        );
        let before = harness.journal_len();

        let commands = device.handle(
            Millis(10),
            &Input::Event(Event::TimerFired {
                timer: crate::time::TimerId {
                    scope: device.scope(),
                    seq: 9_999,
                },
            }),
            &mut harness.ctx(),
        );

        assert!(commands.is_empty());
        assert_eq!(harness.journal_len(), before, "no timeline churn");
    }

    #[test]
    fn status_is_total_over_presence_classification_and_activity() {
        let mut harness = Harness::new();
        let mut device = harness.attached_device();
        assert_eq!(device.status(), DeviceStatus::Attached);

        let id = device.id;
        harness.feed(
            &mut device,
            Millis(0),
            Input::Action(Action::Connect { device: id }),
        );
        assert_eq!(device.status(), DeviceStatus::Busy);

        harness.feed(
            &mut device,
            Millis(10),
            Input::link(
                LinkId(1),
                LinkEvent::Opened {
                    info: LinkInfo::default(),
                },
            ),
        );
        harness.feed(
            &mut device,
            Millis(20),
            Input::link(
                LinkId(1),
                LinkEvent::Frame(ServerFrame::hello(1, harness.hello())),
            ),
        );
        assert_eq!(device.status(), DeviceStatus::Ready);
    }

    struct Harness {
        config: RosterConfig,
        journal: Journal,
        timers: TimerAllocator,
    }

    impl Harness {
        fn new() -> Self {
            Self {
                config: RosterConfig::default(),
                journal: Journal::new(256),
                timers: TimerAllocator::default(),
            }
        }

        fn ctx(&mut self) -> ModelCtx<'_> {
            ModelCtx {
                config: &self.config,
                journal: &mut self.journal,
                timers: &mut self.timers,
            }
        }

        fn hello(&self) -> HelloFacts {
            HelloFacts {
                proto: self.config.expected_proto,
                ..Default::default()
            }
        }

        fn attached_device(&mut self) -> Device {
            let mut device = Device::new(DeviceId(1), IdentityChain::default());
            let attached = Event::LinkAttached {
                link: LinkId(1),
                info: LinkInfo::default(),
            };
            device.fold_only(Millis(0), &attached, &mut self.ctx());
            device
        }

        fn feed(&mut self, device: &mut Device, now: Millis, input: Input) -> Vec<Command> {
            device.handle(now, &input, &mut self.ctx())
        }

        fn journal_len(&self) -> usize {
            self.journal.len()
        }

        fn has_note(&self, predicate: impl Fn(&JournalNote) -> bool) -> bool {
            self.journal.notes().any(|(_, note)| predicate(note))
        }
    }
}
