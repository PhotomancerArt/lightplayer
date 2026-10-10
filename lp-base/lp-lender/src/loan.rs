//! A granted loan.

use crate::LoanKind;

/// A loan's identity, unique for the lender's life.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LoanId(pub u32);

/// The block, lent. Not `Clone`: [`crate::Lender::release`] consumes it, so
/// a loan is returned exactly once.
#[derive(Debug, PartialEq, Eq)]
#[must_use = "a loan must be released with Lender::release"]
pub struct Loan {
    pub(crate) id: LoanId,
    pub(crate) kind: LoanKind,
    pub(crate) tick: u64,
}

impl Loan {
    /// The loan's identity.
    pub fn id(&self) -> LoanId {
        self.id
    }

    /// Who holds it.
    pub fn kind(&self) -> LoanKind {
        self.kind
    }

    /// The tick it was granted in.
    pub fn tick(&self) -> u64 {
        self.tick
    }
}
