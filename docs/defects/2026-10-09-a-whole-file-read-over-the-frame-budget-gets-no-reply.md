---
status: open
found: 2026-10-09      # live-debugging (emulated C6, RAM research E11)
area: fw-esp32-common serial/server_payload + lpa-server handlers (FsRequest::Read)
class: partial-knowledge-loss
related:
  - 2026-10-09-studios-pull-reads-a-file-whole-with-no-memory-check.md
  - 2026-08-04-oversized-display-layout-wedges-project-read.md
  - lp2025/2026-10-09-1203-ram-research (E11)
---
# A whole-file read bigger than the link's frame budget gets no reply

**Symptom** — `FsRequest::Read` of the PLAYFUL Choker's
`playful-mapping.svg` (27,091 B) on an emulated C6 whose largest block
cleared the read's memory check (`block ≥ size + 512`) got **no reply at
all**: the client waited until its own deadline. The board logged:

```
[WARN] fw_esp32_common::serial::server_payload: [usb_link] server message id=560000 Filesystem exceeded frame budget: 30389 B > 16656 (frame_budget=16384)
[ERROR] fw_esp32_common::usb_link::usb_link_transport: [usb_link] dropping message id=560000: Serialization error: server message id=560000 Filesystem exceeded frame budget
[WARN] fw_esp32_common::server_loop: run_server_loop: Server tick error:
```

Seen on the research baseline image (`research/ram-e11` @ `c1caed68a`,
`esp32c6,server,e11_link_standin,e07_ballast` with a 16,000 B ballast:
today's read path, a test ballast only) and on every E11 lender image,
`lp-emu:esp32c6:t1`. Whenever the memory check refuses the read instead
(the usual case once a project has run a while), the client gets the
"board memory busy" refusal and the defect hides.

**Root cause** — the whole file goes into one `FsResponse::Read`, which is
serialized into one server message; the C6's server message buffer
(`server_msg::FRAME_BUF`, 16,656 B) cannot hold a reply over ~16 KB, so
the transport drops it. Nothing turns the drop into an answer for that
request id: the error ends the tick, and the client is never told. The
memory gate (`handlers::fs_read_refusal`) asks only whether the heap can
hold the file, never whether the link can carry it.

**Fix** — none yet. Either refuse in words a file whose reply cannot fit
the frame budget (the gate's second question), or answer large files in
pieces (the ChangesSince cursor already walks offsets).

**Regression coverage** — none: `lp-cli/tests/emu_lender.rs` on
`research/ram-e11` reproduces it (`#[ignore]`, research only).

**Lesson** — a gate that checks one resource (heap) for a job that needs
two (heap, frame) turns the second shortage into silence. Every refusal
path should answer the request id; a dropped reply is worse than a refusal.
