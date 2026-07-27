# ADR: OpenRouter PKCE Connect — the zero-setup agent provider path

- **Status:** Accepted
- **Date:** 2026-07-26
- **Deciders:** Photomancer
- **Supersedes:** None (extends the BYOK provider surface of
  `2026-07-25-studio-shader-agent-architecture.md`; pairs with
  `2026-07-26-no-browser-local-model-provider.md`, which closed the
  local-model route to zero-setup)
- **Superseded by:** None

## Context

The shader agent shipped BYOK-only: every path to a working agent starts
with "create an API key on a developer console and paste it." That is the
single biggest onboarding cliff for the agent-first product direction, and
the two alternatives are closed:

- **Browser-local models** fail at the protocol level at browser-fittable
  sizes (measured 2026-07-26; see the no-browser-local-model ADR).
- **First-party subscription OAuth** (use your ChatGPT/Claude/Gemini plan in
  a third-party app) was actively shut down by all three vendors in H1 2026
  — Anthropic ToS-excluded third-party tools in January–February, Google cut
  Gemini CLI OAuth for third-party software in June, OpenAI's "Sign in with
  ChatGPT" never opened to third parties.
- A **hosted relay** (our metered key behind a server) would break the
  static-hosting / pure-browser posture the agent architecture deliberately
  preserves, and would put us in the billing loop.

OpenRouter's OAuth PKCE flow is the one sanctioned, mature, zero-backend
"click to connect" lane: no client registration or secret, the exchange
endpoint is CORS-open, the created key lands in the USER's OpenRouter
account (charged to their credits, revocable by them), token prices are
provider pass-through (OpenRouter takes ~5.5% at credit purchase), and the
API speaks the OpenAI chat-completions dialect our compat provider already
implements — with every major frontier model available under one account.

## Decision

### OpenRouter is a first-class provider, and the recommended one

`AgentProvider::OpenRouter` resolves to `AgentProviderConfig::OpenAiCompat`
with a baked `https://openrouter.ai/api/v1` base URL — the wasm provider
factory is untouched. It leads the selector order; the enum default stays
Anthropic so existing installs keep their provider.

### Connect alone makes the agent ready

Two baked defaults remove every field from the funnel: the base URL and a
default model (`anthropic/claude-sonnet-5` — the same model the eval corpus
passes 5/5; slug verified against OpenRouter's live `/models` 2026-07-26).
The key is the only stored field (`openrouter_api_key`), written by the
exchange, cleared by Disconnect.

### The PKCE flow is client-only and boot-intercepted

`start_connect` (32-byte crypto verifier, S256 challenge, verifier + return
route stashed in sessionStorage) redirects to `openrouter.ai/auth`; at boot,
`take_pending_callback` consumes `?code=…` synchronously BEFORE the router
parses the URL, scrubs the query, and restores the pre-redirect hash; the
exchange (`POST /api/v1/auth/keys`) runs async and dispatches two settings
commands — set the key, switch the provider. Failures surface in a
transient context signal rendered by both Connect surfaces; the agent stays
NeedsKey. A `?code=` with no stashed verifier (foreign redirect) is scrubbed
and ignored.

### The Connect CTA lives where the user hits the wall

The Agent tab's needs-setup empty state leads with "Connect OpenRouter —
use your own account" whenever the agent is not ready, regardless of the
selected provider (success switches it). The settings popover shows
Connect/Disconnect in place of a pasted-key field for OpenRouter.

### Attribution headers

`OpenAiCompatConfig` gained `extra_headers`; the OpenRouter config sends
`HTTP-Referer: https://lightplayer.app` + `X-Title: LightPlayer Studio`
(OpenRouter's app-directory attribution). Other providers send none.

## Consequences

- Zero-setup onboarding exists: click, approve on openrouter.ai, done —
  billing stays between the user and OpenRouter; we never touch money or
  hold a server-side key.
- New availability dependency on openrouter.ai for connected users (only
  for them; other providers are unaffected).
- The key inherits the localStorage plaintext v1 posture of all agent keys,
  mitigated better than pasted keys: the user can revoke it in their
  OpenRouter dashboard at any time.
- The shared `model` override field still collides across providers when a
  user switches after overriding (pre-existing; the OpenRouter baked
  default covers the no-override path). Per-provider model fields remain
  future work.

## Alternatives Considered

- **First-party subscription OAuth** — closed to third parties by all three
  vendors (see Context); not available at any engineering cost.
- **Puter.js** (the only other user-pays connect flow) — proprietary SDK
  dialect proxied through their servers, opaque per-model pricing, and
  documented hidden-limit complaints; wrong trust profile for a default.
- **Hosted relay / metered proxy** — breaks static hosting and pure-browser
  OPFS mode, centralizes spend and abuse handling on us; rejected for the
  same reasons as the server-side harness in the shader-agent ADR.
- **Deep-link key paste** (send users to openrouter.ai/keys and paste) —
  strictly worse than PKCE on every axis; kept only as the generic
  Custom-provider escape hatch.

## Follow-ups

- Live OAuth walk is the merge gate (real account; verify localhost
  callback, credits billing, Disconnect/reconnect).
- Future: per-provider model fields; an OpenRouter model picker fed by
  `/models`; a credits-empty nudge when runs fail with payment errors.
