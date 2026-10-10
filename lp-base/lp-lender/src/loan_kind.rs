//! Who wants the block, in priority order.

/// A taker of the big block. Declaration order is priority order: an
/// earlier kind outranks a later one.
///
/// - [`LoanKind::Ota`] first: an update that cannot inflate leaves a board
///   on its old build, and the update's window (36 KiB) is the largest
///   single ask a C6 makes. It is also the only kind that holds a loan
///   across ticks.
/// - [`LoanKind::Compile`]: a deferred compile renders keep-last-good (or
///   black), which a person editing a shader sees at once.
/// - [`LoanKind::WholeFile`]: Studio's pull and `FsRequest::Read` read a
///   file whole (the choker's 27 KB SVG); refusing one stalls the editor's
///   open.
/// - [`LoanKind::Read`]: a project read; Studio retries a refused one
///   within a frame or two.
/// - [`LoanKind::Rebuild`]: a discardable tenant rebuilding itself into the
///   idle block. Last, and the only kind that never purges a tenant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LoanKind {
    /// The over-the-air update's inflate window.
    Ota,
    /// A shader (or compute shader) compile.
    Compile,
    /// A file read whole into one buffer.
    WholeFile,
    /// A project read (the device card's feed, the editor's lens).
    Read,
    /// A discardable tenant rebuilding into the idle block.
    Rebuild,
}

impl LoanKind {
    /// Every kind, highest priority first.
    pub const ALL: [LoanKind; 5] = [
        LoanKind::Ota,
        LoanKind::Compile,
        LoanKind::WholeFile,
        LoanKind::Read,
        LoanKind::Rebuild,
    ];

    /// How many kinds there are.
    pub const COUNT: usize = 5;

    /// Whether `self` outranks `other`.
    pub fn outranks(self, other: LoanKind) -> bool {
        self < other
    }

    /// Whether a grant of this kind may purge tenants to make room.
    pub const fn may_purge(self) -> bool {
        !matches!(self, LoanKind::Rebuild)
    }

    /// Whether a loan of this kind may stay held across a tick boundary.
    pub const fn spans_ticks(self) -> bool {
        matches!(self, LoanKind::Ota)
    }

    /// A short name for logs.
    pub const fn name(self) -> &'static str {
        match self {
            LoanKind::Ota => "ota",
            LoanKind::Compile => "compile",
            LoanKind::WholeFile => "whole-file",
            LoanKind::Read => "read",
            LoanKind::Rebuild => "rebuild",
        }
    }

    /// The kind's index into per-kind tables.
    pub const fn index(self) -> usize {
        self as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ota_outranks_everything_and_read_nothing() {
        for kind in LoanKind::ALL {
            assert_eq!(LoanKind::Ota.outranks(kind), kind != LoanKind::Ota);
            assert!(!LoanKind::Rebuild.outranks(kind));
        }
        assert!(LoanKind::Compile.outranks(LoanKind::WholeFile));
        assert!(LoanKind::WholeFile.outranks(LoanKind::Read));
    }

    #[test]
    fn only_ota_spans_ticks() {
        let spanning: [bool; LoanKind::COUNT] = LoanKind::ALL.map(LoanKind::spans_ticks);
        assert_eq!(spanning, [true, false, false, false, false]);
    }
}
