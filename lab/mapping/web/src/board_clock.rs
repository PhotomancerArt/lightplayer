//! The simulated board's frame, for a host with no board: the lab, and a
//! drawing in progress in Studio (nothing has reached the board yet).
//!
//! The board's clock only advances while its pixels move (a selection's
//! pattern, a draft), so a still board costs no redraws.

use dioxus::prelude::*;
use lab_mapping_model::BoardFrame;

use crate::graph_editor::GraphEditor;

/// How often the board's clock ticks while its pixels move, in ms.
const TICK_MS: u32 = 33;

/// What the simulated board shows, worked out again only when the
/// project, where its fixtures were placed, the selection, a drawing in
/// progress, or the clock moves — never for the pointer or the camera.
/// Handing over a placement keeps the revision, so it is counted apart.
pub fn use_simulated_frame(ed: GraphEditor) -> Memo<BoardFrame> {
    let mut now = use_signal(|| 0.0_f64);
    use_future({
        let ed = ed.clone();
        move || {
            let ed = ed.clone();
            async move {
                loop {
                    gloo_timers::future::TimeoutFuture::new(TICK_MS).await;
                    if ed.peek().board_animates() {
                        now.set(seconds());
                    }
                }
            }
        }
    });
    let board_key = use_memo({
        let ed = ed.clone();
        move || {
            let e = ed.read();
            let draft = e
                .drawing()
                .then(|| (e.pointer, e.path_draft.clone(), e.camera));
            (
                e.project.revision(),
                e.project.placements(),
                e.selection.clone(),
                draft,
            )
        }
    });
    use_memo(move || {
        board_key.read();
        let t = now();
        ed.peek().board_frame(t)
    })
}

/// Seconds since the page started: the board's clock.
fn seconds() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map_or(0.0, |p| p.now() / 1000.0)
}
