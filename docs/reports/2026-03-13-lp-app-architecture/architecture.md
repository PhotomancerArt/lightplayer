# LightPlayer App Architecture

Date: 2026-03-13

## Goal

Build a user-facing app (web first, then desktop and mobile) for LightPlayer.
Short-term target: Chromium web app using WebSerial/WebUSB.
Long-term: web, mobile, desktop sharing maximum code via Leptos + Tauri.

## Framework Decision: Leptos

Evaluated: Dioxus, egui, Leptos, Makepad, Iced, React/TypeScript hybrid.

Chose Leptos (Rust web framework, DOM-based rendering, fine-grained reactivity) for:
- Strongest Rust web story; DOM-based means native text input, CSS, accessibility, browser API access
- Embed CodeMirror for shader editing without building a code editor
- WebSerial/WebSocket integration is natural with DOM access
- Desktop/mobile via Tauri v2 webview when needed (same approach Dioxus desktop uses, but with two mature tools instead of one pre-1.0)
- Fine-grained signals fit the data model (node status, FPS, logs, connection state)

Rejected alternatives:
- **Dioxus**: Pre-1.0, API churn risk. Similar webview-based desktop story. Less mature web than Leptos.
- **egui**: Already in use for debug UI. Canvas-rendered on web (no DOM, no native text input, no accessibility). Fine for dev tools, not for user-facing app.
- **React/TS hybrid**: Mature UI ecosystem but ongoing cross-language boundary cost (type systems, build systems, API glue). Not justified given the team's Rust focus.
- **Makepad**: Good cross-platform story, built-in code editor, but tiny community and bus-factor risk. Canvas-rendered like egui.

## GLSL in the Browser

WASM cannot do runtime native code generation, but it CAN dynamically
compile and instantiate new WASM modules via WebAssembly.instantiate().

The GLSL compiler is being split into:
- **lp-glsl-frontend**: shared parser + semantic analysis (no_std, WASM-compatible)
- **lp-glsl-cranelift**: Cranelift backend (native/rv32, not WASM-compatible)
- **lp-glsl-wasm**: WASM codegen backend (TypedShader → WASM bytes)

The WASM backend walks the same TypedShader AST but emits WASM bytecode
(via wasm-encoder) instead of CLIF. Builtins (lp-glsl-builtins) compile
to a separate .wasm module and are linked via WASM imports at
instantiation time.

This enables fully in-browser shader compilation and execution with no
server dependency. Same Q32 fixed-point math, same builtins.

See: `docs/roadmaps/2026-03-13-glsl-wasm-playground/`

## Device Simulation Constraint

lp-server depends on lp-engine which uses Cranelift JIT for the full
rendering pipeline. lp-server will not run in the browser.

Implications for simulated device in the app:
- **Short term**: Web app connects to `lp-cli serve` over WebSocket (companion process)
- **Hosted demo**: Server-side lp-server instances per session
- **Desktop (Tauri)**: lp-server runs natively as backend
- **Onboarding**: Pre-recorded demo mode for zero-install first impression

## WASM Compatibility of Existing Crates

| Crate | WASM ready | Notes |
|-------|-----------|-------|
| lp-model | Yes | no_std, pure data types and messages |
| lp-shared | Yes | no_std, traits (LpFs, OutputProvider), LpFsMemory |
| lp-engine-client | Yes | no_std, ClientProjectView |
| lp-client | No | Needs WASM transport impls, feature-gating tokio/serial |
| lp-server | No | Cranelift JIT — cannot target WASM |
| lp-engine | No | Cranelift JIT — cannot target WASM |

## Crate Structure

```
lp-app/
├── lp-app-core/          # Shared app logic, compiles to WASM
│   └── src/
│       ├── lib.rs
│       ├── device.rs     # DeviceConnection trait + state machine
│       ├── session.rs    # App session (device + project + view)
│       ├── project.rs    # Project management (memory FS, sync)
│       └── transport/
│           ├── mod.rs
│           ├── ws.rs     # WebSocket via web-sys
│           └── serial.rs # WebSerial via web-sys
│
├── lp-app-web/           # Leptos web app
│   ├── Cargo.toml
│   ├── Trunk.toml
│   ├── index.html
│   ├── style/
│   └── src/
│       ├── main.rs
│       ├── app.rs        # Root component, router
│       ├── pages/
│       │   ├── home.rs          # Device selection
│       │   ├── device_setup.rs  # Device-specific config
│       │   └── workspace.rs     # Main workspace
│       └── components/
│           ├── node_list.rs     # Node list (V0), canvas later
│           ├── code_editor.rs   # CodeMirror wrapper
│           ├── file_tree.rs     # Project file browser
│           ├── device_status.rs # Heartbeat, FPS
│           └── log_viewer.rs    # Log stream
```

**lp-app-core** depends on lp-model, lp-shared, lp-engine-client. Pure Rust, no UI framework, no platform APIs. Reusable by Tauri desktop or native mobile.

**lp-app-web** depends on lp-app-core and leptos. Only crate that knows about DOM/HTML/CSS.

Both excluded from default-members (WASM targets, built via Trunk).

## Workspace Integration

New workspace dependencies:
```toml
leptos = { version = "0.7", features = ["csr"] }
web-sys = { version = "0.3", features = [...] }
wasm-bindgen = "0.2"
wasm-bindgen-futures = "0.4"
```

Build via Trunk:
```
just app-dev    # trunk serve (hot-reload dev server)
just app-build  # trunk build --release
just app-sim    # lp-cli serve + trunk serve (simulated device)
```

## Transport Architecture

Two WASM-compatible transports implementing a common async trait:

- **WebSocket** (ws.rs): web-sys::WebSocket, connects to lp-cli serve or remote server. Same message protocol as existing WebSocketClientTransport.
- **WebSerial** (serial.rs): Web Serial API via web-sys, connects to ESP32 over USB. Same JSON line protocol as existing AsyncSerialClientTransport.

## UI Architecture

Three-pane layout (desktop), tabbed (mobile):

| File Tree | Node View | Device View |

Entry flow:
1. Device selection: Simulate (connect to lp-cli serve) or Connect (WebSerial to ESP32)
2. Device-specific setup (project template, LED config, or flash firmware)
3. Main workspace with project view

Code editor: Embed CodeMirror 6 (~30KB gzipped, GLSL syntax highlighting). Small wasm-bindgen glue layer for two-way binding.

## Phased Implementation

| Phase | Scope | Validates |
|-------|-------|-----------|
| 0: Skeleton | Leptos hello-world in workspace; confirm lp-model/lp-shared/lp-engine-client compile to wasm32 | Build pipeline |
| 1: WebSocket client | web-sys WebSocket transport; display heartbeat, logs | End-to-end protocol |
| 2: Project view | ClientProjectView, node list, shader source display | Read path |
| 3: Editing | LpFsMemory in browser, CodeMirror editor, push changes to device | Write path |
| 4: WebSerial | WebSerial transport, connect to real ESP32 | Hardware path |
| 5: Device mgmt | Firmware flash (esptool.js), reset, connection UX | Full device lifecycle |

## Cross-Platform Path

- **Desktop**: Add lp-app-desktop/ using Tauri v2. Rust backend runs lp-server natively. Leptos frontend in webview. lp-app-core shared.
- **Mobile**: Tauri v2 iOS/Android (webview). BLE transport for ESP32 instead of WebSerial — new transport in lp-app-core.
