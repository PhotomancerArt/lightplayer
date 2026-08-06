//! The route table — the whole HTTP surface in one screen.
//!
//! | Route | Plane | Auth |
//! |---|---|---|
//! | `POST /api` | control | cookie → `Actor` (anonymous is a caller too) |
//! | `GET /b/{hash}` | content | none — the hash is the capability |
//! | `PUT /b/{hash}` | content | session required |
//! | `GET /t/{hash}` | content | none |
//! | `PUT /t/{hash}` | content | session required |
//! | `GET /p/{*share}` | page | none; OG tags only when link-visible |
//! | `GET /auth/dev` | auth | localhost + `LP_CLOUD_DEV_AUTH` (else 404) |
//! | `GET /healthz` | ops | none |
//! | everything else | page | none — file, else the SPA document |

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};

use crate::api::api_route;
use crate::app_state::AppState;
use crate::auth::dev_auth;
use crate::content::{blob_route, tree_route};
use crate::page::page_route;

/// The largest upload the content plane accepts.
///
/// axum's default is 2 MiB, which a preview PNG can pass but a project's
/// bundled assets cannot. The cap is here rather than absent because an
/// unbounded `PUT` is a memory-exhaustion button: bodies are buffered whole
/// so their hash can be verified before a single byte is stored.
pub const MAX_UPLOAD_BYTES: usize = 64 * 1024 * 1024;

/// Build the whole application.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/api", post(api_route::post_api))
        .route(
            "/b/{hash}",
            get(blob_route::get_blob).put(blob_route::put_blob),
        )
        .route(
            "/t/{hash}",
            get(tree_route::get_tree).put(tree_route::put_tree),
        )
        .route("/p/{*share}", get(page_route::get_share_page))
        .route("/auth/dev", get(dev_auth::get_dev_auth))
        .route("/healthz", get(page_route::get_healthz))
        .fallback(get(page_route::get_page_or_asset))
        .layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES))
        .with_state(state)
}
