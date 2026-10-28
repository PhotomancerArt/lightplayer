//! **The project-stage controller** (M3 P11): runs `lpa-update`'s project
//! stage on a board's channel 1, so the projects a board stores reach the
//! format its engine reads, with no click.
//!
//! Like the network controller it owns no IO: it reads the device model's
//! evidence, spawns each stage on the link's SHARED wire
//! (`DeviceEffects::conversation_io`) through the one edge every host uses
//! (`lpa_client::run_project_stage`), and every stage posts its progress and
//! its end back onto the actor's queue ([`ProjectStageCommand`], invariant
//! I7).
//!
//! **When a stage runs:**
//!
//! | Board | Here |
//! |---|---|
//! | its Update waits in its project stage (the update host's [`super::update_host::UpdateHost::projects_wanted`]) | runs on the leg's link; its end ends the update |
//! | no activity, a hello on this Studio's wire saying its engine's project format, a link with a tier | runs once per link window (and format) — Q107/Q108: a cut inside the update's window, an update by a host that predates the stage, a USB flash |
//! | a wireless link with no tier yet, another activity, the wire borrowed or held by the editor | waits for the next drive |
//!
//! The stage itself decides what to do: it lists and reads, writes only at
//! edit, and does nothing when every project is current, so a run on a
//! board with nothing to upgrade says nothing. At play (Q103) it reads and
//! writes nothing, and the card says someone who can edit can finish.
//!
//! **The board's report of a refused startup project** (DD27) is not on the
//! wire: the engine logs the refusal and says nothing on channel 1, so every
//! stage here runs with `startup_refused(false)` — a current startup project
//! is never loaded by the stage. See the phase's Implementation Result.

use std::collections::BTreeMap;
use std::rc::Rc;

use lpa_client::push_files::LpClientSink;
use lpa_client::{ProjectStageProgress, run_project_stage};
use lpa_devices::Roster;
use lpa_devices::identity::DeviceId;
use lpa_devices::link::LinkId;
use lpa_update::{ProjectStage, ProjectStageReport, StageEnd};
use lpc_access::Tier;

use super::device_effects::{DeviceEffects, DeviceTaskFuture};
use super::device_update_route::UpdateLink;
use super::project_stage_command::ProjectStageCommand;
use super::project_stage_status::{ProjectStageStatus, project_name};
use crate::app::studio::studio_command::StudioCommand;
use crate::app::studio::studio_view_channel::CommandSender;

/// See the module doc.
#[derive(Default)]
pub struct ProjectStageController {
    /// The stage in flight on each device.
    runs: BTreeMap<DeviceId, StageRun>,
    /// What each board's card says of its stage, when anything.
    status: BTreeMap<DeviceId, ProjectStageStatus>,
    /// The link window (and engine format) a no-click stage last ran on,
    /// per device: it runs once per window.
    tried: BTreeMap<DeviceId, StageKey>,
    next_generation: u64,
    tx: Option<CommandSender>,
    spawner: Option<Rc<dyn Fn(DeviceTaskFuture)>>,
}

/// One stage in flight.
#[derive(Clone, Debug)]
struct StageRun {
    generation: u64,
    key: StageKey,
    /// It is an update's last stage: its end ends the update.
    for_update: bool,
}

/// One link window of one board, at the format its engine reads.
#[derive(Clone, Debug, PartialEq, Eq)]
struct StageKey {
    link: LinkId,
    window: Option<u64>,
    engine_format: u32,
}

/// What a finished stage asks of the studio controller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ProjectStageFollowUp {
    /// End `device`'s update with these lines.
    UpdateDone {
        device: DeviceId,
        lines: Vec<String>,
    },
    /// `device`'s update stage moved.
    UpdateProgress {
        device: DeviceId,
        done: u32,
        total: u32,
    },
    /// Say these lines on `device`'s terminal (no activity runs).
    Say {
        device: DeviceId,
        lines: Vec<String>,
    },
}

impl ProjectStageController {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn set_command_sender(&mut self, tx: CommandSender) {
        self.tx = Some(tx);
    }

    pub(crate) fn set_spawner(&mut self, spawner: Rc<dyn Fn(DeviceTaskFuture)>) {
        self.spawner = Some(spawner);
    }

    /// What `device`'s card says of its project stage, when anything.
    pub fn status(&self, device: DeviceId) -> Option<&ProjectStageStatus> {
        self.status.get(&device)
    }

    // --- the drive --------------------------------------------------------

    /// Start the stages due now (see the module doc). `granted` is the tier
    /// a wireless link holds (the access controller's); `updates` the
    /// devices whose update waits for its project stage, with the leg's
    /// link. Returns the updates that cannot run a stage at all (their
    /// board's hello says no project format): they end now.
    pub(crate) fn drive(
        &mut self,
        roster: &Roster,
        effects: &DeviceEffects,
        granted: impl Fn(DeviceId) -> Option<Tier>,
        updates: &[(DeviceId, LinkId)],
    ) -> Vec<ProjectStageFollowUp> {
        let mut follow_ups = Vec::new();
        // A board with a card, and a link still pending adoption (whose
        // update runs all the same).
        let boards = roster
            .devices()
            .iter()
            .map(|device| Board {
                device: device.id,
                evidence: &device.evidence,
                endpoint: device.identity.endpoint.as_ref(),
                busy: device.activity.is_some(),
            })
            .chain(roster.pending().iter().map(|pending| Board {
                device: pending.device_id(),
                evidence: pending.evidence(),
                endpoint: Some(&pending.info.endpoint),
                busy: pending.activity_kind().is_some(),
            }));
        let mut live = Vec::new();
        for board in boards {
            let device = board.device;
            live.push(device);
            let update_link = updates
                .iter()
                .find(|(wanted, _)| *wanted == device)
                .map(|(_, link)| *link);
            match due(&board, granted(device), update_link) {
                Due::No => {}
                Due::NoFormat => {
                    if update_link.is_some() && !self.runs.contains_key(&device) {
                        // A board that does not say its engine's format (a
                        // hello on another wire, or one that predates the
                        // field): nothing can be checked here.
                        follow_ups.push(ProjectStageFollowUp::UpdateDone {
                            device,
                            lines: Vec::new(),
                        });
                    }
                }
                Due::Yes { key, tier } => {
                    let for_update = update_link.is_some();
                    if self.runs.contains_key(&device)
                        || (!for_update && self.tried.get(&device) == Some(&key))
                    {
                        continue;
                    }
                    if self.dispatch(device, key.clone(), tier, for_update, effects) {
                        self.tried.insert(device, key);
                    }
                }
            }
        }
        self.runs.retain(|device, _| live.contains(device));
        self.status.retain(|device, _| live.contains(device));
        self.tried.retain(|device, _| live.contains(device));
        follow_ups
    }

    /// Spawn one stage on `key.link`. False when it could not start now.
    fn dispatch(
        &mut self,
        device: DeviceId,
        key: StageKey,
        tier: Tier,
        for_update: bool,
        effects: &DeviceEffects,
    ) -> bool {
        if effects.lens_holds_wire(key.link) || effects.wire_borrowed(key.link) {
            // Another holder has the wire; the next drive tries again.
            return false;
        }
        let (Some(io), Some(spawner), Some(tx)) = (
            effects.conversation_io(key.link),
            self.spawner.clone(),
            self.tx.clone(),
        ) else {
            return false;
        };
        self.next_generation += 1;
        let generation = self.next_generation;
        let engine_format = key.engine_format;
        self.runs.insert(
            device,
            StageRun {
                generation,
                key,
                for_update,
            },
        );
        log::info!(
            "projects: {device:?}'s project stage starts (engine reads format {engine_format})"
        );
        spawner(Box::pin(async move {
            let mut client = io.into_client();
            let mut sink = LpClientSink::new(&mut client);
            // The board says nothing of a refused startup project (module
            // doc), so the stage never loads a current one.
            let mut stage = ProjectStage::new(engine_format, Some(tier)).startup_refused(false);
            let mut last = None;
            let progress_tx = tx.clone();
            let report = run_project_stage(
                &mut sink,
                &mut stage,
                &mut |ProjectStageProgress { done, total }| {
                    let now = (done as u32, total as u32);
                    if total > 0 && last != Some(now) {
                        last = Some(now);
                        progress_tx.send(StudioCommand::ProjectStage(
                            ProjectStageCommand::Progress {
                                device,
                                generation,
                                done: now.0,
                                total: now.1,
                            },
                        ));
                    }
                },
            )
            .await;
            tx.send(StudioCommand::ProjectStage(ProjectStageCommand::Done {
                device,
                generation,
                report,
            }));
        }));
        true
    }

    // --- commands -----------------------------------------------------------

    /// Apply one command from a stage. `roster` says whether a stage cut
    /// short still has its link (see [`Self::done`]).
    pub(crate) fn apply(
        &mut self,
        command: ProjectStageCommand,
        roster: &Roster,
    ) -> Option<ProjectStageFollowUp> {
        match command {
            ProjectStageCommand::Progress {
                device,
                generation,
                done,
                total,
            } => {
                let run = self.runs.get(&device)?;
                if run.generation != generation {
                    return None;
                }
                self.status
                    .insert(device, ProjectStageStatus::Running { done, total });
                run.for_update
                    .then_some(ProjectStageFollowUp::UpdateProgress {
                        device,
                        done,
                        total,
                    })
            }
            ProjectStageCommand::Done {
                device,
                generation,
                report,
            } => {
                let run = self.runs.get(&device)?;
                if run.generation != generation {
                    return None;
                }
                let run = self.runs.remove(&device)?;
                self.done(device, run, &report, roster)
            }
        }
    }

    fn done(
        &mut self,
        device: DeviceId,
        run: StageRun,
        report: &ProjectStageReport,
        roster: &Roster,
    ) -> Option<ProjectStageFollowUp> {
        log::info!(
            "projects: {device:?}'s project stage ended: {:?}",
            report.end
        );
        match ProjectStageStatus::after(report) {
            Some(status) => self.status.insert(device, status),
            None => self.status.remove(&device),
        };
        let lines = stage_lines(report);
        if !run.for_update {
            return (!lines.is_empty()).then_some(ProjectStageFollowUp::Say { device, lines });
        }
        // An update's stage cut short by its link: the leg ends with the
        // link, and the next leg runs the stage again from the board. Cut
        // short on a link that is still there, it would never run again on
        // this window: the update ends, and the next connection finishes.
        let evidence = roster
            .device(device)
            .map(|found| &found.evidence)
            .or_else(|| {
                roster
                    .pending()
                    .iter()
                    .find(|pending| pending.device_id() == device)
                    .map(|pending| pending.evidence())
            });
        let link_gone = !evidence.is_some_and(|evidence| {
            evidence.presence.is_open() && evidence.link() == Some(run.key.link)
        });
        if matches!(report.end, StageEnd::Interrupted(_)) && link_gone {
            return None;
        }
        Some(ProjectStageFollowUp::UpdateDone { device, lines })
    }
}

/// A board as the drive reads it: a card's device, or a pending link.
struct Board<'a> {
    device: DeviceId,
    evidence: &'a lpa_devices::Evidence,
    endpoint: Option<&'a lpa_devices::identity::EndpointKey>,
    /// An activity runs on it.
    busy: bool,
}

/// Whether a stage is due on a board now.
enum Due {
    No,
    /// The board is reached, but its hello says no engine format to check.
    NoFormat,
    Yes {
        key: StageKey,
        tier: Tier,
    },
}

fn due(board: &Board<'_>, granted: Option<Tier>, update_link: Option<LinkId>) -> Due {
    let evidence = board.evidence;
    let Some(link) = evidence.link().filter(|_| evidence.presence.is_open()) else {
        return Due::No;
    };
    if update_link.is_some_and(|wanted| wanted != link) {
        return Due::No;
    }
    if update_link.is_none() && board.busy {
        return Due::No;
    }
    let Some(hello) = evidence.classification.hello() else {
        return Due::No;
    };
    let Some(engine_format) = hello
        .project_format
        .filter(|_| hello.proto == lpc_wire::WIRE_PROTO_VERSION)
    else {
        return Due::NoFormat;
    };
    // A cable is trusted (edit); a wireless link holds what it was granted,
    // and nothing is asked before it holds anything.
    let wireless = UpdateLink::of_endpoint(board.endpoint).is_wireless();
    let tier = match (wireless, granted) {
        (false, _) => Tier::Edit,
        (true, Some(tier)) => tier,
        (true, None) => return Due::No,
    };
    Due::Yes {
        key: StageKey {
            link,
            window: evidence.window_started_at().map(|at| at.0),
            engine_format,
        },
        tier,
    }
}

/// What a finished stage says on the board's terminal, in Studio's voice:
/// one line naming every project upgraded, one per project refused, and
/// why the rest were left.
pub(crate) fn stage_lines(report: &ProjectStageReport) -> Vec<String> {
    let mut lines = Vec::new();
    let names = |projects: &mut dyn Iterator<Item = &String>| -> String {
        projects
            .map(|project| format!("\u{201c}{}\u{201d}", project_name(project)))
            .collect::<Vec<_>>()
            .join(", ")
    };
    if !report.upgraded.is_empty() {
        lines.push(format!(
            "upgraded {} to project format {}",
            names(&mut report.upgraded.iter().map(|upgraded| &upgraded.project)),
            report.engine_format
        ));
    }
    for refused in &report.refused {
        lines.push(format!(
            "couldn't upgrade \u{201c}{}\u{201d}: {}",
            project_name(&refused.project),
            refused.why
        ));
    }
    if let (Some(Err(why)), Some(startup)) = (&report.load, &report.startup) {
        lines.push(format!(
            "\u{201c}{}\u{201d} didn't start: {why}",
            project_name(startup)
        ));
    }
    match &report.end {
        StageEnd::Finished => {}
        StageEnd::NeedsEdit => lines.push(format!(
            "{} need upgrading for this firmware; someone who can edit this board can finish it",
            names(&mut report.pending.iter())
        )),
        StageEnd::HostTooOld(error) => lines.push(error.to_string()),
        StageEnd::NotAtomic => lines.push(
            "its projects need upgrading, and this board's files can't be rewritten safely over \
             a link"
                .to_string(),
        ),
        StageEnd::Interrupted(why) if !report.pending.is_empty() => lines.push(format!(
            "{} not upgraded yet ({why}); the next connection finishes them",
            names(&mut report.pending.iter())
        )),
        StageEnd::Interrupted(_) => {}
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_update::{RefusedProject, UpgradedProject};

    #[test]
    fn a_finished_stage_names_every_project_it_upgraded_in_one_line() {
        let report = ProjectStageReport {
            engine_format: 12,
            upgraded: vec![upgraded("projects/eaves"), upgraded("projects/dome")],
            refused: vec![RefusedProject {
                project: "projects/old".to_string(),
                why: "a sentence".to_string(),
            }],
            ..ProjectStageReport::default()
        };
        assert_eq!(
            stage_lines(&report),
            [
                "upgraded \u{201c}eaves\u{201d}, \u{201c}dome\u{201d} to project format 12",
                "couldn't upgrade \u{201c}old\u{201d}: a sentence",
            ]
        );
    }

    #[test]
    fn a_stage_with_nothing_to_do_says_nothing() {
        assert!(stage_lines(&ProjectStageReport::default()).is_empty());
    }

    fn upgraded(project: &str) -> UpgradedProject {
        UpgradedProject {
            project: project.to_string(),
            report: lpa_upgrade::UpgradeReport::new(11),
        }
    }
}
