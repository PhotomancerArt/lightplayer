//! Which project format a build's engine reads, where nothing says it
//! outright (M3 P11) — the fallback.
//!
//! A board says it in its hello (`build.projectFormat`, wire 46 on), and a
//! release in the store in its index entry (`projectFormat`, DD31). A release
//! from before that field says nothing of it, so for one it is read off its
//! wire: the hello learned to say the project format in the same change that
//! made the format 12, so a release on an older wire reads at most 11. From
//! wire 46 on, the wire does not tell: `None`.

/// The newest project format an engine reads when nothing says: an engine
/// from before its hello said it (an older wire) reads at most 11.
pub const FORMAT_BEFORE_THE_HELLO_SAID: u32 = 11;

/// The first wire protocol whose hello says its engine's project format.
pub const FIRST_WIRE_SAYING_PROJECT_FORMAT: u32 = 46;

/// The project format an engine speaking `wire` reads at most, when the wire
/// tells; `None` from [`FIRST_WIRE_SAYING_PROJECT_FORMAT`] on.
pub fn project_format_of_wire(wire: u32) -> Option<u32> {
    (wire < FIRST_WIRE_SAYING_PROJECT_FORMAT).then_some(FORMAT_BEFORE_THE_HELLO_SAID)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_older_wire_reads_at_most_format_11_and_a_newer_one_does_not_say() {
        assert_eq!(project_format_of_wire(45), Some(11));
        assert_eq!(project_format_of_wire(32), Some(11));
        assert_eq!(project_format_of_wire(46), None);
        assert_eq!(project_format_of_wire(lpc_wire::WIRE_PROTO_VERSION), None);
    }
}
