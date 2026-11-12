# Plan: Better Status Change Tracking

## Overview

Improve status change tracking in lp-server and client to ensure status is always visible for all nodes, even those not being watched for detail. This involves adding `status_ver` tracking, removing status from `NodeDetail`, and always including status changes in `GetChanges` responses.

## Phases

1. Add status_ver field to NodeEntry
2. Update status changes to track status_ver
3. Update get_changes to include status changes for all nodes
4. Remove status from NodeDetail and SerializableNodeDetail
5. Update client to track status_ver and log status updates
6. Update client to handle status only from node_changes
7. Cleanup and finalization

## Success Criteria

- All nodes have `status_ver` tracking when status changes
- Status changes are always included in `GetChanges` responses for all nodes
- Status is removed from `NodeDetail` (only comes via `node_changes`)
- Client tracks and logs all status updates
- Code compiles without errors
- All tests pass
- Code formatted with `cargo +nightly fmt`
