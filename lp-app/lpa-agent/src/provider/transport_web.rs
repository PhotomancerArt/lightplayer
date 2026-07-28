//! wasm [`HttpSseTransport`] + [`HttpGetTransport`]: web-sys `fetch` +
//! `ReadableStream` reader.
//!
//! Works from both the main thread (`Window`) and workers
//! (`WorkerGlobalScope`). An `AbortController` is wired to the body stream's
//! drop, so cancelling a turn (dropping the event stream) aborts the fetch.

use futures_util::stream::unfold;
use js_sys::Uint8Array;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    AbortController, Headers, ReadableStreamDefaultReader, Request, RequestInit, Response,
};

use crate::provider::http_transport::{
    HttpGetTransport, HttpRequest, HttpResponse, HttpSseTransport, LocalBoxFuture, TransportError,
};

/// The browser fetch transport (stateless).
#[derive(Clone, Copy, Debug, Default)]
pub struct WebFetchTransport;

impl HttpSseTransport for WebFetchTransport {
    fn post(
        &self,
        req: HttpRequest,
    ) -> LocalBoxFuture<'static, Result<HttpResponse, TransportError>> {
        Box::pin(async move {
            fetch_streaming("POST", &req.url, &req.headers, Some(&req.body))
                .await
                .map_err(to_transport_error)
        })
    }

    fn sleep_ms(&self, ms: u32) -> LocalBoxFuture<'static, ()> {
        Box::pin(gloo_timers::future::TimeoutFuture::new(ms))
    }
}

impl HttpGetTransport for WebFetchTransport {
    fn get(
        &self,
        url: String,
        headers: Vec<(String, String)>,
    ) -> LocalBoxFuture<'static, Result<HttpResponse, TransportError>> {
        Box::pin(async move {
            fetch_streaming("GET", &url, &headers, None)
                .await
                .map_err(to_transport_error)
        })
    }
}

async fn fetch_streaming(
    method: &str,
    url: &str,
    request_headers: &[(String, String)],
    body: Option<&str>,
) -> Result<HttpResponse, JsValue> {
    let controller = AbortController::new()?;

    let headers = Headers::new()?;
    for (name, value) in request_headers {
        headers.append(name, value)?;
    }
    let init = RequestInit::new();
    init.set_method(method);
    init.set_headers(&headers);
    if let Some(body) = body {
        init.set_body(&JsValue::from_str(body));
    }
    init.set_signal(Some(&controller.signal()));

    let request = Request::new_with_str_and_init(url, &init)?;
    let response: Response = JsFuture::from(fetch_with_request(&request)?)
        .await?
        .dyn_into()?;
    let status = response.status();

    let body = response
        .body()
        .ok_or_else(|| JsValue::from_str("response has no body"))?;
    let reader: ReadableStreamDefaultReader = body.get_reader().dyn_into()?;

    // The guard aborts the fetch when the stream state is dropped.
    let state = ReadState {
        reader,
        _guard: AbortGuard(controller),
        done: false,
    };
    let body = Box::pin(unfold(state, |mut state| async move {
        if state.done {
            return None;
        }
        match read_chunk(&state.reader).await {
            Ok(Some(bytes)) => Some((Ok(bytes), state)),
            Ok(None) => None,
            Err(e) => {
                state.done = true;
                Some((Err(to_transport_error(e)), state))
            }
        }
    }));
    Ok(HttpResponse { status, body })
}

struct ReadState {
    reader: ReadableStreamDefaultReader,
    _guard: AbortGuard,
    done: bool,
}

struct AbortGuard(AbortController);

impl Drop for AbortGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// One `reader.read()` step: `Ok(None)` on end of stream.
async fn read_chunk(reader: &ReadableStreamDefaultReader) -> Result<Option<Vec<u8>>, JsValue> {
    let result = JsFuture::from(reader.read()).await?;
    let done = js_sys::Reflect::get(&result, &JsValue::from_str("done"))?
        .as_bool()
        .unwrap_or(true);
    if done {
        return Ok(None);
    }
    let value = js_sys::Reflect::get(&result, &JsValue::from_str("value"))?;
    let array: Uint8Array = value.dyn_into()?;
    Ok(Some(array.to_vec()))
}

/// `fetch` from whatever global scope we are running in (window or worker).
fn fetch_with_request(request: &Request) -> Result<js_sys::Promise, JsValue> {
    let global = js_sys::global();
    if let Some(window) = global.dyn_ref::<web_sys::Window>() {
        return Ok(window.fetch_with_request(request));
    }
    if let Some(scope) = global.dyn_ref::<web_sys::WorkerGlobalScope>() {
        return Ok(scope.fetch_with_request(request));
    }
    Err(JsValue::from_str("no fetch available in this global scope"))
}

fn to_transport_error(v: JsValue) -> TransportError {
    let message = v
        .as_string()
        .or_else(|| {
            js_sys::Reflect::get(&v, &JsValue::from_str("message"))
                .ok()
                .and_then(|m| m.as_string())
        })
        .unwrap_or_else(|| format!("{v:?}"));
    TransportError { message }
}
