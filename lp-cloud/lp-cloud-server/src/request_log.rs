//! One log line per request: method, path, status, duration.
//!
//! This is the whole of the edge's request observability on purpose —
//! `fly logs` (live + recent) over structured-enough lines covers the
//! current scale, and anything heavier (metrics, tracing spans, an APM)
//! is deliberate future work with a debt entry, not a default.
//!
//! What is logged: method, path, status, elapsed milliseconds. What is
//! deliberately NOT logged: query strings (may carry `?next=` paths),
//! cookies and headers (session tokens), bodies (project content), and
//! `/healthz` at all (a 10-second checker would be most of the log).

use std::time::Instant;

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

/// Axum middleware: time the request, log one line at `info`.
pub async fn log_request(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    if path == "/healthz" {
        return next.run(request).await;
    }
    let started = Instant::now();
    let response = next.run(request).await;
    let millis = started.elapsed().as_millis();
    log::info!(
        "{method} {path} -> {status} ({millis}ms)",
        status = response.status().as_u16()
    );
    response
}
