# Future Work

## WGSL Frontend

A WGSL frontend should be evaluated after GLSL parity is materially complete. The desired reuse point is semantic HIR and LPIR lowering, not the GLSL parser.

## More Precise Resumability

The initial parity pass should preserve resumability at phase boundaries. Finer-grained yielding inside complex statements or expression parsing can be added later if hardware measurements show it matters.

## Richer Diagnostic Recovery

Multi-error recovery and suggestions can wait. The near-term value is source spans, line indicators, and clear halt-on-first messages.

## Post-Parity Size Work

Once feature parity is close, repeat the firmware size audit. At that point the comparison against the Naga path is fair enough to guide dependency and code-size cleanup.

## Optional Preprocessor Subset

If real projects start needing simple `#define` or `#include` behavior, add a deliberately small preprocessor subset. Keep it out of the core parity path unless product usage demands it.

