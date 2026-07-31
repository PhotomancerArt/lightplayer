---
status: open      # reframed 2026-07-31: the server deploy path is exonerated
found: 2026-07-30      # how: hardware-walk
area: lp-cli (upload observability) — observed on fw-esp32s3
class: stale-measurement
related:
  - 2026-07-30-q32-native-vs-wasmtime-last-bit.md
---
# A pushed project compiles the *previous* upload's shader source

**Symptom** — `lp-cli upload <dir> serial:<port>` reloads the project and
recompiles the shader, and the recompile uses the source from the **previous**
upload. Reproduced four times in a row on the desk ESP32-S3, deterministically,
with the device reporting the stale length in its own log:

| Upload | local `shader.glsl` | device logged | frame rendered |
|---|---|---|---|
| A | 1628 B (v1) | 1628 B | v1 ✔ |
| B | 1628 B (v2) | 1628 B | **v1** ✘ |
| C | 1660 B (v3) | **1628 B** | v2 ✘ |
| D | 1662 B (v4) | **1660 B** | v3 ✘ |
| E | 1662 B (unchanged) | 1662 B | v4 ✔ |

The log line is `[shader-node] compilation starting (node=…, N bytes)`, and `N`
is `glsl_source.len()` — the node's own copy. Every upload does recompile
(`compilation succeeded`, fresh instruction counts), so this is not a skipped
rebuild; it is a rebuild of stale input. A second upload with no local change
always catches up, which is what makes it a one-step lag rather than a
corruption.

`lp-cli`'s side is not the cause: `collect_project_deploy_files` reads and
sends every file in the directory unconditionally, with no size, mtime, or hash
comparison.

**Root cause** — Not the deploy/reload path. Established 2026-07-31 by host
reproduction attempts (see *Ruled out* below): the server reads the files it
was just handed, on every drain ordering, and the device filesystem is durable
across a reboot.

The leading explanation is that the compile being *observed* is not the
deploy's. `lp-cli upload` over serial resets the device on connect
(`lp-cli/src/client/cli_connect.rs:76`, `reset_after_open: true` — readiness is
granted only by a boot `ServerHello`), so every upload begins with a full boot
that auto-loads and recompiles **whatever the last upload left on flash**
(`lp-fw/fw-esp32-common/src/boot.rs`, `auto_load_project`). The deploy's own
reload then compiles the new source — but `upload` closes the connection the
instant `LoadProject` is acked
(`lp-cli/src/commands/upload/handler.rs:57`–`63`), before the newly loaded
project has rendered a frame, and the S3's USB-Serial-JTAG port is exclusive, so
a monitor can only be attached *outside* the upload. Every compile line an
operator can associate with an upload therefore describes the previous one.

That fits all five rows exactly: each "device logged" value is the byte length
of the *preceding* row's upload, and row E is `✔` only because D and E deployed
the same 1662 B source.

Not yet confirmed on hardware — confirming it needs one walk that reads the
compile line from a monitor attached strictly *after* the upload's own reload
(reflash-then-boot, i.e. the walk's round 2), and compares it against the same
walk's round-1 boot line.

**Fix** — None yet. If the explanation above holds, the defect is in what
`upload` makes observable, not in what the device computes: the command should
wait for evidence that the *newly deployed* project is running (the shader
node's compile outcome through a project read) before it disconnects, instead
of exiting on the `LoadProject` ack. That is a CLI behaviour decision, not a
server fix.

**Ruled out** (2026-07-31, all on host)

- *The deploy's reload reading pre-write state.* `tests/deploy_reload_source.rs`
  drives the real `lpa_client::project_deploy_requests` sequence (stop → write →
  load) through `LpServer::tick_and_send` and renders the LED bytes. Both drain
  orderings pass: one request per frame, and the whole deploy served inside a
  single frame — the latter is the ordering a stale read would need, since
  `advance_frame` runs once *before* any of the writes.
- *`FsVersion` bookkeeping in `advance_frame`'s refresh path.* A fresh load
  reads the filesystem directly; `ArtifactStore::read_bytes` holds no bytes, so
  there is no cache for the refresh path to be one write behind on.
- *The device filesystem losing the last write.* `LpFsFlash` over a RAM block
  device round-trips repeated overwrites, and a snapshot of the block device
  taken after a write (what a reset would leave on flash) remounts to the last
  written content. `littlefs_rust::Filesystem::write_file` closes its handle by
  RAII, so the data is committed before `write_file` returns.

**Regression coverage** — `lp-app/lpa-server/tests/deploy_reload_source.rs`
(`a_deploy_renders_the_source_it_just_wrote`,
`a_batched_deploy_renders_the_source_it_just_wrote`). Green from the start:
these pin the deploy path rather than fix it, so that the next investigation
does not re-suspect it. Existing project tests write files and then load, which
is precisely the ordering that could not have caught a stale read.

**Lesson** — This is the defect the M4 milestone file predicted by name: "watch
specifically for divergences that only appear in the *app* path… the incremental
re-compile loop is the likeliest source." It was right, and the reason it was
found is that the walk had an **independent expectation** to compare against.
A device that recompiles on every push and renders a plausible picture looks
completely healthy; only a byte-exact oracle turns "it rendered" into "it
rendered the wrong thing".

The sharper lesson is about iteration loops. Shader authoring is an
edit-push-look cycle, and a one-push lag inside that cycle is not a cosmetic
delay: it makes every observation describe the previous edit, which is
indistinguishable from "my change did nothing" and invites the author to change
something else. Any lag in a feedback loop must be zero or visible, never one.

The reframing adds a second edge to it: a lag in the *observation channel* is
indistinguishable from a lag in the system, and costs the same. A tool that
resets the device it is measuring, and then stops listening before the thing it
caused has happened, will report the previous state forever — with no bug
anywhere in the system it is reporting on.
