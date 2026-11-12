# Overview: Filesystem Updates Integration

## Goal

Get the new lp-engine working correctly with filesystem changes. Enable the runtime to process filesystem change events and apply them to the scene, including updates to `node.json` and `main.glsl` files.

## Deliverables

1. `handle_fs_changes()` method on `ProjectRuntime` that processes filesystem change events
2. `update_config()` and `handle_fs_change()` methods on `NodeRuntime` trait
3. Implementations of these methods for all node types (shader, texture, fixture, output)
4. Test file `scene_update.rs` that verifies filesystem changes are correctly applied

## Approach

- Keep `tick()` clean by using a separate `handle_fs_changes()` method
- Allow nodes to choose whether to reinit or update in place when config changes
- Process filesystem changes in order: deletions → creates → modifies
- Match file changes to nodes by iterating through nodes and checking if file path belongs to node directory
