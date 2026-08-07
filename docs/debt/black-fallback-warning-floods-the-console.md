---
status: carried
since: 2026-08-02
logged: 2026-08-02
area: lp-core/lpc-engine (shader node), vs the rate-limited path beside it
related:
  [
    "../../lp-core/lpc-engine/src/nodes/shader/shader_node.rs",
    "../../lp-app/lpa-server/src/server.rs",
    "../../lp-cli/src/commands/hardware/bench/run.rs",
  ]
---

# A quarantined shader logs its black fallback every frame, unthrottled

When the recovery ladder disables a shader compile, the shader node falls
back to sampling black — and logs a `WARN` **per frame**, forever:

```
[WARN] lpc_engine::nodes::shader::shader_node: [shader-node] sampling black
  fallback (node=NodeId(4)): shader compile: recovery: shader-compile 'glsl'
  (disabled after 2 crashes)
```

Measured 2026-08-02 on `domraem/dom-z-102` at 600 LEDs during a soft-limit
bench: **90,020 of these lines in a single run**, at roughly 30 fps on a
921,600-baud console.

The tick-error path in `lpa-server` a layer above does exactly the right
thing for the same class of persistent condition — it restates on a counter
(`tick error persists (512 consecutive frames)`) instead of every frame. The
shader node's fallback warning has no such throttle.

## Why it matters

Console spam at frame rate is not a cosmetic problem: it saturates the
serial link the host is *also* using to talk to the device.

The soft-limit bench lost a full run to it. Once the workload was
quarantined, the flood starved the request path so thoroughly that the
bench's own recovery — reset the device, clear the ladder, re-run the
step — could not get a word in, and a step that should take ~30 s was still
unfinished 45 minutes later. On a board whose UART RX FIFO already overflows
under normal console load (see the FIFO note in `bench/run.rs`), this turns
a degraded-but-recoverable state into an unrecoverable one.

It also hurts anyone reading a device console during exactly the situation
they most need to read it: right after something crashed.

## Exit criteria

- The black-fallback warning is rate-limited like its neighbours: log the
  transition, then restate on a frame counter (or a duration), not per
  frame.
- Ideally the *first* line names the cause once, loudly, and the restatement
  is terse.
- Worth a sweep for other per-frame `warn!`/`error!` calls on paths that
  persist by construction; this is unlikely to be the only one.
