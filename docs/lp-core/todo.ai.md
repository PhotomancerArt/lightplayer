# LP Core Todo

This is a rough guide to the slot/domain cutover. It is intentionally practical:
what is done, what is still fuzzy, and what should probably happen next.

## Recently Finished

- [x] Project artifacts load directly into `Engine`
- [x] Removed `CoreProjectRuntime` wrapper
- [x] Removed old `project_runtime` and `runtime` engine modules
- [x] Engine owns services, artifact-node lookup, output flushing, revision/frame state
- [x] `RenderProduct` renamed/split into `VisualProduct`
- [x] Added `ControlProduct` for fixture-to-output logical control flow
- [x] Products travel as `LpValue::Product`
- [x] Runtime state roots for shader, fixture, and texture nodes
- [x] Engine-owned `SlotShapeRegistry`
- [x] Resolver reads produced values through runtime state slot roots
- [x] Removed `ProducedSlotAccess`
- [x] Removed engine `RenderProductStore`
- [x] Product layout: `product/` common, `products/{visual,control}/` concrete
- [x] Resource layout: `resource/` common, `resources/buffer/` concrete
- [x] Removed `lpc-engine::output` re-export facade
- [x] Dataflow layout: `dataflow/{binding,bus,resolver}/`
- [x] Shader value/type conversions moved under `gfx/`
- [x] `NodeDefHandle`
- [x] Authored defs stored by artifact store
- [x] Resolver fallback from consumed slot to authored node-def slots
- [x] Generated read-only slot views for root defs
- [x] Runtime binding storage/indexes live in `NodeTree`
- [x] Removed standalone `BindingRegistry` and global `BindingId`
- [x] Runtime loaders register authored bindings onto the node tree
- [x] Shader node reads compile options through resolver-backed def slots
- [x] Fixture node reads scalar config through resolver-backed def slots
- [x] Literal bindings carry `LpValue` payloads through resolver productions
- [x] Compiled slot accessors can descend into option payloads via `.some`
- [x] Wrote `docs/lp-core` overview/concept docs

## Engine/Core Remaining

- [ ] Convert output node/services config reads to resolver-backed slot views or document why output services are special
- [ ] Generate typed accessors for option payloads instead of hand-authored `.some` accessors
- [ ] Make dynamic shader params first-class runtime slot roots
- [ ] Register and update dynamic shader param shapes
- [ ] Decide binding override precedence beyond exact target lookup
- [ ] Rebuild binding indexes incrementally if mutation frequency makes full rebuild too costly
- [ ] Revisit whether runtime `Bus` state still earns its keep now that bindings live on `NodeTree`
- [ ] Decide texture node near-term role
- [ ] Decide whether `RuntimeBuffer` is enough or whether we need a distinct texture resource/store
- [ ] Audit remaining old wire/view resource naming around runtime buffers

## Wire/View Rebuild

- [ ] Shape registry snapshot message
- [ ] Shape registry diff message
- [ ] Slot data full snapshot message
- [ ] Slot data patch message
- [ ] Client-side shape registry mirror
- [ ] Client-side slot data mirror
- [ ] Full sync of authored def slot roots
- [ ] Full sync of runtime state slot roots
- [ ] Incremental diff patching of slot data
- [ ] Map key add/remove pruning
- [ ] Enum variant switch pruning
- [ ] Option `Some`/`None` pruning
- [ ] Resource metadata sync
- [ ] Lazy resource payload sync
- [ ] Replace old node-specific wire sync shape
- [ ] Replace old node-specific view model shape
- [ ] Remove old detail/watch message model

## UI/Inspection

- [ ] Generic debug UI over slot shapes and slot data
- [ ] Watch/subscription model for slot paths
- [ ] Convention for default watched slots, probably `state`
- [ ] Resource watch/subscription toggle
- [ ] Skeleton resource UI from metadata before payload fetch
- [ ] Control layout/debug view for fixture/output ranges
- [ ] Visual product/debug view for materialized textures

## Mutation Work

- [ ] Define authored def mutation message shape
- [ ] Validate mutation target against slot shape
- [ ] Apply server-side mutations to authored defs
- [ ] Stamp mutations with current revision
- [ ] Diff mutated authored defs into slot patches
- [ ] Track client pending mutations
- [ ] Confirm accepted mutations from server sync
- [ ] Reject stale/conflicting mutations
- [ ] Surface mutation errors to client UI

## Validation Evidence

- [ ] Server tree walk test in real code
- [ ] Client tree walk test in real code
- [ ] Full sync test in real code
- [ ] Incremental slot diff test in real code
- [ ] Dynamic shader param shape-change test in real code
- [ ] Client mutation accept/reject tests
- [ ] Generic UI smoke test

## Domain Questions

- [ ] Do buses remain slot owners long term, or are they only resolver labels?
- [ ] Is `RuntimeBuffer` the final resource abstraction, or do texture/control buffers split?
- [ ] How much of output/device mapping belongs in output nodes versus fixture defs?
- [ ] What is the final `SlotPath` / `ValuePath` boundary for value-object editors?
- [ ] Do semantic value leaves need richer editor metadata before UI work?
- [ ] When do dynamic shader params become authored/mutable defs versus runtime materialization?
