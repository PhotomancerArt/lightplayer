# ADR: Control Product Preview Probes

## Status

Accepted

## Context

Studio needs to render fixture control products in the node UI. A fixture
control product is not a visual texture; it is the native device-control sample
buffer that an output node or physical hardware consumes. For a fixture, the
same buffer can also be shown to a user as lamps in a physical-ish layout.

There are two related but distinct questions:

- What do the native control samples mean?
- Where should logical lamps be drawn in Studio?

The existing engine `ControlLayout` answered the first question with spans such
as RGB pixels and color order. Fixture mapping code answered the second question
internally through mapping points, but that geometry was not exposed to Studio.

Studio also should not receive large mapping geometry every refresh when only
the sample values changed.

## Decision

Control-product previews use request-scoped project probes. The normal output
path stays focused on native control rendering and does not carry UI-only
display geometry unless a probe asks for it.

The durable vocabulary lives in `lpc-model`:

- `ControlSampleLayout` describes what native samples mean.
- `ControlSampleSpan` describes a contiguous native sample range.
- `ControlSampleEncoding` describes the interpretation of a range, initially
  RGB pixels with the existing fixture `ColorOrder`, or raw samples.
- `ControlDisplayLayout` describes optional human-facing geometry.
- `ControlLayout2d` and `ControlLamp2d` describe the first supported display
  layout: normalized 2D lamp positions and radii.

The wire protocol adds a `ControlProductProbeRequest` and
`ControlProductProbeResult`. Probe responses return native sample bytes, sample
layout metadata, and optional display layout metadata. For the first slice the
native wire sample format is little-endian `u16`.

Studio decodes generic RGB sample layout in the web UI instead of asking the
runtime to render a separate RGB8 preview. This keeps the UI preview grounded in
the same sample data that would go to hardware or a debugging tool.

Display layout is separately revisioned. The revision represents the displayed
geometry, not merely fixture mapping config. It must change when any
layout-affecting input changes, such as mapping config, render size/aspect, or
radius calculations. Studio sends its known display-layout revision and the
server can respond with `Unchanged` while still returning new native sample
bytes.

Control display layout is an optional product capability. The engine asks the
control node through the control-product path; it does not downcast to fixture
internals. Fixtures implement the first `Layout2d` display capability.

## Consequences

Studio can show live fixture-like control previews while preserving native
control sample inspection.

The frontend needs generic decoders for supported control encodings. Initially
that means `U16` RGB pixel spans with the existing fixture `ColorOrder`.

Large fixture geometry is not resent every refresh once Studio has the current
display-layout revision.

The model vocabulary can support future control layouts and encodings without
turning fixture-specific mapping logic into frontend code.

## Alternatives Considered

- Return display-ready RGB8 lamp colors from the probe.
  Rejected because Studio would no longer be inspecting the exact native data
  sent to output hardware.
- Put control preview DTOs only in `lpc-wire`.
  Rejected because sample layout and display layout are core control product
  vocabulary, not merely transport shapes.
- Add fixture-specific probe data.
  Rejected because the Studio UI should consume generic product display
  metadata and avoid recomputing or understanding fixture mapping internals.
- Attach display layout to every normal control render.
  Rejected because output nodes do not need UI geometry and should not pay for
  it unless explicitly requested.

## Follow-ups

- Add RGBW, white-only, CCT, or other control encodings when the engine model
  supports them.
- Add 1D, 3D, SVG-shape, or mesh display layouts.
- Generate control product stories from real mini-project data instead of
  hand-built DTO fixtures.
- Extend non-fixture control products with display layouts where useful.
