// Page-side fetch + compile of the fw-browser engine wasm (boot protocol
// v2). Reads the response body chunk-by-chunk so the page can show real
// download progress, then compiles ONE WebAssembly.Module that every
// worker instantiates from — one fetch, one compile, however many workers.
//
// onProgress(receivedBytes, totalBytes) fires per chunk; totalBytes is 0
// when unknown. The total prefers the server's `x-uncompressed-length`
// (lp-cloud sets it on every asset answer): Content-Length counts
// COMPRESSED bytes while the reader yields decompressed ones, so without
// that header a content-encoded response reports total 0 (indeterminate)
// rather than a progress bar that overshoots.
// onCompileStart() fires once, after the last byte and before compile.
export async function fetchAndCompileEngine(url, onProgress, onCompileStart) {
  const response = await engineResponse(url);
  if (!response.ok) {
    throw new Error(`engine wasm fetch failed: HTTP ${response.status} for ${response.url || url}`);
  }
  const declared = Number(response.headers.get("x-uncompressed-length")) || 0;
  const encoding = (response.headers.get("content-encoding") || "identity").toLowerCase();
  const lengthHeader = response.headers.get("content-length");
  const total = declared > 0
    ? declared
    : encoding === "identity" && lengthHeader != null
      ? Number(lengthHeader) || 0
      : 0;
  let buffer;
  if (response.body) {
    const reader = response.body.getReader();
    const chunks = [];
    let received = 0;
    for (;;) {
      const { done, value } = await reader.read();
      if (done) {
        break;
      }
      chunks.push(value);
      received += value.byteLength;
      onProgress(received, total);
    }
    buffer = new Uint8Array(received);
    let offset = 0;
    for (const chunk of chunks) {
      buffer.set(chunk, offset);
      offset += chunk.byteLength;
    }
  } else {
    // No streaming body (older engines): fall back to one-shot download.
    buffer = new Uint8Array(await response.arrayBuffer());
    onProgress(buffer.byteLength, buffer.byteLength);
  }
  onCompileStart();
  return WebAssembly.compile(buffer);
}

// The Response whose body becomes the module, wherever it already is.
//
// Preference order:
//
// 1. The shell loader's pre-fetch (`window.__lpShell.engineFetch`) — the
//    index.html shell starts the engine download the moment the app wasm's
//    bytes finish, so by the time this cache is asked, the response is
//    usually mid-flight or done. Adopted only when it matches this URL,
//    settled ok, and nobody consumed the body (a retry after a previous
//    failed compile must not re-await a drained stream).
// 2. A plain fetch of `url` — the standing path when there is no shell
//    (stories, the fw-browser smoke page) or its pre-fetch failed.
// 3. On a non-ok answer, ONE re-resolution through a fresh, uncached
//    manifest read: the engine's name is content-hashed, and a mid-session
//    sidecar rebuild (dev) strands the page's original manifest on a name
//    that no longer exists — the refreshed name is the recovery.
async function engineResponse(url) {
  const pending = globalThis.__lpShell?.engineFetch;
  if (pending && pending.url === url) {
    try {
      const adopted = await pending.promise;
      if (adopted.ok && adopted.body && !adopted.bodyUsed) {
        return adopted;
      }
    } catch {
      // fall through to a fetch of our own
    }
  }
  const response = await fetch(url);
  if (response.ok) {
    return response;
  }
  const fresh = await fetch("/pkg/engine-manifest.json", { cache: "no-store" })
    .then((manifest) => (manifest.ok ? manifest.json() : null))
    .catch(() => null);
  const freshUrl = fresh?.fw_browser_wasm;
  if (freshUrl && freshUrl !== url) {
    return fetch(freshUrl);
  }
  return response;
}
