# M3 / M4 Sequencing Options

**Status:** open — pick in M1 review

## Option A — Server staging first (original proposal)

```text
M2 wire → M3 server registry (apply/commit to fs, engine unchanged)
       → M4 engine loader cutover
       → M5 SyncResult policy
```

**Pros:** Prove wire + registry on real server without engine risk.  
**Cons:** UI edits commit to disk but running scene stale until M4; awkward demo window.

## Option B — Combined server + loader cutover

```text
M2 wire → M4+M3 single milestone (registry on server + engine loads from registry)
       → M5 SyncResult policy
```

**Pros:** No stale-engine period; matches "not scared of cutover."  
**Cons:** Larger bang; harder to bisect failures.

## Option C — Engine harness first, server second

```text
M2 wire (types only) → M4 engine cutover on host tests / examples
                      → M3 server wire-up
                      → M5 policy
```

**Pros:** Validates loader on CI before server lifecycle work.  
**Cons:** Client still can't edit via server until M3.

## Option D — Wire + engine cutover; server follows

```text
M2 wire → M4 engine (no lpa-server edit path yet)
       → M3 server
       → M5
```

**Pros:** Engine is the hard part; server is thin wiring.  
**Cons:** No E2E client path until M3.

## Questions to answer

1. Is a temporary stale-engine window (A) acceptable at all?
2. Do we need client E2E before or after engine loader lands?
3. Can `lpa-server` integration tests use registry without full engine cutover?

## Recommendation for discussion

Lean **B or D** if cutover risk is low: avoid building two production mutation
paths. Use **A** only if we want a server-only integration test milestone with
clear pass/fail before touching `ProjectLoader`.

**User note (2026-05-21):** not 100% sure — keep open until M1 review.
