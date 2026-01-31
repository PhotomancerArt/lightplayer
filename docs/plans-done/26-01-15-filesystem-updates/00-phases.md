# Phases: Filesystem Updates Integration

1. Add update_config() and handle_fs_change() to NodeRuntime trait
2. Implement update_config() and handle_fs_change() for all node types
3. Add handle_fs_changes() method to ProjectRuntime
4. Refactor load_node() to support loading by path (if needed)
5. Create scene_update.rs test file
6. Test and cleanup
