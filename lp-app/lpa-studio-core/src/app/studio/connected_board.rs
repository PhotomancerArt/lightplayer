//! "Connected": the home page holds the open session.
//!
//! Connect on a board's card (`devices/<board>/connect`), or Edit from a
//! card (`devices/<board>/edit`), opens the editor's session (the lens) on
//! that board the way an address does, and records it here as a
//! [`ConnectedBoard`]. While the record stands, the home page keeps the
//! session: going home leaves it open, and it shows on its board's card
//! instead of in the editor, unless an Edit is waiting for the editor or the
//! user is on the session's own page ([`ConnectedBoard::shows_editor`]).
//! Done (`devices/<board>/done`) closes it.
//!
//! The record is a fact beside the runtime pool, not a change to it: the
//! pool keeps one session, one lens id, its install and eviction rules and
//! its borrow of the board's wire, exactly as before. What the record adds
//! is who holds that session (the home page, or an address) and so which
//! surface core builds for it. A session that left the pool by any road
//! (Done, an open elsewhere, a dropped link whose hold ran out) is not
//! connected, whatever the record still says ([`ConnectedBoard::rides`]).
//!
//! See `docs/adr/2026-10-08-the-board-card-and-one-home-page.md` §4 and
//! `docs/adr/2026-07-24-runtime-pool.md`.

use lpa_devices::DeviceId;

use crate::{RuntimeId, UiPage, UiPlace};

/// The home page holds the open session (Connect, or Edit from a card):
/// going home keeps it, and it shows on its board's card unless the editor
/// is wanted or the user is on its own page.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectedBoard {
    /// The pool session it rides.
    pub session: RuntimeId,
    /// The board the session is on.
    pub device: DeviceId,
    /// The registry uid the lens opened (the board's `/device/<uid>`).
    pub uid: String,
    /// Edit was pressed and the user has not reached the session's page
    /// yet: the editor shows meanwhile.
    pub editor_waiting: bool,
    /// How far the open has got.
    pub phase: ConnectPhase,
}

/// How far a connect's open has got.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectPhase {
    /// The session is installed and the lens is attaching (the board's
    /// build, the running project, the mirror's first read).
    Opening,
    /// The lens is attached: the session's project is open.
    Open,
}

/// Why the last Connect (or an Edit that had to connect first) did not
/// open, for the board's card. Cleared by the next Connect, Edit or Done on
/// any board.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectFailure {
    /// The board the connect was for.
    pub device: DeviceId,
    /// What went wrong, in the words the open failed with.
    pub reason: String,
}

impl ConnectedBoard {
    /// Whether this record is about the session the pool holds now
    /// (`attached`): a session that left the pool by any road is not
    /// connected, and no caller may read the record as if it were.
    pub fn rides(&self, attached: Option<RuntimeId>) -> bool {
        attached == Some(self.session)
    }

    /// Whether the open session shows in the editor rather than on its
    /// card: an Edit is waiting, or the reported `place` is this session's
    /// own page, in any view. `lens_project` is the library project the
    /// lens holds (`/p/<its uid>`). With no place reported (the headless
    /// tests and evals), only a waiting Edit shows the editor.
    ///
    /// A waiting Edit lasts until the first place report that moves the
    /// user to another page ([`Self::note_page_moved`]): to the session's
    /// page, where the place itself goes on showing the editor, or
    /// anywhere else — Edit, then Home, shows home.
    ///
    /// It reads the place; it opens, closes and navigates nothing because
    /// of it (AGENTS.md, "Place is a read-only fact in core").
    pub fn shows_editor(&self, place: Option<&UiPlace>, lens_project: Option<&str>) -> bool {
        self.editor_waiting
            || place.is_some_and(|place| self.is_its_page(&place.page, lens_project))
    }

    /// The web reported the user on `now`, having been on `before` (`None`
    /// before any report): a waiting Edit ends when the page moved. A report
    /// that only opens or closes a panel over the same page (⌘K closing as
    /// it runs Edit, the chat opening) is not a move, so it leaves the Edit
    /// waiting for the lens sync to take the user to the session's page.
    pub fn note_page_moved(&mut self, before: Option<&UiPage>, now: &UiPage) {
        if before != Some(now) {
            self.editor_waiting = false;
        }
    }

    /// Whether `page` is this session's own page: its project's
    /// (`/p/<uid>`, the lens's library project) or its board's
    /// (`/device/<uid>`), any view.
    pub fn is_its_page(&self, page: &UiPage, lens_project: Option<&str>) -> bool {
        match page {
            UiPage::Project { uid, .. } => lens_project == Some(uid.as_str()),
            UiPage::Device { uid, .. } => *uid == self.uid,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UiProjectView;

    #[test]
    fn with_no_place_only_a_waiting_edit_shows_the_editor() {
        let connected = board(false);
        assert!(!connected.shows_editor(None, Some(PROJECT)));
        assert!(board(true).shows_editor(None, Some(PROJECT)));
    }

    #[test]
    fn the_sessions_own_pages_show_the_editor_in_any_view() {
        let connected = board(false);
        for view in [
            UiProjectView::Nodes,
            UiProjectView::Play,
            UiProjectView::Patch,
            UiProjectView::Mapping,
        ] {
            let project = place(UiPage::Project {
                uid: PROJECT.to_string(),
                view,
            });
            assert!(
                connected.shows_editor(Some(&project), Some(PROJECT)),
                "{view:?}"
            );
            let device = place(UiPage::Device {
                uid: UID.to_string(),
                view,
            });
            assert!(
                connected.shows_editor(Some(&device), Some(PROJECT)),
                "{view:?}"
            );
        }
    }

    #[test]
    fn every_other_page_shows_the_card_unless_an_edit_waits() {
        let pages = [
            UiPage::Home,
            UiPage::Explore,
            UiPage::Account,
            UiPage::Docs { article: None },
            // Another project's page, and another board's.
            UiPage::Project {
                uid: "prjother".to_string(),
                view: UiProjectView::Nodes,
            },
            UiPage::Device {
                uid: "devother".to_string(),
                view: UiProjectView::Play,
            },
        ];
        for page in pages {
            let at = place(page.clone());
            assert!(
                !board(false).shows_editor(Some(&at), Some(PROJECT)),
                "{page:?}"
            );
            assert!(
                board(true).shows_editor(Some(&at), Some(PROJECT)),
                "a waiting Edit shows the editor on {page:?}"
            );
        }
    }

    #[test]
    fn a_project_page_is_the_sessions_only_when_the_lens_holds_that_project() {
        let at = place(UiPage::Project {
            uid: PROJECT.to_string(),
            view: UiProjectView::Nodes,
        });
        assert!(
            !board(false).shows_editor(Some(&at), None),
            "an unbound lens has no project page"
        );
        assert!(!board(false).shows_editor(Some(&at), Some("prjother")));
    }

    /// The director's ruling on P2–P3: a waiting Edit ends at the first
    /// place report that moves the user to another page — to the session's
    /// page, or anywhere else (Edit, then Home, shows home). A panel opening
    /// or closing over the same page is not a move.
    #[test]
    fn a_waiting_edit_ends_at_the_first_page_move() {
        let home = UiPage::Home;
        let its_page = UiPage::Project {
            uid: PROJECT.to_string(),
            view: UiProjectView::Nodes,
        };
        for (before, now) in [
            (None, &home),
            (Some(&home), &its_page),
            (Some(&home), &UiPage::Explore),
            (Some(&its_page), &home),
        ] {
            let mut connected = board(true);
            connected.note_page_moved(before, now);
            assert!(!connected.editor_waiting, "{before:?} → {now:?}");
            assert_eq!(
                connected.shows_editor(Some(&place(now.clone())), Some(PROJECT)),
                *now == its_page,
                "the place alone decides, after {before:?} → {now:?}"
            );
        }
        let mut connected = board(true);
        connected.note_page_moved(Some(&home), &home);
        assert!(
            connected.editor_waiting,
            "a panel over the same page: still waiting"
        );
    }

    #[test]
    fn a_session_that_left_the_pool_is_not_connected() {
        let connected = board(false);
        assert!(connected.rides(Some(connected.session)));
        assert!(!connected.rides(None), "nothing in the pool");
        assert!(
            !connected.rides(Some(RuntimeId::new(99))),
            "another session took the pool's place"
        );
    }

    const PROJECT: &str = "prj4b7c";
    const UID: &str = "dev000000conn0001";

    fn board(editor_waiting: bool) -> ConnectedBoard {
        ConnectedBoard {
            session: RuntimeId::new(3),
            device: DeviceId(7),
            uid: UID.to_string(),
            editor_waiting,
            phase: ConnectPhase::Open,
        }
    }

    fn place(page: UiPage) -> UiPlace {
        UiPlace::new(page)
    }
}
