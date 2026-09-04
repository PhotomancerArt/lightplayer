# ADR: The device terminal never folds a line

- **Status:** Accepted
- **Date:** 2026-09-04
- **Deciders:** Photomancer
- **Supersedes:** the long-line fold half of the device-card-v2 plan's
  terminal ruling (T1: "cap 200 / ×N repeats / long-line fold", archived at
  `~/.photomancer/planning/lp2025/_archive/2026-09-02-2310-device-card-v2/`).
  The cap and the repeat collapse stand.
- **Superseded by:** None
- **Related:** [2026-09-03-device-card-fixed-height-and-disconnect-disappears.md](2026-09-03-device-card-fixed-height-and-disconnect-disappears.md)
  (the height rule this decision has to respect)

## Context

The device card's terminal (`lp-app/lpa-studio-web/src/app/home/device_terminal.rs`)
shipped in P5 of the device-card-v2 pass with the spike's long-line fold:
any line over 160 characters rendered as its first 120 characters, an
ellipsis, and a "+N chars" control that expanded the row in place on click.
The fold was there to keep a multi-hundred-character block-plan dump from
"eating the panel".

On the 2026-09-04 bench the one line that mattered was the folded one. A
flash installed firmware but could not stamp the board manifest, and the
outcome the card showed was:

```
firmware installed; writing the board manifest failed (the board never became ready to write to: transport error: Transp… +77 chars
```

Yona's reading of the panel is select + Cmd+C, not clicking rows and not
the copy button. Against that habit the fold had two defects:

- A selection over a folded row copied the head, the ellipsis and the
  literal text "+77 chars" — the reason itself never reached the
  clipboard. Copy/paste is exactly what a verdict line exists for.
- Every long row carried a click handler. A drag-select that ended on a
  long row toggled it, re-rendering the row under the selection.

The panel's rows already wrap (`whitespace-pre-wrap` + `break-all` with a
hanging indent), and the panel is a fixed-height scroll box — the height
rule of the 2026-09-03 ADR is carried by the box, not by what the rows
clip. So the fold was not what kept the card still; it only decided how
much of a line a reader could see and copy.

## Decision

1. **No line is ever folded, clipped, ellipsised or line-clamped.** Every
   terminal row renders its whole text, wrapped under the hanging indent.
   The renderer has no per-line length bound and no per-row click handler.
   A long line costs scroll distance inside the fixed-height box and never
   card height. Guarded by `rows_wrap_whole_inside_a_scrolling_box` in
   `device_terminal.rs`.
2. **The clipboard tells the truth both ways.** Select + Cmd+C copies
   what is on screen, which is now the whole line; the `copy` button keeps
   copying the whole tail with `×N` repeat suffixes. Neither path can
   differ from what the board said.
3. **Verdict kinds (`Outcome`, `Failure`) are the lines people paste into
   bug reports**, and the model's summaries for them are bounded by
   construction (`ActivityOutcome::summary`, `wire_summary`). No
   per-kind rule is needed while nothing truncates anything.
4. **If a bound is ever needed it lives in the model, not the renderer.**
   The only input that could genuinely fill the box is a board printing a
   multi-kilobyte line with no newline (`LineSplitter` has no per-line
   cap). Should that happen, the answer is a bound on what `Evidence::fold`
   keeps — written into the line's text where a selection can see it, and
   never applied to `Outcome`/`Failure` — not a render-side fold that
   shows one thing and copies another.

## Consequences

- `device_terminal_processed` (the terminal-alone story) now ends on two
  long verdict lines wrapping whole in the pinned view: the bench's
  193-character outcome and a 234-character identification failure. Its
  baseline moves with this change.
- The 400-character block-plan dump in that story renders as roughly
  seven wrapped rows; the box scrolls past it like any other run of lines.
- A future "raw frame bytes" row (the module doc's dropped `raw` button)
  would be the first candidate for a model-side length bound under
  decision 4, since frame hex is the one line shape with no natural end.

## Alternatives considered

- **Raise the cutoff (160 → ~1000) and keep the fold.** Cheap, but keeps
  both defects for any line past the new cutoff, and keeps the row click
  handler that a drag-select trips over. Rejected: a limit that still
  lies to the clipboard is the same limit, further away.
- **Expand on click, copy button copies whole lines.** The original brief.
  Rejected at the bench before it was built: it optimises for the copy
  button and for clicking, and the reader does neither.
- **Never fold `Outcome`/`Failure`, keep folding the rest.** A per-kind
  rule in `lpa-studio-core`. Rejected as unnecessary while nothing folds;
  it is the shape decision 4 would take if a model-side bound is ever
  added.
- **Clip a row's visual height with CSS (`max-height` + `overflow:hidden`)
  so selection still copies the full DOM text.** Copies right, but reads
  wrong: a row that ends mid-word with no control looks like a rendering
  defect, and the hidden remainder can only be reached by copying it.
