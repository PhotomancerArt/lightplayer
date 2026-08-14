# Cloud project cards have no preview image

**Condition.** `SidecarMeta.preview_png`
(`lp-core/lpc-cloud-api/src/sidecar_meta.rs`) is always `None`. The
auto-publish driver populates the sidecar's `name` and `format_version`
from the container manifest and stops there
(`lp-app/lpa-studio-web/src/cloud/sync/sidecar_producer.rs`). So every
place a published project is shown without opening it — the cloud
listing, and the OG card a share link unfurls into on Slack, Discord,
iMessage — renders text on an empty rectangle. The blob plane, the
push pre-flight and the wire field are all already in place and
working; the only missing piece is a producer.

**The producer now exists.** The poster-first gallery previews work
(`docs/adr/2026-08-14-poster-first-gallery-previews.md`) built exactly
the frame producer this debt was waiting on, for the local Explore/
Projects gallery: `lp-app/lpa-studio-web/src/app/home/thumb_poster.rs`
captures one PNG per project from a leased preview slot, per quadrant —

- control-first (either tier): `lamp_poster` rasterizes the project's
  own output frame in Rust, no canvas read at all;
- shader-only CPU tier: `canvas_poster` reads the card's live canvas
  back with `to_data_url_with_type`;
- shader-only GPU tier (the one quadrant this entry originally called
  out as unreadable): `pixel_poster` encodes bytes from a new
  worker-side capture — `CapturePoster` envelope
  (`lp-app/lpa-link/src/providers/browser_worker/worker_envelope.rs`) →
  one-shot async texture readback (`read_back_texture_async`,
  `lp-gfx/lp-gfx-wgpu/src/read_back.rs`) → transferable `poster_pixels`
  bytes.

So the GPU-tier blocker this entry originally cited is closed: the
capture no longer depends on reading the transferred canvas back on the
main thread at all. What remains is **persistence only**:

1. encode the already-captured PNG (or re-capture on demand) at save
   time;
2. `put_blob` it through the already-working upload plane
   (`sync/content_transfer.rs` already uploads `sidecar.preview_png`
   alongside the version's own files on every push);
3. set `SidecarMeta.preview_png`'s hash so the sidecar carries it.

The still-open question the local work deliberately left alone (Q8,
poster-first plan): **when** to capture for a project that is not open
in this tab — the sign-in sweep publishes projects nobody has rendered
this session, so save-time capture (open the project, capture, save)
covers the common case but not that one; a headless/background
capture path is separate work.

**Trigger to fix.** Unchanged: before sharing is announced or marketed.
A share link whose unfurl is a blank card is a worse first impression
than no unfurl, and the OG card is the *only* thing most recipients see
before deciding whether to click.

**Incident log.** (none yet)
