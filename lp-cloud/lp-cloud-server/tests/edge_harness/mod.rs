//! A whole service in a test: mem-backed stores, a tempdir "artifact", and
//! a router driven by `tower::ServiceExt::oneshot`.
//!
//! No socket is bound and no port is chosen, because none of these tests is
//! about the network — they are about routes, headers, and status codes.
//! The domain is the real one (D13: the in-memory adapters are first-class,
//! not stubs), so a test here that needs a published project publishes one
//! through the API rather than reaching behind it.

#![allow(
    dead_code,
    reason = "one harness serves four test binaries; each uses the part of it that it needs"
)]

use axum::Router;
use axum::body::Body;
use axum::http::{Request, Response, StatusCode, header};
use http_body_util::BodyExt as _;
use lp_cloud_server::app_state::AppState;
use lp_cloud_server::config::ServerConfig;
use lp_cloud_server::page::static_site::StaticSite;
use lp_cloud_server::ports::{AnyBlobStore, AnyMetaStore};
use lp_cloud_server::router::build_router;
use lp_cloud_store_mem::{MemBlobStore, MemMetaStore};
use lpc_cloud_api::{CLOUD_API_VERSION, CloudCall, CloudReply, CloudRequest};
use tempfile::TempDir;
use tower::ServiceExt as _;

/// The `index.html` the fake artifact holds. Small, but with the two things
/// the page plane cares about: a `</head>` to inject before, and a body that
/// proves the document was served rather than an asset.
pub const INDEX_HTML: &str =
    "<!doctype html><html><head><title>LightPlayer</title></head><body>app</body></html>";

/// One hashed asset, to prove long-cache headers and the file-vs-document
/// fork.
pub const ASSET_PATH: &str = "/assets/app-a1b2c3d4.js";
pub const ASSET_BODY: &str = "console.log('studio')";

/// A service under test.
pub struct TestServer {
    router: Router,
    /// Kept alive: dropping it deletes the artifact directory.
    _artifact: TempDir,
}

impl TestServer {
    /// A service with dev auth enabled — the usual case, since a test that
    /// needs a session has no other way to get one.
    pub fn new() -> Self {
        Self::with_vars(&[("LP_CLOUD_DEV_AUTH", "1")])
    }

    /// A service configured by environment variables, minus the two the
    /// harness owns (`LP_CLOUD_STORE`/`LP_CLOUD_BLOBS` are always `mem`, and
    /// the base URL is a localhost one unless a test overrides it).
    pub fn with_vars(vars: &[(&str, &str)]) -> Self {
        let artifact = tempfile::tempdir().expect("a temp artifact directory");
        std::fs::write(artifact.path().join("index.html"), INDEX_HTML).unwrap();
        std::fs::create_dir_all(artifact.path().join("assets")).unwrap();
        std::fs::write(
            artifact.path().join(ASSET_PATH.trim_start_matches('/')),
            ASSET_BODY,
        )
        .unwrap();

        let mut settings: Vec<(String, String)> = vec![
            ("LP_CLOUD_STORE".into(), "mem".into()),
            ("LP_CLOUD_BLOBS".into(), "mem".into()),
            ("LP_CLOUD_BASE_URL".into(), "http://localhost:31415".into()),
        ];
        for (name, value) in vars {
            settings.retain(|(existing, _)| existing != name);
            settings.push(((*name).to_string(), (*value).to_string()));
        }

        let config = ServerConfig::from_vars(move |name| {
            settings
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.clone())
        })
        .expect("the harness configuration parses");

        let state = AppState::new(
            config,
            AnyMetaStore::new(MemMetaStore::new()),
            AnyBlobStore::new(MemBlobStore::new()),
            StaticSite::open(Some(artifact.path())),
        );

        Self {
            router: build_router(state),
            _artifact: artifact,
        }
    }

    /// Drive one request through the whole router.
    pub async fn request(&self, request: Request<Body>) -> Response<Body> {
        self.router
            .clone()
            .oneshot(request)
            .await
            .expect("the router answers every request")
    }

    /// `GET path` with no session.
    pub async fn get(&self, path: &str) -> Response<Body> {
        self.request(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
    }

    /// `GET path` carrying a session cookie.
    pub async fn get_as(&self, path: &str, session: &Session) -> Response<Body> {
        self.request(
            Request::builder()
                .uri(path)
                .header(header::COOKIE, &session.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
    }

    /// `PUT path` with a body, optionally signed in.
    pub async fn put(
        &self,
        path: &str,
        body: Vec<u8>,
        session: Option<&Session>,
    ) -> Response<Body> {
        let mut request = Request::builder().method("PUT").uri(path);
        if let Some(session) = session {
            request = request.header(header::COOKIE, &session.cookie);
        }
        self.request(request.body(Body::from(body)).unwrap()).await
    }

    /// One control-plane call, at the current API version.
    pub async fn call(&self, request: CloudRequest, session: Option<&Session>) -> CloudReply {
        self.call_raw(
            &serde_json::to_vec(&CloudCall {
                version: CLOUD_API_VERSION,
                request,
            })
            .unwrap(),
            session,
        )
        .await
    }

    /// One control-plane call with a hand-built body — the only way to send
    /// a version this build does not speak.
    pub async fn call_raw(&self, body: &[u8], session: Option<&Session>) -> CloudReply {
        let mut request = Request::builder().method("POST").uri("/api");
        if let Some(session) = session {
            request = request.header(header::COOKIE, &session.cookie);
        }
        let response = self
            .request(request.body(Body::from(body.to_vec())).unwrap())
            .await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "a refusal is still a 200 answer on the control plane"
        );
        serde_json::from_slice(&body_bytes(response).await).expect("a CloudReply")
    }

    /// Sign in through the dev-auth route and keep the cookie.
    pub async fn sign_in(&self, email: &str) -> Session {
        let response = self.get(&format!("/auth/dev?email={email}")).await;
        assert_eq!(
            response.status(),
            StatusCode::SEE_OTHER,
            "dev auth redirects to the app"
        );
        let set_cookie = header_value(&response, header::SET_COOKIE)
            .expect("dev auth sets a session cookie")
            .to_string();
        Session {
            cookie: set_cookie
                .split(';')
                .next()
                .expect("a cookie name=value pair")
                .to_string(),
            set_cookie,
        }
    }
}

/// A signed-in caller's cookie.
pub struct Session {
    /// The `name=value` pair, as a request would send it back.
    pub cookie: String,
    /// The whole `Set-Cookie` header, for tests that assert on its
    /// attributes.
    pub set_cookie: String,
}

/// A response's body as bytes.
pub async fn body_bytes(response: Response<Body>) -> Vec<u8> {
    response
        .into_body()
        .collect()
        .await
        .expect("a complete body")
        .to_bytes()
        .to_vec()
}

/// A response's body as text.
pub async fn body_text(response: Response<Body>) -> String {
    String::from_utf8(body_bytes(response).await).expect("utf-8 body")
}

/// One header, as a string.
pub fn header_value(response: &Response<Body>, name: header::HeaderName) -> Option<&str> {
    response.headers().get(name)?.to_str().ok()
}
