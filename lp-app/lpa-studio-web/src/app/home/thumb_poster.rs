//! Gallery poster frames: capture one picture from a live preview slot,
//! keep it for the session, and hand it back as an `<img>` source.
//!
//! The poster-first policy (see the plan's D1/D2) says a gallery card shows
//! a **stable picture instantly** and spends live rendering deliberately.
//! That makes a card's preview lease a producer of *one image*, not a
//! forever-running canvas: the card leases a slot with a small
//! `PreviewProfile::frame_budget`, waits for the budget to be spent,
//! captures whatever the slot's quadrant can be captured from, caches the
//! data URL, and drops the handle.
//!
//! # Where the picture comes from, per quadrant
//!
//! | | control-first (lamps) | shader-only (raster) |
//! |---|---|---|
//! | **CPU tier** | [`lamp_poster`] | [`canvas_poster`] |
//! | **GPU tier** | [`lamp_poster`] | [`pixel_poster`] |
//!
//! The lamp field is drawn from the slot's own output frame — the same
//! voronoi cell paths the live layer fills ([`LampCellPaths`]) — so the
//! capture never scrapes the DOM and its size is ours to pick. A CPU-tier raster slot presents through the host's
//! `putImageData` blit onto a plain canvas, which reads back fine. A
//! GPU-tier raster slot's canvas was permanently handed to a worker by
//! `transferControlToOffscreen`, so `toDataURL` on it is impossible by
//! construction — that quadrant asks the WORKER to capture: the host posts
//! `capture_poster`, the worker renders once and reads the texture back
//! asynchronously, and the bytes come home as a transferable buffer
//! ([`pixel_poster`] encodes them).
//!
//! # Cache
//!
//! Session-lifetime and in memory only (D3: no persisted PNGs). Keyed by
//! the lease's [`PreviewSource`], bounded at [`POSTER_CACHE_LIMIT`] entries
//! with insertion-order eviction — a card-sized PNG data URL is tens of KB,
//! and a gallery never shows more than a screenful at once. Data URLs, not
//! blob URLs, so eviction is a plain drop with no `revokeObjectURL` owed.

use std::cell::RefCell;
use std::collections::VecDeque;

use lpa_studio_core::{PreviewSource, UiControlProductPreview};
use wasm_bindgen::{Clamped, JsCast};

use crate::app::node::lamp_view::LampCellPaths;

/// Posters kept for the session before the oldest is dropped.
pub(crate) const POSTER_CACHE_LIMIT: usize = 64;

/// Poster size in device pixels: the card thumb's 256×96 box at 2×, crisp
/// on a retina panel and still tens of KB as a PNG.
pub(crate) const POSTER_WIDTH: u32 = 512;
pub(crate) const POSTER_HEIGHT: u32 = 192;

/// The lamp layer's inset on a card (`card_thumb.rs`: `tw:inset-[6%]`, so
/// an edge lamp is not clipped by the thumb's rounded corner). The poster
/// frames the field exactly like the live layer it stands in for, so the
/// hover swap (P4) lands on the same picture instead of jumping.
const POSTER_LAMP_INSET: f64 = 0.06;

thread_local! {
    /// The session poster cache, oldest first (`gallery_preview`'s HOST and
    /// `library_host_opfs`'s uid map are the same `thread_local!` +
    /// `RefCell` singleton shape).
    static POSTERS: RefCell<VecDeque<(String, String)>> =
        const { RefCell::new(VecDeque::new()) };
}

/// The cache key for a lease source — the source string itself, namespaced
/// so an example id can never collide with a project uid.
pub(crate) fn poster_cache_key(source: &PreviewSource) -> String {
    match source {
        PreviewSource::Example(id) => format!("example:{id}"),
        PreviewSource::ProjectUid(uid) => format!("project:{uid}"),
    }
}

/// This session's poster for `source`, if one has been captured.
pub(crate) fn cached_poster(source: &PreviewSource) -> Option<String> {
    let key = poster_cache_key(source);
    POSTERS.with(|cell| {
        cell.borrow()
            .iter()
            .find(|(cached, _)| *cached == key)
            .map(|(_, url)| url.clone())
    })
}

/// Remember `data_url` as `source`'s poster, evicting the oldest entry past
/// the cap. A re-capture of a source already cached replaces it in place
/// (same slot in the order — a card that recaptured is not "newer" in any
/// way worth keeping).
pub(crate) fn store_poster(source: &PreviewSource, data_url: String) {
    let key = poster_cache_key(source);
    POSTERS.with(|cell| {
        let mut posters = cell.borrow_mut();
        if let Some(entry) = posters.iter_mut().find(|(cached, _)| *cached == key) {
            entry.1 = data_url;
            return;
        }
        posters.push_back((key, data_url));
        while posters.len() > POSTER_CACHE_LIMIT {
            posters.pop_front();
        }
    });
}

/// Capture a control-first slot's lamp field as a PNG data URL.
///
/// Both tiers land here: a control-first project's picture IS its lamps,
/// and they arrive as an output frame the host already carries home — the
/// raster canvas underneath is hidden on those cards either way.
pub(crate) fn lamp_poster(preview: &UiControlProductPreview) -> Result<String, String> {
    let canvas = scratch_canvas(POSTER_WIDTH, POSTER_HEIGHT)?;
    let context = canvas_2d_context(&canvas)?;

    // The live layer draws lamps over a black frame (they screen-blend
    // against dark, never against the identity gradient), so the poster
    // carries that frame with it — the `<img>` sits over the same gradient.
    context.set_fill_style_str("#000000");
    context.fill_rect(0.0, 0.0, f64::from(POSTER_WIDTH), f64::from(POSTER_HEIGHT));

    let inset_x = (f64::from(POSTER_WIDTH) * POSTER_LAMP_INSET).round();
    let inset_y = (f64::from(POSTER_HEIGHT) * POSTER_LAMP_INSET).round();
    let width = (f64::from(POSTER_WIDTH) - inset_x * 2.0).max(1.0);
    let height = (f64::from(POSTER_HEIGHT) - inset_y * 2.0).max(1.0);
    // The same voronoi cells the live layer fills — one-shot, so no cache
    // slot to keep: a poster is captured once per source per session.
    let mut paths = None;
    LampCellPaths::ensure(&mut paths, preview)?.fill(
        &context,
        preview,
        true,
        [inset_x, inset_y, width, height],
    )?;
    encode_png(&canvas)
}

/// Encode a worker-captured poster frame (shader-only GPU tier) as a PNG
/// data URL.
///
/// The bytes arrive already sized (the host requests [`POSTER_WIDTH`] ×
/// [`POSTER_HEIGHT`]) and already sRGB RGBA8 — this is just
/// bytes → scratch canvas → PNG.
pub(crate) fn pixel_poster(frame: &lpa_studio_core::PreviewPosterFrame) -> Result<String, String> {
    if frame.bytes.len() != (frame.width as usize) * (frame.height as usize) * 4 {
        return Err(format!(
            "poster frame byte count {} does not match {}x{}",
            frame.bytes.len(),
            frame.width,
            frame.height
        ));
    }
    let canvas = scratch_canvas(frame.width, frame.height)?;
    let context = canvas_2d_context(&canvas)?;
    let image = web_sys::ImageData::new_with_u8_clamped_array_and_sh(
        Clamped(&frame.bytes),
        frame.width,
        frame.height,
    )
    .map_err(|error| format!("build ImageData: {error:?}"))?;
    context
        .put_image_data(&image, 0.0, 0.0)
        .map_err(|error| format!("putImageData: {error:?}"))?;
    encode_png(&canvas)
}

/// Capture a CPU-tier raster slot's live canvas as a PNG data URL.
///
/// Sound only on the CPU tier: those frames are blitted onto the card's own
/// canvas with `putImageData` by the host, on this thread, so the element
/// still owns its pixels. A GPU-tier canvas was transferred to the worker
/// and throws here — callers must not reach this with one.
pub(crate) fn canvas_poster(canvas_id: &str) -> Result<String, String> {
    let canvas = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(canvas_id))
        .ok_or_else(|| format!("canvas #{canvas_id} not mounted"))?
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| format!("element #{canvas_id} is not a canvas"))?;
    encode_png(&canvas)
}

/// A detached canvas to compose a poster on: never in the document, so it
/// costs no layout and nothing can paint over it.
fn scratch_canvas(width: u32, height: u32) -> Result<web_sys::HtmlCanvasElement, String> {
    let canvas = web_sys::window()
        .and_then(|window| window.document())
        .ok_or_else(|| "no document to build a poster canvas in".to_string())?
        .create_element("canvas")
        .map_err(|error| format!("create poster canvas: {error:?}"))?
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| "created element is not a canvas".to_string())?;
    canvas.set_width(width);
    canvas.set_height(height);
    Ok(canvas)
}

fn canvas_2d_context(
    canvas: &web_sys::HtmlCanvasElement,
) -> Result<web_sys::CanvasRenderingContext2d, String> {
    canvas
        .get_context("2d")
        .map_err(|error| format!("get 2d context: {error:?}"))?
        .ok_or_else(|| "poster canvas has no 2d context".to_string())?
        .dyn_into::<web_sys::CanvasRenderingContext2d>()
        .map_err(|_| "2d context has an unexpected type".to_string())
}

fn encode_png(canvas: &web_sys::HtmlCanvasElement) -> Result<String, String> {
    canvas
        .to_data_url_with_type("image/png")
        .map_err(|error| format!("encode poster: {error:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clear() {
        POSTERS.with(|cell| cell.borrow_mut().clear());
    }

    #[test]
    fn sources_are_namespaced_so_ids_cannot_collide() {
        assert_ne!(
            poster_cache_key(&PreviewSource::Example("a".to_string())),
            poster_cache_key(&PreviewSource::ProjectUid("a".to_string()))
        );
    }

    #[test]
    fn a_stored_poster_comes_back_for_its_source() {
        clear();
        let source = PreviewSource::Example("examples/plasma".to_string());
        assert_eq!(cached_poster(&source), None);
        store_poster(&source, "data:image/png;base64,AAA".to_string());
        assert_eq!(
            cached_poster(&source).as_deref(),
            Some("data:image/png;base64,AAA")
        );
        // A different source is a different picture, never this one.
        assert_eq!(
            cached_poster(&PreviewSource::ProjectUid("examples/plasma".to_string())),
            None
        );
    }

    #[test]
    fn a_recapture_replaces_in_place_without_growing_the_cache() {
        clear();
        let source = PreviewSource::Example("one".to_string());
        store_poster(&source, "first".to_string());
        store_poster(&source, "second".to_string());
        assert_eq!(cached_poster(&source).as_deref(), Some("second"));
        POSTERS.with(|cell| assert_eq!(cell.borrow().len(), 1));
    }

    #[test]
    fn the_cache_evicts_the_oldest_past_its_cap() {
        clear();
        for index in 0..POSTER_CACHE_LIMIT + 3 {
            store_poster(
                &PreviewSource::Example(format!("example-{index}")),
                format!("url-{index}"),
            );
        }
        POSTERS.with(|cell| assert_eq!(cell.borrow().len(), POSTER_CACHE_LIMIT));
        // The three oldest went; the newest are all still here.
        for index in 0..3 {
            assert_eq!(
                cached_poster(&PreviewSource::Example(format!("example-{index}"))),
                None,
                "example-{index} should have been evicted"
            );
        }
        let newest = POSTER_CACHE_LIMIT + 2;
        assert_eq!(
            cached_poster(&PreviewSource::Example(format!("example-{newest}"))).as_deref(),
            Some(format!("url-{newest}").as_str())
        );
    }
}
