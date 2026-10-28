//! **The update's project stage, end to end** (M3 P11): Studio's own
//! controller, its update host and its project-stage controller, over the
//! model board whose channel 1 is a REAL `lpa-server` on the tree store
//! (simulated NOR flash, the C6's and the S3's store), holding real
//! format-11 projects from `lpa-upgrade`'s corpus.
//!
//! The model board's engine X reads format 11 and Y format 12 (its hello
//! says so); the server behind channel 1 is this build's either way, which
//! refuses a format-11 project and answers the stage's reads, batches and
//! load. What each test asserts it reaches through the controller — the
//! card's standing and words, the terminal's lines, the update's outcome —
//! and the board's own files, read back off its flash.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use lp_gfx_lpvm::TargetLpvmGraphics;
use lp_nor_sim::{NorFlashSim, NorGeometry};
use lp_tree_store::{LpFsTree, SoftSha256, StoreConfig, TreeStore};
use lpa_server::{LpGraphics, LpServer};
use lpc_model::server::server_config::ServerConfig;
use lpc_model::{AsLpPath, PROJECT_FORMAT_VERSION};
use lpc_shared::output::MemoryOutputProvider;
use lpc_shared::transport::Incoming;
use lpc_wire::{ClientMessage, ClientRequest, FsRequest, TransportError, WireServerMessage};

use super::*;
use crate::ProjectsHeldBack;

/// The startup project: `eaves-wired` (a patch file the step removes).
const STARTUP: &str = "eaves";
/// A second project: `fiber-headband` (hand-placed lamps; a map2d added).
const SECOND: &str = "headband";

// ---------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------

/// An update X → Y of a board holding format-11 projects ends with the
/// project stage: the card says it is upgrading them, the terminal names
/// each project upgraded, every project is at format 12 on the board's
/// flash, the startup project runs — and only then is the update done.
#[test]
fn an_update_ends_with_every_project_upgraded_and_the_startup_project_running() {
    let mut bench = Bench::new(board_with_projects(old_projects()), Some(y()));
    bench.cache_engine(&x());
    let device = bench.connect_device();
    bench.press_update(device);

    bench.run_until_update_ends(device);
    bench.assert_runs(&y());
    assert_eq!(bench.outcome(device), Some(UpdateOutcomeFacts::UpToDate));
    assert!(
        bench.said("upgrading its projects to the new format"),
        "lines: {:#?}",
        bench.lines
    );
    assert!(
        bench.said("upgraded \u{201c}eaves\u{201d}, \u{201c}headband\u{201d} to project format 12"),
        "lines: {:#?}",
        bench.lines
    );
    let channel = channel_of(&bench);
    assert_eq!(
        channel.project_format(STARTUP),
        Some(PROJECT_FORMAT_VERSION)
    );
    assert_eq!(channel.project_format(SECOND), Some(PROJECT_FORMAT_VERSION));
    assert!(!channel.has_file(STARTUP, "fixture.patch.json"));
    assert_eq!(channel.running(), [format!("/projects/{STARTUP}")]);
    drop(channel);
    assert_eq!(bench.presses, 1, "one press, no other click");
    assert!(matches!(
        bench.standing(device),
        UpdateStanding::UpToDate { .. }
    ));
}

/// While the stage runs, the card says so in plain words, counted in
/// projects, and the update is still running.
#[test]
fn the_card_says_its_projects_are_being_upgraded_one_of_two() {
    let mut bench = Bench::new(board_with_projects(old_projects()), Some(y()));
    bench.cache_engine(&x());
    let device = bench.connect_device();
    bench.press_update(device);

    bench.run_until("the card to count the projects", |bench| {
        matches!(
            bench.standing(device),
            UpdateStanding::UpgradingProjects { total: 2, .. }
        )
    });
    let standing = bench.standing(device);
    let words = crate::app::devices::device_update_words::update_words(&standing)
        .expect("words for the stage");
    assert!(
        words
            .sentence
            .starts_with("Upgrading its projects to the new format — "),
        "{}",
        words.sentence
    );
    assert!(
        words.line == "Upgrading projects · 1 of 2" || words.line == "Upgrading projects · 2 of 2",
        "{}",
        words.line
    );
    assert!(
        bench.outcome(device).is_none(),
        "the update is not done yet"
    );
    bench.run_until_update_ends(device);
    assert_eq!(bench.outcome(device), Some(UpdateOutcomeFacts::UpToDate));
}

/// The link drops mid-stage (the second project's commit never reaches
/// the board): the leg ends like any other, and when the board is back the
/// stage runs again from the board — the startup project is already done,
/// the other is finished — and the update ends up to date, with no click.
#[test]
fn a_link_dropped_mid_stage_finishes_on_the_next_link_with_no_click() {
    let mut channel = old_projects();
    channel.drop_at_commit = Some(2);
    let mut bench = Bench::new(board_with_projects(channel), Some(y()));
    bench.cache_engine(&x());
    let device = bench.connect_device();
    bench.press_update(device);

    bench.run_until("the link to drop mid-stage", |bench| {
        bench.board().unplugged
    });
    {
        let channel = channel_of(&bench);
        assert_eq!(
            channel.project_format(STARTUP),
            Some(PROJECT_FORMAT_VERSION)
        );
        assert_eq!(
            channel.project_format(SECOND),
            Some(PROJECT_FORMAT_VERSION - 1),
            "the dropped batch never landed: wholly old"
        );
    }
    assert!(
        bench.outcome(device).is_none(),
        "the update waits for the board"
    );
    bench.steps(2_000);
    bench.board_mut().replug();
    bench.run_until_update_ends(device);

    assert_eq!(bench.outcome(device), Some(UpdateOutcomeFacts::UpToDate));
    let channel = channel_of(&bench);
    assert_eq!(
        channel.project_format(STARTUP),
        Some(PROJECT_FORMAT_VERSION)
    );
    assert_eq!(channel.project_format(SECOND), Some(PROJECT_FORMAT_VERSION));
    drop(channel);
    assert_eq!(bench.presses, 1, "no click after the drop");
    assert!(
        bench.said("upgraded \u{201c}headband\u{201d} to project format 12"),
        "lines: {:#?}",
        bench.lines
    );
}

/// Q107/Q108: a board found already running Y with older projects (a cut
/// inside the window, an update by a host that predates the stage, a USB
/// flash) has them upgraded on connect over a link holding edit, with no
/// click and no update at all.
#[test]
fn a_board_found_with_older_projects_upgrades_them_on_connect_with_no_click() {
    let mut board = board_with_projects(old_projects());
    board.rig = BoardRig::new(
        FakeBoard::flashed_with(vec![y(), x()], 0, REGION),
        AccessFacts::from_store(None),
        SessionConfig {
            entropy: Some(|b: &mut [u8]| b.fill(0x3c)),
            ..SessionConfig::default()
        },
    )
    .expect("the board boots");
    let mut bench = Bench::new(board, Some(y()));
    let device = bench.connect_device();

    bench.run_until("the projects to be upgraded", |bench| {
        let channel = channel_of(bench);
        channel.project_format(STARTUP) == Some(PROJECT_FORMAT_VERSION)
            && channel.project_format(SECOND) == Some(PROJECT_FORMAT_VERSION)
    });
    bench.run_until("the terminal to say so", |bench| {
        bench.said("upgraded \u{201c}eaves\u{201d}, \u{201c}headband\u{201d} to project format 12")
    });
    assert_eq!(bench.presses, 0, "no click");
    assert_eq!(bench.update_starts, 0, "no update ran");
    assert_eq!(
        channel_of(&bench).running(),
        [format!("/projects/{STARTUP}")]
    );
    bench.run_until("the card to settle on up to date", |bench| {
        matches!(bench.standing(device), UpdateStanding::UpToDate { .. })
    });
}

/// Q104: a project the step refuses is named with the step's own sentence
/// and left as it is; the others are upgraded, and the card says which one
/// could not be.
#[test]
fn a_refused_project_is_named_and_the_others_are_upgraded() {
    let mut channel = old_projects();
    channel.write_project("quad", &upgrade_tests().join("refused/quad-wire-oracle"));
    let mut board = board_with_projects(channel);
    board.rig = BoardRig::new(
        FakeBoard::flashed_with(vec![y(), x()], 0, REGION),
        AccessFacts::from_store(None),
        SessionConfig {
            entropy: Some(|b: &mut [u8]| b.fill(0x3c)),
            ..SessionConfig::default()
        },
    )
    .expect("the board boots");
    let mut bench = Bench::new(board, Some(y()));
    let device = bench.connect_device();

    bench.run_until("the card to name the refused project", |bench| {
        matches!(
            bench.standing(device),
            UpdateStanding::ProjectsUnfinished {
                why: ProjectsHeldBack::Refused { .. },
                ..
            }
        )
    });
    let standing = bench.standing(device);
    let UpdateStanding::ProjectsUnfinished {
        why: ProjectsHeldBack::Refused { project, why },
        ..
    } = &standing
    else {
        unreachable!()
    };
    assert_eq!(project, "quad");
    assert!(
        why.contains("`panel` is cut"),
        "the step's own sentence: {why}"
    );
    let words = crate::app::devices::device_update_words::update_words(&standing)
        .expect("words for a refused project");
    assert_eq!(
        words.sentence,
        format!("Couldn't upgrade \u{201c}quad\u{201d}: {why}")
    );
    assert!(bench.said_starting("couldn't upgrade \u{201c}quad\u{201d}: "));
    let channel = channel_of(&bench);
    assert_eq!(
        channel.project_format(STARTUP),
        Some(PROJECT_FORMAT_VERSION)
    );
    assert_eq!(channel.project_format(SECOND), Some(PROJECT_FORMAT_VERSION));
    assert_eq!(
        channel.project_format("quad"),
        Some(PROJECT_FORMAT_VERSION - 1)
    );
}

// ---------------------------------------------------------------------
// The board's channel 1
// ---------------------------------------------------------------------

/// A model board running X (format 11) and able to boot Y (format 12),
/// whose channel 1 is `channel`.
fn board_with_projects(channel: ChannelOne) -> Board {
    let mut board = Board::running_x();
    board.channel1 = Some(Box::new(channel));
    board
}

/// The board's channel 1 holding two format-11 projects, `eaves` the
/// startup choice, as engine Y finds them (it refused `eaves` at boot).
fn old_projects() -> ChannelOne {
    let mut channel = ChannelOne::new([(x().version, 11), (y().version, PROJECT_FORMAT_VERSION)]);
    channel.write_project(STARTUP, &corpus("v11/eaves-wired"));
    channel.write_project(SECOND, &corpus("v11/fiber-headband"));
    channel.write(
        ServerConfig::PATH,
        format!("{{\"startup_project\":\"{STARTUP}\"}}").as_bytes(),
    );
    channel.boot();
    channel
}

/// The board's channel 1, borrowed off the bench.
fn channel_of(bench: &Bench) -> std::cell::Ref<'_, ChannelOne> {
    std::cell::Ref::map(bench.board(), |board| {
        board
            .channel1
            .as_deref()
            .expect("the board has a channel 1")
    })
}

/// A board's channel 1: this build's server over a tree store on simulated
/// NOR flash, answering one request at a time.
pub(super) struct ChannelOne {
    server: LpServer,
    transport: VecTransport,
    /// The project format each build's engine reads, by version: what the
    /// board's hello says.
    formats: BTreeMap<String, u32>,
    /// Drop the link instead of forwarding the `n`th `CommitBatch` (the
    /// batch never lands; the board drops a batch whose link closes).
    pub(super) drop_at_commit: Option<u32>,
    commits: u32,
    drop_now: bool,
}

impl ChannelOne {
    fn new(formats: impl IntoIterator<Item = (String, u32)>) -> Self {
        let mut flash = NorFlashSim::new(NorGeometry::c6(64));
        TreeStore::format(&mut flash, SoftSha256, StoreConfig::default())
            .map_err(|(error, ..)| error)
            .expect("format the store");
        let store = match TreeStore::mount(flash, SoftSha256, StoreConfig::default()) {
            Ok(store) => store,
            Err((error, ..)) => panic!("mount: {error:?}"),
        };
        let server = LpServer::new(
            Rc::new(RefCell::new(MemoryOutputProvider::new())),
            Box::new(LpFsTree::new(store)),
            "projects".as_path(),
            None,
            None,
            Arc::new(TargetLpvmGraphics::new(lpa_server::DEVICE_SHADER_FRONTEND))
                as Arc<dyn LpGraphics>,
        );
        Self {
            server,
            transport: VecTransport::default(),
            formats: formats.into_iter().collect(),
            drop_at_commit: None,
            commits: 0,
            drop_now: false,
        }
    }

    /// The project format the engine of `version` reads.
    pub(super) fn engine_format(&self, version: &str) -> Option<u32> {
        self.formats.get(version).copied()
    }

    /// One `M!` request line, answered as `Passthrough` events.
    pub(super) fn answer(&mut self, line: &str) -> Vec<LinkEvent> {
        let json = line.strip_prefix("M!").unwrap_or(line);
        let Ok(message) = lpc_wire::json::from_str::<ClientMessage>(json) else {
            return Vec::new();
        };
        if matches!(
            message.msg,
            ClientRequest::Filesystem(FsRequest::CommitBatch)
        ) {
            self.commits += 1;
            if self.drop_at_commit == Some(self.commits) {
                // The link goes before the commit arrives: the server drops
                // the open batch, as a board does when a link closes.
                self.drop_now = true;
                self.request(ClientRequest::Filesystem(FsRequest::AbortBatch));
                return Vec::new();
            }
        }
        let id = message.id;
        self.transport.sent.clear();
        block_on(self.server.tick_and_send(
            16,
            vec![Incoming::primary(message)],
            &mut self.transport,
        ))
        .expect("the server ticks");
        self.transport
            .sent
            .drain(..)
            .filter(|reply| reply.id == id)
            .map(|reply| LinkEvent::Passthrough {
                request_id: id as u32,
                line: format!(
                    "M!{}",
                    lpc_wire::json::to_string(&reply).expect("a reply encodes")
                ),
            })
            .collect()
    }

    /// Whether the link should drop now (once).
    pub(super) fn take_drop(&mut self) -> bool {
        core::mem::take(&mut self.drop_now)
    }

    /// The boot's startup load: refused (format 11), nothing runs.
    fn boot(&mut self) {
        let path = format!("/projects/{STARTUP}");
        assert!(
            self.server
                .load_startup_project(path.as_str().as_path())
                .is_err(),
            "a format-11 startup project is refused by this build's engine"
        );
    }

    fn request(&mut self, msg: ClientRequest) {
        self.transport.sent.clear();
        block_on(self.server.tick_and_send(
            16,
            vec![Incoming::primary(ClientMessage { id: 9, msg })],
            &mut self.transport,
        ))
        .expect("the server ticks");
    }

    fn write(&mut self, path: &str, bytes: &[u8]) {
        self.server
            .base_fs()
            .write_file(path.as_path(), bytes)
            .expect("write");
    }

    fn write_project(&mut self, name: &str, dir: &Path) {
        for (path, bytes) in read_dir(dir) {
            self.write(&format!("/projects/{name}/{path}"), &bytes);
        }
    }

    /// The format `project`'s `project.json` says, read off the flash.
    fn project_format(&self, project: &str) -> Option<u32> {
        let bytes = self
            .server
            .base_fs()
            .read_file(
                format!("/projects/{project}/project.json")
                    .as_str()
                    .as_path(),
            )
            .ok()?;
        let mut files = lpa_upgrade::ProjectFiles::new();
        files.insert(lpa_upgrade::PROJECT_MANIFEST, bytes);
        lpa_upgrade::stored_format(&files).ok()
    }

    fn has_file(&self, project: &str, file: &str) -> bool {
        self.server
            .base_fs()
            .file_exists(format!("/projects/{project}/{file}").as_str().as_path())
            .unwrap_or(false)
    }

    /// The projects the engine runs.
    fn running(&self) -> Vec<String> {
        self.server
            .project_manager()
            .list_loaded_projects()
            .iter()
            .map(|project| project.path.as_str().to_string())
            .collect()
    }
}

#[derive(Default)]
struct VecTransport {
    sent: Vec<WireServerMessage>,
}

impl lpc_shared::transport::ServerTransport for VecTransport {
    async fn send(
        &mut self,
        _link: lpc_shared::transport::LinkId,
        msg: WireServerMessage,
    ) -> Result<(), TransportError> {
        self.sent.push(msg);
        Ok(())
    }

    async fn receive(&mut self) -> Result<Option<Incoming>, TransportError> {
        Ok(None)
    }

    async fn receive_all(&mut self) -> Result<Vec<Incoming>, TransportError> {
        Ok(Vec::new())
    }

    fn links(&self) -> Vec<lpc_shared::transport::Link> {
        vec![lpc_shared::transport::Link::PRIMARY]
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}

fn upgrade_tests() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../lpa-upgrade/tests")
}

fn corpus(name: &str) -> PathBuf {
    upgrade_tests().join("corpus").join(name)
}

fn read_dir(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    collect(dir, dir, &mut out);
    assert!(!out.is_empty(), "{} is missing", dir.display());
    out
}

fn collect(dir: &Path, root: &Path, out: &mut Vec<(String, Vec<u8>)>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("a corpus directory")
        .map(|entry| entry.expect("an entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect(&path, root, out);
        } else {
            let relative = path
                .strip_prefix(root)
                .expect("inside")
                .to_string_lossy()
                .replace('\\', "/");
            out.push((relative, std::fs::read(&path).expect("a corpus file")));
        }
    }
}
