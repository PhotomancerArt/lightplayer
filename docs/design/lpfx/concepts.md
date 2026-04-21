# lpfx concepts

The vocabulary for the lpfx architecture. One-line definitions only —
detail lives in `02-concept-notes.md`.

## Entity & Artifact

| Term               | One-line                                                    | Example                                          |
| ------------------ | ----------------------------------------------------------- | ------------------------------------------------ |
| **Artifact**       | A collection of files that can be used to create an Entity  | `fluid.vis/` directory with `entity.toml`        |
| **ArtifactSpec**   | String specifying an Artifact that can be loaded at runtime | `./fluid.vis`, `lib:/std/rainbow.vis`            |
| **Entity**         | Basic identifiable runtime object                           | the live fluid pattern instance, UID `7Kp2MqZ`   |
| **EntitySpec**     | String identifying an Entity _at runtime_                   | `7Kp2MqZ` (UID) or `/main.show/fluid.vis` (path) |
| **EntityPath**     | Runtime path identifying an entity `main.show/fluid.vis`    | `/dome.rig/main.layout/sector4.fixture`          |
| **Property**       | A typed value attached to an Entity.                        | the `speed` parameter on a Pattern               |
| **PropPath**       | Property path (`config.spacing`). See LpsValuePath          | `speed`, `config.spacing`, `wiring[0].universe`  |
| **EntityPropSpec** | String specifying a Property on an Entity                   | `/main.show/fluid.vis#speed`                     |

Questions:

1. Multiple instances of the same Artifact? e.g. two perlin noise visuals in a timeline.
2. `entity.toml` vs `vis.toml`?
3. Allow single-file artifacts? e.g. `fluid.vis.toml`?

## Signal

| Term        | One-line                                                            | Example                                           |
| ----------- | ------------------------------------------------------------------- | ------------------------------------------------- |
| **Signal**  | Typed data flowing on the bus (Audio, Video, Texture, Float, etc.). | a frame of camera video; an audio sample buffer   |
| **Bus**     | Implicit typed channel space. Derived from modules connected to it. | the project's runtime channel set                 |
| **Channel** | Typed named address on the bus: `<type>_<dir>/<idx>`.               | `audio_in/0`, `video_out/0`, `touch_in/1`         |
| **Module**  | Self-contained bus participant. Either a Show or a Rig.             | `main.show`, `dome.rig`                           |
| **Binding** | Connection from a Signal to a Parameter (modulation).               | `audio_in/0 → main.show/fluid.vis#speed`          |
| **Source**  | Anything that produces a Signal.                                    | a microphone, an LFO, a touch surface             |
| **Sink**    | Anything that consumes a Signal.                                    | a speaker, an Art-Net DMX output, the debug scope |

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

## Rig

| Term          | One-line                                                                | Example                                              |
| ------------- | ----------------------------------------------------------------------- | ---------------------------------------------------- |
| **Rig**       | Module containing hardware sources, sinks, layouts.                     | `dome.rig` (10 panels + USB audio + iPad touch)      |
| **Fixture**   | A lighting fixture, a collection of lamps that are controlled together. | one dome panel, a 144-LED strip, a single GPIO pixel |
| **Layout**    | 2D canvas with Fixtures. Texture in → lamp array out.                   | flattened-dome map sampled into 10 panel fixtures    |
| **DmxOutput** | Hardware delivery channel for lamp data (RMT, Art-Net, OPC, etc.).      | Pixlite-1 over Art-Net; RMT on GPIO 18               |

TODO: refine dmx terms, add audio, video, etc. sources and sinks.

## UI

| Term                | One-line                                                             | Example                                                |
| ------------------- | -------------------------------------------------------------------- | ------------------------------------------------------ |
| **Control**         | UI element derived from a Parameter or bus channel.                  | knob for `main.show/fluid.vis#speed`                   |
| **Panel**           | Curated arrangement of Controls. (Concept; not yet a manifest type.) | "Live performance" panel: speed, color, intensity      |
| **Control Surface** | Rendering target for Controls and Panels (touchscreen, web, MIDI).   | the 7" attached touchscreen, a remote browser, BCR2000 |

## Top-level

| Term        | One-line                                                  | Example                                                  |
| ----------- | --------------------------------------------------------- | -------------------------------------------------------- |
| **Project** | Top-level deployment artifact. Lists modules + overrides. | `dome-2026.project` referencing `dome.rig` + `main.show` |

## Cross-cutting concepts

| Term           | One-line                                                        | Example                                           |
| -------------- | --------------------------------------------------------------- | ------------------------------------------------- |
| **Arity**      | Number of primary texture inputs a Visual takes (0 / 1 / N).    | Pattern=0, Effect=1, Transition=2, Mixer=N        |
| **Convention** | Default channel-binding rules. Most projects need no overrides. | `audio_in/0` auto-binds to the only mic source    |
| **Override**   | Per-device deviation from convention. Lives on the device.      | "this rig: audio_out → audio_in" loopback         |
| **Wiring**     | The concrete publisher↔consumer connection record.              | resolved binding `mic-1 → fluid.vis#speed`        |
| **Manifest**   | The `*.toml` file for any Entity. Suffix indicates type.        | `fluid.vis/entity.toml`, `dome.rig/entity.toml`   |
| **Scope**      | Where a software source is declared (rig / show / visual).      | LFO declared in a Visual is local to that Visual  |
| **Recursion**  | Visuals can reference Visuals. Cycle-free, depth-bounded.       | a Show containing a Mixer containing two Patterns |
| **Validation** | All checks happen at compose time, never runtime.               | type-mismatch on a Binding fails project load     |
