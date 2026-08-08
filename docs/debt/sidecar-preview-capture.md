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

**Why it stands.** Ruled at P4 of the project-identity-and-sharing plan
(Q8, explicitly timeboxed). There is no readable frame to capture:

- The **GPU tier** transfers its canvas into a worker as an
  `OffscreenCanvas` (`preview_host_impl.rs`, `preview_worker.rs`), so
  the main thread cannot read pixels back from it at all. Fighting
  that means new worker-protocol surface and a GPU readback path —
  the same readback debt `gpu-tier-cannot-sample-led-output.md`
  records.
- The **CPU tier's** canvas is readable in principle, but a sidecar is
  computed for every trip the driver makes, including the sign-in
  sweep's — and the sweep publishes projects that are **not open** and
  have never been rendered in this tab. A capture path that only works
  for the one project currently open in the one tier that happens to
  be CPU would leave most cards blank anyway.

Shipping `None` costs nothing that was working before (there has never
been a preview) and keeps the auto-publish slice from growing a
rendering dependency.

**Trigger to fix.** Before sharing is announced or marketed. A share
link whose unfurl is a blank card is a worse first impression than no
unfurl, and the OG card is the *only* thing most recipients see before
deciding whether to click.

**The likely free ride.** The root-module product-display plan's
frame-tap + `LampView` work introduces a readable frame source for a
project's root module that does not go through the preview canvas at
all. If that lands first, this becomes "call the tap, encode ~256px
PNG, `put_blob`, set the hash" — the upload half is already written
(`sync/content_transfer.rs` uploads `sidecar.preview_png` alongside the
version's own files on every push).

**Incident log.** (none yet)
