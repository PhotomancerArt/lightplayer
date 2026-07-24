//! Pool-scoped identity of one runtime session.

use core::fmt;

/// Identity of one [`RuntimeSession`](super::RuntimeSession) in the
/// [`RuntimePool`](super::RuntimePool).
///
/// Minted by the pool when a session is installed — stable within the tab,
/// never reused — so the id exists BEFORE any device identity is known
/// (a hardware session's `dev_` uid only arrives with the wire hello).
/// The device-uid association is a derivation on the session
/// ([`RuntimeSession::device_uid`](super::RuntimeSession::device_uid)):
/// once the hello lands the session state carries the uid, keyed under the
/// same minted `RuntimeId` it had while booting.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RuntimeId(u64);

impl RuntimeId {
    /// A pool-minted id. Only the pool mints these (monotonic counter).
    pub(crate) fn new(value: u64) -> Self {
        Self(value)
    }
}

impl fmt::Display for RuntimeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "runtime-{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_ids_are_ordered_and_displayable() {
        let first = RuntimeId::new(1);
        let second = RuntimeId::new(2);

        assert!(first < second);
        assert_eq!(first, RuntimeId::new(1));
        assert_eq!(first.to_string(), "runtime-1");
    }
}
