//! Version-and-refuse policy.
//!
//! This is **not** `lpc-wire`'s no-compat policy (wire/protocol
//! compatibility is deliberately not maintained during heavy development,
//! because client/server/firmware are built and deployed together). The
//! cloud API has no such lockstep guarantee — a browser tab can sit open for
//! days while the service redeploys — so a version mismatch must be a
//! first-class, named refusal rather than a silent decode failure or a
//! best-effort partial-compat decode.

use crate::error::CloudError;

/// The cloud API vocabulary version this build of the crate implements.
///
/// Bump on every breaking change to [`crate::request::CloudRequest`],
/// [`crate::response::CloudResponse`], or [`crate::error::CloudError`].
/// Carried in every [`crate::envelope::CloudCall`] and
/// [`crate::envelope::CloudReply`].
///
/// v2 = account/session/login-options calls (2026-08-07): `GetMe`,
/// `UpdateMe`, `ListSessions`, `RevokeSession`, `LoginOptions`.
pub const CLOUD_API_VERSION: u32 = 2;

/// Refuse a call whose declared version does not match [`CLOUD_API_VERSION`].
///
/// Callers on both sides of the wire are expected to check this before
/// interpreting a request or reply body: a mismatch means one side is
/// running against a vocabulary the other side does not speak, and no
/// attempt is made to guess a compatible subset.
pub fn check_version(peer_version: u32) -> Result<(), CloudError> {
    if peer_version == CLOUD_API_VERSION {
        Ok(())
    } else {
        Err(CloudError::VersionMismatch {
            client: peer_version,
            server: CLOUD_API_VERSION,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_matching_version() {
        assert_eq!(check_version(CLOUD_API_VERSION), Ok(()));
    }

    #[test]
    fn refuses_mismatched_version() {
        let bad = CLOUD_API_VERSION + 1;
        assert_eq!(
            check_version(bad),
            Err(CloudError::VersionMismatch {
                client: bad,
                server: CLOUD_API_VERSION,
            })
        );
    }
}
