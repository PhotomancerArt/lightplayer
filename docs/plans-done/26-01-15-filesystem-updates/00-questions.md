# Questions: Filesystem Updates Integration

## Scope

Getting the new lp-engine working correctly with filesystem changes. The goal is to:
1. Send filesystem events to `tick()` 
2. Ensure filesystem changes are correctly applied to the scene
3. Create a test (`scene_update.rs`) that verifies `node.json` and `main.glsl` changes are applied correctly

## Questions

1. **tick() signature**: ✅ **ANSWERED**
   
   Use a separate method like `handle_fs_changes()` that's called before `tick()`. This keeps `tick()` clean for when we aren't dealing with fs changes. The caller can decide when to process filesystem changes.

2. **Node reload behavior**: ✅ **ANSWERED**
   
   Add a separate function on the node runtime where it's given the new config. Nodes can choose to reinit or just update things in place. This allows for smooth transitions (like rescaling a texture) and avoids the cost of full recreation. Re-creating simplifies things but is costly and doesn't allow smooth transitions.

3. **NodeRuntime trait method**: ✅ **ANSWERED**
   
   - `update_config(&mut self, new_config: Box<dyn NodeConfig>, ctx: &dyn NodeInitContext) -> Result<(), Error>` seems fine for now
   - For other file changes (like `main.glsl`), the node should have its own fs change function that's called
   - Need to add a method to `NodeRuntime` trait for handling filesystem changes to non-config files

4. **Node kind changes**: ✅ **ANSWERED**
   
   Node kind can't change - it's defined by the directory name (e.g., `1.texture`). Renames will be treated as removed and readded nodes since their path changes.

5. **Filesystem change method signature**: ✅ **ANSWERED**
   
   `handle_fs_change(&mut self, change: &FsChange, ctx: &dyn NodeInitContext) -> Result<(), Error>` seems fine. The `FsChange` contains both the path and change type (Create/Modify/Delete), so nodes can handle each case appropriately.

6. **Node path extraction**: ✅ **ANSWERED**
   
   Iterate through the nodes and check if any of the file changes match the node's directory. It's O(n*m) but we don't have that many file changes to deal with. This avoids path parsing complexity.

7. **Node directory changes**: ✅ **ANSWERED**
   
   Refactor out a `load_node(path)` function. For deletions, remove the node directly. For creations, detect it's a node directory (check if path ends with `.shader`, `.texture`, etc.) and call `load_node(path)`. More efficient than re-calling `load_nodes()`.

8. **Multiple changes**: ✅ **ANSWERED**
   
   Not worried about deduplication in the engine - let the filesystem handle that. Process all changes as-is. If both `node.json` and `main.glsl` change for the same shader, call both `update_config()` and `handle_fs_change()`.
