# lpa-client

`lpa-client` owns the typed client protocol for talking to a running
LightPlayer `lp-server`.

The crate is split into two layers:

- `LpClient<Io>` is the portable protocol client. It owns request ids,
  response correlation, server errors, heartbeats/log events, and typed
  filesystem/project/overlay operations. It depends on a small `ClientIo` trait
  and does not require Tokio or `Send`.
- Host adapters provide the current native ergonomics: cloneable shared
  transports, Tokio timeouts, serial/websocket/local transports, and CLI-style
  heartbeat/log rendering.

This keeps Studio web, CLI, host runtimes, and future agents on one protocol
model while allowing each runtime to bind its own I/O.

Project reads are streaming operations: one `ProjectReadRequest` can produce
several same-id server messages carrying `ProjectReadEvent` batches, completed
by the envelope's `fin` flag. The public client API returns the flattened
`Vec<ProjectReadEvent>`; callers apply them to a `ProjectView` via
`lpc-view`'s `ProjectReadApplier` rather than reconstructing an aggregate
response.

## Pull Loop (`pull_loop`)

Driving one streamed project read — send, receive frames until `fin`, feed each
to `ProjectReadStream`, and stop on completion, timeout, or cancellation — is
this crate's single responsibility, owned by `pull_loop::run_project_read`.
Both `LpClient::project_read` and `TokioLpClient`'s native read call into it, so
the send/receive/collect state machine exists exactly once.

The pull loop is the **single timeout owner** for a read. Its contract is three
runtime-neutral pieces:

- **`ProgressDeadline`** — a *quiet-gap* deadline, not a total-duration one. It
  is reset on every received frame and fires only when no frame arrives within
  its `budget`, so a slow but progressing multi-frame stream never trips it. It
  carries a caller-supplied **timer factory** (`FnMut(Duration) -> impl Future`)
  instead of a concrete timer, keeping the module free of Tokio and `web-sys`:
  native callers back it with `tokio::time::sleep`, wasm callers with a
  `setTimeout` future. The loop races `io.receive()` against that timer with a
  hand-rolled poll (no executor `select!`), so it compiles and runs on both
  native and `wasm32-unknown-unknown` under `ClientIo`'s `?Send` contract.
- **`CancelSignal`** — explicit, not drop-based. The loop observes it between
  receives and returns `PullOutcome::Cancelled` at a frame boundary, leaving the
  transport consistent (the receive adapters discard any stale frames on the
  next request) rather than abandoning a half-consumed stream mid-`receive`. A
  bare `Fn() -> bool` and the `NeverCancel` marker both implement it.
- **`BackoffPolicy`** — exponential-with-cap failure backoff, reset on success.
  The loop itself never sleeps; this is the retry-cadence *policy* a caller
  applies between reads based on the `PullOutcome`. The type lives here so the
  whole timing contract of a read is one place (the old flat 3s passive-refresh
  backoff becomes `BackoffPolicy::new(base, max)`).

`run_project_read` returns a `PullOutcome`: `Completed { events, observed }`
(the ordered read events plus the unsolicited `ClientEvent`s seen en route,
preserving the buffering the open-coded loops had), `Cancelled`, `TimedOut`, or
`Failed(ClientError)`. The two existing clients apply no deadline of their own —
`LpClient` has no runtime and `TokioLpClient` keeps its outer `tokio::time::timeout`
as the native timeout owner — so both pass a never-firing deadline into the
shared loop today; the actor layer (M7/P3) is where a real `ProgressDeadline`,
`CancelSignal`, and `BackoffPolicy` get wired in.

## Feature Model

| Feature | Purpose |
|---|---|
| `default` | Enables `host` for existing native callers. |
| `host` | Tokio/shared transport adapter, local in-memory transport, host specifier parsing, logging, and `TokioLpClient`. |
| `serial` | Host serial transport for ESP32/emulator-style JSON-lines links. Implies `host`. |
| `emu` | Emulator serial transport support. Implies `host`. |
| `ws` | Host websocket transport. Implies `host`. |

Portable/browser-oriented consumers should depend on the core without defaults:

```toml
lpa-client = { path = "../lpa-client", default-features = false }
```

The core compile check is:

```bash
cargo check -p lpa-client --target wasm32-unknown-unknown --no-default-features
```

## Important Types

- `ClientIo`: runtime-neutral send/receive/close trait for `lpc-wire` messages.
- `LpClient<Io>`: typed protocol client over any `ClientIo`.
- `ClientOutcome<T>`: operation result plus protocol events observed while
  waiting for the correlated response.
- `ClientEvent`: heartbeat/log/uncorrelated-response events surfaced to the
  caller.
- `ProjectDeployFile`: one project file for shared stop/write/load deploy
  helpers.
- `TokioLpClient`: host wrapper that preserves the CLI/native shared-client API.
- `ClientTransport`: host-only Tokio transport trait used by native providers.
- `run_project_read` / `PullOutcome`: the shared streamed-read driver and its
  result (`Completed`/`Cancelled`/`TimedOut`/`Failed`).
- `ProgressDeadline`: quiet-gap deadline built from a caller-supplied timer
  factory (runtime-neutral).
- `CancelSignal` / `NeverCancel`: explicit between-frame cancellation.
- `BackoffPolicy`: exponential-with-cap retry cadence applied by callers.

## Project Deploy Semantics

Server-protocol project deploys should use this crate rather than open-coding
request sequences. The shared deploy flow is currently:

1. `StopAllProjects`
2. write files under `/projects/{project_id}/...`
3. `LoadProject { path: "projects/{project_id}" }`

That ordering avoids the ESP32 trying to run multiple loaded projects during a
replace-in-place upload. Direct bootloader/raw filesystem image access is not a
server-protocol deploy; it belongs below this layer in `lpa-link` management.

Use `deploy_project_files` for initial upload/load flows such as CLI upload,
CLI dev startup, firmware demo checks, and browser hardware demo loading. Use
`push_project_files` only when the caller intentionally wants write-only sync,
such as an already-loaded file-watch update.

## Relationship To lpa-link

`lpa-link` owns device/runtime discovery, endpoint status, raw logs,
diagnostics, reset, flashing, and raw filesystem access. When a link is
connected to a running `lp-server`, it exposes a server connection that callers
can wrap with this crate.

Keep server protocol semantics here. Keep low-level device management in
`lpa-link`.

## Validation

```bash
cargo check -p lpa-client
cargo test -p lpa-client
cargo check -p lpa-client --target wasm32-unknown-unknown --no-default-features
```
