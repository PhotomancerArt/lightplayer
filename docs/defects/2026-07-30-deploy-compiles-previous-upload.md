---
status: open
found: 2026-07-30      # how: hardware-walk
area: lpa-server (project deploy/reload) — reproduced on fw-esp32s3
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

**Root cause** — Not localised. The shape is that the project reload triggered
by a deploy reads its artifacts from state populated *before* the deploy's file
writes are visible, so the reader is one write behind the writer. Whether that
is the `FsVersion` bookkeeping in `advance_frame`'s refresh path, the deploy
handler's ordering of write-then-load, or the artifact cache in the project
runtime is not established.

**Fix** — None yet.

**Regression coverage** — None. A host test for this needs the *deploy* path
(`deploy_project_files`), not the load path; every existing project test writes
files and then loads, which is precisely the ordering that hides it.

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
