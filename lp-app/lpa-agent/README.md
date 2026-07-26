# lpa-agent

Model-facing shader agent core: the `ModelProvider` abstraction with two
streaming implementations (Anthropic and OpenAI-compatible, over shared
wasm/host transports), the agentic loop, the single `iterate` tool bound to
`lps-probe` through an injected host trait, and the system-prompt builder.
No Studio/UI/settings code — Studio implements `AgentHost` and maps its
persisted settings into `AnthropicConfig` / `OpenAiCompatConfig` (P5/P8 of
the shader-agent plan).

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
src/provider/model_provider.rs          trait + TurnRequest/TurnEvent/ChatMessage
src/provider/sse_parser.rs              pure incremental bytes → SSE events (shared framing)
src/provider/http_transport.rs          trait HttpSseTransport (the platform seam)
src/provider/transport_web.rs           wasm: web-sys fetch + ReadableStream
src/provider/transport_host.rs          host (feature "host-transport"): reqwest
src/provider/anthropic/
  anthropic_provider.rs                 turn state machine, retry policy
  anthropic_wire.rs                     request/SSE serde types
src/provider/openai_compat/
  openai_compat_provider.rs             turn state machine, retry policy
  openai_compat_wire.rs                 request/chunk serde types + transcript mapping
src/session/                            AgentSession loop, transcript, AgentEvents
src/tool/                               iterate tool + AgentHost seam
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

## The `iterate` tool

One tool, one write surface: optionally stage new GLSL source (an unsaved
overlay edit), compile, evaluate probes on the `lps-probe` f32 oracle, and
always return compile diagnostics + a health report. Compile errors are
returned as data (`is_error` is reserved for host failures). A per-session
cache of the last compiled shader powers `diff: { "vs": "previous" }`;
`capture` is reserved until the M6 preview snapshot seam lands. Staged
source is pre-checked against the 10 KB asset cap (`MAX_SOURCE_BYTES`,
mirroring `lpa_studio_core::MAX_ASSET_BODY_BYTES`).

## Session semantics

- `MAX_TURNS_PER_RUN = 16` model turns per user message; on hitting the
  limit the transcript gets a system-style nudge and the run stops cleanly.
- Abort: a shared `AtomicBool` (`abort_handle()`) checked between events and
  tool executions; dropping the turn stream cancels the transport (the wasm
  transport aborts the fetch via `AbortController`).
- Transcript is in-memory only (v1 decision); usage totals accumulate on it.
- Provider retry: one retry with short backoff on 429/5xx/network failures
  that happen before any event was emitted; everything else surfaces as a
  `ProviderError` event.

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
