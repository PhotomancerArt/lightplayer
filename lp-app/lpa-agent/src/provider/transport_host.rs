//! Host [`HttpSseTransport`] + [`HttpGetTransport`] over reqwest (feature
//! `host-transport`).
//!
//! Used by evals and integration tests; never built for wasm. Requires a
//! tokio runtime (reqwest's requirement) — this is a transport edge, so the
//! executor dependency is allowed here and nowhere else in the crate.

use futures_util::StreamExt;

use crate::provider::http_transport::{
    HttpGetTransport, HttpRequest, HttpResponse, HttpSseTransport, LocalBoxFuture, TransportError,
};

/// reqwest-backed transport.
#[derive(Clone, Debug, Default)]
pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    pub fn new() -> Self {
        Self::default()
    }
}

impl HttpSseTransport for ReqwestTransport {
    fn post(
        &self,
        req: HttpRequest,
    ) -> LocalBoxFuture<'static, Result<HttpResponse, TransportError>> {
        let client = self.client.clone();
        Box::pin(async move {
            let mut builder = client.post(&req.url);
            for (name, value) in &req.headers {
                builder = builder.header(name, value);
            }
            let response = builder
                .body(req.body)
                .send()
                .await
                .map_err(|e| TransportError::new(e.to_string()))?;
            Ok(streamed_response(response))
        })
    }

    fn sleep_ms(&self, ms: u32) -> LocalBoxFuture<'static, ()> {
        Box::pin(tokio::time::sleep(std::time::Duration::from_millis(
            u64::from(ms),
        )))
    }
}

impl HttpGetTransport for ReqwestTransport {
    fn get(
        &self,
        url: String,
        headers: Vec<(String, String)>,
    ) -> LocalBoxFuture<'static, Result<HttpResponse, TransportError>> {
        let client = self.client.clone();
        Box::pin(async move {
            let mut builder = client.get(&url);
            for (name, value) in &headers {
                builder = builder.header(name, value);
            }
            let response = builder
                .send()
                .await
                .map_err(|e| TransportError::new(e.to_string()))?;
            Ok(streamed_response(response))
        })
    }
}

/// Wrap a reqwest response into the transport's streamed shape.
fn streamed_response(response: reqwest::Response) -> HttpResponse {
    let status = response.status().as_u16();
    let body = response.bytes_stream().map(|chunk| match chunk {
        Ok(bytes) => Ok(bytes.to_vec()),
        Err(e) => Err(TransportError::new(e.to_string())),
    });
    HttpResponse {
        status,
        body: Box::pin(body),
    }
}
