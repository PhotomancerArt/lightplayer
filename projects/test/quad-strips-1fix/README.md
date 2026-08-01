# Quad strips (1 fixture)

The single-fixture sibling of `../quad-strips`, kept as the **engine
frame-cost oracle**. Same shader, same fixture and mapping, same output
endpoint as channel 1 of the four-channel project — only the other three
fixture+output chains are gone.

Its reason to exist: frame cost on the desk ESP32-S3 was measured as flat
~8.4 ms *per fixture+output chain*, independent of render resolution and LED
count (see `docs/debt/s3-frame-cost-scales-per-fixture.md`). Profiling one
chain against four is how that per-chain cost is attributed, so both
workloads must stay comparable — **change one, change the other**.

## Profiling

```bash
lp-cli profile projects/test/quad-strips-1fix --collect cpu
```

Defaults to steady-render mode; the run writes `report.txt` (self/inclusive
cycles, stack high-water) under `profiles/<timestamp>--…/`. Run the
four-fixture project the same way for the scaling comparison.
