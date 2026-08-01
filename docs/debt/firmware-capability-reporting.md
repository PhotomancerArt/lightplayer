---
status: carried
since: 2026-07-30
logged: 2026-07-30
area: lpc-engine/nodes + lpc-wire
related:
  [
    "../../lp-core/lpc-engine/Cargo.toml",
    "../adr/2026-07-14-wire-hello-versioning.md",
    "../adr/2026-07-05-artifact-format-version-and-schema-snapshots.md",
  ]
---
# A gated-out node is silently inert, with no way for anyone to ask why

**Shape** — `lpc-engine`'s node runtimes are now individually feature-gated
(`node-button`, `node-radio`, `node-fluid`, `node-fixture`, `node-texture`,
`node-playlist`, `node-clock`, `node-shader`; all default-on) so a firmware
build can link only the node kinds it actually runs. Gating a runtime out
does not change the wire format or the schema — `lpc-model`'s `NodeDef`
variants are untouched, so every build still parses every project
identically — but when a project references a node kind the running build
doesn't compile in, `project_loader.rs`'s attach loop for that kind attaches
`CorePlaceholderNode::new_leaf(kind)` instead of the real runtime. The
placeholder is a legitimate `NodeRuntime`: it reports `NodeRuntimeStatus`
same as any freshly-created node, produces nothing, consumes nothing, and
rejects runtime commands with the same "node accepts no runtime commands"
default every quiet node gives. Nothing — not the device, not the studio —
records that the node's *definition* asked for a capability this *build*
doesn't have. The project loads; the node does nothing; the difference
between "intentionally does nothing" and "silently missing" is invisible on
the wire.

This is a deliberate, scoped-down stopgap, not an oversight. The original
draft plan for this milestone designed the full contract — a
`NodeRuntimeStatus` variant, a capability list on `ServerHello`, studio-side
handling — and Yona pulled it explicitly (2026-07-30 M2 plan, "⚠️ Scope
decision"): *"I'm OK not solving the missing-node problem robustly quite
yet... this is just temporary for development... we need the firmware to
report its capabilities in general, and I don't think we have that yet."*
Building the half-measure risked locking in exactly the kind of shim the
wire-compat policy exists to avoid.

**Why acceptable now** — every build in the fleet today is
development-only, targets one board (the C6), and whichever node kinds are
gated off are gated off *deliberately* by whoever configured that build —
the same person who authored (or chose) the project. There is no user
downstream of that choice to confuse.

**What makes it unacceptable later** — the moment boards with genuinely
different capabilities ship to people who aren't the one who built the
firmware, "the project silently does nothing" stops being a build
configuration detail and becomes an unexplained support problem: someone
authors a project against one board's capabilities, deploys it to another,
and gets a project that loads clean and produces nothing, with no signal
anywhere about which node or why.

**The real fix** — general firmware capability reporting: the device
advertises what it supports, the studio renders accordingly (grays out or
flags nodes the connected device can't run, before or after load). The
candidate seam is `ServerHello` (`lp-core/lpc-wire/src/server/hello.rs`),
already the self-describing boot/handshake frame carrying
`WIRE_PROTO_VERSION` — a natural place to add a capability set. The
constraint that makes this sticky, and is exactly why the stopgap was
rejected rather than shipped as a partial version: `NodeRuntimeStatus`
(`lp-core/lpc-model/src/node/node_runtime_status.rs`) derives both `Serialize`
and, behind `schema-gen`, `schemars::JsonSchema`. Any capability signal
routed through it — a new variant, a new field — is simultaneously a wire
change and a generated-schema change, gated by the `format:1` conformance
check (`docs/adr/2026-07-05-artifact-format-version-and-schema-snapshots.md`)
and subject to the `WIRE_PROTO_VERSION` bump rule. That is real design work (what shape
does a capability list take, does it live on `ServerHello` or somewhere
node-tree-local, how does the studio degrade a project it can't fully run),
not a field that can be bolted on without deciding the contract first.

**Workarounds** — None needed yet; there is exactly one board and the
person choosing the firmware's feature set is the person authoring the
project. If a project mysteriously does nothing on a gated-down build,
check `lpc-engine/Cargo.toml`'s `default` feature list against the node
kinds the project's `project.json` actually uses.

**Incident log**
- **2026-07-30** — Filed alongside the node-runtime feature gates
  themselves (M2 of the S3 app-layer plan); no incidents yet, since nothing
  ships a gated-down build to anyone but the developer who configured it.
- **2026-07-30** — First gated-down build ran on hardware (M3 P5, the ESP32-S3
  app layer with all eight gates off and `NullGraphics`). Two observations that
  sharpen this entry:

  **"Silently inert" is not silent at the output boundary.** The `Output` node
  is ungated, so it registers a sink and resolves its input every frame — and
  every frame `LpServer::tick` warns
  `node NodeId(3): resolve output input: produce: node NodeId(2) does not
  produce slot "output"` (rate-limited to `persists (N consecutive frames)`).
  So a gated-out producer *does* surface, but as a **per-frame runtime warning
  naming a symptom**, not as a capability statement. Nothing says "this build
  has no shader node"; it says a node did not produce a slot, which reads as a
  project-authoring error rather than a build-configuration one. That is
  arguably worse than silence, because it is a plausible wrong explanation.

  **A whole class of node is unreachable by construction.** With every gate off
  *and* a null graphics backend, no node kind can produce pixels at all, so the
  output path cannot be exercised end-to-end. That was accepted deliberately
  for M3, but it means "does this build actually work?" is unanswerable from
  the device's own output — exactly the question capability reporting would
  answer.

- **2026-07-31** — The S3 node-gates plan flipped fw-esp32s3 to all eight
  gates, so both shipping firmwares now have IDENTICAL node-gate sets and the
  "two boards genuinely differ in gates" trigger is (for now) defused on that
  axis. Two sharpenings survive it: (a) the residual difference is **hardware
  capability**, not gates — the S3 has button + WS281x drivers but no radio
  transport, so a `node-radio` project on the S3 fails with "control radio
  node has no radio service", which is a *visible node error* rather than a
  silent placeholder but is still a symptom, not a capability statement;
  (b) reporting remains deferred deliberately — a dedicated planning session
  for this system was spawned the same day (Yona's call), so the design work
  has a home. Status stays `carried`.

**Exit criteria** — A device that omits a node kind's runtime says so on
`ServerHello` (or an equivalent capability seam), and the studio visibly
distinguishes "this node is disabled by this device" from "this node is
broken" for any project loaded against it. Landing that promotes this entry
to `retired` and should cite whatever ADR settles the capability-list shape
and its `format:N` bump.
