//! `GET /auth/dev`: the password-free login, and the two locks on it.

mod edge_harness;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use edge_harness::{Session, TestServer, header_value};
use lpc_cloud_api::response::UserInfo;
use lpc_cloud_api::{Actor, CloudRequest, CloudResponse};

/// The happy path: a cookie is set, and the next request is somebody.
#[tokio::test]
async fn signing_in_sets_a_session_cookie_that_names_a_user() {
    let server = TestServer::new();
    let session = server.sign_in("yona@example.com").await;

    assert!(session.set_cookie.starts_with("lp_session="));
    assert!(session.set_cookie.contains("HttpOnly"));
    assert!(session.set_cookie.contains("SameSite=Lax"));
    assert!(session.set_cookie.contains("Path=/"));

    let reply = server.call(CloudRequest::WhoAmI, Some(&session)).await;
    let Ok(CloudResponse::UserInfo(UserInfo { actor })) = reply.result else {
        panic!("expected UserInfo");
    };
    assert!(matches!(actor, Actor::User(_)));
}

/// Off by default. A disabled back door answers 404 rather than 403: a
/// refusal that names the feature is still an advertisement for it.
#[tokio::test]
async fn the_route_does_not_exist_when_dev_auth_is_off() {
    let server = TestServer::with_vars(&[("LP_CLOUD_DEV_AUTH", "0")]);
    let response = server.get("/auth/dev?email=yona@example.com").await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(header_value(&response, header::SET_COOKIE), None);
}

/// Q23's second lock: the flag alone does not open the door on a deployed
/// origin. A secrets file copied from a laptop to production must not grow
/// the service a password-free login.
#[tokio::test]
async fn the_flag_is_ignored_on_a_non_localhost_origin() {
    let server = TestServer::with_vars(&[
        ("LP_CLOUD_DEV_AUTH", "1"),
        ("LP_CLOUD_BASE_URL", "https://lightplayer.app"),
    ]);

    let response = server.get("/auth/dev?email=yona@example.com").await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(header_value(&response, header::SET_COOKIE), None);
}

/// The cookie's `Secure` attribute follows the origin's scheme, because a
/// `Secure` cookie on a plain-HTTP dev origin is silently dropped by the
/// browser — a login that appears to work and does nothing.
#[tokio::test]
async fn secure_is_left_off_the_plain_http_dev_origin() {
    let server = TestServer::new();
    let session = server.sign_in("yona@example.com").await;
    assert!(!session.set_cookie.contains("Secure"));
}

#[tokio::test]
async fn a_missing_or_malformed_email_is_a_usage_error() {
    let server = TestServer::new();

    assert_eq!(
        server.get("/auth/dev").await.status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        server.get("/auth/dev?email=nobody").await.status(),
        StatusCode::BAD_REQUEST
    );
}

/// Signing in twice as the same person is the same account — the upsert is
/// keyed on identity, not on the login event.
#[tokio::test]
async fn signing_in_twice_is_one_account() {
    let server = TestServer::new();
    let first = server.sign_in("yona@example.com").await;
    let second = server.sign_in("yona@example.com").await;

    assert_ne!(
        first.cookie, second.cookie,
        "each login mints its own session token"
    );
    assert_eq!(actor(&server, &first).await, actor(&server, &second).await);
}

/// No provider profile exists behind the dev login, so it falls back to the
/// same thing the real login falls back to when Google reports no name — the
/// email local-part — written through the shared derivation
/// (`CloudUser::recompute_display_name`) rather than set directly.
#[tokio::test]
async fn a_first_login_seeds_the_given_name_from_the_email_local_part() {
    let server = TestServer::new();
    let session = server.sign_in("yona@example.com").await;

    let reply = server.call(CloudRequest::GetMe, Some(&session)).await;
    let Ok(CloudResponse::MeInfo(info)) = reply.result else {
        panic!("expected MeInfo, got {:?}", reply.result);
    };
    assert_eq!(info.given_name.as_deref(), Some("yona"));
    assert_eq!(info.family_name, None);
    assert_eq!(info.display_name, "yona");
}

/// `?given=`/`?family=` exist only so a test can exercise the capture path
/// without a stub Google — a real login never carries them.
#[tokio::test]
async fn given_and_family_query_params_seed_a_new_account() {
    let server = TestServer::new();
    let response = server
        .get("/auth/dev?email=yona@example.com&given=Yona&family=Appletree")
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let session = session_from(&response);

    let reply = server.call(CloudRequest::GetMe, Some(&session)).await;
    let Ok(CloudResponse::MeInfo(info)) = reply.result else {
        panic!("expected MeInfo, got {:?}", reply.result);
    };
    assert_eq!(info.given_name.as_deref(), Some("Yona"));
    assert_eq!(info.family_name.as_deref(), Some("Appletree"));
    assert_eq!(info.display_name, "Yona Appletree");
}

/// The edge captures the request's `User-Agent`, same as the Google callback
/// does — this is the dev login's half of that.
#[tokio::test]
async fn the_user_agent_lands_on_the_session_row() {
    let server = TestServer::new();
    let response = server
        .request(
            Request::builder()
                .uri("/auth/dev?email=yona@example.com")
                .header(header::USER_AGENT, "DevBrowser/9.0")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let session = session_from(&response);

    let reply = server
        .call(CloudRequest::ListSessions, Some(&session))
        .await;
    let Ok(CloudResponse::SessionList(list)) = reply.result else {
        panic!("expected SessionList, got {:?}", reply.result);
    };
    assert_eq!(list.sessions.len(), 1);
    assert_eq!(
        list.sessions[0].user_agent.as_deref(),
        Some("DevBrowser/9.0")
    );
}

async fn actor(server: &TestServer, session: &edge_harness::Session) -> Actor {
    let reply = server.call(CloudRequest::WhoAmI, Some(session)).await;
    match reply.result {
        Ok(CloudResponse::UserInfo(UserInfo { actor })) => actor,
        other => panic!("expected UserInfo, got {other:?}"),
    }
}

/// Pull a `Session` out of a redirect's `Set-Cookie`, for the tests above
/// that drive `/auth/dev` with a hand-built request rather than
/// `TestServer::sign_in`.
fn session_from(response: &axum::http::Response<Body>) -> Session {
    let set_cookie = header_value(response, header::SET_COOKIE)
        .expect("a session cookie")
        .to_string();
    Session {
        cookie: set_cookie.split(';').next().unwrap().to_string(),
        set_cookie,
    }
}
