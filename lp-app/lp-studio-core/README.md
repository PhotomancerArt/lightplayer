# lp-studio-core

`lp-studio-core` owns the UI-independent LightPlayer Studio domain model.

It defines Studio state, documented actions, action metadata, effects, runtime
events, diagnostics, capabilities, and session records. UI code, runtime code,
tests, harnesses, and future agents should all speak through this vocabulary.

## Boundaries

- This crate owns state transitions and effect descriptions.
- This crate does not perform I/O, spawn runtimes, talk to browser workers, open
  serial ports, or render UI.
- `lp-studio-runtime` executes effects and emits events.
- `lp-studio-web` renders state and dispatches actions.
- `lpa-link` remains the lower-level link/session/connection layer below Studio
  capabilities.

Actions are documented program objects. Their descriptors provide labels,
summaries, categories, and history policy so generic UI help and future agents
can inspect the available action surface.

The hardware action surface separates:

- device access and browser permission requests;
- link/session operations such as connect, disconnect, reset, and flash;
- project operations such as uploading the built-in demo through `lp-server`;
- local navigation such as selecting a project node.

Operational hardware actions are not undoable. Future undo should attach to
successful project edit transactions, not to permission prompts, flashing,
resets, or connection lifecycle events.

M1 does not implement undo/redo. It only classifies action history behavior so
future undo can attach to successful project edit transactions instead of every
operational action.

## Validation

```bash
cargo check -p lp-studio-core
cargo test -p lp-studio-core
```
