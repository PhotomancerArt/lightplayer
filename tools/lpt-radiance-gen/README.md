# lpt-radiance-gen

Writes the **radiance dome** test project, `projects/test/radiance-dome/`,
in the product's own formats: one fixture whose mapping (map2d format 6,
written by `lpc-mapping`'s writer) holds its layout and its patching, a
shader, a clock and ten outputs.

The Graph needs a project at the radiance dome's scale to test against:
275 panels, 22,000 lamps, ten control boxes. The geometry is lp2014's: a
frequency-5 geodesic dome, the top 8 face rings lit, drawn as a plan view.
The plan is 5-fold symmetric, so the mapping is 55 panel shapes, each
repeated five times.

**The lamp counts, the port counts and the patching are placeholders**:
80 lamps a panel, 10 outputs of 32 ports, one panel a port. Nobody knows
yet how many LEDs each real panel carries or which panel is plugged into
which box. The project's own README says what is unknown.

```bash
cargo run -p lpt-radiance-gen                # rewrites projects/test/radiance-dome
cargo run -p lpt-radiance-gen -- <out-dir>   # or writes somewhere else
```

The same source always writes the same bytes. `lpa-graph-files` bakes the
project in for the Graph's stories and tests.
