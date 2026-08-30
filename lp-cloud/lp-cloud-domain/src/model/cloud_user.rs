//! An account on the cloud service.

use alloc::format;
use alloc::string::{String, ToString};
use lpc_history::PrefixedUid;

/// A user account, identified by their Google `sub` (identity) rather than
/// their email (which can change).
///
/// The uid is minted with [`lpc_history::UidPrefix::User`] (`usr…`) from
/// random bytes supplied by the [`crate::ports::id_mint::IdMint`] port — the
/// domain never generates randomness itself.
///
/// # Profile field seeding rules
///
/// - `given_name` / `family_name` are seeded once, at account creation, and
///   **never overwritten by a later login** — a user who edits their name
///   (`UpdateMe`) has the final word, and a provider re-reporting its own
///   idea of the name must not clobber that edit.
/// - `picture_url` is the opposite: it is refreshed on **every** login, so
///   an avatar changed at the provider shows up here without the account
///   holder doing anything. The bytes themselves are never stored — this is
///   always a hotlink.
/// - `display_name` is a derived, denormalized field, recomputed by
///   [`recompute_display_name`](CloudUser::recompute_display_name) whenever
///   `given_name`/`family_name` change (account creation or `UpdateMe`) —
///   see that method for the exact fallback order.
#[derive(Debug, Clone, PartialEq)]
pub struct CloudUser {
    /// The account's uid (`usr…`).
    pub uid: PrefixedUid,
    /// Google's stable subject identifier. Identity lives here, not in
    /// `email`, because a Google account can change its address.
    pub google_sub: String,
    /// Verified email address, normalized to lowercase. Matched against
    /// pending membership rows at first login (Q4).
    pub email: String,
    /// Display name, as Google reported it.
    pub display_name: String,
    /// Given (first) name. Seeded at account creation, editable by the
    /// account holder afterward — see the struct-level seeding rules.
    pub given_name: Option<String>,
    /// Family (last) name. Same seeding rules as `given_name`.
    pub family_name: Option<String>,
    /// Profile photo, hotlinked to the provider — never stored as bytes.
    /// Refreshed on every login (struct-level rules).
    pub picture_url: Option<String>,
    /// The connection this account was created through: `"google"` or
    /// `"dev"` today. Set once, at creation, and never changed afterward —
    /// an account's sign-in method is not something a later login can
    /// silently reassign. [`crate::cloud_service::CloudService::get_me`]
    /// maps this to the wire's human `provider_label`.
    pub provider: String,
    /// When the account was first seen, f64 epoch seconds from the clock
    /// port.
    pub created_at: f64,
    /// A guest account (examples vision D8): minted for an anonymous
    /// fork's publish, owned by whoever holds its browser session cookie.
    /// **The pruning mark** — D8 requires guest-owned rows to be clearly
    /// queryable, and this flag (mirrored by `provider = "anonymous"`) is
    /// that lever: guest users → their owned projects is one obvious join.
    pub anonymous: bool,
}

impl CloudUser {
    /// Recompute `display_name` from `given_name`/`family_name`.
    ///
    /// Both set: space-joined (`"{given} {family}"`). Only one set: that one
    /// alone (the mononym case). Neither set (or both empty after
    /// trimming): fall back to `display_name`'s own existing value if that
    /// is non-empty, and only past that to the email local-part — the same
    /// derivation `google_auth.rs:234-237` does before the account exists.
    /// In practice an account already has a non-empty `display_name` by the
    /// time this runs (creation seeds it from the provider or that same
    /// email fallback), so the email branch here is a defensive last
    /// resort, not the common path.
    ///
    /// Called from account creation once given/family are captured (P3) and
    /// from `UpdateMe` (P2) — the one place this logic lives, so the two
    /// callers cannot drift.
    pub fn recompute_display_name(&mut self) {
        let given = self.given_name.as_deref().unwrap_or("").trim();
        let family = self.family_name.as_deref().unwrap_or("").trim();
        let joined = match (given.is_empty(), family.is_empty()) {
            (false, false) => format!("{given} {family}"),
            (false, true) => given.to_string(),
            (true, false) => family.to_string(),
            (true, true) => String::new(),
        };
        self.display_name = if !joined.is_empty() {
            joined
        } else if !self.display_name.trim().is_empty() {
            return;
        } else {
            email_local_part(&self.email)
        };
    }
}

/// The part of an email before the `@`, for a display name that has nothing
/// better to show. Matches `google_auth.rs:234-237`'s own fallback.
fn email_local_part(email: &str) -> String {
    email.split('@').next().unwrap_or(email).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_history::UidPrefix;

    fn user() -> CloudUser {
        CloudUser {
            uid: PrefixedUid::mint(UidPrefix::User, &[1u8; 16]),
            google_sub: "g-1".to_string(),
            email: "yona@example.com".to_string(),
            display_name: "Provider Name".to_string(),
            given_name: None,
            family_name: None,
            picture_url: None,
            provider: "google".to_string(),
            created_at: 1.0,
            anonymous: false,
        }
    }

    #[test]
    fn both_names_join_with_a_space() {
        let mut u = user();
        u.given_name = Some("Yona".to_string());
        u.family_name = Some("Appletree".to_string());
        u.recompute_display_name();
        assert_eq!(u.display_name, "Yona Appletree");
    }

    /// The mononym case: only one name set.
    #[test]
    fn a_cleared_family_name_leaves_only_the_given_name() {
        let mut u = user();
        u.given_name = Some("Yona".to_string());
        u.family_name = None;
        u.recompute_display_name();
        assert_eq!(u.display_name, "Yona");
    }

    #[test]
    fn whitespace_only_names_count_as_unset() {
        let mut u = user();
        u.given_name = Some("  ".to_string());
        u.family_name = Some(" Appletree ".to_string());
        u.recompute_display_name();
        assert_eq!(u.display_name, "Appletree");
    }

    /// Neither name set: the existing `display_name` is left alone rather
    /// than being blanked.
    #[test]
    fn no_names_leaves_the_existing_display_name() {
        let mut u = user();
        u.recompute_display_name();
        assert_eq!(u.display_name, "Provider Name");
    }

    /// The defensive last resort: no names *and* no existing display name.
    #[test]
    fn no_names_and_no_display_name_falls_back_to_the_email_local_part() {
        let mut u = user();
        u.display_name = String::new();
        u.recompute_display_name();
        assert_eq!(u.display_name, "yona");
    }
}
