//! The Google sign-in round trip, end to end, against a stub Google.
//!
//! `LP_CLOUD_GOOGLE_ENDPOINT_BASE` points the flow's two outbound calls at an
//! axum router bound to a loopback port in this process, so these tests run
//! the *real* handler — real reqwest, real form encoding, real bearer header
//! — with no network and no credentials. Nothing here talks to Google, and
//! nothing here is a mock of our own code.

mod edge_harness;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::extract::{Form, State};
use axum::http::{HeaderMap, Request, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use edge_harness::{Session, TestServer};
use lpc_cloud_api::request::{AddMember, PublishProject};
use lpc_cloud_api::response::{ProjectList, UserInfo};
use lpc_cloud_api::{Actor, CloudRequest, CloudResponse, Visibility};
use lpc_history::{PrefixedUid, UidPrefix};

/// The happy path, from the button to a session: the redirect pins a state,
/// the callback spends it, and the next request is somebody.
#[tokio::test]
async fn a_full_round_trip_signs_the_user_in() {
    let google = StubGoogle::start().await;
    let server = google.server(&[]);

    let started = start_sign_in(&server, None).await;
    let (response, session) = finish_sign_in(&server, &started, "yona").await;

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(location(&response), Some("/".to_string()));

    let session = session.expect("the callback sets a session cookie");
    assert!(session.set_cookie.contains("HttpOnly"));
    assert!(session.set_cookie.contains("SameSite=Lax"));
    assert!(matches!(actor(&server, &session).await, Actor::User(_)));

    // The state cookie is retired by the same response that used it: a state
    // is good for exactly one attempt.
    let retired = cookie_named(&response, "lp_oauth_state").expect("the state cookie is cleared");
    assert!(retired.contains("Max-Age=0"));

    // And the exchange was the one Google's console will have to match.
    let exchange = google.last_exchange();
    assert_eq!(exchange["grant_type"], "authorization_code");
    assert_eq!(exchange["client_id"], "test-client-id");
    assert_eq!(exchange["client_secret"], "test-client-secret");
    assert_eq!(
        exchange["redirect_uri"],
        "http://localhost:31415/auth/google/callback"
    );
}

/// What the browser is actually sent to. Every parameter here is one Google
/// will reject the request without.
#[tokio::test]
async fn the_authorize_redirect_carries_our_client_and_a_state() {
    let google = StubGoogle::start().await;
    let server = google.server(&[]);

    let response = server.get("/auth/google").await;
    assert_eq!(response.status(), StatusCode::FOUND);

    let location = location(&response).expect("a Location");
    assert!(location.starts_with(&format!("{}/o/oauth2/v2/auth?", google.base)));
    assert!(location.contains("client_id=test-client-id"));
    assert!(location.contains("response_type=code"));
    assert!(location.contains("scope=openid%20email%20profile"));
    assert!(location.contains("prompt=select_account"));
    assert!(
        location.contains("redirect_uri=http%3A%2F%2Flocalhost%3A31415%2Fauth%2Fgoogle%2Fcallback")
    );

    let cookie = cookie_named(&response, "lp_oauth_state").expect("a state cookie");
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Lax"));
    assert!(cookie.contains("Path=/auth"));
    assert!(cookie.contains("Max-Age=600"));
    assert!(!cookie.contains("Secure"), "the dev origin is plain HTTP");

    // The state in the URL is the state in the cookie, or the check at the
    // callback could never pass.
    let state = query_param(&location, "state").expect("a state parameter");
    assert!(cookie.starts_with(&format!("lp_oauth_state={state};")));
}

/// A callback whose state does not match the cookie is somebody else's
/// request — a CSRF'd sign-in, or a tab left open through a restart.
#[tokio::test]
async fn a_state_that_does_not_match_the_cookie_is_refused() {
    let google = StubGoogle::start().await;
    let server = google.server(&[]);
    let started = start_sign_in(&server, None).await;

    let forged = Started {
        state: "AAAA.".to_string(),
        cookie: started.cookie.clone(),
    };
    let (response, session) = finish_sign_in(&server, &forged, "yona").await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(session.is_none(), "a refused callback mints no session");
    assert_eq!(google.exchanges(), 0, "the code is never spent");
}

/// No cookie at all is the same refusal: the cookie *is* the credential that
/// says this round trip started here.
#[tokio::test]
async fn a_callback_with_no_state_cookie_is_refused() {
    let google = StubGoogle::start().await;
    let server = google.server(&[]);
    let started = start_sign_in(&server, None).await;

    let response = server
        .get(&format!(
            "/auth/google/callback?code=yona&state={}",
            started.state
        ))
        .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(cookie_named(&response, "lp_session").is_none());
}

/// An unverified address would let anybody claim an invitation sent to
/// somebody else's mailbox.
#[tokio::test]
async fn an_unverified_email_never_becomes_a_session() {
    let google = StubGoogle::start().await;
    let server = google.server(&[]);
    let started = start_sign_in(&server, None).await;

    let (response, session) = finish_sign_in(&server, &started, "unverified").await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(session.is_none());
}

/// Q4/D10: a membership added for an address that has no account yet grants
/// nothing — until that address signs in, which is this moment.
#[tokio::test]
async fn an_invitation_becomes_access_at_first_sign_in() {
    let google = StubGoogle::start().await;
    let server = google.server(&[("LP_CLOUD_DEV_AUTH", "1")]);

    // The owner invites an address that belongs to nobody yet.
    let owner = server.sign_in("owner@example.com").await;
    let project = PrefixedUid::mint(UidPrefix::Project, &[7u8; 16]);
    server
        .call(
            CloudRequest::PublishProject(PublishProject {
                uid: project,
                visibility: Visibility::Private,
                slug: "zook-dome".into(),
            }),
            Some(&owner),
        )
        .await
        .result
        .expect("the project publishes");
    server
        .call(
            CloudRequest::AddMember(AddMember {
                uid: project,
                email: "invitee@example.com".into(),
            }),
            Some(&owner),
        )
        .await
        .result
        .expect("the invitation is recorded");

    // Now the invitee logs in with Google for the first time.
    let started = start_sign_in(&server, None).await;
    let (response, session) = finish_sign_in(&server, &started, "invitee").await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let session = session.expect("a session for the invitee");

    let reply = server
        .call(CloudRequest::ListMyProjects, Some(&session))
        .await;
    let Ok(CloudResponse::ProjectList(ProjectList { projects })) = reply.result else {
        panic!("expected a project list");
    };
    assert_eq!(
        projects.iter().map(|meta| meta.uid).collect::<Vec<_>>(),
        vec![project],
        "the pending membership resolved at login"
    );
}

/// The `next` never leaves the state we minted, so an attacker-supplied
/// absolute URL cannot become the landing page — it is dropped on the way
/// out and refused again on the way back.
#[tokio::test]
async fn a_hostile_next_lands_on_the_app_root() {
    let google = StubGoogle::start().await;
    let server = google.server(&[]);

    for hostile in [
        "https://evil.example/steal",
        "//evil.example",
        "/\\evil.example",
    ] {
        let started = start_sign_in(&server, Some(hostile)).await;
        let (response, session) = finish_sign_in(&server, &started, "yona").await;

        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            location(&response),
            Some("/".to_string()),
            "for next={hostile:?}"
        );
        assert!(session.is_some(), "the sign-in itself still succeeds");
    }
}

/// The other half of that rule: an ordinary in-app path is honoured, which is
/// what makes "sign in to see this project" land on the project.
#[tokio::test]
async fn a_relative_next_is_where_you_land() {
    let google = StubGoogle::start().await;
    let server = google.server(&[]);

    let started = start_sign_in(&server, Some("/p/zook-dome-prjabc")).await;
    let (response, _) = finish_sign_in(&server, &started, "yona").await;

    assert_eq!(location(&response), Some("/p/zook-dome-prjabc".to_string()));
}

/// Identity is the Google `sub`, not the address: changing your email at
/// Google does not hand you a second account.
#[tokio::test]
async fn a_changed_email_is_still_the_same_account() {
    let google = StubGoogle::start().await;
    let server = google.server(&[]);

    let first = sign_in_with(&server, "yona").await;
    let second = sign_in_with(&server, "yona-renamed").await;

    assert_eq!(actor(&server, &first).await, actor(&server, &second).await);
}

/// Logout deletes the row before it clears the cookie: a cookie cleared in
/// one browser must not leave a live credential for whatever else copied it.
#[tokio::test]
async fn logout_ends_the_session_on_the_server() {
    let google = StubGoogle::start().await;
    let server = google.server(&[]);
    let session = sign_in_with(&server, "yona").await;
    assert!(matches!(actor(&server, &session).await, Actor::User(_)));

    let response = server
        .request(
            Request::builder()
                .method("POST")
                .uri("/auth/logout")
                .header(header::COOKIE, &session.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let cleared = cookie_named(&response, "lp_session").expect("the cookie is cleared");
    assert!(cleared.contains("Max-Age=0"));
    // The token the browser still holds is now worth nothing.
    assert_eq!(actor(&server, &session).await, Actor::Anonymous);
}

/// Logging out without a session is not an error — a stale tab pressing the
/// button must not see a failure.
#[tokio::test]
async fn logging_out_twice_is_fine() {
    let server = TestServer::new();
    let response = server
        .request(
            Request::builder()
                .method("POST")
                .uri("/auth/logout")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

/// A server with no credentials says so, rather than redirecting the user to
/// a Google that will refuse them. This is the state of every local run that
/// has not been given a client id.
#[tokio::test]
async fn without_credentials_the_flow_says_it_is_not_configured() {
    let server = TestServer::new();

    assert_eq!(
        server.get("/auth/google").await.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        server
            .get("/auth/google/callback?code=x&state=y")
            .await
            .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
}

/// Google's own refusal — the user pressing "cancel" — is a 400 with no
/// session, not a 500.
#[tokio::test]
async fn a_provider_refusal_is_reported_not_crashed() {
    let google = StubGoogle::start().await;
    let server = google.server(&[]);
    let started = start_sign_in(&server, None).await;

    let response = server
        .request(
            Request::builder()
                .uri("/auth/google/callback?error=access_denied")
                .header(header::COOKIE, &started.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(cookie_named(&response, "lp_session").is_none());
}

/// A token endpoint that refuses the code is an upstream failure, and it says
/// so: 502, not a 500 that reads like our own bug.
#[tokio::test]
async fn a_failed_token_exchange_is_a_bad_gateway() {
    let google = StubGoogle::start().await;
    let server = google.server(&[]);
    let started = start_sign_in(&server, None).await;

    let (response, session) = finish_sign_in(&server, &started, "boom").await;

    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    assert!(session.is_none());
}

// ---- the stub Google ---------------------------------------------------

/// Every form the token endpoint was posted, in order.
type Exchanges = Arc<Mutex<Vec<BTreeMap<String, String>>>>;

/// An in-process stand-in for Google's token and userinfo endpoints.
///
/// It answers profiles by *code*: the token endpoint hands back `at-{code}`
/// as the access token, and the userinfo endpoint reads the code back out of
/// the bearer header. That makes a test's choice of profile a single string
/// at the call site, and it proves the access token made the trip.
struct StubGoogle {
    base: String,
    exchanges: Exchanges,
}

impl StubGoogle {
    async fn start() -> Self {
        let exchanges: Exchanges = Arc::new(Mutex::new(Vec::new()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a loopback port");
        let base = format!("http://{}", listener.local_addr().unwrap());

        let router = Router::new()
            .route("/token", post(stub_token))
            .route("/v1/userinfo", get(stub_userinfo))
            .with_state(Arc::clone(&exchanges));
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });

        Self { base, exchanges }
    }

    /// A service configured to sign in against this stub.
    fn server(&self, extra: &[(&str, &str)]) -> TestServer {
        let mut vars: Vec<(&str, &str)> = vec![
            ("LP_CLOUD_GOOGLE_CLIENT_ID", "test-client-id"),
            ("LP_CLOUD_GOOGLE_CLIENT_SECRET", "test-client-secret"),
            ("LP_CLOUD_GOOGLE_ENDPOINT_BASE", &self.base),
        ];
        vars.extend_from_slice(extra);
        TestServer::with_vars(&vars)
    }

    fn exchanges(&self) -> usize {
        self.exchanges.lock().unwrap().len()
    }

    fn last_exchange(&self) -> BTreeMap<String, String> {
        self.exchanges
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("the code was exchanged")
    }
}

async fn stub_token(
    State(exchanges): State<Exchanges>,
    Form(form): Form<BTreeMap<String, String>>,
) -> Response {
    let code = form.get("code").cloned().unwrap_or_default();
    exchanges.lock().unwrap().push(form);

    if code == "boom" {
        return (StatusCode::BAD_REQUEST, r#"{"error":"invalid_grant"}"#).into_response();
    }
    Json(serde_json::json!({
        "access_token": format!("at-{code}"),
        "token_type": "Bearer",
        "expires_in": 3599,
    }))
    .into_response()
}

async fn stub_userinfo(headers: HeaderMap) -> Response {
    let profile = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer at-"))
        .map(str::to_string);

    let body = match profile.as_deref() {
        Some("yona") => serde_json::json!({
            "sub": "1001", "email": "yona@example.com",
            "email_verified": true, "name": "Yona Appletree",
        }),
        // Same `sub`, new address — the account is the subject, not the email.
        Some("yona-renamed") => serde_json::json!({
            "sub": "1001", "email": "yona@newmail.example",
            "email_verified": true, "name": "Yona Appletree",
        }),
        Some("invitee") => serde_json::json!({
            "sub": "2002", "email": "invitee@example.com",
            "email_verified": true, "name": "Invited Person",
        }),
        Some("unverified") => serde_json::json!({
            "sub": "3003", "email": "someone@example.com",
            "email_verified": false, "name": "Unverified",
        }),
        _ => return StatusCode::UNAUTHORIZED.into_response(),
    };
    Json(body).into_response()
}

// ---- driving the flow --------------------------------------------------

/// A sign-in that has reached Google: the state we sent, and the cookie the
/// browser now holds.
struct Started {
    state: String,
    cookie: String,
}

/// `GET /auth/google`, keeping what the browser would keep.
async fn start_sign_in(server: &TestServer, next: Option<&str>) -> Started {
    let path = match next {
        Some(next) => format!("/auth/google?next={}", encode(next)),
        None => "/auth/google".to_string(),
    };
    let response = server.get(&path).await;
    assert_eq!(response.status(), StatusCode::FOUND, "for {path}");

    let location = location(&response).expect("a Location");
    let cookie = cookie_named(&response, "lp_oauth_state").expect("a state cookie");
    Started {
        state: query_param(&location, "state").expect("a state parameter"),
        cookie: cookie
            .split(';')
            .next()
            .expect("a name=value pair")
            .to_string(),
    }
}

/// The callback Google would send, with the cookie the browser would send.
async fn finish_sign_in(
    server: &TestServer,
    started: &Started,
    code: &str,
) -> (axum::http::Response<Body>, Option<Session>) {
    let response = server
        .request(
            Request::builder()
                .uri(format!(
                    "/auth/google/callback?code={code}&state={}",
                    encode(&started.state)
                ))
                .header(header::COOKIE, &started.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await;

    let session = cookie_named(&response, "lp_session")
        .filter(|cookie| !cookie.contains("Max-Age=0"))
        .map(|set_cookie| Session {
            cookie: set_cookie.split(';').next().unwrap().to_string(),
            set_cookie,
        });
    (response, session)
}

/// The whole round trip for a profile, for tests that only need the session.
async fn sign_in_with(server: &TestServer, code: &str) -> Session {
    let started = start_sign_in(server, None).await;
    let (response, session) = finish_sign_in(server, &started, code).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    session.expect("a session cookie")
}

async fn actor(server: &TestServer, session: &Session) -> Actor {
    let reply = server.call(CloudRequest::WhoAmI, Some(session)).await;
    match reply.result {
        Ok(CloudResponse::UserInfo(UserInfo { actor })) => actor,
        other => panic!("expected UserInfo, got {other:?}"),
    }
}

fn location(response: &axum::http::Response<Body>) -> Option<String> {
    Some(
        response
            .headers()
            .get(header::LOCATION)?
            .to_str()
            .ok()?
            .to_string(),
    )
}

/// One `Set-Cookie` header by cookie name — the whole attribute string.
fn cookie_named(response: &axum::http::Response<Body>, name: &str) -> Option<String> {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with(&format!("{name}=")))
        .map(str::to_string)
}

fn query_param(url: &str, name: &str) -> Option<String> {
    url.split_once('?')?
        .1
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.to_string())
}

/// Percent-encode a query value, the way a browser would.
fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}
