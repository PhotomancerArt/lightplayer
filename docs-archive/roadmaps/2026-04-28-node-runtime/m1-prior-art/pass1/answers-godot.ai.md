# Godot 4 — Prior Art Answers

Reference token: `godot`
Codebase root: `/Users/yona/dev/photomancer/prior-art/godot/`

---

## Section 1: Node lifecycle

### 1.1 What named lifecycle hooks does this system define?

Godot uses a **notification-based** lifecycle system with both integer notifications and virtual methods. The ~5 most-used hooks:

1. **`_init`** — Called when object is created (constructor-like). Defined via `GDVIRTUAL0(_init)` in `Object` class.
2. **`_enter_tree`** — Called when node enters the scene tree. `NOTIFICATION_ENTER_TREE` (value 10) fires first, then virtual `_enter_tree()` is called `(godot:scene/main/node.cpp:L360-L362)`.
3. **`_ready`** — Called once when node is ready (after all children entered tree). `NOTIFICATION_READY` (value 13) fires after `NOTIFICATION_POST_ENTER_TREE` `(godot:scene/main/node.cpp:L334-L337)`.
4. **`_process(delta)`** — Per-frame update. `NOTIFICATION_PROCESS` (value 17) fires each frame if processing enabled `(godot:scene/main/node.h:L464)`.
5. **`_exit_tree`** — Called when leaving the tree. Virtual `_exit_tree()` fires, then `NOTIFICATION_EXIT_TREE` (value 11) `(godot:scene/main/node.cpp:L427-L431)`.
6. **`_notification(what)`** — Catch-all for all notification types `(godot:scene/main/node.cpp:L68-L321)`.

Additional hooks: `_physics_process`, `_input`, `_shortcut_input`, `_unhandled_input`, `_unhandled_key_input`, `_predelete`.

### 1.2 Enter-phase hook invocation order?

Order when adding a node to the running tree `(godot:scene/main/node.cpp:L341-L389)`:

1. Parent's `_add_child_nocheck` adds child to `data.children` HashMap
2. `_propagate_enter_tree()` called recursively:
   - Sets `data.tree` pointer from parent
   - `notification(NOTIFICATION_ENTER_TREE)` `(godot:scene/main/node.cpp:L360)`
   - `GDVIRTUAL_CALL(_enter_tree)` `(godot:scene/main/node.cpp:L362)`
   - `emit_signal(SceneStringName(tree_entered))` `(godot:scene/main/node.cpp:L364)`
   - `data.tree->node_added(this)` notifies SceneTree `(godot:scene/main/node.cpp:L366)`
   - Recurses to children
3. After all children entered: `_propagate_ready()`:
   - `notification(NOTIFICATION_POST_ENTER_TREE)` `(godot:scene/main/node.cpp:L332)`
   - `notification(NOTIFICATION_READY)` (first time only) `(godot:scene/main/node.cpp:L336)`
   - `emit_signal(SceneStringName(ready))` `(godot:scene/main/node.cpp:L337)`

### 1.3 Per-frame hooks and tree traversal order?

Godot has two processing phases: **physics** (fixed timestep) and **idle** (variable/frame).

**Per-frame dispatcher**: `SceneTree::_process(bool p_physics)` `(godot:scene/main/scene_tree.cpp:L1243-L1368)`

**Order**:
- Nodes sorted by `process_priority` (or `physics_process_priority` for physics) using `ComparatorWithPriority` `(godot:scene/main/scene_tree.cpp:L1194)`
- Within same priority: `Node::is_greater_than()` compares tree order (depth + index) `(godot:scene/main/node.h:L136-L138)`
- Sibling order: index-based (internal nodes front, external middle, internal back)

**Notifications per frame**:
1. `NOTIFICATION_INTERNAL_PROCESS` (if `set_process_internal(true)`)
2. `NOTIFICATION_PROCESS` (if `set_process(true)`) `(godot:scene/main/scene_tree.cpp:L1225-L1230)`

Physics frame adds `NOTIFICATION_INTERNAL_PHYSICS_PROCESS` and `NOTIFICATION_PHYSICS_PROCESS`.

### 1.4 Teardown traversal order?

**Exit tree**: Child-first (reverse order), deepest first `(godot:scene/main/node.cpp:L410-L457)`

Order in `_propagate_exit_tree()`:
1. Block modifications (`data.blocked++`)
2. Iterate children in **reverse** (from `data.children.last()` backwards) `(godot:scene/main/node.cpp:L421-L423)`
3. Recurse to each child
4. Unblock
5. `GDVIRTUAL_CALL(_exit_tree)` `(godot:scene/main/node.cpp:L427)`
6. `emit_signal(SceneStringName(tree_exiting))` `(godot:scene/main/node.cpp:L429)`
7. `notification(NOTIFICATION_EXIT_TREE, true)` — `true` means "reversed" `(godot:scene/main/node.cpp:L431)`

**Destruction** (`NOTIFICATION_PREDELETE`):
- Owner cleanup first `(godot:scene/main/node.cpp:L295-L302)`
- Parent removes child `(godot:scene/main/node.cpp:L304-L306)`
- Children destroyed from **end to beginning** (LIFO) `(godot:scene/main/node.cpp:L309-L312)`

### 1.5 Eager vs deferred lifecycle?

**Enter tree**: Eager/synchronous — happens immediately in `add_child()` `(godot:scene/main/node.cpp:L2758-L2836)`

**Teardown/destroy**: Deferred via `queue_free()` → `SceneTree::queue_delete()` → `delete_queue` → `_flush_delete_queue()` at frame end `(godot:scene/main/scene_tree.cpp:L1625-L1642)`

Deletion is deferred to end of frame to avoid use-after-free during processing. `_flush_delete_queue()` called at `(godot:scene/main/scene_tree.cpp:L666)` after physics process and `(godot:scene/main/scene_tree.cpp:L735)` after idle process.

### 1.6 Dispatcher behavior on hook panic/error?

Godot uses **error macros** that log and continue, not exceptions. From `(godot:core/error/error_macros.h)`:

- `ERR_FAIL_COND()` — prints error, returns from function
- `ERR_PRINT()` — prints error, continues
- `CRASH_BAD_INDEX()` — fatal trap in debug builds only

On script/hook failure: Godot prints to stderr/log, optionally notifies editor, and **continues**. No node isolation or frame abort. Node errors surface via `get_configuration_warnings()` (visual warnings in editor) `(godot:scene/main/node.h:L797-L799)`.

---

## Section 2: Node identity and addressing

### 2.1 Runtime node identifier?

**ObjectID** — a 64-bit generational index `(godot:core/object/object_id.h:L41-L63)`

Structure:
- Lower bits: slot index (object's position in ObjectDB slot array)
- Middle bits: validator (generation counter, incremented on reuse)
- High bit: ref-counted flag `(godot:core/object/object.cpp:L2483-L2485)`

Minted by `ObjectDB::add_instance()` `(godot:core/object/object.cpp:L2450-L2492)`:
- Slot allocated from free list
- Validator incremented
- ID = `(validator << SLOT_BITS) | slot | (is_ref_counted ? REF_BIT : 0)`

Stable for object's lifetime. Invalidated on destruction (validator cleared).

### 2.2 Stable across save/load?

**No** — ObjectIDs are runtime-only, regenerated each session.

For persisted identity, Godot uses:
- **`unique_scene_id`** — `int32_t` per-node within a scene file `(godot:scene/main/node.h:L295)`
- Set via `set_unique_scene_id()` / `get_unique_scene_id()` `(godot:scene/main/node.cpp:L2117-L2122)`
- SceneState saves these IDs in `id_paths` vector `(godot:scene/resources/packed_scene.h:L44)`

For references in saved scenes: NodePath (hierarchical path) is used for persistence, not IDs.

### 2.3 Path syntax?

**NodePath** class handles path syntax `(godot:core/string/node_path.h:L38-L99)`

Examples:
- `get_node("Player")` — relative child named "Player"
- `get_node("Player/Sprite")` — nested child
- `get_node("../Sibling")` — parent then sibling (relative)
- `get_node("/root/Main/Player")` — absolute from root
- `get_node(^"Player")` — unique name in owner (Godot 4 syntax)

Path stored as `Vector<StringName> path` + `Vector<StringName> subpath` (for property access).

### 2.4 Path grammar features?

From `(godot:core/string/node_path.h)` and `(godot:scene/main/node.cpp)`:

- **Relative paths**: default (no leading `/`)
- **Absolute paths**: leading `/` `(godot:core/string/node_path.h:L56)`
- **Parent reference**: `..` `(godot:scene/main/node.cpp:L568-L570)`
- **Current node**: `.` (implicit)
- **Indexed segments**: Not directly; indices resolved via `data.index` in children cache
- **Wildcards/globs**: Not supported in NodePath syntax
- **Unique names**: `%Name` syntax for owner-unique names `(godot:scene/main/node.cpp:L3478-L3480)`

### 2.5 Id-based vs specifier-based references?

Godot has both:

**ID-based** (runtime):
- `ObjectID` → `ObjectDB::get_instance(id)` O(1) lookup `(godot:core/object/object.cpp:L2450-L2492)`
- Used for signals, deferred calls, delete queue

**Specifier-based** (paths/queries):
- `NodePath` → `get_node(path)` O(depth) traversal `(godot:scene/main/node.cpp:L2686-L2720)`
- `find_child(pattern, recursive, owned)` — first match by name pattern `(godot:scene/main/node.cpp:L2704-L2714)`
- `find_children(pattern, type, recursive, owned)` — all matches `(godot:scene/main/node.cpp:L2716-L2746)`
- `get_node_or_null(path)` — safe lookup returning null on failure

### 2.6 Name uniqueness enforcement?

**Sibling uniqueness**: No enforcement. Siblings **can** share names, but this breaks path lookup which finds first match `(godot:scene/main/node.cpp:L2686-L2702)`.

**Owner-unique names**: Opt-in via `set_unique_name_in_owner(true)` `(godot:scene/main/node.h:L592-L593)`:
- Stored in `owned_unique_nodes` HashMap `(godot:scene/main/node.h:L209)`
- Accessed via `%Name` syntax
- Enforced at add time: collision fails with error `(godot:scene/main/node.cpp:L2780-L2785)`

Auto-generated names: When adding unnamed child, `_generate_serial_child_name()` appends number to avoid collision `(godot:scene/main/node.cpp:L2739-L2745)`.

### 2.7 Lookup complexity?

| Type | Complexity | Function |
|------|------------|----------|
| ObjectID → Object | O(1) | `ObjectDB::get_instance(id)` — slot index extraction + validator check `(godot:core/object/object.cpp:L2450-L2492)` |
| NodePath → Node | O(depth) | `get_node(path)` — traverses parent/child chain `(godot:scene/main/node.cpp:L2686-L2720)` |
| Name → Child | O(1) avg | `_get_child_by_name(name)` — HashMap lookup `(godot:scene/main/node.h:L206)` |
| Pattern query | O(n) | `find_child(pattern)` — linear search with string match |
| Group lookup | O(1) to get list | `SceneTree::get_nodes_in_group()` — HashMap to Vector |

---

## Section 3: Resource refcount / asset management

### 3.1 Refcounted "asset" or "resource" type?

**`Resource`** extends `RefCounted` extends `Object` `(godot:core/io/resource.h:L52-L193)`

Resources are refcounted assets that can be saved/loaded from disk. Key members:
- `String name` — resource name
- `String path_cache` — filesystem path
- `bool local_to_scene` — whether to duplicate per scene instance

### 3.2 Handle type and refcount mechanism?

**`Ref<T>`** template handle `(godot:core/object/ref_counted.h:L59-L229)`:
- Holds `T* reference` pointer
- **Automatic** via RAII: constructor calls `reference()`, destructor calls `unreference()`
- `ref_pointer()` increments refcount via `reference->reference()` (or `init_ref()` for first ref)
- `unref()` decrements, deletes when zero: `if (ref->unreference()) memdelete(ref)` `(godot:core/object/ref_counted.h:L199-L213)`

`RefCounted` base class provides:
- `SafeRefCount refcount` — atomic counter
- `reference()` / `unreference()` / `get_reference_count()` `(godot:core/object/ref_counted.h:L36-L56)`

### 3.3 Loading synchronous, async, or lazy?

**Synchronous by default**: `ResourceLoader::load(path)` blocks until loaded `(godot:core/io/resource_loader.h:L249)`

**Async loading available**:
- `load_threaded_request(path)` — starts async load `(godot:core/io/resource_loader.h:L239)`
- `load_threaded_get_status(path)` — poll progress `(godot:core/io/resource_loader.h:L240)`
- `load_threaded_get(path)` — get result `(godot:core/io/resource_loader.h:L241)`

**Caching**: `CACHE_MODE_REUSE` returns existing cached resource; `CACHE_MODE_REPLACE` forces reload; `CACHE_MODE_IGNORE` skips cache `(godot:core/io/resource_loader.h:L114-L119)`

### 3.4 Unloading / eviction trigger?

**Refcount-zero**: When last `Ref<T>` drops, destructor calls `unreference()`. If count hits zero, `memdelete()` is called `(godot:core/object/ref_counted.h:L204-L210)`.

**ResourceCache**: Maintains weak refs to loaded resources. When all strong refs drop, resource can be reaped (though Godot doesn't aggressively reap; cache holds raw pointer until explicit clear or resource freed).

**Manual**: `resource->unreference()` possible but rare.

### 3.5 Hot reload support?

**Yes** — via `Resource::reload_from_file()` `(godot:core/io/resource.h:L135)`

For PackedScene: `reload_from_file()` recreates state from disk `(godot:scene/resources/packed_scene.cpp:L1785-L1812)`

Propagation:
- `ResourceCache` tracks resources by path; new load replaces cached entry
- `emit_changed()` signals to listeners `(godot:core/io/resource.h:L137)`
- Editor connects to `changed` signal to refresh UI

---

## Section 4: Scene / patch / instance instantiation

### 4.1 How authored thing becomes runtime instance?

**PackedScene → instantiate()** `(godot:scene/resources/packed_scene.h:L268-L273)`:

Path from disk to tree:
1. `ResourceLoader::load("res://scene.tscn")` → returns `Ref<PackedScene>`
2. `packed_scene->instantiate(edit_state)` → `SceneState::instantiate()` `(godot:scene/resources/packed_scene.h:L163)`
3. SceneState creates nodes from `nodes` vector:
   - Allocates node by type `(godot:scene/resources/packed_scene.cpp:L215-L234)`
   - Sets name, properties, groups
   - Recursively builds parent-child hierarchy via `parent` indices
4. Node enters tree via `add_child()` → `_propagate_enter_tree()`
5. `_propagate_ready()` fires when all children ready

### 4.2 Multiple concurrent instances?

**Yes** — same PackedScene can be instantiated multiple times `(godot:scene/resources/packed_scene.cpp:L168-L180)`.

Independence:
- Each `instantiate()` creates **new Node instances** (not shared)
- **Shared**: Resources referenced by path (textures, meshes) via ResourceCache
- **Per-instance**: Node transform, scripts, local-to-scene resources (if `local_to_scene=true`)

### 4.3 Shared vs per-instance pieces?

**Shared** (immutable references):
- Mesh data, textures, materials via `Ref<Resource>` — same resource referenced by all instances
- Physics shapes, animation libraries

**Per-instance** (stateful):
- Node: transform, visibility, process flags, script variables
- `local_to_scene` resources: duplicated per instance via `duplicate_for_local_scene()` `(godot:core/io/resource.h:L158-L159)`

### 4.4 Nested instantiation?

**Yes** — scenes can instantiate other scenes infinitely deep.

SceneState handles this:
- `instance` field in `NodeData` points to sub-scene `(godot:scene/resources/packed_scene.h:L63)`
- `instantiate()` recursively calls `instance->instantiate()` for nested scenes `(godot:scene/resources/packed_scene.cpp:L168-L180)`
- No explicit depth limit enforced by engine

---

## Section 5: Change tracking and editor / wire sync

**N/A for most of this section** — Godot is single-process; editor and game are same binary (with `TOOLS_ENABLED` distinction). No separate client/server wire protocol like Lightplayer.

### 5.1 Property change tracking?

**Opt-in dirty flags + signals**:
- `Resource::emit_changed()` — fires when resource modified `(godot:core/io/resource.h:L137)`
- `Object::notify_property_list_changed()` — property list mutated
- Editor uses `EditorNode::reload_scene` for external file changes

No automatic per-property dirty tracking at runtime.

### 5.2–5.6 Not applicable

Godot doesn't have a separate client/server sync protocol. Editor modifications are in-process via Object::set() calls.

---

## Section 6: Property reflection

### 6.1 Enumerate node properties at runtime?

**`Object::_get_property_listv()`** → `ClassDB::get_property_list()` `(godot:core/object/object.h:L197-L227)`

Usage:
```cpp
List<PropertyInfo> plist;
node->get_property_list(&plist);
```

Returns `PropertyInfo` structs with:
- `name` — StringName
- `type` — Variant::Type
- `hint` — PROPERTY_HINT_*
- `usage` — PROPERTY_USAGE_* flags

### 6.2 Static vs dynamic property types?

**Hybrid**:
- **Static**: Core classes define properties in `_bind_methods()` via `ADD_PROPERTY()` macro `(godot:core/object/object.h:L47-L65)`
- **Dynamic**: Scripts (GDScript/C#) add properties via `_get_property_list()` override; GDExtensions can provide dynamic lists

`ClassDB` stores static property bindings in hash maps `(godot:core/object/class_db.cpp)`.

### 6.3 Set/get by string name?

**Yes** — `Object::set(name, value)` and `Object::get(name)` `(godot:core/object/object.cpp:L292-L377)`:

Dispatch path:
1. Check `ClassDB` for setter/getter bindings
2. Call bound method or virtual `_set`/`_get`
3. Fall back to script instance's fallback accessors

Indexed access: `set_indexed(NodePath, value)` for nested properties `(godot:core/object/object.cpp:L1693-L1697)`.

### 6.4 Authority-checked property edits?

**Partially**:
- Read-only: `PROPERTY_USAGE_READ_ONLY` hint prevents editor edits `(godot:core/object/property_info.h)`
- Type checking: Setters receive `Variant`, expected to validate type
- No runtime permission system; any code with Object* can call `set()`

---

## Section 7: Composition — dynamic children

### 7.1 Children change at runtime?

**Yes** — `add_child(node)` and `remove_child(node)` are public API `(godot:scene/main/node.h:L525-L527)`.

Implementation:
- Children stored in `HashMap<StringName, Node *> data.children` `(godot:scene/main/node.h:L206)`
- Cached in `LocalVector<Node *> children_cache` for ordered access `(godot:scene/main/node.h:L208)`
- Adding triggers `_propagate_enter_tree()` if parent in tree
- Removing triggers `_propagate_exit_tree()`

### 7.2 Structurally-owned vs externally-composed children?

**Both patterns supported**:

**Structural** (internal):
- Nodes created by parent type (e.g., Viewport's internal canvas layers)
- `InternalMode` enum: `INTERNAL_MODE_FRONT`, `INTERNAL_MODE_BACK` `(godot:scene/main/node.h:L124-L128)`
- Internal children not visible in editor tree by default

**External** (author-composed):
- Regular children added via editor or `add_child()`
- `INTERNAL_MODE_DISABLED` (default)

### 7.3 "Property's value is itself a node"?

**Not directly** — Godot uses **NodePath properties** for cross-references, not embedded nodes.

Example: `export(NodePath) var target` in GDScript stores path; resolved at runtime via `get_node(target)`.

No automatic "parameter becomes child" like Lightplayer's gradient → Pattern composition.

### 7.4 Child type constraints?

**No compile-time type checks**. Any Node can have any Node as child.

Runtime validation:
- `add_child_notify()` virtual for derived classes to intercept `(godot:scene/main/node.h:L382-L384)`
- `get_configuration_warnings()` can return warnings for invalid children `(godot:scene/main/node.h:L797)`
- Editor-only: certain nodes show warnings (e.g., Button inside Button)

---

## Section 8: Inter-node dependencies and execution ordering

### 8.1 "Node A reads node B's output" expression?

**Explicit reference + polling**, not dataflow:
- Store `NodePath` to target node
- Each frame: `target_node = get_node(target_path)`, then read property
- Or use `get_node_and_resource(path)` for property drilling `(godot:scene/main/node.h:L545)`

No built-in "cable" or "slot binding" system at Node level.

### 8.2 Execution order determination?

**Tree traversal + priority sorting** `(godot:scene/main/scene_tree.cpp:L1176-L1235)`:

1. Nodes sorted by `process_priority` (or `physics_process_priority`)
2. Within same priority: tree order (parent before children at same depth)
3. Process groups batch nodes for multi-threading

No topological sort of data dependencies — nodes poll state from others synchronously.

### 8.3 Push or pull evaluation?

**Pull** — requesting node reads from source node at evaluation time.

Example: A `Camera2D` reading target position:
```cpp
void _process(float delta) {
    if (target_node) {
        position = target_node->position; // pull
    }
}
```

### 8.4 Cycle detection/prevention?

**None at Node level** — cycles in reference paths don't break tree; infinite loops in scripts are user error.

Signals can create cycles (A connects to B, B connects to A), but signal emission is immediate/synchronous by default, so infinite recursion is possible.

### 8.5 Dependency graph cached?

**N/A** — no dependency graph. Tree structure is the "graph", traversed every frame.

Process groups are rebuilt when nodes add/remove processing flags `(godot:scene/main/scene_tree.cpp:L1244-L1270)`.

### 8.6 Cross-tree dependencies allowed?

**Anywhere via NodePath**:
- Siblings: `get_node("../Sibling")`
- Ancestors: `get_node("../..")`
- Distant: absolute path `/root/Scene/OtherNode`
- Cross-scene: via unique names `%Node` in owner

No "bus" abstraction; direct pointer access after path resolution.

---

## Section 9: Node state, errors, and logging

### 9.1 Explicit operational state enum?

**No single enum**. State tracked via:
- `bool ready_notified` — `_ready` fired `(godot:scene/main/node.h:L286)`
- `bool ready_first` — still in first ready phase `(godot:scene/main/node.h:L287)`
- `SceneTree *tree` — null if not in tree `(godot:scene/main/node.h:L219)`
- `ProcessMode process_mode` — processing state `(godot:scene/main/node.h:L246)`
- `bool _is_queued_for_deletion` — set by `queue_delete()` `(godot:scene/main/scene_tree.cpp:L1640)`

No unified `Loading/Ready/Error` state machine.

### 9.2 Error categories?

**Typed by macro/context** `(godot:core/error/error_macros.h:L38-L56)`:
- `ERR_HANDLER_ERROR` — general error
- `ERR_HANDLER_WARNING` — warning
- `ERR_HANDLER_SCRIPT` — script error
- `ERR_HANDLER_SHADER` — shader error

No structured error object hierarchy; errors are function-scoped prints.

### 9.3 Load-time errors (missing/malformed asset)?

**Missing resource**: `ResourceLoader::load()` returns `Ref<Resource>()` (null) and sets `Error *r_error` `(godot:core/io/resource_loader.h:L249)`

**Malformed**: Parser errors logged, null returned.

**Wrong type**: Godot is dynamically typed; type mismatch typically caught at access time, not load.

**MissingNode placeholder**: Editor creates `MissingNode` for unavailable class types `(godot:scene/main/missing_node.h)`.

### 9.4 Runtime errors (OOM, panic, exception)?

**OOM**: Godot uses custom allocators; OOM typically triggers crash or `ERR_PRINT` depending on allocator.

**Panic/hook failure**: Error printed, execution continues. No process termination.

**Script exceptions**: Caught at script VM level, error printed to console and editor debugger.

### 9.5 Error storage location?

**Global log + optional editor notification** `(godot:core/error/error_macros.h:L60-L80)`:
- `_err_print_error()` prints to stdout/stderr
- `ErrorHandlerList` allows registering custom handlers
- Editor subscribes to display errors in debugger panel

**Not attached to node** — errors are events, not persistent state.

### 9.6 Error logging mechanism?

**Print-based** via `_err_print_error()` functions `(godot:core/error/error_macros.h:L74-L82)`:
- File, line, function captured via `__FILE__`, `__LINE__`, `__FUNCTION__`
- Message printed with `print_error()` → OS::print_error
- Editor notification optional via `p_editor_notify`

**No structured query interface** — text search only.

### 9.7 Warnings tracked separately?

**Yes**, via `ERR_HANDLER_WARNING` type `(godot:core/error/error_macros.h:L40-L55)`.

Node warnings: `get_configuration_warnings()` returns `PackedStringArray` of warnings displayed in editor inspector `(godot:scene/main/node.h:L797-L799)`.

### 9.8 Behavior in error state / recovery?

**No automatic error state**. Node continues running or fails silently.

**Script errors**: Script execution aborts for that call, but node remains in tree.

**Recovery**: User must fix issue and reload scene; no automatic retry.

---

## Section 10: Schema versioning and evolution

### 10.1 Schema version embedded in files?

**No explicit version field** in scene/resource files.

Godot uses:
- **Format identifier**: Text scenes (`.tscn`) have `[gd_scene load_steps=1 format=3]` — format 3 is Godot 4.x `(godot:scene/resources/packed_scene.cpp)`
- **Binary format**: Version in header (major.minor.patch)

Per-resource: No granular version; type name determines expected properties.

### 10.2 Migration path from older schema?

**Compatibility methods** via `ClassDB`:
- `_bind_compatibility_methods()` macro for API renames `(godot:core/object/object.h:L448-L452)`
- Virtual `set()`/`get()` can handle deprecated property names

**Property renames**: Handled in `_set()` override by checking old name.

No declarative migration DSL.

### 10.3 Forward compatibility?

**Partial** — unknown properties ignored with warning. Scene loads, missing properties use defaults.

Binary format: Strict — unknown fields may cause load failure.

### 10.4 Backward compatibility?

**Goal is yes**, achieved via:
- Deprecated property handlers in `_set()`
- Compatibility method bindings
- ResourceFormatLoader version detection

### 10.5 Deprecated field handling?

**Silently dropped or remapped**:
- `_set()` override checks for old name, maps to new, may print warning
- `ClassDB` compatibility bindings for method renames

No blocking error on deprecated fields; graceful degradation preferred.

---

## Summary Notes

**Concepts that don't cleanly map:**
- **Editor/client split**: Godot editor and runtime are same process; no wire protocol
- **Automatic property-to-child**: Godot uses NodePath references, not parameter-becomes-child
- **Explicit node operational state**: Godot tracks via flags, not unified enum
- **Dependency graph**: Godot uses tree + polling, not dataflow graph

**Sections with many N/A**: 5 (change tracking/sync), parts of 9 (structured error objects)
