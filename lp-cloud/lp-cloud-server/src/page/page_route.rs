//! Serving the app: share URLs, static files, and the SPA fallback.
//!
//! # The share URL answers 200 either way
//!
//! `GET /p/<slug>-prj…` looks the project up **as an anonymous caller**,
//! which is exactly what an unfurler is, and injects OG tags only if the
//! answer came back. A private project, a uid that never existed, and a
//! malformed path all produce the *same* response: the plain document, status
//! 200, and the app shows its own not-found once it boots. Anything else —
//! a 404, a different body length, a redirect — would turn the share route
//! into an oracle for which project uids are real, which is the leak the
//! domain's `NotFound`-not-`NotAuthorized` rule exists to prevent. The edge
//! does not get to undo it.
//!
//! # The fallback rule
//!
//! A path that names an existing file gets the file. A path that looks like
//! a file (it has an extension) and does not exist gets a 404 — an SPA
//! document served where a `.js` was requested is a confusing failure. Every
//! other path gets the document, because it is a client route.

use axum::extract::{Path, State};
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use lpc_cloud_api::request::GetProject;
use lpc_cloud_api::response::ProjectInfo;
use lpc_cloud_api::{Actor, CloudCallSpec};

use crate::app_state::AppState;
use crate::page::og_inject::{self, OgTags};
use crate::page::{cache_policy, media_type, share_path};

/// `GET /p/{share}` — the app, with a share card when the link is public.
pub async fn get_share_page(State(state): State<AppState>, Path(share): Path<String>) -> Response {
    let tags = match share_path::project_uid(&share) {
        Some(uid) => share_card(&state, uid).await,
        None => None,
    };

    let document = match &tags {
        Some(tags) => og_inject::inject(state.site().index_html(), tags),
        None => state.site().index_html().to_vec(),
    };
    document_response(document)
}

/// Everything else: a static file if there is one, otherwise the app.
pub async fn get_page_or_asset(State(state): State<AppState>, uri: Uri) -> Response {
    let path = uri.path();

    if let Some(bytes) = state.site().file(path) {
        let file_name = path.rsplit('/').next().unwrap_or_default();
        return (
            [
                (header::CONTENT_TYPE, media_type::for_file(file_name)),
                (header::CACHE_CONTROL, cache_policy::for_file(file_name)),
            ],
            bytes,
        )
            .into_response();
    }

    if looks_like_a_file(path) {
        return (StatusCode::NOT_FOUND, "not found\n").into_response();
    }

    document_response(state.site().index_html().to_vec())
}

/// `GET /healthz` — is this process answering.
///
/// Deliberately does not touch the store: a health check that takes the
/// service lock reports "unhealthy" for a slow request, and a rolling deploy
/// then kills a machine that was merely busy.
/// Liveness plus the two version facts an operator actually asks for:
/// which build is running (git sha, from the image build arg) and which
/// cloud API vocabulary it speaks. Never takes the store lock — a wedged
/// store must read as unhealthy via timeouts, not hang the health check.
pub async fn get_healthz(State(state): State<AppState>) -> Response {
    let build = state.config().build_sha.as_deref().unwrap_or("dev");
    let body = format!(
        "{{\"status\":\"ok\",\"build\":\"{}\",\"cloud_api_version\":{}}}\n",
        build,
        lpc_cloud_api::CLOUD_API_VERSION
    );
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        body,
    )
        .into_response()
}

/// The OG tags for a project, or `None` if an anonymous caller cannot see
/// it.
async fn share_card(state: &AppState, uid: lpc_history::PrefixedUid) -> Option<OgTags> {
    // Anonymous on purpose: the tags are for whoever holds the link, and the
    // link is all an unfurler has. `Access::View` (or `Edit`) is what makes
    // this succeed; a project whose link opens nothing — including an
    // archived one — answers `NotFound` here exactly as it would to any
    // other caller, so the card follows the access rule without knowing it.
    let answer = state
        .with_service(move |core| {
            core.service
                .handle(Actor::Anonymous, GetProject { uid }.into())
        })
        .await
        .ok()?;

    // `GetProject` names its own answer (`CloudCallSpec`), so this is the
    // pairing table's `extract` rather than a second, hand-written match on
    // `CloudResponse` that could drift from it.
    let ProjectInfo { meta, sidecar, .. } = GetProject::extract(answer)?;
    let config = state.config();
    Some(OgTags {
        title: sidecar.name.clone(),
        description: format!("{} — a LightPlayer light show.", sidecar.name),
        image: sidecar
            .preview_png
            .map(|hash| config.absolute(&format!("/b/{hash}"))),
        url: config.absolute(&share_path::canonical(&meta.slug, meta.uid)),
    })
}

fn document_response(document: Vec<u8>) -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, cache_policy::NO_CACHE),
        ],
        document,
    )
        .into_response()
}

/// Whether a request path names a file rather than a client route.
fn looks_like_a_file(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .is_some_and(|segment| segment.contains('.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Client routes may be deep and may contain a uid; none of them is a
    /// file, and a missing asset must not be answered with a document.
    #[test]
    fn a_path_is_a_file_only_when_it_has_an_extension() {
        assert!(looks_like_a_file("/assets/app-a1b2c3d4.js"));
        assert!(looks_like_a_file("/favicon.ico"));
        assert!(!looks_like_a_file("/"));
        assert!(!looks_like_a_file("/sim/zook-dome"));
        assert!(!looks_like_a_file("/p/zook-dome-prj0000000000000000"));
    }
}
