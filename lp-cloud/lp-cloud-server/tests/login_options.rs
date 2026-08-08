//! `LoginOptions`: what the server reports it can sign in with.
//!
//! The connections themselves are wired from config at startup (P3,
//! `ServerConfig::login_providers`) — this file is the edge-level check that
//! every config permutation answers truthfully. The dev picker's `choices`
//! are the other half: not config at all, read live from whoever has
//! actually signed in.

mod edge_harness;

use edge_harness::TestServer;
use lpc_cloud_api::{CloudRequest, CloudResponse, LoginOptionsInfo};

/// Neither connection configured is the state of a fresh checkout with no
/// secrets and no flag — `LoginOptions` should say so rather than default to
/// either one.
#[tokio::test]
async fn neither_connection_is_configured_by_default() {
    let server = TestServer::with_vars(&[("LP_CLOUD_DEV_AUTH", "0")]);
    let info = login_options(&server).await;
    assert!(info.oidc.is_empty());
    assert!(info.dev_picker.is_none());
}

#[tokio::test]
async fn google_alone_reports_one_connection_and_no_picker() {
    let server = TestServer::with_vars(&[
        ("LP_CLOUD_DEV_AUTH", "0"),
        ("LP_CLOUD_GOOGLE_CLIENT_ID", "id.apps.googleusercontent.com"),
        ("LP_CLOUD_GOOGLE_CLIENT_SECRET", "shh"),
    ]);
    let info = login_options(&server).await;
    assert_eq!(info.oidc.len(), 1);
    assert_eq!(info.oidc[0].id, "google");
    assert_eq!(info.oidc[0].label, "Google");
    assert_eq!(info.oidc[0].start_path, "/auth/google");
    assert!(info.dev_picker.is_none());
}

#[tokio::test]
async fn dev_alone_reports_the_picker_and_no_oidc() {
    // `TestServer::new` is dev-auth-on, localhost, no google credentials.
    let server = TestServer::new();
    let info = login_options(&server).await;
    assert!(info.oidc.is_empty());
    assert_eq!(
        info.dev_picker.expect("dev picker on").start_path,
        "/auth/dev"
    );
}

#[tokio::test]
async fn both_together_report_both() {
    let server = TestServer::with_vars(&[
        ("LP_CLOUD_DEV_AUTH", "1"),
        ("LP_CLOUD_GOOGLE_CLIENT_ID", "id.apps.googleusercontent.com"),
        ("LP_CLOUD_GOOGLE_CLIENT_SECRET", "shh"),
    ]);
    let info = login_options(&server).await;
    assert_eq!(info.oidc.len(), 1);
    assert!(info.dev_picker.is_some());
}

/// Half a Google credential is unconfigured, same as `config.rs`'s own rule
/// — `LoginOptions` must not show a connection the callback would refuse.
#[tokio::test]
async fn a_half_configured_google_credential_reports_no_connection() {
    let server = TestServer::with_vars(&[
        ("LP_CLOUD_DEV_AUTH", "0"),
        ("LP_CLOUD_GOOGLE_CLIENT_ID", "id.apps.googleusercontent.com"),
    ]);
    let info = login_options(&server).await;
    assert!(info.oidc.is_empty());
}

/// The dev picker's `choices` are today's seeded accounts, live from the
/// store — empty until somebody has actually signed in, and never carried in
/// configuration.
#[tokio::test]
async fn the_pickers_choices_are_todays_seeded_accounts() {
    let server = TestServer::new();
    let before = login_options(&server).await;
    assert!(before.dev_picker.expect("picker on").choices.is_empty());

    server.sign_in("alice@example.com").await;
    server.sign_in("bob@example.com").await;

    let after = login_options(&server).await;
    let emails: Vec<_> = after
        .dev_picker
        .expect("picker on")
        .choices
        .into_iter()
        .map(|choice| choice.email)
        .collect();
    assert_eq!(emails, vec!["alice@example.com", "bob@example.com"]);
}

/// Anonymous-callable: this is how a signed-out client discovers what "Sign
/// in" should even do, before it knows whether anyone is logged in.
#[tokio::test]
async fn login_options_answers_with_no_session() {
    let server = TestServer::new();
    let info = login_options(&server).await;
    assert!(info.dev_picker.is_some());
}

async fn login_options(server: &TestServer) -> LoginOptionsInfo {
    let reply = server.call(CloudRequest::LoginOptions, None).await;
    let Ok(CloudResponse::LoginOptionsInfo(info)) = reply.result else {
        panic!("expected LoginOptionsInfo, got {:?}", reply.result);
    };
    info
}
