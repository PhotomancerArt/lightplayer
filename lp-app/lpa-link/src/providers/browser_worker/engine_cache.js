// Page-side fetch + compile of the fw-browser engine wasm (boot protocol
// v2). Reads the response body chunk-by-chunk so the page can show real
// download progress, then compiles ONE WebAssembly.Module that every
// worker instantiates from — one fetch, one compile, however many workers.
//
// onProgress(receivedBytes, totalBytes) fires per chunk; totalBytes is 0
// when unknown. Content-Length counts COMPRESSED bytes while the reader
// yields decompressed ones, so a content-encoded response reports total 0
// (indeterminate) rather than a progress bar that overshoots.
// onCompileStart() fires once, after the last byte and before compile.
export async function fetchAndCompileEngine(url, onProgress, onCompileStart) {
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`engine wasm fetch failed: HTTP ${response.status} for ${url}`);
  }
  const encoding = (response.headers.get("content-encoding") || "identity").toLowerCase();
  const lengthHeader = response.headers.get("content-length");
  const total =
    encoding === "identity" && lengthHeader != null ? Number(lengthHeader) || 0 : 0;
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
