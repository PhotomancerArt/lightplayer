# lpa-agent

Model-facing shader agent core: the `ModelProvider` abstraction with two
streaming implementations (Anthropic and OpenAI-compatible, over shared
wasm/host transports), the agentic loop, the `iterate` + `upsert_param`
tools bound to `lps-probe` through an injected host trait, model-list
discovery, and the system-prompt builder. No Studio/UI/settings code —
Studio implements `AgentHost` and maps its persisted settings into
`AnthropicConfig` / `OpenAiCompatConfig`.

## Why `ModelProvider` exists

The trait is deliberately tiny — one method, `run_turn(TurnRequest) ->
BoxStream<TurnEvent>` — because a future **in-web local model** must be
implementable against it. Anything provider-specific (event grammar, retry
policy, headers) stays inside `provider/anthropic` /
`provider/openai_compat`; the session loop only sees provider-neutral
`TurnEvent`s. Streams are runtime-neutral and `!Send`: whatever polls them
drives the transport (Studio's wasm main thread, `block_on` in tests and
evals).

## Layout

```
src/provider/model_provider.rs          trait + TurnRequest/TurnEvent/ChatMessage/TokenUsage
src/provider/sse_parser.rs              pure incremental bytes → SSE events (shared framing)
src/provider/http_transport.rs          traits HttpSseTransport + HttpGetTransport (platform seams)
src/provider/transport_web.rs           wasm: web-sys fetch + ReadableStream
src/provider/transport_host.rs          host (feature "host-transport"): reqwest
src/provider/model_discovery.rs         GET /models for both providers → ModelInfo lists
src/provider/anthropic/
  anthropic_provider.rs                 turn state machine, retry policy
  anthropic_wire.rs                     request/SSE serde types, cache breakpoints, thinking config
src/provider/openai_compat/
  openai_compat_provider.rs             turn state machine, retry policy
  openai_compat_wire.rs                 request/chunk serde types + transcript mapping
src/session/                            AgentSession loop, transcript, AgentEvents
src/tool/                               iterate + upsert_param tools, AgentHost seam, ToolPhase
src/prompt/                             system prompt + builtin reference
```

## The OpenAI-compatible provider

`OpenAiCompatProvider` speaks the OpenAI Chat Completions dialect
(`POST {base_url}/chat/completions`, `stream: true`,
`stream_options.include_usage`, `max_completion_tokens`), which is also
what Ollama, LM Studio, llama.cpp, and vLLM serve. That makes one provider
cover both hosted OpenAI (`base_url = https://api.openai.com/v1`, key
required) and local/OSS servers (`base_url = http://localhost:11434/v1`
for Ollama; `api_key: None` omits the `Authorization` header entirely).
`OpenAiCompatConfig` has **no default model** — model ids belong to the
serving provider and are never guessed here. The SSE framing (`data:`
lines, `[DONE]` terminator) rides the same shared `sse_parser`; only the
JSON payloads differ. The transcript stays in the neutral
`ChatMessage`/`ContentBlock` form; the provider converts per request (tool
results become `role: "tool"` messages, tool calls become
`tool_calls: [{type: "function", …}]`). Missing usage on the final chunk
(some local servers) is tolerated and reported as zeros; a dropped
`[DONE]` after a `finish_reason` still completes the turn.
`reasoning_content` / `reasoning` deltas are passed through as thinking
events where servers emit them; nothing reasoning-related is ever sent on
the request (unsafe on arbitrary compat servers).

## Prompt caching and usage buckets

Anthropic requests carry two `cache_control` breakpoints: one on the
system content block (which by prefix order also covers the tool
definitions) and one rolling marker on the last content block of the last
message. `TokenUsage` splits four disjoint buckets — `input_tokens`
(uncached remainder), `output_tokens`, `cache_write_tokens`,
`cache_read_tokens` — on both providers (the compat dialect's
`prompt_tokens` includes cached tokens; the provider subtracts so the
buckets stay disjoint). Cache pricing convention: write = 1.25× input
rate, read = 0.1×. Known caveat (measured): the system prompt embeds the
current shader source and is rebuilt per turn, so a staged edit
invalidates the cached prefix for the following request — see the
2026-07-27 addendum to `docs/adr/2026-07-25-studio-shader-agent-architecture.md`.

## Extended thinking

Anthropic requests send `thinking: {type: "adaptive", display:
"summarized"}` (the current model family rejects `budget_tokens`; without
an explicit `display`, thinking blocks stream empty). Thinking streams as
`TurnEvent::ThinkingDelta` / `ThinkingSignature` / `RedactedThinking`;
the session rebuilds blocks from the stream and replays **signed** blocks
verbatim through the tool-use loop (unsigned blocks are dropped at
serialization; `cache_control` never lands on thinking-family blocks).

## The tools

- **`iterate`** — one hypothesis per round-trip: optionally stage new
  GLSL source (an unsaved overlay edit), compile, evaluate probes on the
  `lps-probe` f32 oracle, and always return compile diagnostics + a
  health report, plus an **`engine` section** (the real engine verdict,
  awaited bounded by the host after a stage — the probe oracle and the
  engine are different compile worlds) and a **`params` section**
  (declared uniforms diffed against def-side param records, orphans
  flagged both ways). Compile errors are returned as data (`is_error` is
  reserved for host failures). A per-session cache of the last compiled
  shader powers `diff: { "vs": "previous" }`; `capture` is reserved until
  the M6 preview snapshot seam lands. Staged source is pre-checked
  against the 10 KB asset cap (`MAX_SOURCE_BYTES`).
- **`upsert_param`** — narrow f32 param record upsert (name, label,
  default, min/max, unit, panel), dispatched through the host to the same
  Save-gated overlay path. The name must be a declared uniform or an
  existing def record; when the staged source does not compile, a textual
  declaration scan substitutes for the declared set.

Tool execution reports progress via `ToolPhase` (staging / compiling /
probe i-of-n / waiting for engine / finishing) with UI yield points, so a
wasm host repaints during long experiments. `AgentHost` write/verdict
methods are async (`HostFuture`, boxed `!Send`); read accessors stay sync.

## Session semantics

- `MAX_TURNS_PER_RUN = 16` model turns per user message; on hitting the
  limit the transcript gets a system-style nudge and the run stops cleanly.
- Runs never end silently: a stop reason other than end-turn emits
  `AgentEvent::Truncated { stop_reason, dropped_tool_call }`; a truncated
  dangling `tool_use` is dropped from the recorded assistant message so
  the replayed transcript stays protocol-valid; aborting mid-loop
  synthesizes cancelled tool_results.
- Abort: a shared `AtomicBool` (`abort_handle()`) checked between events and
  tool executions; dropping the turn stream cancels the transport (the wasm
  transport aborts the fetch via `AbortController`).
- Transcript is in-memory only (v1 decision); usage totals accumulate on it.
- Provider retry: one retry with short backoff on 429/5xx/network failures
  that happen before any event was emitted; everything else surfaces as a
  `ProviderError` event.

## Model discovery

`provider/model_discovery.rs` lists models over the plain-GET transport
seam (`HttpGetTransport`): Anthropic `GET {base}/v1/models`, compat
`GET {base_url}/models`, both parsed from `{"data":[{id,…}]}` with
display names where the server provides them. Errors are typed
(`Auth`/`Network`/`Http`/`Parse`) so hosts can render actionable
guidance. Studio's settings store keys results by a credentials+endpoint
fingerprint.

## Testing

Everything is host-testable against fakes (scripted transports, providers,
hosts). The assembled system prompt is snapshot-tested; regenerate with:

```bash
LPA_AGENT_UPDATE_SNAPSHOTS=1 cargo test -p lpa-agent
```

Validation:

```bash
cargo test -p lpa-agent
cargo test -p lpa-agent --features host-transport
cargo check -p lpa-agent --target wasm32-unknown-unknown
```

Live evals (opt-in, never CI-blocking; see `tests/evals.rs` for the
provider/env conventions):

```bash
ANTHROPIC_API_KEY=... cargo test -p lpa-agent --features host-transport -- --ignored evals
```
