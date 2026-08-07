//! Server-configured sign-in connections, injected at
//! [`crate::cloud_service::CloudService`] construction.

use alloc::string::String;
use alloc::vec::Vec;

/// What ways there are to sign in — server *configuration*, as opposed to
/// [`lpc_cloud_api::LoginOptionsInfo`], which is the *answer*
/// [`crate::cloud_service::CloudService::login_options`] builds from this
/// plus a live store read.
///
/// It is one level removed from the wire type on purpose: the dev picker's
/// `choices` are not configuration, they are today's seeded accounts, so
/// they are queried live via `MetaStore::users` rather than carried here —
/// see [`DevPickerConnection`].
///
/// Empty by default (`CloudService::new` starts here) — a service that
/// never calls
/// [`with_login_providers`](crate::cloud_service::CloudService::with_login_providers)
/// answers `LoginOptions` with nothing configured, the same as a deployment
/// whose config never set any up.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LoginProviders {
    /// External (OIDC) connections, in render order.
    pub oidc: Vec<OidcConnection>,
    /// The passwordless dev picker, present only when this deployment has
    /// it turned on (P3 wires that from server config; local dev only).
    pub dev_picker: Option<DevPickerConnection>,
}

/// One external sign-in connection the server is configured with.
#[derive(Debug, Clone, PartialEq)]
pub struct OidcConnection {
    /// Stable connection id, e.g. `"google"`.
    pub id: String,
    /// Human label for the sign-in affordance, e.g. `"Google"`.
    pub label: String,
    /// Path the client links to, e.g. `"/auth/google"`.
    pub start_path: String,
}

/// The dev-picker connection is on; its live choices come from
/// [`MetaStore::users`](crate::ports::meta_store::MetaStore::users), not
/// from here.
#[derive(Debug, Clone, PartialEq)]
pub struct DevPickerConnection {
    /// Path the client links to, e.g. `"/auth/dev"`.
    pub start_path: String,
}
