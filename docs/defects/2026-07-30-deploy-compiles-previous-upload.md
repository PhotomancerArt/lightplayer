---
status: fixed      # CLI-side fix (P5, 2026-07-31); the observation-lag
                    # hypothesis itself is still unconfirmed on hardware —
                    # that walk is P7. See "Fix" and "Ruled out".
found: 2026-07-30      # how: hardware-walk
fixed: 2026-07-31 (PR #234, upload wait)
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

**Fix** — `lp-cli upload` (`lp-cli/src/commands/upload/handler.rs`) no longer
disconnects on the `LoadProject` ack. It now polls `project.read` on the
*same* connection (the S3 serial port is exclusive; a monitor cannot attach
separately) using the `WireProjectHandle` that `LoadProject`'s own response
returned, every 250 ms, until one of:

- the project has rendered at least one frame (`RuntimeReadResult.project.frame_num
  > 0`) with no node error observed in the same read — reported as running,
  exit 0;
- a node reports a definitive failure (`NodeRuntimeStatus::Error` /
  `InitError` — the shader node's `runtime_status()` surfaces its
  `compilation_error` exactly this way) — reported immediately, exit nonzero,
  without waiting out the timeout;
- `--wait-timeout <secs>` (default 30) elapses with neither — exit nonzero,
  message states the deploy was acked but no running evidence arrived.

The read is scoped to the handle server-side, so a stale/foreign handle
surfaces as a protocol error rather than silently describing someone else's
project — no separate project-uid check was needed. `--no-wait` restores the
exact pre-fix behaviour (disconnect the instant the deploy is acked) for
callers that don't want to block. See `lp-cli/src/commands/upload/wait.rs`.

This closes the *CLI-side* half of the defect (what `upload` makes
observable). It does not, by itself, confirm the reset/reload explanation in
"Root cause" — that needs a hardware walk that reads the compile line from a
monitor attached strictly after `upload` returns and compares it against the
same walk's boot line, which lands at the P7 gate.

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

For the P5 CLI fix: `lp-cli/tests/upload_wait.rs` drives the real
`handle_upload` entry point against `HostSpecifier::Local` (an in-process
`fw-host` runtime ticking a real `LpServer`, `lp-cli/src/client/host_process.rs`)
— `upload_waits_and_reports_the_project_running` (the happy path resolves),
`upload_wait_ends_nonzero_on_a_shader_compile_failure` (a broken shader ends
the wait immediately, not by timing out), `no_wait_skips_the_wait_even_when_the_shader_would_fail`
(`--no-wait` restores fire-and-forget), and
`upload_wait_times_out_nonzero_when_no_evidence_arrives` (a zero-budget wait
reports the acked-but-no-evidence message). `lp-cli/src/commands/upload/wait.rs`
also unit-tests the event-stream reduction (pending/running/error) directly,
independent of a live connection.

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
