//! **The project stage, as `lp-cli` runs it** (M3 P11): after an update (or
//! an install) leaves a board running a build, bring the projects it stores
//! to the project format that build's engine reads.
//!
//! One function, [`finish_board_projects`], used by every `lp-cli` host that
//! ends with a board up to date: `firmware install` (after the USB write and
//! the hello check) and `emu run --ota-offer` (after the update driver ends
//! `UpToDate`). It runs `lpa-update`'s sans-IO `ProjectStage` through
//! `lpa_client::run_project_stage` — the one edge Studio runs it through
//! too — over the host's own channel-1 client, at the tier the link already
//! holds (the hello's `auth.granted`: edit on a trusted cable, a login's or
//! a key's tier on a radio link). It does nothing when every stored project
//! is current: the stage lists and reads, and decides.
//!
//! Its words are lines for a console, each starting `projects:`; the caller
//! prints them (with its own prefix, if it has one) and fails the command
//! when [`BoardProjects::ok`] is false.

use lpa_client::push_files::LpClientSink;
use lpa_client::{ClientIo, LpClient, run_project_stage};
use lpa_update::{ProjectStage, ProjectStageReport, StageEnd};
use lpc_wire::{ServerHello, WIRE_PROTO_VERSION};

/// The hello the board said on the link the stage will run over.
#[derive(Clone, Debug)]
pub enum BoardHello {
    /// A hello this lp-cli decodes, on its own wire.
    Ours(Box<ServerHello>),
    /// A hello on another wire: only its `proto` was read
    /// (`lpc_wire::hello_proto`), or it decoded and named another `proto`.
    OtherWire { proto: u32 },
}

impl BoardHello {
    /// The hello as this lp-cli reads it: [`Self::OtherWire`] for any hello
    /// whose `proto` is not this build's.
    pub fn from_hello(hello: ServerHello) -> Self {
        if hello.proto == WIRE_PROTO_VERSION {
            Self::Ours(Box::new(hello))
        } else {
            Self::OtherWire { proto: hello.proto }
        }
    }
}

/// What finishing a board's projects did: the lines to print, and whether
/// the board was left with every project at its engine's format.
#[derive(Clone, Debug)]
pub struct BoardProjects {
    pub lines: Vec<String>,
    /// Every stored project is at the engine's format (or there are none),
    /// and the startup project, when the stage loaded it, started.
    pub ok: bool,
}

/// Bring the board's stored projects to its engine's project format over
/// `client`, the channel-1 client of the link `hello` was said on.
/// `startup_refused`: the board said it refused its startup project for its
/// format (`ProjectStage::startup_refused`, DD27).
pub async fn finish_board_projects<Io: ClientIo>(
    client: &mut LpClient<Io>,
    hello: &BoardHello,
    startup_refused: bool,
) -> BoardProjects {
    let hello = match hello {
        BoardHello::Ours(hello) => hello,
        BoardHello::OtherWire { proto } => {
            return BoardProjects {
                lines: vec![other_wire_line(*proto)],
                ok: false,
            };
        }
    };
    let mut stage = ProjectStage::new(hello.build.project_format, hello.auth.granted)
        .startup_refused(startup_refused);
    let mut sink = LpClientSink::new(client);
    let report = run_project_stage(&mut sink, &mut stage, &mut |_| {}).await;
    BoardProjects {
        lines: report_lines(&report),
        ok: report_ok(&report),
    }
}

/// The line for a board whose engine speaks another wire than this lp-cli.
pub fn other_wire_line(proto: u32) -> String {
    format!(
        "projects: this board's firmware speaks wire {proto} and this lp-cli speaks wire \
         {WIRE_PROTO_VERSION}, so it can't check the board's projects. If they need upgrading, \
         finish them from a Studio or lp-cli that speaks wire {proto}."
    )
}

/// One line per project the stage touched or could not, and one for how it
/// ended when that is not plain success.
pub fn report_lines(report: &ProjectStageReport) -> Vec<String> {
    let format = report.engine_format;
    let mut lines = Vec::new();
    for upgraded in &report.upgraded {
        lines.push(format!(
            "projects: upgraded “{}” to format {format}",
            project_name(&upgraded.project)
        ));
    }
    for refused in &report.refused {
        lines.push(format!(
            "projects: couldn't upgrade “{}”: {}",
            project_name(&refused.project),
            refused.why
        ));
    }
    if let (Some(Err(why)), Some(startup)) = (&report.load, &report.startup) {
        lines.push(format!(
            "projects: “{}” is at format {format} but didn't start: {why}",
            project_name(startup)
        ));
    }
    match &report.end {
        StageEnd::Finished => {
            if report.upgraded.is_empty() && report.refused.is_empty() {
                lines.push(if report.untouched.is_empty() {
                    "projects: the board stores none".to_string()
                } else {
                    format!("projects: every project is at format {format}")
                });
            }
        }
        StageEnd::NeedsEdit => {
            for project in &report.pending {
                lines.push(format!(
                    "projects: “{}” needs upgrading; this link can't edit the board",
                    project_name(project)
                ));
            }
        }
        StageEnd::HostTooOld(error) => lines.push(format!("projects: {error}")),
        StageEnd::NotAtomic => lines.push(
            "projects: this board's files can't be rewritten safely, so its projects were left \
             as they are"
                .to_string(),
        ),
        StageEnd::Interrupted(why) => {
            lines.push(format!(
                "projects: the link went before the projects were finished ({why}); run this \
                 again to finish them"
            ));
            for project in &report.pending {
                lines.push(format!(
                    "projects: “{}” still needs upgrading",
                    project_name(project)
                ));
            }
        }
    }
    lines
}

/// Whether the board was left with every project at its engine's format and
/// its startup project, when loaded, running.
pub fn report_ok(report: &ProjectStageReport) -> bool {
    report.end == StageEnd::Finished
        && report.refused.is_empty()
        && !matches!(report.load, Some(Err(_)))
}

/// A listed project's name: the last segment of its path
/// (`projects/eaves` → `eaves`).
fn project_name(project: &str) -> &str {
    let trimmed = project.trim_end_matches('/');
    trimmed.rsplit('/').next().unwrap_or(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpa_update::{RefusedProject, UpgradedProject};

    #[test]
    fn an_upgraded_and_a_refused_project_each_get_a_line() {
        let report = ProjectStageReport {
            engine_format: 12,
            startup: Some("projects/eaves".into()),
            upgraded: vec![UpgradedProject {
                project: "projects/eaves".into(),
                report: lpa_upgrade::UpgradeReport::default(),
            }],
            refused: vec![RefusedProject {
                project: "projects/odd".into(),
                why: "its fixture names no output".into(),
            }],
            load: Some(Ok(())),
            ..ProjectStageReport::default()
        };
        assert_eq!(
            report_lines(&report),
            [
                "projects: upgraded “eaves” to format 12",
                "projects: couldn't upgrade “odd”: its fixture names no output",
            ]
        );
        assert!(!report_ok(&report), "a refused project is not finished");
    }

    #[test]
    fn a_link_below_edit_names_each_project_it_cannot_upgrade() {
        let report = ProjectStageReport {
            engine_format: 12,
            pending: vec!["projects/eaves".into()],
            end: StageEnd::NeedsEdit,
            ..ProjectStageReport::default()
        };
        assert_eq!(
            report_lines(&report),
            ["projects: “eaves” needs upgrading; this link can't edit the board"]
        );
        assert!(!report_ok(&report));
    }

    #[test]
    fn a_board_with_current_projects_says_so_in_one_line() {
        let report = ProjectStageReport {
            engine_format: 12,
            untouched: vec!["projects/eaves".into()],
            ..ProjectStageReport::default()
        };
        assert_eq!(
            report_lines(&report),
            ["projects: every project is at format 12"]
        );
        assert!(report_ok(&report));
    }

    #[test]
    fn a_board_on_another_wire_is_sent_to_a_host_that_speaks_it() {
        let line = other_wire_line(WIRE_PROTO_VERSION - 1);
        assert!(
            line.contains(&format!(
                "from a Studio or lp-cli that speaks wire {}",
                WIRE_PROTO_VERSION - 1
            )),
            "{line}"
        );
    }
}
