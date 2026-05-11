# Plan: Better Status Change Tracking

## Overview

Improve status change tracking in lp-server and client to ensure status is always visible for all nodes, even those not being watched for detail.

## Goals

1. Track the last frame_id that status changed (including error message changes)
2. Always send new status for nodes with GetChanges, even for nodes where detail isn't watched
3. Enable clients to always see the status of nodes, even those they aren't watching

## Current State

### Runtime (`ProjectRuntime`)

- `NodeEntry` tracks `config_ver` and `state_ver` (FrameId when config/state last changed)
- `NodeEntry` does NOT track `status_ver` (no frame tracking for status changes)
- `get_changes()` checks for config and state changes, but does NOT check for status changes
- Status is only included in `node_details` for nodes in `detail_handles` (watched nodes)
- Status changes occur in multiple places:
  - During initialization (`init_nodes()`)
  - During rendering (`render()`)
  - During filesystem change handling (`handle_fs_changes()`)

### Client (`ClientProjectView`)

- Tracks `previous_status` map to detect status changes
- Only reports status changes when transitioning from Ok -> Error or Error -> Ok
- Receives status changes via `NodeChange::StatusChanged` in `node_changes`
- Status is stored in `ClientNodeEntry` but may be stale for unwatched nodes

## Questions

### Question 1: Status Version Tracking

**Context:**
Currently, `NodeEntry` tracks `config_ver` and `state_ver` but not `status_ver`. We need to track when status last changed.

**Question:**
Should we add a `status_ver: FrameId` field to `NodeEntry` to track the last frame when status changed?

**Suggested Answer:**
Yes. Add `status_ver: FrameId` to `NodeEntry`, similar to `config_ver` and `state_ver`. This will allow `get_changes()` to efficiently determine which nodes have status changes since a given frame.

**Answer:**
Yes - confirmed.

### Question 2: Error Message Changes

**Context:**
The user wants to see new errors when the error message changes (e.g., updating GLSL code changes the error message from "syntax error" to "type mismatch").

**Question:**
Should `status_ver` be updated whenever the status changes, even if it's the same variant but with a different message (e.g., `Error("msg1")` -> `Error("msg2")`)?

**Suggested Answer:**
Yes. Any change to the status value (including error message changes) should update `status_ver`. This ensures clients see updated error messages when they change.

**Answer:**
Yes - confirmed.

### Question 3: Status in GetChanges Response

**Context:**
Currently, status is only included in `node_details` for watched nodes. The goal is to always send status for all nodes.

**Question:**
How should we include status in GetChanges responses?
- Option A: Always include status changes in `node_changes` as `NodeChange::StatusChanged` for all nodes with status changes
- Option B: Always include status in `node_details` for all nodes (not just watched ones)
- Option C: Both - include status changes in `node_changes` AND include status in `node_details` for all nodes

**Suggested Answer:**
Option A + partial B: Include status changes in `node_changes` for all nodes with status changes (this is the efficient incremental approach). Also ensure that when we include `node_details` for watched nodes, we always include status. For unwatched nodes, status will come via `node_changes` as `StatusChanged` events.

**Answer:**
Separate node details and node status so they are handled separately. For now, always return status updates for all nodes via `node_changes` as `StatusChanged` events. This allows for future opt-in behavior similar to detail tracking.

### Question 4: Status in NodeDetails

**Context:**
`NodeDetail` already includes `status`. Currently, `node_details` is only populated for nodes in `detail_handles`.

**Question:**
Should we always include status in `node_details` for all nodes, or is it sufficient to send status changes via `node_changes`?

**Suggested Answer:**
It's sufficient to send status changes via `node_changes`. When a node is first created, it will have a `Created` change which the client can use to initialize status. Subsequent status changes will come via `StatusChanged` events. However, we should ensure that when `node_details` is included (for watched nodes), it always includes the current status.

**Answer:**
Remove status from `node_details` entirely. Status will only come via `node_changes` as `StatusChanged` events.

### Question 5: Initial Status for New Nodes

**Context:**
When a node is created, it starts with `NodeStatus::Created`. This status is set when the node entry is created.

**Question:**
Should the initial `Created` status trigger a `StatusChanged` event, or is the `Created` change notification sufficient?

**Suggested Answer:**
The `Created` change notification is sufficient. The client can initialize the status to `Created` when processing the `Created` change. We should ensure `status_ver` is set to the creation frame when the node is created.

**Answer:**
Created is enough - confirmed.

### Question 6: Client Status Tracking

**Context:**
The client currently tracks `previous_status` to detect status changes, but only reports Ok <-> Error transitions.

**Question:**
Should the client continue to track all status changes internally, or is it sufficient to rely on the server sending all status changes?

**Suggested Answer:**
The client should continue to track status internally (it already does via `ClientNodeEntry.status`). The server will now send all status changes, so the client can update its view accordingly. The client's `previous_status` tracking for detecting Ok <-> Error transitions can remain as-is for UI purposes.

**Answer:**
Update the code to track and log all status updates. We can track the version (status_ver) to know when status changes. We want to log all status updates.
