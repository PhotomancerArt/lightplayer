# Three.js Prior Art Survey — Node/Artifact Runtime Spine

Reference token: `threejs`
Codebase: `/Users/yona/dev/photomancer/prior-art/three.js/`

---

## Section 1: Node lifecycle

### 1.1 Named lifecycle hooks

Three.js defines **minimal explicit lifecycle hooks** for Object3D nodes. The key hooks are:

1. **`onBeforeRender`** — Called immediately before a 3D object is rendered. Used for per-frame updates. `(threejs:src/core/Object3D.js:L428)`

2. **`onAfterRender`** — Called immediately after rendering completes. `(threejs:src/core/Object3D.js:L440)`

3. **`onBeforeShadow`** / **`onAfterShadow`** — Called before/after shadow map rendering. `(threejs:src/core/Object3D.js:L403-L416)`

4. **Events (not hooks)**: `added`, `removed`, `childadded`, `childremoved` — fired via EventDispatcher when tree structure changes. `(threejs:src/core/Object3D.js:L32-L56)`

**N/A** — There are no explicit "enter tree", "ready", or "exit tree" lifecycle callbacks. The pattern relies on the `added`/`removed` events and render-time callbacks instead.

### 1.2 Enter-phase hook order

**N/A** — Three.js has no formal enter-phase sequence. Adding a child is synchronous and immediate:

1. `object.removeFromParent()` — detaches from previous parent `(threejs:src/core/Object3D.js:L769)`
2. `object.parent = this` — sets new parent `(threejs:src/core/Object3D.js:L770)`
3. `this.children.push(object)` — adds to children array `(threejs:src/core/Object3D.js:L771)`
4. `object.dispatchEvent(_addedEvent)` — fires event `(threejs:src/core/Object3D.js:L773)`
5. `this.dispatchEvent(_childaddedEvent)` — parent notification `(threejs:src/core/Object3D.js:L776)`

No staged initialization (no `_init` → `_enter_tree` → `_ready` equivalent).

### 1.3 Per-frame hooks and tree order

**Per-node**: `onBeforeRender` and `onAfterRender` fire during rendering, not on every Object3D universally. The renderer calls these only for visible renderable objects (Mesh, Line, Points, etc.).

**Traversal order for matrix updates**: `updateMatrixWorld()` traverses **parent-first** (root-to-leaves), then processes children in array order:

```javascript
if ( this.matrixAutoUpdate ) this.updateMatrix();
// ... update world matrix ...
for ( let i = 0, l = children.length; i < l; i ++ ) {
    children[ i ].updateMatrixWorld( force );
}
```
`(threejs:src/core/Object3D.js:L1165-L1203)`

**N/A** — No universal per-tick callback on all Object3D instances. Render loop callback order is determined by the renderer's render list, not the scene graph structure.

### 1.4 Teardown traversal order

**N/A** — There is no automatic teardown traversal. Calling `remove()` or `clear()` is immediate and synchronous:

- `remove()` detaches a specific child, fires `removed` event `(threejs:src/core/Object3D.js:L798-L828)`
- `clear()` calls `remove()` on all children `(threejs:src/core/Object3D.js:L859-L863)`

**No hooks** for "about to be destroyed" or "being destroyed now". The `removed` event fires after detachment is complete.

### 1.5 Eager vs deferred lifecycle

All lifecycle events are **eager/synchronous**:
- `add()` completes immediately `(threejs:src/core/Object3D.js:L746-L787)`
- `remove()` completes immediately `(threejs:src/core/Object3D.js:L798-L828)`

**N/A** — No `queue_free` equivalent, no deferred destruction queue. The application must manage disposal timing manually.

### 1.6 Dispatcher error handling

**N/A** — Three.js has no central lifecycle dispatcher that could catch hook failures. Callbacks like `onBeforeRender` are invoked directly by the renderer; any exception propagates to the browser's event loop. The renderer does not isolate failing objects or abort frames.

---

## Section 2: Node identity and addressing

### 2.1 Runtime node identifier

Three.js nodes carry **three runtime identifiers**:

1. **`id`** — Auto-incrementing integer (`let _object3DId = 0`), assigned at construction via `Object.defineProperty`. `(threejs:src/core/Object3D.js:L11)` and `(threejs:src/core/Object3D.js:L89)`

2. **`uuid`** — Random-generated UUID (16 bytes via Math.random, formatted per RFC 4122). `(threejs:src/math/MathUtils.js:L17-L33)` and `(threejs:src/core/Object3D.js:L97)`

3. **`name`** — Optional string, default empty. `(threejs:src/core/Object3D.js:L104)`

4. **`type`** — String class identifier (e.g., 'Mesh', 'Scene'). `(threejs:src/core/Object3D.js:L113)`

None are stable across sessions except UUID which round-trips through JSON.

### 2.2 Id stability across save/load

- **`id`**: **Regenerated each session** — it's a process-local counter. Not serialized.
- **`uuid`**: **Round-trips** — explicitly serialized in `toJSON()` and restored in ObjectLoader. `(threejs:src/core/Object3D.js:L1299)`

The `uuid` plays the role of persisted reference in saved files.

### 2.3 Path syntax

**N/A** — Three.js has no path grammar for node addressing. Lookup is by property matching:

```javascript
scene.getObjectByName('Player')  // first match by name
scene.getObjectById(123)         // first match by id
scene.getObjectByProperty('uuid', 'abc-123')  // by uuid
```

`(threejs:src/core/Object3D.js:L917-L962)`

### 2.4 Path grammar features

**N/A** — No path grammar. The system provides:
- No relative/absolute path syntax
- No wildcards
- No indexed segments
- No multi-element/glob segments

Query APIs (`getObjectsByProperty`) return arrays, but there's no query language.

### 2.5 Id-based vs specifier-based references

Three.js uses **only specifier-based lookup** via the methods above. References between objects are typically **direct pointers**:

- `mesh.geometry` → direct `BufferGeometry` reference
- `mesh.material` → direct `Material` reference  
- `object.parent` → direct `Object3D` reference

No id-based resolution at runtime; direct references are the norm.

### 2.6 Name uniqueness enforcement

**No enforcement** — sibling names can collide. `getObjectByName()` returns the **first match** in depth-first traversal order. `(threejs:src/core/Object3D.js:L930-L933)`

Collision disambiguation: manual naming or use UUID-based lookup.

### 2.7 Lookup complexity

- **`id` lookup**: O(N) depth-first search via `getObjectById()`. `(threejs:src/core/Object3D.js:L917-L920)`
- **`name` lookup**: O(N) depth-first search via `getObjectByName()`. `(threejs:src/core/Object3D.js:L930-L933)`
- **`uuid` lookup**: O(N) via `getObjectByProperty('uuid', ...)`. `(threejs:src/core/Object3D.js:L944-L962)`
- **Direct reference**: O(1) — the predominant pattern

**N/A** — No index structures for O(1) id lookup. Traversal is recursive through `children` arrays.

---

## Section 3: Resource refcount / asset management

### 3.1 Refcounted asset type

**N/A** — Three.js has **no refcounted asset type**. Resources are held by direct reference.

Shared resources include:
- `BufferGeometry` — can be reused across multiple meshes
- `Material` — can be shared across meshes
- `Texture` — has a `Source` which decouples data from texture instance, allowing multiple textures to share the same image data `(threejs:src/textures/Source.js:L7-L12)`

### 3.2 Handle type and refcount mechanism

**N/A** — No handle type, no refcount. JavaScript garbage collection manages memory. GPU resources (WebGL buffers, textures) are tracked by the renderer via WeakMaps keyed by the JS object.

### 3.3 Loading model

Loading is **async via callbacks/Promises**. The Loader base class defines:
- `load(url, onLoad, onProgress, onError)` — callback style `(threejs:src/loaders/Loader.js:L82)`
- `loadAsync(url, onProgress)` — Promise wrapper `(threejs:src/loaders/Loader.js:L91-L101)`

Actual loading uses browser fetch/XHR via `FileLoader` `(threejs:src/loaders/FileLoader.js)`.

**N/A for our use case** — This is browser-network loading with threads/processes; not applicable to `no_std` embedded filesystem loading.

### 3.4 Unloading / eviction

**Manual disposal only**. GPU resources are freed by calling `dispose()`:

- `Texture.dispose()` — fires 'dispose' event `(threejs:src/textures/Texture.js:L647-L657)`
- `Material.dispose()` — fires 'dispose' event `(threejs:src/materials/Material.js:L993-L1003)`
- `BufferGeometry.dispose()` — fires 'dispose' event `(threejs:src/core/BufferGeometry.js:L1450-L1454)`

The renderer listens for these events and cleans up GPU-side resources. CPU-side JS objects remain until GC.

**N/A** — No automatic eviction, no reference counting, no LRU cache.

### 3.5 Hot reload

**N/A** — Three.js does not implement hot reload. To reload an asset:
1. Load new version via loader
2. Replace reference on consuming objects
3. Call `dispose()` on old resource
4. Wait for GC

Some devtools extensions implement hot reload externally, but it's not a core framework feature.

---

## Section 4: Scene / patch / instance instantiation

### 4.1 Instantiation entry point

`ObjectLoader.parse()` is the primary entry point for JSON → runtime object conversion. Flow:

1. `ObjectLoader.load()` fetches JSON via FileLoader `(threejs:src/loaders/ObjectLoader.js:L107-L148)`
2. `parse()` deserializes object by `type` field using a large switch/if chain `(threejs:src/loaders/ObjectLoader.js:L210-L600)`
3. Geometries, materials, textures are looked up in meta-libraries by UUID
4. Children are recursively parsed and `add()`ed to parent
5. Root object returned

Example: A mesh is instantiated by `type === 'Mesh'` check, then `new Mesh(geometry, material)` with resolved references. `(threejs:src/loaders/ObjectLoader.js:L555-L567)`

### 4.2 Multiple instantiation

Yes, the same authored asset (geometry, material, scene JSON) can be instantiated multiple times. Independence is achieved through:

- **Shared immutable**: Geometry data, texture source data, material properties
- **Per-instance mutable**: `Object3D` transform (position, rotation, scale), `visible`, `renderOrder`, `userData`

Cloning: `object.clone(recursive)` uses `copy()` which creates new Object3D instances but keeps references to shared geometry/material unless explicitly cloned. `(threejs:src/core/Object3D.js:L1571-L1633)`

### 4.3 Shared vs per-instance split

Representative split for `Mesh`:

```javascript
// Per-instance (in Object3D and Mesh)
this.position, this.rotation, this.scale  // transform
this.visible, this.castShadow, this.receiveShadow  // visibility
this.matrix, this.matrixWorld  // computed transforms

// Shared (referenced)
this.geometry  // BufferGeometry
this.material  // Material
```
`(threejs:src/objects/Mesh.js:L47-L75)` and `(threejs:src/core/Object3D.js:L160-L378)`

### 4.4 Nested instantiation

Yes, scenes can contain scenes (or any Object3D hierarchy). `ObjectLoader` recursively parses `children` arrays with no explicit depth limit. `(threejs:src/loaders/ObjectLoader.js:L590-L599)`

**N/A** — No recursion limits enforced by the loader.

---

## Section 5: Change tracking and editor / wire sync

**N/A — Entire Section**

Three.js is a single-process, single-threaded browser library with no editor/server split. There is no change tracking system, no dirty flags for properties, no wire protocol, and no reconciliation.

Editors (like the Three.js Editor) implement their own ad-hoc change detection by wrapping Three.js objects or maintaining parallel state. This is not part of the core framework.

---

## Section 6: Property reflection

### 6.1 Property enumeration

**N/A** — No formal property enumeration API. Material has `setValues()` which iterates over keys in a provided object and sets matching properties:

```javascript
for ( const key in values ) {
    const newValue = values[ key ];
    const currentValue = this[ key ];
    // ... set if property exists
}
```
`(threejs:src/materials/Material.js:L555-L597)`

This is a simple key-matching approach, not true reflection.

### 6.2 Static vs dynamic property types

**Static** — Property types are defined by the class constructor and prototype. No runtime type introspection beyond JavaScript's `typeof` and `instanceof` checks. The `type` string property indicates class but not property schemas.

### 6.3 Get/set by string name

**Ad-hoc** — `Material.setValues()` allows setting by string key. Getting uses normal property access. There's no centralized property dispatch system.

`(threejs:src/materials/Material.js:L555-L597)`

### 6.4 Authority checking

**Minimal** — `setValues()` warns if a key doesn't exist on the material. No type checking beyond JavaScript's implicit conversions. No read-only property enforcement.

`(threejs:src/materials/Material.js:L573-L576)`

---

## Section 7: Composition — dynamic children

### 7.1 Runtime child changes

Yes, children can be added/removed at any time:

```javascript
parent.add(child)      // adds to children array, fires events
parent.remove(child)   // removes from array by indexOf, fires events
parent.clear()         // removes all children
```
`(threejs:src/core/Object3D.js:L746-L787)` and `(threejs:src/core/Object3D.js:L798-L863)`

### 7.2 Structural ownership vs external composition

Three.js supports both patterns:

- **Structural ownership**: `Group` is a container that owns children structurally. Any Object3D can have children. `(threejs:src/objects/Group.js)`

- **External composition**: `Mesh` references `geometry` and `material` by direct reference, not by being "parent" in the scene graph. These are property-level compositions.

### 7.3 "Property value is itself a node"

**N/A** — Three.js does not have a slot/parameter system where a property value dynamically becomes a child node. Relationships are explicit:
- Scene graph: via `parent`/`children`
- Data dependencies: via direct reference properties

No automatic child creation from parameter values.

### 7.4 Child type constraints

**No constraints enforced** — `add()` only checks `object.isObject3D` truthiness. `(threejs:src/core/Object3D.js:L767-L783)`

Any Object3D subclass can be added as a child to any other Object3D. Type safety is by convention, not framework enforcement.

---

## Section 8: Inter-node dependencies and execution ordering

### 8.1 "Node A reads node B's output"

**N/A** — Three.js has no dataflow concept of "outputs". Renderable objects (Mesh, etc.) are rendered by the renderer reading their properties (geometry, material, transform). There are no explicit "port" connections.

The closest analog is material texture references: a material references textures via properties like `map`, `normalMap`, etc. These are direct references set at configuration time.

### 8.2 Execution order determination

**Fixed tree traversal** for matrix updates: `updateMatrixWorld()` parent-first. `(threejs:src/core/Object3D.js:L1165-L1203)`

**Renderer-determined order** for rendering: The renderer builds a render list based on scene graph traversal plus material/visibility sorting, not a dataflow graph.

### 8.3 Push vs pull evaluation

**Pull** — The renderer pulls data from objects during render passes:
- Reads `object.matrixWorld` (which was computed by prior `updateMatrixWorld` call)
- Reads `geometry.attributes`, `material` properties
- No "push" of computed data between nodes

### 8.4 Cycle detection

**N/A** — No dataflow graph means no cycles to detect. Parent-child cycles are prevented by `add()` rejecting self-parenting: `if ( object === this ) error(...)`. `(threejs:src/core/Object3D.js:L760-L764)`

### 8.5 Dependency graph caching

**N/A** — No dependency graph exists. Matrix world updates are lazy: `matrixWorldNeedsUpdate` flag propagates down the tree. `(threejs:src/core/Object3D.js:L274)` and `(threejs:src/core/Object3D.js:L1169-L1187)`

### 8.6 Cross-tree dependencies

**N/A** — No formal dependency system. The only cross-tree references are direct property references (e.g., a material shared by multiple meshes in different branches). No "bus" or message system between arbitrary nodes.

---

## Section 9: Node state, errors, and logging

### 9.1 Operational state value

**N/A** — No explicit operational state enum. State is implicit:
- Object exists and is in tree → active
- `visible === false` → hidden but still "live"
- `parent === null` → detached from tree

Some subsystems have internal state (texture upload state, geometry GPU upload state) but these are private renderer internals.

### 9.2 Error categories

**N/A** — No typed error hierarchy. Errors are handled via:
- `console.error()` via `error()` utility
- `console.warn()` via `warn()` utility
- Exceptions thrown for programmer errors (e.g., adding self as child)

The `error()` and `warn()` functions are simple wrappers around `console.*`. `(threejs:src/utils.js)`

### 9.3 Load-time errors

Missing/malformed assets in ObjectLoader:
1. JSON parse error → caught, passed to `onError` callback `(threejs:src/loaders/ObjectLoader.js:L120-L133)`
2. Missing metadata/type → early return with error message `(threejs:src/loaders/ObjectLoader.js:L136-L145)`
3. Unknown type during parse → silently skipped (no `else` case for unknown types in parse) `(threejs:src/loaders/ObjectLoader.js:L210-L600)`

The partially loaded object graph may be incomplete; the application must handle null checks.

### 9.4 Runtime errors

**N/A** — No framework-level runtime error handling. Allocation failures (OOM) throw standard JS exceptions. Division by zero produces `Infinity` per IEEE 754. The animation loop catches and logs errors but doesn't isolate failing objects:

```javascript
try {
    render();
} catch (e) {
    console.error(e);
}
```

This is in user code, not the framework.

### 9.5 Error storage

**N/A** — Errors are logged to browser console immediately. No in-memory storage, no ring buffer, no attachment to nodes.

### 9.6 Error logging mechanism

Browser `console.error()` / `console.warn()` via utility functions. No filtering, no querying, no severity levels beyond browser's console filtering.

`(threejs:src/utils.js)`

### 9.7 Warning severity

**N/A** — No severity enum. `warn()` vs `error()` functions distinguish informational warnings from errors, but there's no structured severity system.

### 9.8 Error state behavior and recovery

**N/A** — No explicit error state on nodes. If loading fails, the application must:
1. Detect failure via `onError` callback
2. Decide to retry or skip
3. Manually retry by calling loader again

No automatic recovery or placeholder rendering for failed loads.

---

## Section 10: Schema versioning and evolution

### 10.1 Schema version in saved files

Yes — metadata version at the root level:

```javascript
output.metadata = {
    version: 4.7,
    type: 'Object',
    generator: 'Object3D.toJSON'
};
```
`(threejs:src/core/Object3D.js:L1287-L1291)`

Same for Materials:
```javascript
data.metadata = {
    version: 4.7,
    type: 'Material',
    generator: 'Material.toJSON'
};
```
`(threejs:src/materials/Material.js:L622-L626)`

### 10.2 Migration path

**N/A** — No migration system. ObjectLoader does not check version or migrate data. The version is informational only.

### 10.3 Forward compatibility

**Ignore unknown fields** — JavaScript objects naturally ignore unknown properties during deserialization. ObjectLoader only reads expected keys; extra fields are discarded.

### 10.4 Backward compatibility

**Manual** — Older files can be loaded if they match the expected schema. If a property was removed, the loader may expect it and fail (undefined checks needed). No automatic backward compatibility layer.

### 10.5 Deprecated field handling

**N/A** — No deprecation system. Removed fields are simply not read. Some legacy properties may still be checked in loaders for compatibility (e.g., `geometry` vs `geometry.uuid` references), but this is ad-hoc per loader, not a systematic approach.

---

## Summary of N/A Sections

Three.js is intentionally minimal compared to other surveyed systems. The following major areas are **N/A** (by design, not by omission):

| Section | N/A Count | Notes |
|---------|-----------|-------|
| 1 (Lifecycle) | 2/6 | No staged hooks, no deferred ops |
| 3 (Resource refcount) | 5/5 | Entirely N/A — manual dispose pattern |
| 5 (Change tracking) | 6/6 | Entirely N/A — single-process |
| 7.3 (Param-as-child) | 1/1 | N/A — no slot system |
| 8 (Dependencies) | 6/6 | Entirely N/A — no dataflow graph |
| 9 (Errors/state) | 7/8 | Almost entirely N/A — minimal error handling |
| 10.2-10.5 (Migration) | 4/5 | Version present but unused |

Three.js's value for this survey is as a **counter-example**: it shows how a scene graph can function without heavy framework machinery (refcounting, lifecycle hooks, error states, dataflow). The tradeoff is that applications must manage resource disposal, error handling, and cross-process sync manually.

---

*Survey completed for Three.js (token: `threejs`)*
