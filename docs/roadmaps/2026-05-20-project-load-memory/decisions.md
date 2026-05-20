# Decisions

## Preserve On-Device Compilation

Project-load memory work must not remove, stub, feature-gate, or host-offload
the GLSL compiler. The compiler and runtime execution path stay on ESP32-C6.

## Measure Load Separately From First Frame

Startup profiles are useful but too broad. Add a load-only event/mode before
using profile numbers to judge graph changes.

## Treat Authored Definitions As Cold Data

Full `NodeDef` values are source/debug material after load. The hot resident
runtime graph should use compact records and handles.

## Prefer Static Registry Data

Built-in slot shape metadata should not be copied into every project heap.
Keep dynamic registration as an overlay for genuinely dynamic shapes.

## Use Embedded-Shaped Indexes

Loaded project indexes should reflect embedded access patterns: dense ids,
frozen arrays, sorted slices, and interned strings where useful.

## Optimize Peak And Resident Memory Separately

Resident graph structures and temporary parse buffers are different problems.
Track both so wins in one area do not hide regressions in the other.
