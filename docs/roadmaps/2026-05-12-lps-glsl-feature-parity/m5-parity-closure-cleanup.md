# M5: Parity Closure, Diagnostics, and Cleanup

## Objective

Turn the implementation from a feature branch into something maintainable enough to keep beside or replace the Naga path.

## Features

- full `rv32lpn.q32` filetest triage
- negative test behavior for type errors and invalid syntax
- consistent diagnostic presentation with source line and span indicator
- resumability review across parser, semantic analysis, and lowering
- crate organization cleanup
- docs for supported language surface and known exclusions
- final firmware size and compile-time report update

## Implementation Notes

Do not leave support encoded as scattered `M3 does not support` strings. By this point unsupported behavior should be either implemented, intentionally diagnosed, or explicitly annotated in filetests.

Diagnostics do not need perfect recovery. Halt-on-first-error is acceptable if the message has:

- source location
- line text
- caret/span indicator
- concise cause

Resumability should be reviewed at natural phase boundaries:

- lexing/tokenization
- top-level indexing
- per-function parsing
- semantic/HIR construction
- LPIR lowering

If fine-grained yielding inside expressions or statements adds too much complexity, document that as future work rather than forcing it into parity closure.

## Filetest Gate

Run the broad target:

```bash
cargo run -p lps-filetests-app -- test --target rv32lpn.q32 --concise
```

Compare against existing native/Naga targets where useful:

```bash
cargo run -p lps-filetests-app -- test --target rv32n.q32 --concise
```

Final build checks:

```bash
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --no-default-features --features esp32c6,server-lps-glsl
cargo check -p lpa-server
cargo test -p lpa-server --no-run
cargo test -p fw-tests --test scene_render_emu --test profile_alloc_emu
```

## Done

- all supported success-path filetests pass on `rv32lpn.q32`
- intentional exclusions are documented
- diagnostics are usable enough for shader authors
- the no-Naga firmware build remains intact
- the experiment report has updated size/time numbers and a fair caveat about parity level
- the codebase is split into small, navigable files

