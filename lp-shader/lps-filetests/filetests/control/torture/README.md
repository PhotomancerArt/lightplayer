# Control-flow torture corpus

Generated regression corpus for structured-control-flow lowering. Motivated by
the 2026-07-08 nested-if/else LPIR-interpreter bug: control-flow bug classes
are combinatorial, so this directory systematically enumerates the shapes
instead of keeping one hand-written example per category.

**GENERATED FILES — do not edit by hand.** Regenerate with:

```bash
python3 lp-shader/scripts/gen-control-torture.py --write
```

The generator is deterministic (pure enumeration, no randomness). Expected
values are computed by a reference integer interpreter inside the generator;
all programs are bounded and terminate. Verify all backends agree before
committing:

```bash
scripts/filetests.sh --target rv32n.q32,rv32c.q32,wasm.q32 control/torture/
```

## Trace encoding

Every test function threads an int accumulator through distinct "sites":
`t = t * 10 + k` with a per-site digit `k`. The returned value is therefore a
base-10 execution trace — a wrong branch, a skipped merge point, an extra loop
iteration, or a re-ordered side effect each produce a different value.
Short-circuit tests use a global `g_trace` mutated by a helper `chk(k, v)`,
so the value also proves which operands were (not) evaluated, in order.

## Known bug: eager `&&` / `||` evaluation

GLSL requires `&&` and `||` to short-circuit, but the current frontend
lowering evaluates both operands (`docs/design/lpir/02-core-ops.md` documents
the gap; `docs/design/lpir/08-glsl-mapping.md` says side-effecting cases must
lower to control flow). All three verified targets agree with each other on
the eager behavior, so this is a frontend-lowering conformance bug, not a
backend divergence. Expected values in this corpus are the GLSL-correct
short-circuit results; the generator evaluates every directive under both
semantics and marks exactly those whose value differs as
`@broken(rv32n.q32) @broken(rv32c.q32) @broken(wasm.q32)`. When the lowering
is fixed these will show up as unexpected passes; strip the annotations with
`scripts/filetests.sh --fix` and delete this generator feature.
Ternary conditions and arms already evaluate lazily (see
`terncond_sideeffect.glsl`, which passes unannotated).

## Enumeration axes

| Prefix        | Axis                                                              |
| ------------- | ----------------------------------------------------------------- |
| `ifnest_*`    | if/else nested in then vs else arms to depth 3: chain shapes (`d3_te` = child in Then arm at depth 1, Else arm at depth 2), full binary trees (`both`), else-less chains (`noelse`), else-if chains (`chain`) |
| `loopif_*`    | branches inside loops: {for, while, dowhile} x {if, if/else, else-if chain} |
| `ifloop_*`    | loops inside branches: {for, while, dowhile} in then / else / both arms |
| `mix_*`       | loop-in-branch-in-loop, mixed loop kinds                          |
| `brk_*`       | break at depth 1 (then/else guard) and depth 2 (inner loop only, nested guard) per loop kind |
| `cont_*`      | continue, same enumeration; while/do-while variants exercise the continue-to-condition edge |
| `brkcont_*`   | break and continue mixed across nesting levels / in one body      |
| `ret_*`       | early returns from nested ifs, from each loop kind, from inner loops of nested pairs, from loops inside branches |
| `sc_*`        | short-circuit `&&`/`||` whose right operand calls a global-mutating function: bare ops, precedence chains, nested groups, and as if/while/ternary conditions |
| `terncond_*`  | ternaries nested in branch conditions: if conditions, loop bounds, nested ternaries, side-effecting arms |

Each file holds one enumerated shape with `// run:` directives covering every
(reachable) combination of the branch-selecting parameters, so file names are
regular and machine-friendly — the metamorphic fuzzing harness
(compiler-robustness roadmap M4) seeds from this corpus.
