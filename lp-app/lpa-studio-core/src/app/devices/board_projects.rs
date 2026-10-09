//! Which board plays which project — the one join, read by the board card
//! and the home page alike.
//!
//! Studio cannot ask a board "which library project are you?": the wire
//! carries a storage-directory label, never a `prj…` uid. So the answer is a
//! join of three kinds of evidence (see [`BoardPlays`] for what each is), in
//! this order, first match wins:
//!
//! 1. **The lens is open on the board and binds a library project** →
//!    [`BoardPlays::Open`]. The editor is looking at it right now.
//! 2. **The board says it runs nothing** → [`BoardPlays::Nothing`].
//! 3. **The registry row's association names a project in this library** →
//!    [`BoardPlays::Given`], `at_head` when the version given is the
//!    project's newest. This holds whether the board says it runs something
//!    or has said nothing at all (offline, or not yet reported): an offline
//!    board keeps playing what it was given.
//! 4. **The board says it runs something** → [`BoardPlays::Running`], a
//!    label this library cannot turn into a project.
//! 5. Otherwise → [`BoardPlays::Unknown`].
//!
//! Step 2 sits *before* step 3 because a live "nothing" beats a stale
//! association: a Remove, an erase or another browser's push leaves the
//! registry row naming a project the board no longer holds, and the board's
//! own report is the only thing that has seen it. The association is the
//! fallback for when the board is silent, not a veto over what it says.
//!
//! A project on an offline board still counts as on a board
//! ([`BoardProjects::on_no_board`], [`BoardProjects::sharing`]). Pending
//! links — boards that have not said who they are — are not in the join.

use std::collections::BTreeMap;

use lpa_devices::view::LoadedProject;

use crate::app::home::ui_package_card::UiPackageCard;
use crate::app::places::RegisteredDevice;
use crate::{ContentHash, DeviceId, DeviceView};

use super::board_plays::BoardPlays;
use super::device_by_base_mac::given_association;

/// Which board plays which project — the answers of [`board_projects`], per
/// board, in roster order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BoardProjects {
    plays: BTreeMap<DeviceId, BoardPlays>,
    /// The boards in the order the roster listed them: `boards_playing`
    /// keeps it, so a project's boards read in a stable order.
    order: Vec<DeviceId>,
}

impl BoardProjects {
    /// A join built from answers already decided, in roster order. The join
    /// itself ends here ([`board_projects`]); stories and tests that need a
    /// `BoardProjects` without a whole roster behind it start here.
    pub fn from_answers(answers: impl IntoIterator<Item = (DeviceId, BoardPlays)>) -> Self {
        let mut plays = BTreeMap::new();
        let mut order = Vec::new();
        for (board, answer) in answers {
            if plays.insert(board, answer).is_none() {
                order.push(board);
            }
        }
        Self { plays, order }
    }

    /// One board's answer; [`BoardPlays::Unknown`] for a board the join
    /// never saw.
    pub fn plays(&self, board: DeviceId) -> &BoardPlays {
        static UNKNOWN: BoardPlays = BoardPlays::Unknown;
        self.plays.get(&board).unwrap_or(&UNKNOWN)
    }

    /// The boards whose answer names `project_uid`, in roster order.
    pub fn boards_playing(&self, project_uid: &str) -> Vec<DeviceId> {
        self.order
            .iter()
            .copied()
            .filter(|board| self.plays(*board).project_uid() == Some(project_uid))
            .collect()
    }

    /// How many boards play the same project as `board` (itself included);
    /// 0 when its answer names no project.
    pub fn sharing(&self, board: DeviceId) -> usize {
        match self.plays(board).project_uid() {
            Some(project_uid) => self.boards_playing(project_uid).len(),
            None => 0,
        }
    }

    /// The library projects no board plays — the page's "Other projects".
    /// A project on an offline board, or open on the lens's board, is on a
    /// board.
    pub fn on_no_board<'a>(&self, projects: &'a [UiPackageCard]) -> Vec<&'a UiPackageCard> {
        projects
            .iter()
            .filter(|project| {
                !self
                    .plays
                    .values()
                    .any(|answer| answer.project_uid() == Some(project.uid.as_str()))
            })
            .collect()
    }
}

/// What the join reads. Pure: the controller gathers it.
pub struct BoardProjectInputs<'a> {
    /// The roster, online and offline.
    pub boards: &'a [DeviceView],
    /// Each board's registry row key (its `dev…` uid, or `mac:<base mac>`
    /// before the firmware provisions one); absent for a board with no
    /// honest key.
    pub registry_keys: &'a BTreeMap<DeviceId, String>,
    /// The registry's rows, which hold each board's association.
    pub registry: &'a [RegisteredDevice],
    /// The library.
    pub projects: &'a [UiPackageCard],
    /// Each library project's history head, by uid. A project without one
    /// (a package that would not open) is never "at head".
    pub project_heads: &'a BTreeMap<String, ContentHash>,
    /// The editor's lens: its board, and the library uid bound to it. `None`
    /// when no lens is open, or it is open on a project with no library
    /// package behind it (a transient session).
    pub lens: Option<(DeviceId, String)>,
}

/// Join the roster to the library: which project each board plays.
pub fn board_projects(inputs: &BoardProjectInputs<'_>) -> BoardProjects {
    BoardProjects::from_answers(
        inputs
            .boards
            .iter()
            .map(|board| (board.id, plays_of(board, inputs))),
    )
}

/// One board's answer, by the rule in the module docs.
fn plays_of(board: &DeviceView, inputs: &BoardProjectInputs<'_>) -> BoardPlays {
    if let Some((lens_board, project_uid)) = &inputs.lens
        && *lens_board == board.id
    {
        return BoardPlays::Open {
            project_uid: project_uid.clone(),
        };
    }
    if board.loaded_project == LoadedProject::Empty {
        return BoardPlays::Nothing;
    }
    if let Some(given) = given_project(board.id, inputs) {
        return given;
    }
    match &board.loaded_project {
        LoadedProject::Running { label } => BoardPlays::Running {
            label: label.clone(),
        },
        LoadedProject::Empty | LoadedProject::Unknown => BoardPlays::Unknown,
    }
}

/// The registry's answer: the project this board was last given, when the
/// library still holds it.
fn given_project(board: DeviceId, inputs: &BoardProjectInputs<'_>) -> Option<BoardPlays> {
    let key = inputs.registry_keys.get(&board)?;
    let association = given_association(inputs.registry, key)?;
    let project_uid = association.project.to_string();
    inputs
        .projects
        .iter()
        .any(|project| project.uid == project_uid)
        .then(|| {
            let at_head = inputs.project_heads.get(&project_uid) == Some(&association.version);
            BoardPlays::Given {
                project_uid,
                at_head,
            }
        })
}

#[cfg(test)]
mod tests {
    use lpa_devices::device::DeviceStatus;
    use lpa_devices::view::FirmwareFace;
    use lpc_history::{DeviceAssociation, PrefixedUid, UidPrefix};

    use crate::app::library::PackageHealth;

    use super::*;

    #[test]
    fn the_lens_on_a_board_with_a_bound_project_is_open() {
        let fixture = Fixture::new()
            .board(
                1,
                LoadedProject::Running {
                    label: "studio".into(),
                },
            )
            .given(1, "alpha", Version::Head)
            .lens(1, "alpha");
        assert_eq!(
            fixture.join().plays(DeviceId(1)),
            &BoardPlays::Open {
                project_uid: fixture.uid("alpha")
            }
        );
    }

    #[test]
    fn the_lens_beats_the_registry_and_the_boards_own_report() {
        // The editor shows `beta` on the board, whatever the registry
        // remembers and whatever the board says.
        let fixture = Fixture::new()
            .board(1, LoadedProject::Empty)
            .given(1, "alpha", Version::Head)
            .lens(1, "beta");
        assert_eq!(
            fixture.join().plays(DeviceId(1)),
            &BoardPlays::Open {
                project_uid: fixture.uid("beta")
            }
        );
    }

    #[test]
    fn a_board_that_says_nothing_is_running_beats_a_stale_association() {
        let fixture =
            Fixture::new()
                .board(1, LoadedProject::Empty)
                .given(1, "alpha", Version::Head);
        let join = fixture.join();
        assert_eq!(join.plays(DeviceId(1)), &BoardPlays::Nothing);
        // And a board that is not playing the project does not count as
        // playing it.
        assert!(join.boards_playing(&fixture.uid("alpha")).is_empty());
        assert_eq!(join.sharing(DeviceId(1)), 0);
    }

    #[test]
    fn the_association_names_the_project_a_board_that_says_nothing_has_not_denied() {
        // Offline, or not yet reported: the board has said nothing, and
        // keeps playing what it was given.
        let fixture =
            Fixture::new()
                .board(1, LoadedProject::Unknown)
                .given(1, "alpha", Version::Head);
        assert_eq!(
            fixture.join().plays(DeviceId(1)),
            &BoardPlays::Given {
                project_uid: fixture.uid("alpha"),
                at_head: true
            }
        );
    }

    #[test]
    fn the_association_names_the_project_a_running_board_cannot() {
        // The board reports a storage label; the registry says which
        // library project that is.
        let fixture = Fixture::new()
            .board(
                1,
                LoadedProject::Running {
                    label: "studio".into(),
                },
            )
            .given(1, "alpha", Version::Head);
        assert_eq!(
            fixture.join().plays(DeviceId(1)),
            &BoardPlays::Given {
                project_uid: fixture.uid("alpha"),
                at_head: true
            }
        );
    }

    #[test]
    fn a_board_given_an_older_version_is_not_at_head() {
        let fixture = Fixture::new()
            .board(
                1,
                LoadedProject::Running {
                    label: "studio".into(),
                },
            )
            .given(1, "alpha", Version::Older);
        assert_eq!(
            fixture.join().plays(DeviceId(1)),
            &BoardPlays::Given {
                project_uid: fixture.uid("alpha"),
                at_head: false
            }
        );
    }

    #[test]
    fn a_project_with_no_known_head_is_never_at_head() {
        let fixture = Fixture::new()
            .board(1, LoadedProject::Unknown)
            .given(1, "alpha", Version::Head)
            .without_head("alpha");
        assert_eq!(
            fixture.join().plays(DeviceId(1)),
            &BoardPlays::Given {
                project_uid: fixture.uid("alpha"),
                at_head: false
            }
        );
    }

    #[test]
    fn an_association_naming_a_project_this_library_lacks_falls_through_to_the_label() {
        let fixture = Fixture::new()
            .board(
                1,
                LoadedProject::Running {
                    label: "studio".into(),
                },
            )
            .given_elsewhere(1);
        assert_eq!(
            fixture.join().plays(DeviceId(1)),
            &BoardPlays::Running {
                label: "studio".to_string()
            }
        );
    }

    #[test]
    fn an_association_naming_a_project_this_library_lacks_falls_through_to_unknown() {
        let fixture = Fixture::new()
            .board(1, LoadedProject::Unknown)
            .given_elsewhere(1);
        assert_eq!(fixture.join().plays(DeviceId(1)), &BoardPlays::Unknown);
    }

    #[test]
    fn a_board_running_something_with_no_association_is_running_its_label() {
        let fixture = Fixture::new().board(
            1,
            LoadedProject::Running {
                label: "studio".into(),
            },
        );
        assert_eq!(
            fixture.join().plays(DeviceId(1)),
            &BoardPlays::Running {
                label: "studio".to_string()
            }
        );
    }

    #[test]
    fn a_board_with_no_report_and_no_association_is_unknown() {
        let fixture = Fixture::new().board(1, LoadedProject::Unknown);
        assert_eq!(fixture.join().plays(DeviceId(1)), &BoardPlays::Unknown);
    }

    #[test]
    fn a_board_the_join_never_saw_is_unknown() {
        let fixture = Fixture::new().board(1, LoadedProject::Empty);
        assert_eq!(fixture.join().plays(DeviceId(99)), &BoardPlays::Unknown);
        assert_eq!(fixture.join().sharing(DeviceId(99)), 0);
    }

    #[test]
    fn a_board_with_no_registry_key_has_no_association_to_read() {
        let fixture = Fixture::new()
            .board(
                1,
                LoadedProject::Running {
                    label: "studio".into(),
                },
            )
            .given(1, "alpha", Version::Head)
            .without_key(1);
        assert_eq!(
            fixture.join().plays(DeviceId(1)),
            &BoardPlays::Running {
                label: "studio".to_string()
            }
        );
    }

    #[test]
    fn the_lens_on_a_board_with_no_bound_project_falls_through() {
        // A transient session: the controller hands the join no lens at
        // all, so the registry's answer stands.
        let fixture = Fixture::new()
            .board(
                1,
                LoadedProject::Running {
                    label: "studio".into(),
                },
            )
            .given(1, "alpha", Version::Head);
        assert_eq!(fixture.lens, None);
        assert_eq!(
            fixture.join().plays(DeviceId(1)),
            &BoardPlays::Given {
                project_uid: fixture.uid("alpha"),
                at_head: true
            }
        );
    }

    #[test]
    fn the_lens_on_another_board_leaves_this_one_alone() {
        let fixture = Fixture::new()
            .board(1, LoadedProject::Unknown)
            .board(2, LoadedProject::Unknown)
            .given(1, "alpha", Version::Head)
            .lens(2, "beta");
        let join = fixture.join();
        assert_eq!(
            join.plays(DeviceId(1)),
            &BoardPlays::Given {
                project_uid: fixture.uid("alpha"),
                at_head: true
            }
        );
        assert_eq!(
            join.plays(DeviceId(2)),
            &BoardPlays::Open {
                project_uid: fixture.uid("beta")
            }
        );
    }

    #[test]
    fn two_boards_given_the_same_project_share_it_in_roster_order() {
        // Roster order, not id order: board 7 is listed before board 3.
        let fixture = Fixture::new()
            .board(7, LoadedProject::Unknown)
            .board(3, LoadedProject::Unknown)
            .board(5, LoadedProject::Unknown)
            .given(7, "alpha", Version::Head)
            .given(3, "alpha", Version::Head)
            .given(5, "beta", Version::Head);
        let join = fixture.join();
        assert_eq!(
            join.boards_playing(&fixture.uid("alpha")),
            vec![DeviceId(7), DeviceId(3)]
        );
        assert_eq!(join.boards_playing(&fixture.uid("beta")), vec![DeviceId(5)]);
        assert_eq!(join.sharing(DeviceId(7)), 2);
        assert_eq!(join.sharing(DeviceId(3)), 2);
        assert_eq!(join.sharing(DeviceId(5)), 1);
    }

    #[test]
    fn open_and_given_boards_playing_the_same_project_share_it() {
        let fixture = Fixture::new()
            .board(1, LoadedProject::Unknown)
            .board(2, LoadedProject::Unknown)
            .given(1, "alpha", Version::Head)
            .lens(2, "alpha");
        let join = fixture.join();
        assert_eq!(join.sharing(DeviceId(1)), 2);
        assert_eq!(join.sharing(DeviceId(2)), 2);
    }

    #[test]
    fn running_and_unknown_boards_name_no_project() {
        let fixture = Fixture::new()
            .board(
                1,
                LoadedProject::Running {
                    label: "studio".into(),
                },
            )
            .board(2, LoadedProject::Unknown);
        let join = fixture.join();
        assert_eq!(join.sharing(DeviceId(1)), 0);
        assert_eq!(join.sharing(DeviceId(2)), 0);
    }

    #[test]
    fn on_no_board_lists_the_projects_nobody_plays() {
        let fixture =
            Fixture::new()
                .board(1, LoadedProject::Unknown)
                .given(1, "alpha", Version::Head);
        let join = fixture.join();
        let other = fixture.other_projects(&join);
        assert_eq!(other, vec!["beta", "gamma"]);
    }

    #[test]
    fn on_no_board_counts_a_project_on_an_offline_board_as_on_a_board() {
        let fixture = Fixture::new()
            .offline_board(1)
            .given(1, "alpha", Version::Head);
        let join = fixture.join();
        assert_eq!(
            join.plays(DeviceId(1)),
            &BoardPlays::Given {
                project_uid: fixture.uid("alpha"),
                at_head: true
            }
        );
        assert_eq!(fixture.other_projects(&join), vec!["beta", "gamma"]);
    }

    #[test]
    fn on_no_board_counts_the_lens_boards_project_as_on_a_board() {
        let fixture = Fixture::new()
            .board(1, LoadedProject::Unknown)
            .lens(1, "gamma");
        let join = fixture.join();
        assert_eq!(fixture.other_projects(&join), vec!["alpha", "beta"]);
    }

    #[test]
    fn on_no_board_ignores_a_project_a_board_only_runs_by_label() {
        // A bare label names no library project, so the library's own
        // project of that name is still on no board.
        let fixture = Fixture::new().board(
            1,
            LoadedProject::Running {
                label: "alpha".into(),
            },
        );
        let join = fixture.join();
        assert_eq!(
            fixture.other_projects(&join),
            vec!["alpha", "beta", "gamma"]
        );
    }

    #[test]
    fn on_no_board_with_no_boards_lists_the_whole_library() {
        let fixture = Fixture::new();
        let join = fixture.join();
        assert_eq!(
            fixture.other_projects(&join),
            vec!["alpha", "beta", "gamma"]
        );
    }

    // -----------------------------------------------------------------
    // The fixture: a roster, a registry and a three-project library.
    // -----------------------------------------------------------------

    #[derive(Clone, Copy)]
    enum Version {
        /// The project's newest.
        Head,
        /// A version the project has since moved past.
        Older,
    }

    struct Fixture {
        boards: Vec<DeviceView>,
        registry_keys: BTreeMap<DeviceId, String>,
        registry: Vec<RegisteredDevice>,
        projects: Vec<UiPackageCard>,
        project_heads: BTreeMap<String, ContentHash>,
        lens: Option<(DeviceId, String)>,
    }

    impl Fixture {
        fn new() -> Self {
            let slugs = ["alpha", "beta", "gamma"];
            let projects: Vec<UiPackageCard> = slugs
                .iter()
                .map(|slug| package(&Self::project_uid(slug), slug))
                .collect();
            let project_heads = projects
                .iter()
                .map(|project| (project.uid.clone(), Self::head_of(&project.slug)))
                .collect();
            Self {
                boards: Vec::new(),
                registry_keys: BTreeMap::new(),
                registry: Vec::new(),
                projects,
                project_heads,
                lens: None,
            }
        }

        /// A board in the roster, with a registry row of its own.
        fn board(mut self, id: u64, loaded: LoadedProject) -> Self {
            self.boards
                .push(board_view(id, DeviceStatus::Ready, loaded));
            self.registry_keys.insert(DeviceId(id), Self::key(id));
            self.registry.push(RegisteredDevice {
                uid: Self::key(id),
                ..RegisteredDevice::default()
            });
            self
        }

        /// A board that is off the bus and has said nothing since.
        fn offline_board(mut self, id: u64) -> Self {
            self.boards.push(board_view(
                id,
                DeviceStatus::Offline,
                LoadedProject::Unknown,
            ));
            self.registry_keys.insert(DeviceId(id), Self::key(id));
            self.registry.push(RegisteredDevice {
                uid: Self::key(id),
                ..RegisteredDevice::default()
            });
            self
        }

        /// The board's registry row says `slug` was last given to it, at
        /// `version`.
        fn given(self, id: u64, slug: &str, version: Version) -> Self {
            let version = match version {
                Version::Head => Self::head_of(slug),
                Version::Older => ContentHash::of(format!("{slug}-older").as_bytes()),
            };
            self.association(id, Self::project_uid(slug), version)
        }

        /// The board's registry row names a project this library does not
        /// hold.
        fn given_elsewhere(self, id: u64) -> Self {
            let elsewhere = PrefixedUid::mint(UidPrefix::Project, &[200; 16]).to_string();
            self.association(id, elsewhere, ContentHash::of(b"elsewhere"))
        }

        fn association(mut self, id: u64, project: String, version: ContentHash) -> Self {
            let key = Self::key(id);
            let row = self
                .registry
                .iter_mut()
                .find(|row| row.uid == key)
                .expect("the board has a registry row");
            row.association = Some(DeviceAssociation {
                device: PrefixedUid::mint(UidPrefix::Device, &[id as u8; 16]),
                project: project.parse().expect("a project uid"),
                version,
                at: 0.0,
            });
            self
        }

        fn without_key(mut self, id: u64) -> Self {
            self.registry_keys.remove(&DeviceId(id));
            self
        }

        fn without_head(mut self, slug: &str) -> Self {
            self.project_heads.remove(&Self::project_uid(slug));
            self
        }

        fn lens(mut self, id: u64, slug: &str) -> Self {
            self.lens = Some((DeviceId(id), Self::project_uid(slug)));
            self
        }

        fn join(&self) -> BoardProjects {
            board_projects(&BoardProjectInputs {
                boards: &self.boards,
                registry_keys: &self.registry_keys,
                registry: &self.registry,
                projects: &self.projects,
                project_heads: &self.project_heads,
                lens: self.lens.clone(),
            })
        }

        /// The slugs of the library's projects no board plays.
        fn other_projects(&self, join: &BoardProjects) -> Vec<String> {
            join.on_no_board(&self.projects)
                .into_iter()
                .map(|project| project.slug.clone())
                .collect()
        }

        fn uid(&self, slug: &str) -> String {
            Self::project_uid(slug)
        }

        fn key(id: u64) -> String {
            PrefixedUid::mint(UidPrefix::Device, &[id as u8; 16]).to_string()
        }

        fn project_uid(slug: &str) -> String {
            let seed = match slug {
                "alpha" => 1,
                "beta" => 2,
                _ => 3,
            };
            PrefixedUid::mint(UidPrefix::Project, &[seed; 16]).to_string()
        }

        fn head_of(slug: &str) -> ContentHash {
            ContentHash::of(format!("{slug}-head").as_bytes())
        }
    }

    fn package(uid: &str, slug: &str) -> UiPackageCard {
        UiPackageCard {
            uid: uid.to_string(),
            kind: "Module".to_string(),
            project_kind: "General".to_string(),
            exports: Vec::new(),
            slug: slug.to_string(),
            last_saved_at: None,
            provenance: None,
            on_boards: Vec::new(),
            open_elsewhere: false,
            target: None,
            health: PackageHealth::Ready,
        }
    }

    fn board_view(id: u64, status: DeviceStatus, loaded: LoadedProject) -> DeviceView {
        DeviceView {
            id: DeviceId(id),
            title: format!("board {id}"),
            status,
            state_label: String::new(),
            detail: None,
            freshness_label: None,
            identity_label: None,
            detected_chip: None,
            board_id: None,
            firmware_face: FirmwareFace::Unknown,
            remembered_firmware: None,
            degraded: None,
            loaded_project: loaded,
            engine_fps: None,
            link_counters: None,
            can_receive_project: false,
            can_remove_project: false,
            activity: None,
            last_outcome: None,
            terminal: Vec::new(),
            terminal_dropped: 0,
            firmware_blocked: None,
            escapes: Vec::new(),
            update_blocked: None,
            last_update_outcome: None,
        }
    }
}
