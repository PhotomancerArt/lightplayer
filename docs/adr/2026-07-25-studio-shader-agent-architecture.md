# ADR: Studio shader agent — in-app harness, providers, write surface

- **Status:** Accepted
- **Date:** 2026-07-25
- **Deciders:** Photomancer
- **Supersedes:** None (pairs with
  `2026-07-25-shader-probe-experiment-api.md`; builds on the editing
  overlay model in `2026-07-04-studio-editing-model` and the auto-apply
  seam in `2026-07-14-shader-auto-apply`)
- **Superseded by:** None

## Context

Writing GLSL is the hardest part of using LightPlayer; the target user
can build fixtures but bounces off shaders. The decision here is where
an authoring agent lives and what it may touch. Constraints that shaped
it:

- **Synthetic UI input does not reach Dioxus state** (established
  2026-07-16): an external agent driving the browser cannot edit through
  the editor. Any agent must enter *below* the view layer.
- Studio has a **pure-browser mode** (OPFS + in-worker sim) with no
  backing server; lightplayer.app is static hosting. A server-side
  harness would break that mode and centralize user API keys.
- The whole compile+probe stack is wasm-proven (`lps-probe`), so the
  feedback loop can run entirely client-side.
- Yona's product direction is agent-first: "in reality no one will
  bother looking at the code."

## Decision

### The agent runs in the Studio wasm app

Two crates: `lp-app/lpa-agent` (model-facing core: providers, session
loop, the tool; no Studio/UI/settings code) and an `agent/` module in
`lpa-studio-core` (AgentController + host bridge, below the view
layer). The web shell only renders DTOs and forwards actions, per the
humble-view architecture.

### ModelProvider abstraction; two streaming implementations

`ModelProvider` is deliberately one method — `run_turn(TurnRequest) ->
BoxStream<TurnEvent>` — with provider-neutral events, because a future
**in-web local model** (WebGPU-served, for users without keys) must be
implementable against it. Streams are runtime-neutral and `!Send`;
whatever polls them drives the transport (sans-IO edges intact).

- **Anthropic** — BYOK, direct browser calls with the
  `anthropic-dangerous-direct-browser-access` header; no proxy, keys
  never leave the client.
- **OpenAI-compatible** — the Chat Completions dialect, which covers
  hosted OpenAI *and* local/OSS servers (Ollama, LM Studio, llama.cpp,
  vLLM) through one `base_url` field; keyless local servers omit the
  Authorization header entirely; no default model (model ids belong to
  the serving provider); tolerant of sloppy local streams (missing
  usage ⇒ zeros, EOF after `finish_reason` ⇒ turn completes).

Both ride a **shared SSE framing layer** (`sse_parser`,
`HttpSseTransport`, wasm fetch/ReadableStream + host reqwest
transports) that lives at `provider/` level — the parser was never
Anthropic-specific; only the JSON payloads differ per provider.
Providers are rebuilt from fresh effective settings on every run, so a
settings change applies without losing the conversation.

### One tool, one write surface

The single `iterate` tool (optionally stage source, set transient
bindings, run probes, diff vs previous; `capture` reserved until M6)
batches one hypothesis per round-trip. Staged source lands as the SAME
`AssetEditOp::ApplyBody` the human editor Apply uses — the existing
overlay is the only write path, so **dirty tracking, diff, Save, and
revert are the permission system**; the agent gets no second door.
Compile errors return as tool *data* (`is_error` is reserved for host
failures) so the model iterates instead of aborting. Schema violations
come back in-band for the same reason.

### Per-shader sessions with injected context

Sessions key on (runtime, shader node); each shader gets its own chat
and memory, in-memory for the app lifetime (v1 — no persistence).
Context is **injected** into the system prompt each run — shader
source (overlay-aware), the consumed-uniform binding table, fixture
summary, LED sample points, and the `lpfn_*` builtin reference
generated from `CANONICAL_GLSL` — there are no navigation/read tools.
This keeps the tool surface at one and the token budget predictable.

### UI: Agent | Code tabs where the editor lives

The node editor region gets Agent | Code tabs — each shader's chat
owns the editor's vertical space (agent-first). Default tab = Agent
when a provider is configured, Code otherwise; the Code tab carries a
warning-toned dirty dot so unsaved state is visible from the Agent
tab. Assistant text renders through a safe markdown path
(pulldown-cmark → Dioxus nodes, HTML escaped, link schemes
allowlisted, zero `dangerous_inner_html`) — model output is untrusted
input.

### Layered settings; machine-level dev source

`SettingsStore` merges **user > host-channel > defaults**:

- Defaults are baked (`DEFAULT_AGENT_MODEL`).
- The **host channel** is a boot fetch of `/dev-settings.json`; `just
  studio-dev` syncs it from `~/.lightplayer/settings.json`, so worktree
  dev servers share one machine-level key file. 404 (lightplayer.app)
  ⇒ layer absent. Electron later provides the same shape via IPC — a
  channel contract, not a dev-server hack.
- User overrides live in localStorage (`lp.settings.v1`), edited in the
  header settings popover (masked key, provenance hints). Plaintext v1
  posture is documented where the key is entered.

Settings carry an `AgentProvider` selector with per-provider fields
and **single-source onboarding guidance** (key-page links for
Anthropic/OpenAI; base_url + `OLLAMA_ORIGINS` CORS caveat for local) —
the not-configured state must never be a dead end. **Cost estimation**
uses a dated pricing table (prefix-matched model ids) plus optional
per-MTok rate overrides in settings; the estimate renders with the
usage totals in the chat footer.

### Direction (recorded, not scope)

- A shader node's UI trends toward **preview + chat**, code secondary.
- Capabilities expand **outward from GLSL** — bindings, bus
  visibility, node suggestions — as new tools/context on the *same*
  per-shader session, not a new architecture. A project-level agent is
  explicitly later.

## Consequences

- Works identically in pure-browser and dev-server modes; no key ever
  transits a LightPlayer server.
- The overlay write path means a hostile/buggy agent edit is exactly as
  recoverable as a human edit (revert), and live-sim compile feedback
  ("verdict chase") is inherited for free.
- E2E coverage runs a real actor + in-process server with a scripted
  fake provider (split JSON fragments → real lps-probe compile → staged
  overlay + dirty save panel); provider failures (401) surface as
  status + notice with working retry.
- The eval harness (P6) graded the full loop live: **5/5 tasks, 23/23
  probe assertions, first run, zero tuning**, ≈ $0.30 — evidence the
  injected-context + single-tool contract is sufficient for the v1
  scope.
- Anthropic requires the dangerous-direct-browser-access opt-in; that
  is the accepted BYOK trade (the alternative is a key-holding proxy).
- The pricing table will go stale; rates are overridable in settings
  and the table is dated so staleness is visible.

## Alternatives Considered

- **UI automation (external agent drives the app)**: rejected — fails
  outright; Dioxus never sees synthetic input (2026-07-16 finding).
- **Local agent + filesystem sync**: rejected — no feedback channel
  (diagnostics, probes, sim state live in the app), and it abandons the
  pure-browser mode.
- **Server-side harness**: rejected — breaks static hosting/OPFS mode
  and centralizes user keys.
- **Patch-based edits**: rejected — shaders are small; full-source
  replace removes a whole class of patch-application failures.
- **Navigation/read tools instead of injected context**: rejected for
  v1 — more round-trips, larger tool surface, and the shader scope
  makes full context injection cheap.
- **Markdown via raw HTML injection**: rejected — model output is
  untrusted; escaped rendering with allowlisted link schemes only.
- **Per-provider settings guidance in the web layer**: rejected — the
  guidance copy lives core-side, single-sourced next to the provider
  enum, so shells (web now, Electron later) render one truth.

## Follow-ups

Recorded, deliberately not built in this slice:

- **Probe/agent-activity visualization** — render probe domains and
  results on the preview ("right now it just sits there thinking");
  pairs naturally with M6 capture. Likely its own plan.
- **Full node-UX pass** toward preview + chat (a design spike is
  separately underway); this slice added only the tab strip + dirty
  dot.
- **Live-sim binding push** — agent binding overrides currently affect
  only the probe oracle, not the running sim; transient push with
  visible indication + auto-clear is future capability work.
- **Worker offload for probe eval** (shared with the probe ADR) — also
  the pragmatic bound on infinite-loop shaders until interp fuel.
- **In-web local model provider** — the reason ModelProvider exists;
  implement when a credible in-browser model is worth serving.
- **M6 capture** — unreserve the `capture` tool field when the preview
  snapshot seam lands.

## 2026-07-27 Addendum: Params write surface, cost, visibility, session history

One round of prod testing later (Yona: "it actually works very well" —
cost and opacity were the complaints, not quality), the
agent-params-and-polish round (PR #158) grew the architecture along the
recorded "outward from GLSL" direction. The original decisions stand;
this addendum records what changed and what the round measured.

### Write surface: two tools, still one door

`upsert_param` joins `iterate` as the second tool — a narrow f32 param
upsert (name, label, default, min/max, unit, panel) that materializes as
ONE `MutationCmdBatch` of `PutSlotEdit`s on the def artifact, through
the SAME Save-gated overlay as source edits. The "overlay is the
permission system" rule is unchanged; the write surface widened, the
door count did not. The pre-check accepts a param name from the
declared-uniform set (fresh compile) or the existing def set; when the
staged source does not compile, a comment-stripped textual declaration
scan substitutes for the declared set — declare → upsert → fix-code is
a normal repair order and must not be refused exactly when the agent is
mid-repair (live-session finding).

### `iterate` is feedback-dense: engine verdict + params diff

The result (and transcript summary) now carries two new sections:

- **`engine`** — the REAL engine-side verdict (`ok | error | unknown`,
  message, line/col), fetched by the host bridge awaiting the verdict
  chase bounded (`ENGINE_VERDICT_BUDGET_MS = 1500`) after a stage.
  Probe oracle (naga) and engine runtime are different compile worlds;
  before this the agent literally could not see "shader render: missing
  uniform field" errors, and a live session burned 14 turns on "compile
  ok" rows the engine rejected every one of.
- **`params`** — declared uniforms (a `LpsModuleSig` leaf walker in
  `lps-probe`, `outputSize` flagged reserved) diffed against def-side
  `ShaderSlotDef` records, orphans flagged both ways. This is the
  static catch for the missing-uniform-field class, and the read half
  of the `upsert_param` loop.

### Async host, live progress, honest run ends

`AgentHost` write/verdict methods return boxed `!Send` `HostFuture`s
(read accessors stay sync — host-owned snapshots). `run_iterate` emits
`ToolPhase` progress (`staging / compiling / probe i/of / waiting for
engine / finishing`) with UI yield points, so the tab paints while
experiments run. Runs can no longer end silently: `stop_reason` other
than end-turn surfaces as `AgentEvent::Truncated`, a truncated dangling
`tool_use` is dropped from the recorded assistant message (replay stays
protocol-valid), and aborts synthesize cancelled tool_results.

### Extended thinking (adaptive), replayed and displayed

Anthropic requests send `thinking: {type: "adaptive", display:
"summarized"}` — on the current model family (Sonnet 5) adaptive
thinking is on regardless and `display` defaults to omitted, which
streams EMPTY thinking blocks; the explicit shape is what makes
thinking visible. The old `enabled + budget_tokens` shape is rejected
by this family (pre-4.6 models would need it back — follow-up). Signed
thinking blocks are accumulated from the stream and replayed verbatim
through the tool-use loop (unsigned ones dropped at serialization;
`cache_control` never lands on thinking-family blocks). The
OpenAI-compat provider passes through `reasoning_content` / `reasoning`
deltas where servers emit them but never opts in on the request
(OpenRouter's `reasoning: {}` opt-in is a follow-up).

### Prompt caching: implemented, and measured leaky

Two `cache_control` breakpoints (system block — which by prefix order
also covers tools — plus a rolling marker on the last content block),
four disjoint usage buckets (`input` / `output` / `cache_write` /
`cache_read`) through both providers, and cache-aware cost estimates
(write 1.25×, read 0.1× of the input rate) in the chat footnote, the
debug export, and the eval report.

**Measured P0 caveat (P9 eval run, live)**: the system prompt embeds
the *current shader source* and is rebuilt every turn, and system sits
ahead of the conversation in the cache hash prefix — so **every staged
edit invalidates both cache entries for the next request**. Cache reads
only materialize on consecutive stage-free turns (eval evidence:
2-turn staging tasks read 0 cached tokens; a 3-turn task read only its
stage-free pair). An editing agent stages most turns, which is why live
session costs still look uncached. Fix direction (deliberately not done
blind in the closeout): keep the cached prefix static per session and
inject volatile source state append-only via the message stream — a
prompt-shape redesign with its own eval pass.

### Session edit history, thumbnails, revert

`AgentEditRecord`s (turn, note, staged source, engine verdict, 32×32
preview thumb captured post-verdict from the existing probe preview
bytes — no GPU capture seam needed) accumulate per session (cap 50).
UI: a filmstrip above the composer plus inline thumbs on the staged
tool rows; revert restages the old source through the normal
overlay/Save flow (`AgentOp::RevertToTurn`, idle-only). Reverts are as
recoverable as any other edit — same door.

### Chat log + debug export

Idle-only `AgentOp::ExportDebug` builds a versioned JSON dump — the
exact model-facing transcript (thinking blocks included), per-turn
stop reasons and usage buckets, the staged-edit history with full GLSL,
provider and model names, **never the key** (tested) — plus a readable
markdown chat log with per-row probe-compile AND engine verdicts and a
cache-bucket usage footline ("NO CACHE HITS" on a long session is a
regression signal, not a detail). The export paid for itself the day it
landed: it produced the upsert-on-broken-compile finding, the dialect
doctrine, and the caching caveat above.

### Model discovery

Every provider serves a models list (Anthropic `GET /v1/models`,
OpenAI-compat `GET {base_url}/models`), so a plain-GET sibling
transport (`HttpGetTransport`) + `provider/model_discovery.rs` feed a
fingerprint-keyed store (credentials+endpoint fingerprint; stale
responses can never land on new credentials). The settings model field
and a chat-footer model chip render dropdowns with a free-text escape
hatch; fetches are popover-open/chip-open triggered and debounced, boot
stays quiet.

### Dialect-landmine doctrine

The system prompt now carries a short list of known landmines where
GLSL compiles but a backend rejects it (currently: no swizzle-stores
through indexed array elements — a naga *frontend* limitation). Doctrine
learned the hard way: landmine entries require root-cause verification
before they enter the prompt — a "no nested break/continue" entry was
retracted after the real cause landed as the Q32 `fabs` wasm stack leak
(`docs/defects/2026-07-27-wasm-q32-fabs-stack-leak.md`); the control
flow was never broken. The prompt also gained bisection doctrine
(diagnostic detours capped at 2–3 calls, restage the best working
source before a run ends) from the same live sessions.

### Runtime command channel (related ADR)

The activate-entry work landed as its own decision —
`2026-07-27-runtime-node-command-channel.md`, the first non-overlay
client→engine write. It deliberately does NOT touch the agent's write
surface; the agent still owns exactly the overlay door.

### Addendum follow-ups

- **Cache-friendly prompt shape** (the P0 caveat above) — static cached
  prefix, volatile source injected append-only; re-measure with evals.
- Pre-4.6 Anthropic models need the `enabled + budget_tokens` thinking
  shape (they 400 on `adaptive`).
- OpenRouter reasoning requires a request opt-in (`reasoning: {}`) —
  provider-gated request field when wanted.
- Transcript/session persistence across reloads remains v1-out-of-scope.
