//! The published-frame read: the pure state shared by every live picture.
//!
//! # What this module exists to guarantee
//!
//! - **Revision-only change detection.** A new frame is a frame whose
//!   `revision` moved — the buffer's `changed_at`, which advances only when
//!   the device or engine publishes. Arrival time never counts: a pull or
//!   read that answers with the same revisions leaves the frame, its bytes
//!   `Rc`, and its age stamp exactly as they were, so a card ages honestly
//!   toward the stale threshold instead of pretending a re-read is a new
//!   picture.
//! - **The LampView `Rc` contract.** The renderer repaints on `Rc` POINTER
//!   identity, so every genuinely new frame gets a FRESH `bytes` `Rc` and
//!   the display layout keeps a STABLE one across frames whose geometry did
//!   not move. The per-output folding and the composed picture both live in
//!   [`OutputFrameCache`], which keeps those identities.
//! - **Every output joins the picture.** A project can drive several
//!   outputs, and a consumer composes ALL of them
//!   ([`OutputFrameCache::composed_frame`]) — one buffer, every wire's
//!   lamps at their own offsets. Consumers used to latch the FIRST output
//!   that published, which is how the small dome's second box (2,975
//!   lamps) never appeared on the sim card.
//! - **Last-known survives the link going dark.** Nothing here is cleared
//!   on disconnect (Q4: offline shows the last in-session frame, dimmed).
//!   Only the connection-scoped facts — the project handle and the
//!   per-output geometry claims — are invalidated, because those are claims
//!   about a connection.
//!
//! Three consumers share these guarantees for their own transports: the sim
//! card feed in `runtime_pool` (a host-driven pull with its own pacing and
//! offline story), the editor lens's `project::project_sync` (the mirror
//! tree's own read), and the device card feed the sibling phases are
//! adding for a session at the far end of a serial link. The browser
//! preview host keeps a deliberate twin,
//! [`crate::app::preview_host::preview_output_feed`], because it rides a
//! frame the preview host already schedules and owns neither pacing nor a
//! connection.

pub mod card_feed;
pub mod output_frame_cache;
pub mod output_frame_entries;

pub use card_feed::{CardFeedApply, CardFeedState};
pub use output_frame_cache::OutputFrameCache;
pub use output_frame_entries::output_frame_entries;
