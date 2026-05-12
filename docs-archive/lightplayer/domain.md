# Lightplayer domain model

The vocabulary and domain model for the Lightplayer system.

Manifested in lp-core, lp-app, and lpfx.

# Overview

Lightplayer is a data flow system at its core. It has a data bus that various modules are
connected to.

Modules are either a Show or a Rig.

Shows consume data and generate video output. They are the core visual generation module.

Rigs are modules that define collections of hardware sources and sinks. Includes fixtures, layouts, and DMX outputs.

## Node & Artifact

| Term             | One-line                                                    | Example                                          |
| ---------------- | ----------------------------------------------------------- | ------------------------------------------------ |
| **Artifact**     | A collection of files that can be used to create a Node     | `fluid.vis/` directory with `Node.toml`          |
| **ArtifactSpec** | String specifying an Artifact that can be loaded at runtime | `./fluid.vis`, `lib:/std/rainbow.vis`            |
| **Node**         | Basic identifiable runtime object                           | the live fluid pattern instance, UID `7Kp2MqZ`   |
| **NodeSpec**     | String identifying a Node _at runtime_                      | `7Kp2MqZ` (UID) or `/main.show/fluid.vis` (path) |
| **NodePath**     | Runtime path identifying a Node `main.show/fluid.vis`       | `/dome.rig/main.layout/sector4.fixture`          |
| **Property**     | A typed value attached to a Node.                           | the `speed` parameter on a Pattern               |
| **PropPath**     | Property path (`config.spacing`). See LpsValuePath          | `speed`, `config.spacing`, `wiring[0].universe`  |
| **NodePropSpec** | String specifying a Property on a Node                      | `/main.show/fluid.vis#speed`                     |

Questions:

1. Multiple instances of the same Artifact? e.g. two perlin noise visuals in a timeline.
2. `Node.toml` vs `vis.toml`?
3. Allow single-file artifacts? e.g. `fluid.vis.toml`?

## Signal

| Term        | One-line                                                            | Example                                           |
| ----------- | ------------------------------------------------------------------- | ------------------------------------------------- |
| **Signal**  | Typed data flowing on the bus (Audio, Video, Texture, Float, etc.). | a frame of camera video; an audio sample buffer   |
| **Bus**     | Implicit typed channel space. Derived from modules connected to it. | the project's runtime channel set                 |
| **Channel** | Typed named address on the bus: `<type>/<dir>[/<n>]`.               | `audio/in`, `video/out`, `touch/in/1`             |
| **Module**  | Self-contained bus participant. Either a Show or a Rig.             | `main.show`, `dome.rig`                           |
| **Binding** | Connection from a Signal to a Parameter (modulation).               | `audio/in → main.show/fluid.vis#speed`            |
| **Source**  | Anything that produces a Signal.                                    | a microphone, an LFO, a touch surface             |
| **Sink**    | Anything that consumes a Signal.                                    | a speaker, an Art-Net DMX output, the debug scope |

Questions:

1. Are Source and Sink Entities, or Properties of a Module?
2. Strict `<type>/<dir>/<idx>` channel names, or allow user-named (`drawpad/0`)? Force `<type>/<name>[/idx]` style, but `<name>` and `<idx>` are configurable. Numbers are default, but we allow user-named indices like `video/in/webcam` or `video/app`.
3. One bus per project, or one per Module pair? One per project.
4. Binding transforms (scale, smoothing, curve) — inline on Binding, or via a separate Modulator Node?
5. Naming: `audio_in/0`, `audio/in/0`, or `audio/in` & `audio/in/1`? **Decided**: `<kind>/<dir>/<channel>[/<sub>...]` — always include the channel index, even for the default. `audio/in/0`, `audio/in/1`, `audio/in/0/bands`. Positional clarity (kind, direction, channel, sub-channel) and easy sub-channel addressing without retroactively shifting names. Convention only for now; may be codified later.

## Signal types

| Term           | One-line                                                      | Example                                  |
| -------------- | ------------------------------------------------------------- | ---------------------------------------- |
| **Video**      | Video data, 2d array of pixels, represented as a GPU texture. | live USB webcam feed, fluid sim output   |
| **Audio**      | Audio data, 1d array of samples.                              | line-in stage feed, microphone capture   |
| **AudioFFT**   | Audio data, 1d array of frequency bins.                       | 64-band spectrum from line-in            |
| **AudioLevel** | Audio data, 1d array of level meters.                         | bass / mid / high RMS levels             |
| **Beat**       | TBD, beat detection results.                                  | 120 BPM clock + downbeat flag            |
| **Touch**      | Touch data, 1d array of touch points.                         | iPad fingers, capacitive XY pad          |
| **Motion**     | TBD, IMU data, accelerometer, gyroscope, etc.                 | accelerometer in a sand-box installation |

Questions:

1. `Texture` vs `Video` — separate types, or aliases of one? One type, `Video`, I think.
2. Audio sample format (f32 / i16 / Q-fixed)? Good question. TBD later.
3. Touch: XY only, or include pressure / id / continuity? Yes all those things. Touch struct.
4. Is the FFT step a Transformer Node, or is `AudioFFT` materialized by some other mechanism? Figure out later.
5. Event type for button presses, etc.? TBD.

# Domain

## Visual

| Term              | One-line                                                             | Example                                                  |
| ----------------- | -------------------------------------------------------------------- | -------------------------------------------------------- |
| **Visual**        | Core unit of visual generation.                                      | any pattern, effect, mixer, transition, or show          |
| **Pattern**       | 0-arity Visual (source). Placable in a Show.                         | rainbow, perlin noise, fluid sim, fire2012               |
| **Effect**        | 1-arity Visual (transform). Texture in → texture out.                | kaleidoscope, palette remap, color shift, blur           |
| **Transition**    | 2-arity Visual with a `progress` parameter. Used by Shows to switch. | crossfade, wipe, dissolve                                |
| **Mixer**         | N-arity stateless Visual. Combines textures simultaneously.          | additive blend, picture-in-picture, mask compositor      |
| **Show**          | N-arity, stateful, time-aware Visual. Also a Module.                 | the top-level show selecting between fluid / perlin / VU |
| **Live Show**     | All candidates loaded; selection by self-reported priority.          | "fluid if audio, else perlin" running at burning man     |
| **Playlist Show** | Bounded loading; cue / sequence / schedule-driven selection.         | choreographed setlist for a 4-song DJ set                |
| **Timeline Show** | Ordered (visual, transition) sequence on wall-clock.                 | a fireworks finale at midnight on NYE                    |

Questions:

1. `.vis` for all kinds, or per-kind suffix (`.pattern`, `.effect`, `.show`)?
2. Is `Visual` itself an Artifact kind, or only the concrete kinds (Pattern/Effect/...)?
3. Show subtypes: separate kinds, or one Show with a `mode` field?
4. Can a Show be a candidate inside another Show?
5. Mixer + Transition: collapse into one (Mixer with optional `progress`)?

## Rig

| Term          | One-line                                                                | Example                                              |
| ------------- | ----------------------------------------------------------------------- | ---------------------------------------------------- |
| **Rig**       | Module containing hardware sources, sinks, layouts.                     | `dome.rig` (10 panels + USB audio + iPad touch)      |
| **Fixture**   | A lighting fixture, a collection of lamps that are controlled together. | one dome panel, a 144-LED strip, a single GPIO pixel |
| **Layout**    | 2D canvas with Fixtures. Texture in → lamp array out.                   | flattened-dome map sampled into 10 panel fixtures    |
| **DmxOutput** | Hardware delivery channel for lamp data (RMT, Art-Net, OPC, etc.).      | Pixlite-1 over Art-Net; RMT on GPIO 18               |

Questions:

1. `DmxOutput` name when transport isn't DMX (RMT, OPC, raw IP)?
2. Non-lamp sinks (audio out, video out) — in Rig, or somewhere else?
3. Layouts shared between Rigs, or rig-bound?
4. Power budgeting — per-Fixture, per-Rig, both?
5. TODO: refine dmx terms, add audio / video / etc. sources and sinks.

## UI

| Term                | One-line                                                             | Example                                                |
| ------------------- | -------------------------------------------------------------------- | ------------------------------------------------------ |
| **Control**         | UI element derived from a Parameter or bus channel.                  | knob for `main.show/fluid.vis#speed`                   |
| **Panel**           | Curated arrangement of Controls. (Concept; not yet a manifest type.) | "Live performance" panel: speed, color, intensity      |
| **Control Surface** | Rendering target for Controls and Panels (touchscreen, web, MIDI).   | the 7" attached touchscreen, a remote browser, BCR2000 |

Questions:

1. Are Controls Entities (with UID / NodePath)?
2. Panel: in Project, in a Module, or standalone Artifact?
3. Bus-tap debug Controls — auto-generated, or opt-in?
4. Control Surfaces — auto-discovered, or declared in a Rig?

## Top-level

| Term        | One-line                                                  | Example                                                  |
| ----------- | --------------------------------------------------------- | -------------------------------------------------------- |
| **Project** | Top-level deployment artifact. Lists modules + overrides. | `dome-2026.project` referencing `dome.rig` + `main.show` |

Questions:

1. One Project per device, or one Project deployed to many devices?
2. Channel bindings/overrides live in the Project, or in the Modules?
3. Modules referenced by ArtifactSpec, or inlined?

## Cross-cutting concepts

| Term           | One-line                                                        | Example                                           |
| -------------- | --------------------------------------------------------------- | ------------------------------------------------- |
| **Arity**      | Number of primary texture inputs a Visual takes (0 / 1 / N).    | Pattern=0, Effect=1, Transition=2, Mixer=N        |
| **Convention** | Default channel-binding rules. Most projects need no overrides. | `audio/in` auto-binds to the only mic source      |
| **Override**   | Per-device deviation from convention. Lives on the device.      | "this rig: audio/out → audio/in" loopback         |
| **Wiring**     | The concrete publisher↔consumer connection record.              | resolved binding `mic-1 → fluid.vis#speed`        |
| **Manifest**   | The `*.toml` file for any Node. Suffix indicates type.          | `fluid.vis/Node.toml`, `dome.rig/Node.toml`       |
| **Scope**      | Where a software source is declared (rig / show / visual).      | LFO declared in a Visual is local to that Visual  |
| **Recursion**  | Visuals can reference Visuals. Cycle-free, depth-bounded.       | a Show containing a Mixer containing two Patterns |
| **Validation** | All checks happen at compose time, never runtime.               | type-mismatch on a Binding fails project load     |

Questions:

1. `Wiring` vs `Binding` — what's the actual difference (resolved-vs-declared)?
2. Validation: warnings vs hard errors — both, or fail-fast only?
3. `Convention` — codified per-signal-type, or per-project?
