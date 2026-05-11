# Slot Domain Cutover Todo

## Recently Ported To Real Code

- [x] `RenderProduct` as `LpValue`
- [x] Runtime state roots for shader and texture nodes
- [x] Engine-owned `SlotShapeRegistry`
- [x] Resolver reads produced values through `runtime_state_slots`
- [x] Removed `ProducedSlotAccess`
- [x] `NodeDefHandle`
- [x] Authored def store/provider via concrete artifact store
- [x] Resolver fallback from consumed slot to node def slot
- [x] Generated read-only slot views for root defs
- [x] Removed engine `RenderProductStore`
- [x] Moved runtime binding storage/indexes into `NodeTree`
- [x] Removed standalone `BindingRegistry` and global `BindingId`
- [x] Resolver reads runtime bindings through the active engine host
- [x] Runtime loaders register authored bindings directly onto the node tree

## Mockup Concepts Still To Port

- [ ] Generic slot-tree access against shape registry
- [ ] Source/default slot roots as fallback data across all node defs
- [ ] Server-side mutation model
- [ ] Client pending mutation behavior
- [ ] Full sync of slot data
- [ ] Diff patching of slot data
- [ ] Dynamic shader param shape changes
- [ ] Typed-ish access pressure around source/runtime/view

## Real Code Remaining

- [ ] Apply generated slot views beyond `TextureDef`
- [ ] Convert shader node config reads to resolver-backed slot views
- [ ] Convert fixture node config reads to resolver-backed slot views
- [ ] Convert output node config reads to resolver-backed slot views
- [ ] Decide and implement binding override precedence beyond exact target lookup
- [ ] Rebuild binding indexes incrementally if mutation frequency makes full rebuild too costly
- [ ] Make dynamic shader params first-class runtime slot roots
- [ ] Register and update dynamic shader param shapes
- [ ] Replace old node-specific wire sync with generic slot registry/data sync
- [ ] Replace old node-specific view model with generic slot mirror
- [ ] Rebuild generic debug UI over slot shapes and slot data
- [ ] Add watch/subscription model for slot paths
- [ ] Add resource watch/subscription toggle

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

## Wire/View Rebuild

- [ ] Shape registry snapshot message
- [ ] Shape registry diff message
- [ ] Slot data full snapshot message
- [ ] Slot data patch message
- [ ] Client-side shape registry mirror
- [ ] Client-side slot data mirror
- [ ] Map key add/remove pruning
- [ ] Enum variant switch pruning
- [ ] Option `Some`/`None` pruning
- [ ] Resource metadata sync
- [ ] Lazy resource payload sync

## Domain Modeling Cleanup

- [ ] Audit remaining legacy sync/resource naming
- [ ] Decide texture node near-term role
- [ ] Decide texture resource registry shape
- [ ] Decide control/DMX product shape for fixture to output flow
- [ ] Revisit buses as slot owners
- [ ] Revisit slot path versus value path boundaries
- [ ] Revisit `LpValue` / value-shape naming
- [ ] Revisit semantic slot leaves and editor hints

## Validation Evidence

- [ ] Server tree walk test in real code
- [ ] Client tree walk test in real code
- [ ] Full sync test in real code
- [ ] Incremental slot diff test in real code
- [ ] Dynamic shader param shape-change test in real code
- [ ] Client mutation accept/reject tests
- [ ] Generic UI smoke test

## User Notes

- [ ] TextureStore?
