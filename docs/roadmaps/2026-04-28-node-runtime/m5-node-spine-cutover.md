# Milestone 5: Node spine — trait + tree + lifecycle + cutover

## Goal

Implement the **instance** half of the new spine in
`lpc-runtime` and port the legacy nodes onto it in one
milestone — no parallel-runtime bridge:

- New `Node` trait (tree-aware, with lifecycle, slot views,
  status / frame-versioning hooks).
- `NodeTree` container.
- Lifecycle / status / frame-versioning machinery lifted from
  the existing `lp-engine` `ProjectRuntime` into
  `lpc-runtime`.
- Filesystem change routing, panic recovery, shed plumbing,
  client / server protocol surface — all moved into
  `lpc-runtime` as generic concerns.
- `lpl-runtime`'s legacy nodes (`Texture` / `Shader` /
  `Output` / `Fixture`) ported to implement the new `Node`
  trait directly.
- `ProjectRuntime` cut over to be `NodeTree`-backed.
- ESP32 + emulator + lp-cli still green.

The cutover *is* the validation. If a legacy node can't be
expressed cleanly under the new shape, the trait surface
changes (or M3's `design.md` is updated and the plan
re-iterated). No "bridge" intermediate where old and new
runtimes coexist.

## Suggested plan location

`docs/roadmaps/2026-04-28-node-runtime/m5-node-spine-cutover/`

Full plan: `plan.md` plus per-phase notes / checkpoints. The
plan splits the milestone into clear phases (probably:
trait + tree first, then per-legacy-node port, then
`ProjectRuntime` cutover, then sync layer cleanup).

## Scope

**In scope:**

- `lpc-runtime::Node` trait — final shape from M3:
  - Identity (`uid`, `path`, `parent`).
  - Slot views (`params`, `inputs`, `outputs`, `state`).
  - Lifecycle (`init`, `render`, `destroy`,
    `shed_optional_buffers`, `update_config`,
    `handle_fs_change`).
  - Children enumeration.
  - Object-safe, `no_std + alloc`.
- `lpc-runtime::NodeTree`:
  - `BTreeMap<Uid, Node>` (or whatever M3 picked).
  - `BTreeMap<NodePath, Uid>` index.
  - Parent / child relationships, ordered children.
  - Insertion / removal (with cascading destroy of
    descendants).
  - Status enum + frame-versioned change events at the tree
    level.
- Lifecycle machinery in `lpc-runtime`:
  - `NodeStatus` (Created / InitError / Ok / Warn / Error).
  - `FrameId` and frame-versioned change events.
  - Panic recovery wrapping (lifted from current
    `ProjectRuntime`).
  - Shed budget + per-node `shed_optional_buffers` calls.
  - Filesystem change routing (lifted from current
    `ProjectRuntime`'s `handle_fs_changes`).
  - Lazy / demand-driven render (lifted from current texture
    rendering).
- Sync layer surface in `lpc-runtime`:
  - Change events flow to clients via the existing
    protocol.
  - Protocol stays compatible with `lp-engine-client` /
    `lp-client` / `lp-cli` (the wire shape may evolve, but
    consumers don't break in a way that needs new code in
    those crates beyond import fixes).
- Legacy node port (`lpl-runtime`):
  - `TextureRuntime` implements `Node`.
  - `ShaderRuntime` implements `Node`.
  - `OutputRuntime` implements `Node`.
  - `FixtureRuntime` implements `Node`.
  - Each existing behaviour (lazy demand render,
    shed-before-recompile, panic-recovery wrapping, status
    transitions, fs-change handling) preserved.
- `ProjectRuntime` cutover:
  - Replaced by a `NodeTree`-backed engine in `lpc-runtime`.
  - Legacy nodes register via the new path; loaded artifacts
    instantiate via M4's `ArtifactManager`.
  - The existing `lp-server` / `lp-client` / `lp-cli`
    surfaces continue to work (import updates aside).
- Conformance tests:
  - Behavioural parity tests for each legacy node before
    the cutover (capture current behaviour as snapshot /
    assertion suite).
  - Same tests pass after cutover.
  - ESP32 release build, emu release build, host workspace
    tests all green.

**Out of scope:**

- Visual subsystem (`lp-vis`) — next roadmap. The spine has
  to *support* dynamic / recursive visuals (Stack containing
  Effect containing Pattern), but no visual artifact types
  are implemented here.
- `lpfx` rendering abstraction split — next roadmap.
- Filetest harness for CPU↔GPU comparison — next roadmap.
- Adding new types to `lpv-model` or refining the visual
  model — next roadmap. (M2 already moved the existing
  visual types into `lpv-model` as-is.)
- Editor UX changes; existing surfaces stay working.
- New legacy node features; only the *port* happens here.

## Key decisions

- **No bridge.** Validating the shape *is* the point of
  porting legacy. Running old and new runtimes in parallel
  defers the validation and adds risk. The cutover is
  decisive.
- **Conformance suite before cutover.** Capture the existing
  legacy node behaviour as a regression suite *first*, then
  port + cut over. The suite is the safety net.
- **Lifecycle / status / fs-watch / panic recovery / shed
  are all generic.** They live on `lpc-runtime` (the spine),
  not on each per-domain runtime. `lpl-runtime` only
  provides the per-node behaviour; the container manages the
  rest.
- **Dual children sources go through one mechanism.** Both
  structural children (an `Effect`'s `input`) and
  param-promoted children (a `gradient` param sourcing a
  `Pattern`) end up as ordered `Vec<Uid>` on the parent.
  M3's `design.md` fixes the exact mechanism; M5 implements
  it.
- **Sync layer wire compatibility is preserved.** ESP32
  firmware in the field doesn't need to be updated to talk
  to a re-architected server. `lp-engine-client` keeps
  following. If wire format must evolve, it's behind a
  protocol version gate, not a hard break.
- **Behavioural parity is the success criterion.** Not
  "works on my machine" — full ESP32 release build, full
  emu release build, full conformance suite green.

## Deliverables

- `lpc-runtime::Node` trait + impl support.
- `lpc-runtime::NodeTree` + lifecycle / status /
  versioning / fs-watch / panic recovery / shed.
- Sync layer in `lpc-runtime` consumed by `lp-server` /
  `lp-client` unchanged externally.
- `lpl-runtime` nodes implement `Node` directly; old
  `NodeRuntime` trait retired.
- `ProjectRuntime` replaced by the `NodeTree`-backed engine.
- Behavioural conformance suite green pre- and post-
  cutover.
- Workspace gates green: `just ci` (full pipeline), ESP32
  release build, emu release build, lp-cli end-to-end.

## Dependencies

- M3 (spine design pass) — implements `design.md`'s `Node`
  trait + `NodeTree` shape.
- M4 (artifact spine) — `NodeTree` instantiation calls
  `ArtifactManager`.
- Blocks: M6 (cleanup + validation + summary).

## Execution strategy

**Option C — full plan (`/plan`).**

Justification: M5 is the highest-risk milestone. Cutting
over `ProjectRuntime` while keeping ESP32 / emulator / lp-cli
green requires careful phasing: build the spine, capture
conformance, port nodes, switch the engine, retire the old
trait. The plan-iteration loop is essential here — surprises
land in the plan, not in the cutover. Probably the
longest-running milestone in the roadmap.

> I suggest we use the `/plan` process for this milestone, after
> which I will automatically implement. Agree?
