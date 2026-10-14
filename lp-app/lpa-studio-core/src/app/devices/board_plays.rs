//! Which project one board plays, as well as Studio can say, and how it
//! knows.
//!
//! Studio has three sources for "what is on this board", and none of them is
//! the whole truth:
//!
//! - the **editor's lens**, when it is open on the board with a library
//!   project bound: the truth, this very second;
//! - the board's **own report** (its heartbeat), which is live but
//!   anonymous: the wire carries a storage-directory label, never a `prj…`
//!   uid, so it can say *that* something runs and not *which* library project
//!   it is;
//! - the **registry's association**, written when a push verified: it names
//!   a library project and the version given, and it can be stale (another
//!   browser pushed since).
//!
//! [`BoardPlays`] is the join's answer for one board, with the source kept
//! in the variant so the card can word it honestly ("Holiday Eaves" for an
//! open lens, "last sent" for a given project, a bare label for a board
//! running something this library cannot name). The rule that picks the
//! variant is [`super::board_projects`]'s.

/// Which project one board plays, and how Studio knows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BoardPlays {
    /// The editor's lens is open on this board with this library project
    /// bound.
    Open { project_uid: String },
    /// The registry says this library project was last given to the board
    /// (a verified push), and the board has not said it runs nothing.
    /// `at_head`: the version given is the project's newest.
    Given { project_uid: String, at_head: bool },
    /// The board says it runs something this library cannot name.
    Running { label: String },
    /// The board said it runs nothing.
    Nothing,
    /// Nothing is known: no report and no association.
    Unknown,
}

impl BoardPlays {
    /// The library project, when the answer names one ([`Self::Open`],
    /// [`Self::Given`]).
    pub fn project_uid(&self) -> Option<&str> {
        match self {
            Self::Open { project_uid } | Self::Given { project_uid, .. } => Some(project_uid),
            Self::Running { .. } | Self::Nothing | Self::Unknown => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_open_and_given_name_a_library_project() {
        let open = BoardPlays::Open {
            project_uid: "prjaaa".to_string(),
        };
        let given = BoardPlays::Given {
            project_uid: "prjbbb".to_string(),
            at_head: false,
        };
        assert_eq!(open.project_uid(), Some("prjaaa"));
        assert_eq!(given.project_uid(), Some("prjbbb"));
        assert_eq!(
            BoardPlays::Running {
                label: "studio".to_string()
            }
            .project_uid(),
            None
        );
        assert_eq!(BoardPlays::Nothing.project_uid(), None);
        assert_eq!(BoardPlays::Unknown.project_uid(), None);
    }
}
