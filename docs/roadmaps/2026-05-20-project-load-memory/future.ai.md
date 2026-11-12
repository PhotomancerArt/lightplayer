# Future Work

## Allocator And Fragmentation Study

If structural changes leave enough total free heap but allocation still fails,
study fragmentation, allocation size distribution, and whether project-load
temporaries should use short-lived arenas.

## Packed Project Images

A compact project image could be useful as a cache or sync optimization later,
but it should not replace source artifacts or on-device shader compilation.

## Multi-Project Residency

This roadmap optimizes one loaded project. Loading multiple projects at once
may need separate residency policies once the single-project footprint is under
control.
