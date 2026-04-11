# Design: Better Status Change Tracking

## Overview

Improve status change tracking in lp-server and client to ensure status is always visible for all nodes, even those not being watched for detail. This involves:

1. Adding `status_ver` tracking to `NodeEntry` in the runtime
2. Removing status from `NodeDetail` (status will only come via `node_changes`)
3. Always including status changes in `GetChanges` responses for all nodes
4. Updating client to track and log all status updates

## File Structure

```
lp-app/crates/lp-engine/src/project/
└── runtime.rs                    # UPDATE: Add status_ver to NodeEntry, update status changes, remove status from NodeDetail

lp-app/crates/lp-model/src/project/
└── api.rs                        # UPDATE: Remove status field from NodeDetail, remove status from SerializableNodeDetail

lp-app/crates/lp-engine-client/src/project/
└── view.rs                       # UPDATE: Track status_ver, log all status updates, handle status from node_changes only
```

## Type Changes

### NodeEntry - UPDATE: Add status_ver field
```
NodeEntry
├── status_ver: FrameId           # NEW: Last frame when status changed
└── (existing fields unchanged)
```

### NodeDetail - UPDATE: Remove status field
```
NodeDetail
├── path: LpPathBuf               # (unchanged)
├── config: Box<dyn NodeConfig>   # (unchanged)
├── state: NodeState              # (unchanged)
└── status: NodeStatus            # REMOVE: Status now only comes via node_changes
```

### SerializableNodeDetail - UPDATE: Remove status field
```
SerializableNodeDetail
├── Texture { ... }               # UPDATE: Remove status field
├── Shader { ... }                # UPDATE: Remove status field
├── Output { ... }                # UPDATE: Remove status field
└── Fixture { ... }               # UPDATE: Remove status field
```

### ClientNodeEntry - UPDATE: Add status_ver field
```
ClientNodeEntry
├── status_ver: FrameId            # NEW: Track status version for logging
└── (existing fields unchanged)
```

## Function Changes

### ProjectRuntime::get_changes() - UPDATE: Include status changes for all nodes
```
get_changes()
├── Check status_ver > since_frame for all nodes  # NEW: Always check status changes
├── Add StatusChanged to node_changes if changed  # NEW: Include for all nodes
└── Remove status from NodeDetail construction      # UPDATE: No longer include status
```

### ProjectRuntime - UPDATE: Update status_ver when status changes
```
All places that set entry.status:
├── Set status_ver = self.frame_id                 # NEW: Track when status changes
└── (existing status updates unchanged)
```

### ClientProjectView::apply_changes() - UPDATE: Handle status from node_changes only
```
apply_changes()
├── Process StatusChanged from node_changes        # UPDATE: Status only comes from node_changes
├── Track status_ver in ClientNodeEntry            # NEW: Track version for logging
├── Log all status updates                         # NEW: Log when status_ver changes
└── Remove status handling from node_details       # UPDATE: No status in node_details
```

## Implementation Details

### Status Version Tracking

- `status_ver` is set to `self.frame_id` whenever `entry.status` changes
- This includes changes to error messages (e.g., `Error("msg1")` -> `Error("msg2")`)
- Initial status (`Created`) sets `status_ver` to the creation frame
- Status changes are detected by comparing `status_ver > since_frame` in `get_changes()`

### Status in GetChanges

- Status changes are always included in `node_changes` as `StatusChanged` events for all nodes
- Status is no longer included in `node_details` (removed from `NodeDetail`)
- Clients receive status updates via `node_changes` regardless of detail tracking

### Client Status Logging

- Client tracks `status_ver` in `ClientNodeEntry` to detect status changes
- When `status_ver` changes, log the status update
- This allows tracking all status changes, not just Ok <-> Error transitions

## Migration Notes

- Existing code that reads `detail.status` will need to track status separately
- `SerializableNodeDetail` needs to be updated to remove status field
- Client code that relies on status in `node_details` needs to use `node_changes` instead
