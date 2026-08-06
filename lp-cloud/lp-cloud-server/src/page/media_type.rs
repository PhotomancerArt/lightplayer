//! The `Content-Type` for a static file, by extension.
//!
//! Small and hand-written rather than a MIME crate: the artifact is one
//! directory produced by one build, so the set of extensions in it is known.
//! Two of them are load-bearing — a `.wasm` served as anything but
//! `application/wasm` refuses to stream-compile, and a `.js` module served
//! as text is blocked outright — and the rest are ordinary.

/// The fallback for an extension not in the table.
pub const OCTET_STREAM: &str = "application/octet-stream";

/// A content type for a file name.
pub fn for_file(file_name: &str) -> &'static str {
    let extension = file_name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default();

    match extension.as_str() {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "wasm" => "application/wasm",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "txt" => "text/plain; charset=utf-8",
        "xml" => "application/xml",
        "webmanifest" => "application/manifest+json",
        _ => OCTET_STREAM,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two that break the app outright when they are wrong.
    #[test]
    fn wasm_and_js_get_the_types_the_browser_demands() {
        assert_eq!(for_file("lpa_studio_web_bg.wasm"), "application/wasm");
        assert_eq!(
            for_file("main-a1b2c3d4.js"),
            "text/javascript; charset=utf-8"
        );
    }

    #[test]
    fn an_unknown_extension_is_opaque_bytes() {
        assert_eq!(for_file("firmware.bin"), OCTET_STREAM);
        assert_eq!(for_file("LICENSE"), OCTET_STREAM);
    }

    #[test]
    fn extensions_are_case_insensitive() {
        assert_eq!(for_file("LOGO.PNG"), "image/png");
    }
}
