//! `POST /api` at the edge: the envelope, the version, and the cookie.
//!
//! What is deliberately *not* here: visibility, membership, push validation,
//! and every other rule `lp-cloud-domain` already owns and tests. These
//! tests only prove that a request reaches the domain as the right caller,
//! or is refused before it gets there.

mod edge_harness;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use edge_harness::TestServer;
use lpc_cloud_api::request::{GetProject, PublishProject};
use lpc_cloud_api::response::{ProjectInfo, UserInfo};
use lpc_cloud_api::{
    Actor, CLOUD_API_VERSION, CloudError, CloudRequest, CloudResponse, Visibility,
};
use lpc_history::{PrefixedUid, UidPrefix};

/// A client speaking another vocabulary is refused by name, with both
/// versions, and its request is never looked at.
#[tokio::test]
async fn a_mismatched_version_is_refused_before_the_request_is_read() {
    let server = TestServer::new();
    let body = format!(
        r#"{{"version":{},"request":"whoAmI"}}"#,
        CLOUD_API_VERSION + 1
    );

    let reply = server.call_raw(body.as_bytes(), None).await;

    assert_eq!(reply.version, CLOUD_API_VERSION);
    assert_eq!(
        reply.result,
        Err(CloudError::VersionMismatch {
            client: CLOUD_API_VERSION + 1,
            server: CLOUD_API_VERSION,
        })
    );
}

/// The refusal above must reach the client as an *answer*, not as a
/// transport failure: the client's error vocabulary distinguishes "the
/// service said no" from "the service was not reached", and a 4xx here would
/// collapse the two.
#[tokio::test]
async fn a_refusal_is_still_http_200() {
    let server = TestServer::new();
    let response = server
        .request(
            Request::builder()
                .method("POST")
                .uri("/api")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "version": CLOUD_API_VERSION,
                        "request": { "getProject": { "uid": uid().to_string() } }
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
}

/// No cookie is a caller too: an anonymous `WhoAmI` is answered, not
/// rejected.
#[tokio::test]
async fn no_cookie_resolves_to_the_anonymous_actor() {
    let server = TestServer::new();
    let reply = server.call(CloudRequest::WhoAmI, None).await;

    assert_eq!(
        reply.result,
        Ok(CloudResponse::UserInfo(UserInfo {
            actor: Actor::Anonymous
        }))
    );
}

/// The cookie is what makes a request somebody's. This is the edge's whole
/// contribution to identity.
#[tokio::test]
async fn the_session_cookie_becomes_the_calling_actor() {
    let server = TestServer::new();
    let session = server.sign_in("yona@example.com").await;

    let signed_in = server.call(CloudRequest::WhoAmI, Some(&session)).await;
    let anonymous = server.call(CloudRequest::WhoAmI, None).await;

    let Ok(CloudResponse::UserInfo(UserInfo { actor })) = signed_in.result else {
        panic!("expected a UserInfo answer");
    };
    assert!(matches!(actor, Actor::User(_)));
    assert_eq!(
        anonymous.result,
        Ok(CloudResponse::UserInfo(UserInfo {
            actor: Actor::Anonymous
        }))
    );
}

/// A cookie that is not a live session logs you out; it never becomes an
/// error the user cannot clear.
#[tokio::test]
async fn an_unknown_cookie_is_anonymous_rather_than_an_error() {
    let server = TestServer::new();
    let response = server
        .request(
            Request::builder()
                .method("POST")
                .uri("/api")
                .header("cookie", "lp_session=AAAA-not-a-session")
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "version": CLOUD_API_VERSION,
                        "request": "whoAmI"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let reply: lpc_cloud_api::CloudReply =
        serde_json::from_slice(&edge_harness::body_bytes(response).await).unwrap();
    assert_eq!(
        reply.result,
        Ok(CloudResponse::UserInfo(UserInfo {
            actor: Actor::Anonymous
        }))
    );
}

/// A body that is not a `CloudCall` at all is the one control-plane failure
/// that is *not* an answer — there is no envelope to answer inside.
#[tokio::test]
async fn a_body_that_is_not_an_envelope_is_a_400() {
    let server = TestServer::new();
    let response = server
        .request(
            Request::builder()
                .method("POST")
                .uri("/api")
                .body(Body::from("not json"))
                .unwrap(),
        )
        .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// The plane carries the real vocabulary, not a subset: a publish followed
/// by a read comes back with the domain's own answer.
#[tokio::test]
async fn a_published_project_is_readable_through_the_plane() {
    let server = TestServer::new();
    let session = server.sign_in("yona@example.com").await;
    let uid = uid();

    let published = server
        .call(
            CloudRequest::PublishProject(PublishProject {
                uid,
                visibility: Visibility::Link,
                slug: "zook-dome".into(),
            }),
            Some(&session),
        )
        .await;
    assert!(published.result.is_ok(), "{:?}", published.result);

    let read = server
        .call(CloudRequest::GetProject(GetProject { uid }), None)
        .await;
    let Ok(CloudResponse::ProjectInfo(ProjectInfo { meta, .. })) = read.result else {
        panic!("expected ProjectInfo");
    };
    assert_eq!(meta.uid, uid);
    assert_eq!(meta.slug, "zook-dome");
}

fn uid() -> PrefixedUid {
    PrefixedUid::mint(UidPrefix::Project, &[42u8; 16])
}
