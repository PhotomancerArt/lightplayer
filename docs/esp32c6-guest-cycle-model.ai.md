# ESP32-C6 guest cycle estimate (emulator)

The RV32 emulator (`lp-riscv-emu`) can accumulate either raw retired-instruction counts or a **coarse cycle estimate** aimed at ESP32-C6–class cores (Andes N22–style in-order RV32IMAC), using fixed per-instruction-class costs plus branch-taken vs not-taken handling.

- **Reference:** [Counting CPU cycles on ESP32-C3 & ESP32-C6](https://ctrlsrc.io/posts/2023/counting-cpu-cycles-on-esp32c3-esp32c6/) (background on cycle semantics on these SoCs).

## Limitations

The model does **not** include I-cache effects, branch-predictor warm-up, variable `DIV`/`REM` latency, load-use hazards, or memory-system stalls. It is intended for **relative** comparisons in filetests (`vs fastest`), not wall-clock prediction.

GLSL filetests select the displayed metric with `--perf insts` (retired instructions, 1:1 with the `InstructionCount` model) or `--perf esp32c6` (default).
