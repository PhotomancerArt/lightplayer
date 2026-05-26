# Legacy Mutation Stack — Deletion Inventory

**Status:** draft for M1 review  
**Execution:** M8 (after new edit path works)  
**M1 job:** confirm list complete; no deletion in M1.

## lpc-wire

| File / symbol | Role |
|---------------|------|
| `slot/mutation.rs` | `WireSlotMutationRequest`, `WireSlotMutationOp`, responses, rejections |
| `messages/project_read/project_read_request.rs` | `mutations: Vec<WireSlotMutationRequest>` |
| `messages/project_read/project_read_response.rs` | `mutations: Vec<WireSlotMutationResponse>` |

## lpc-view

| File / symbol | Role |
|---------------|------|
| `slot/mirror.rs` | `prepare_set_value`, `apply_mutation_response`, pending map |
| `slot/pending.rs` | `PendingSlotMutation` |

## lpc-engine

| File / symbol | Role |
|---------------|------|
| `engine/slot_mutation.rs` | `mutate_project_slots`, in-memory def mutation |
| `engine/mod.rs` | module export |

## lpa-server

| File / symbol | Role |
|---------------|------|
| `project_read_source.rs` | `apply_project_mutations` → engine |

## lpc-shared / server trait

| Symbol | Role |
|--------|------|
| `TransportServer` mutation hook | passes mutations to server impl |

## lp-cli (debug UI)

| File / symbol | Role |
|---------------|------|
| `debug_ui/ui.rs` | `queued_mutations`, `prepare_queued_mutations` |
| `debug_ui/slot_edit.rs` | `SlotEditIntent`, mutation status by id |

## Replacement (target)

| Old | New |
|-----|-----|
| `WireSlotMutationRequest` | model/wire `SyncOp` / `ArtifactEdit` |
| `prepare_set_value` | build `AssignValue` + apply sync op |
| `mutate_project_slots` | `registry.sync` + engine refresh on commit |
| immediate accept/reject | apply errors + commit outcome |

## Grep commands (M8 verification)

```bash
rg 'WireSlotMutation|prepare_set_value|mutate_project_slots|slot_mutation' lp-core lp-app lp-cli
```

Expect zero hits in production paths after M8.
