//! What ways there are to sign in, as the server has them configured.
//!
//! Provider-based sign-in (spike §4 ruling): the client renders its sign-in
//! affordance from this vocabulary rather than hard-coding "Google" — prod
//! reports one [`OidcOption`], local dev adds a `dev_picker`, and a future
//! self-host password method is another `oidc`-shaped entry or a sibling
//! field, not a client code fork.

use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

/// Answers [`crate::request::LoginOptions`]. Anonymous-callable — this is
/// how a signed-out client discovers what "Sign in" should even do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginOptionsInfo {
    /// External (OIDC) connections, in render order. Empty means none are
    /// configured.
    pub oidc: Vec<OidcOption>,
    /// Present only when the deployment's local connection has the
    /// passwordless dev picker enabled. Always `None` in production.
    pub dev_picker: Option<DevPickerOptions>,
}

/// One external sign-in connection the server has configured.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OidcOption {
    /// Stable connection id, e.g. `"google"`.
    pub id: String,
    /// Human label for the sign-in affordance, e.g. `"Google"`.
    pub label: String,
    /// Path the client links to, e.g. `"/auth/google"`. The client appends
    /// `?next=<path>` itself.
    pub start_path: String,
}

/// The passwordless dev-picker connection, present only in local
/// development.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevPickerOptions {
    /// Path the client links to, e.g. `"/auth/dev"`.
    pub start_path: String,
    /// The seeded profiles the picker offers.
    pub choices: Vec<DevChoice>,
}

/// One seeded dev-picker profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevChoice {
    /// The profile's email, used to look up or mint its account.
    pub email: String,
    /// The profile's display name.
    pub display_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;

    fn sample() -> LoginOptionsInfo {
        LoginOptionsInfo {
            oidc: vec![OidcOption {
                id: "google".to_string(),
                label: "Google".to_string(),
                start_path: "/auth/google".to_string(),
            }],
            dev_picker: Some(DevPickerOptions {
                start_path: "/auth/dev".to_string(),
                choices: vec![DevChoice {
                    email: "dev@example.com".to_string(),
                    display_name: "Dev User".to_string(),
                }],
            }),
        }
    }

    #[test]
    fn serde_round_trip() {
        let info = sample();
        let json = serde_json::to_string(&info).unwrap();
        let back: LoginOptionsInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back, info);
    }

    #[test]
    fn serde_round_trip_prod_shape() {
        let info = LoginOptionsInfo {
            oidc: vec![OidcOption {
                id: "google".to_string(),
                label: "Google".to_string(),
                start_path: "/auth/google".to_string(),
            }],
            dev_picker: None,
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: LoginOptionsInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back, info);
    }

    /// Pinned JSON literal: the deployed format is the contract.
    #[test]
    fn pinned_json_literal() {
        let info = LoginOptionsInfo {
            oidc: vec![OidcOption {
                id: "google".to_string(),
                label: "Google".to_string(),
                start_path: "/auth/google".to_string(),
            }],
            dev_picker: None,
        };
        assert_eq!(
            serde_json::to_string(&info).unwrap(),
            r#"{"oidc":[{"id":"google","label":"Google","startPath":"/auth/google"}],"devPicker":null}"#
        );
    }

    /// The dev-picker family pinned too: nested `choices` must not have
    /// picked up an unexpected rename.
    #[test]
    fn pinned_json_literal_dev_picker() {
        let info = LoginOptionsInfo {
            oidc: vec![],
            dev_picker: Some(DevPickerOptions {
                start_path: "/auth/dev".to_string(),
                choices: vec![DevChoice {
                    email: "dev@example.com".to_string(),
                    display_name: "Dev User".to_string(),
                }],
            }),
        };
        assert_eq!(
            serde_json::to_string(&info).unwrap(),
            r#"{"oidc":[],"devPicker":{"startPath":"/auth/dev","choices":[{"email":"dev@example.com","displayName":"Dev User"}]}}"#
        );
    }
}
