# glsl-compile-working-set

How does the GLSL compiler's peak heap scale with shader size?

```bash
cd spikes/glsl-compile-working-set && cargo run --release
```

Written to re-check the flash-budget ADR's claim that shaders of 17–50 KB are
already unreachable because "a 4 KB shader needs ~65 KB of compile working set"
— which PR #284 put in doubt by shrinking `ChunkedVec`'s chunk allocations.

A counting `#[global_allocator]` measures peak live bytes across a real
`lps_glsl::compile`, for `examples/basic/shader.glsl` and a synthetic size sweep.

## Results (2026-08-02)

| GLSL | peak heap | largest single allocation |
|---|---|---|
| 4,092 B (`examples/basic`) | 156,972 B | **24,576 B** |
| 17,714 B (synthetic) | 1,680,167 B | **196,608 B** |
| 35,634 B (synthetic) | 3,353,511 B | 393,216 B |

Peak scales **linearly**, ~94 B per byte of GLSL on the expression-dense
synthetic sweep and ~38 B/B on the real shader (which is more
function-declaration-heavy).

**The ADR's claim holds, with a large margin.** At 17 KB the *single largest
allocation alone* is 196,608 B, which exceeds even the two-region 178,176 B heap
of #288 — before counting anything else. The compile working set, not the JIT
code region, remains the binding constraint at that size.

## The largest allocation is the lexer, not the HIR arena

Measuring `lps_glsl::lex` on its own returns the same 24,576 B, so the token
vector owns it — a plain doubling `Vec<Token>` with no chunking. `Token` is
**12 bytes on both** `riscv32imac` and the 64-bit host, so this figure transfers
to the device unchanged: `examples/basic` asks the classic's allocator for a
single 24,576 B block, 22 % of its 112,640 B arena and **8× the 3,072 B request
that OOM'd** in `docs/defects/2026-08-02-classic-oom-retry-succeeds.md`.

⚠️ Absolute *peak* figures are 64-bit and over-estimate the device, since
pointer-bearing types are smaller there. The scaling slope and the token-vector
numbers are the transferable parts.

Not a workspace member: a measurement harness, not shipped code.
