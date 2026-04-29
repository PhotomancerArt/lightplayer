# Prior Art Answers: Bevy

Reference codebase: Bevy Engine (ECS-based game engine)
Codebase root: `/Users/yona/dev/photomancer/prior-art/bevy/`

---

## Section 1: Node lifecycle

Bevy does not have "nodes" in the traditional sense; the closest analog is the **entity/component** system. Entities are lightweight identifiers, while components hold data. Lifecycle hooks exist at the component level via the `Component` trait and `ComponentHooks`.

### 1.1 Named lifecycle hooks

Bevy defines five component lifecycle hooks via `ComponentHooks` in `bevy_ecs::lifecycle`:

1. **`on_add`** — Triggered when a component is added to an entity that didn't already have it. Runs before `on_insert` (bevy:crates/bevy_ecs/src/lifecycle.rs:L148-L150)
2. **`on_insert`** — Triggered when a component is inserted, regardless of whether the entity already had it. Runs after `on_add` if applicable (bevy:crates/bevy_ecs/src/lifecycle.rs:L152-L154)
3. **`on_discard`** — Triggered when a component is about to be dropped (replaced or removed). Runs before `on_remove` (bevy:crates/bevy_ecs/src/lifecycle.rs:L156-L158)
4. **`on_remove`** — Triggered when a component is removed from an entity and not replaced (bevy:crates/bevy_ecs/src/lifecycle.rs:L160-L162)
5. **`on_despawn`** — Triggered for each component on an entity when it is despawned (bevy:crates/bevy_ecs/src/lifecycle.rs:L164-L166)

Additionally, Bevy provides observer-based events: `Add`, `Insert`, `Discard`, `Remove`, and `Despawn` as `EntityEvent` types that can be observed (bevy:crates/bevy_ecs/src/lifecycle.rs:L336-L394).

### 1.2 Enter-phase hook order

When a new entity is spawned with components, the order is: `on_add` → `on_insert` for each component. There is no global "entity enter tree" hook; each component's hooks fire independently. The hooks are invoked synchronously during the spawn/insert call via `ComponentHooks::update_from_component` (bevy:crates/bevy_ecs/src/lifecycle.rs:L157-L176).

### 1.3 Per-frame hooks and traversal order

Bevy does not have per-frame hooks on individual components. Instead, systems run on a schedule. The most common schedule is `Update`, which runs every frame. Systems within a schedule run in a deterministic order derived from a dependency graph (DAG). The order is not parent-first or leaf-first but determined by system dependencies (`.before()`, `.after()`, `.in_set()`). The scheduler computes a topological sort at build time (bevy:crates/bevy_ecs/src/schedule/schedule.rs:L1-L300).

### 1.4 Teardown traversal order

During teardown (entity despawn), the `on_despawn` hook fires for each component on the entity. Bevy does not have an explicit parent-first or child-first traversal for hooks. However, the `Children` relationship has `LINKED_SPAWN = true`, which means despawning a parent automatically despawns all children (and recursively their descendants) via the `RelationshipTarget::on_despawn` hook (bevy:crates/bevy_ecs/src/relationship/mod.rs:L317-L323). This is effectively parent-first in that the parent's despawn triggers the children's despawn.

### 1.5 Eager vs deferred lifecycle calls

Lifecycle hooks are **eager** (synchronous). They run immediately during the triggering call (`insert`, `remove`, `despawn`). However, when using `Commands` (deferred command buffers), the actual spawn/insert/remove/despawn operations are deferred until `apply_deferred` is called, typically at the end of the stage or when `flush` is called explicitly (bevy:crates/bevy_ecs/src/system/commands/mod.rs).

There is no `queue_free`-style deferred destruction for entities directly on the `World`; `Commands::despawn` queues the despawn for the next command buffer flush.

### 1.6 Dispatcher behavior on hook panic/error

Bevy's ECS is single-threaded by default (though it supports multithreaded schedulers). If a hook panics, the panic propagates up. There is no isolation of offending entities; a panic in a hook will likely crash the app or be caught by a higher-level panic handler. The `App` does not have a per-hook error boundary. Systems that fail return `Result` types, but component hooks are `fn` pointers that are expected not to panic.

---

## Section 2: Node identity and addressing

### 2.1 Runtime node identifier

Entities are identified by `Entity`, a 64-bit generational index. It consists of:
- `EntityIndex` (32-bit): The index in the entity array
- `EntityGeneration` (32-bit): Generation counter to detect use-after-free

Entity IDs are minted by `EntityAllocator::alloc()` which recycles freed indices with incremented generations (bevy:crates/bevy_ecs/src/entity/mod.rs:L424-L433, L700-L796). The `Entity` type is stable for the entity's lifetime and guaranteed unique (until generation wrap, which is extremely unlikely).

### 2.2 ID stability across save/load

**N/A** — `Entity` IDs are explicitly **not stable** across sessions. The documentation states: "directly serializing with `Serialize` and `Deserialize` make zero guarantee of long term wire format compatibility" (bevy:crates/bevy_ecs/src/entity/mod.rs:L356-L364). For persistence, Bevy recommends inserting a secondary identifier as a component (e.g., a UUID or `Name` component) rather than relying on `Entity` IDs.

### 2.3 Path syntax for addressing

Bevy does not have a built-in path syntax for addressing entities. For reflection on component data, Bevy uses "reflect paths" with syntax like:
- `.field_name` — Access struct field by name
- `#0` — Access struct field by index
- `.0` — Access tuple field
- `[0]` — Access list/array element

Examples: `".transform.translation.x"`, `"#0#1"` (bevy:crates/bevy_reflect/src/path/mod.rs:L86-L200)

For entity relationships, you traverse via queries (`Query<&Children>`) or use the `Name` component to look up entities, but there is no string path syntax like `"/Player/Weapon"`.

### 2.4 Path grammar features

The reflect path grammar (for component data, not entity hierarchy) supports:
- Relative paths (field access)
- No absolute paths (no root specifier)
- No wildcards
- Indexed segments via `[index]` for lists/arrays
- Field index access via `#index` for structs

No glob or range segments are supported for entity addressing (bevy:crates/bevy_reflect/src/path/mod.rs).

### 2.5 Id-based vs specifier-based references

Bevy uses **id-based references** exclusively. The `Entity` type is a direct index. There is no built-in query-based reference system. Users typically store `Entity` IDs in components (e.g., `ChildOf(Entity)`) or use the `Name` component with queries to find entities:

```rust
fn find_player(query: Query<Entity, With<Player>>) { ... }
```

The `Relationship` trait provides typed entity-to-entity references (bevy:crates/bevy_ecs/src/relationship/mod.rs:L96-L119).

### 2.6 Name uniqueness enforcement

The `Name` component (bevy:crates/bevy_ecs/src/name.rs) holds a string name but does **not** enforce uniqueness. Two siblings can share a name. There is no built-in disambiguation; queries by name return the first match or require manual filtering. The `bsn!` macro supports named entities (`#Name`) but names are for lookup convenience, not uniqueness.

### 2.7 Lookup complexity

- **Entity ID lookup**: O(1) via `World::get_entity` using generational index check (bevy:crates/bevy_ecs/src/entity/mod.rs:L846-L862)
- **Component query**: O(n) over entities with that component archetype
- **Name-based lookup**: O(n) scan required (no index by default)
- **Relationship traversal**: O(1) to get the `Children` or `ChildOf` component, then O(m) to iterate m children

---

## Section 3: Resource refcount / asset management

### 3.1 Refcounted asset type

Assets are stored in the `Assets<A>` resource, a collection parameterized by asset type `A: Asset`. The asset data itself is stored in dense storage (`DenseAssetStorage`) indexed by `AssetIndex` (generational index) or in a `HashMap<Uuid, A>` for UUID-addressed assets (bevy:crates/bevy_asset/src/assets.rs:L114-L296).

### 3.2 Handle type and refcount mechanism

The `Handle<A>` enum provides two variants:
- **`Handle::Strong(Arc<StrongHandle>)`** — Reference-counted via `Arc`. When the last strong handle is dropped, a `DropEvent` is sent via channel to the asset system. The asset is then freed when `Assets::track_assets` processes the drop event.
- **`Handle::Uuid(Uuid)`** — Weak reference, does not keep the asset alive.

Refcount is **automatic** via `Arc` and `Drop`. No explicit `addref`/`release` calls are needed (bevy:crates/bevy_asset/src/handle.rs:L117-L211, L96-L103).

### 3.3 Loading pattern

Loading is **async lazy**. `AssetServer::load(path)` returns a `Handle` immediately and queues the load. The asset is not available until it is fully loaded. The `AssetEvent::LoadedWithDependencies` event is emitted when ready (bevy:crates/bevy_asset/src/handle.rs:L211-L236, bevy:crates/bevy_asset/src/event.rs:L47-L89).

For inline values, `Assets::add(asset)` provides synchronous strong-handle creation (bevy:crates/bevy_asset/src/assets.rs:L399-L405).

### 3.4 Unloading/eviction trigger

Assets are unloaded when:
1. The last strong handle is dropped (refcount reaches zero)
2. The `DropEvent` is processed by `Assets::track_assets`
3. The asset entry is removed from storage

There is no explicit "shed" call. The `AssetIndexAllocator` recycles indices with incremented generations when assets are dropped (bevy:crates/bevy_asset/src/handle.rs:L96-L103, bevy:crates/bevy_asset/src/assets.rs:L495-L515).

### 3.5 Hot reload support

**Yes**, hot reload is supported via `AssetEvent`:
- `AssetEvent::Modified` — Emitted when an asset is modified on disk and reloaded
- `AssetEvent::Added` — New asset
- `AssetEvent::Removed` — Asset removed

Systems can listen to these events via `EventReader<AssetEvent<T>>` and react accordingly. Handle dereferences remain valid; the underlying asset data is updated in place. Strong handles automatically keep the asset alive during reload (bevy:crates/bevy_asset/src/event.rs:L47-L89).

---

## Section 4: Scene / patch / instance instantiation

### 4.1 Instantiation pipeline

Bevy's scene system works as follows:
1. A `Scene` trait describes what an entity should look like (components, templates)
2. `Scene::resolve()` converts a `Scene` into a `ResolvedScene` (a collection of templates)
3. `World::spawn_scene(scene)` resolves dependencies, then spawns an entity and applies the scene
4. Templates are built via `Template::build_template()` to create components
5. Components are inserted on the spawned entity

Entry points: `WorldSceneExt::spawn_scene` (bevy:crates/bevy_scene/src/spawn.rs:L55-L191), `Scene::resolve` (bevy:crates/bevy_scene/src/scene.rs:L50-L68).

### 4.2 Multiple instantiation

Yes, the same scene can be instantiated multiple times. Each `spawn_scene` call creates a new entity with independent components. Shared data is achieved by storing `Handle<T>` (refcounted) in components; the scene/template defines the structure but asset handles share the underlying data (bevy:crates/bevy_scene/src/spawn.rs:L186-L218).

### 4.3 Shared vs per-instance pieces

- **Shared**: Asset data referenced by `Handle<T>` (e.g., mesh data, textures, materials)
- **Per-instance**: Component data stored directly on the entity (e.g., `Transform`, custom game components)

When a scene is spawned, `Handle` fields are cloned (incrementing refcount), while other components are newly constructed (bevy:crates/bevy_asset/src/handle.rs:L143-L150).

### 4.4 Nested instantiation

Yes, scenes support nesting via `RelatedScenes<R, L>` which spawns child entities with a relationship (e.g., `Children`). The `bsn!` macro supports `Children [ ... ]` syntax for nested entities. Depth is limited only by stack/heap memory; there is no explicit depth limit enforced (bevy:crates/bevy_scene/src/scene.rs:L333-L368, bevy:crates/bevy_ecs/src/hierarchy.rs:L93-L107).

---

## Section 5: Change tracking and editor / wire sync

**N/A** — Bevy is primarily a single-process game engine. There is no built-in client/server split or wire protocol for editor sync. However, the underlying change detection mechanisms could support such a system:

### 5.1 Property change tracking

Bevy tracks changes via:
- `Changed<T>` query filter — detects component changes since last system run
- `Added<T>` query filter — detects newly added components
- `AssetEvent<T>` — tracks asset additions/modifications/removals
- `RemovedComponents<T>` — tracks component removals

Change detection uses system tick comparison; each component tracks the last-changed tick (bevy:crates/bevy_ecs/src/query/filter.rs, bevy:crates/bevy_ecs/src/lifecycle.rs:L396-L616).

### 5.2 Wire serialization format

**N/A** — No built-in wire sync format. Bevy uses serde-compatible serialization for scenes (e.g., `.bsn` files which are Rust-like syntax, or standard formats via `bevy_reflect`).

### 5.3 Change tracking opt-in vs automatic

Change detection is **automatic** for all components when queried with `Changed`/`Added` filters. However, actually reacting to changes requires opt-in by using these filters in systems.

### 5.4 Save format vs sync format relationship

**N/A** — No sync format exists. Save format is scene-based using `Scene`/`ScenePatch` with BSN (Bevy Scene Notation) or custom serializers via `bevy_reflect`.

### 5.5 Changes during render/process

Systems run in stages; changes made during a system will be visible to subsequent systems in the same frame. There is no "mutation during iteration" protection at the ECS level. Observers (`On<E>`) can trigger immediate reactions to changes (bevy:crates/bevy_ecs/src/observer/mod.rs).

### 5.6 Client/server reconciliation

**N/A** — Bevy does not have built-in client/server reconciliation. For multiplayer, users typically integrate third-party networking crates or implement custom prediction/reconciliation.

---

## Section 6: Property reflection

### 6.1 Runtime property enumeration

The `bevy_reflect` crate provides `Reflect` and `PartialReflect` traits for runtime introspection. Types derive `#[derive(Reflect)]` to enable:
- `reflect_ref()` — Get kind of type (Struct, Tuple, Enum, etc.)
- `as_struct()`/`as_tuple()` — Cast to subtrait for field access
- `field()`/`field_mut()` — Access fields by name or index
- `iter_fields()` — Iterate over all fields

The `TypeRegistry` stores type information for all registered types (bevy:crates/bevy_reflect/src/reflect.rs:L101-L200, bevy:crates/bevy_reflect/src/type_registry.rs).

### 6.2 Static vs dynamic type info

**Hybrid approach**:
- Static: The `Reflect` derive generates compile-time type info via `TypeInfo`
- Dynamic: `DynamicStruct`, `DynamicList`, etc. allow runtime construction of type-erased values
- The `TypeRegistry` resolves `TypeId` to `TypeInfo` at runtime for dynamic dispatch

The `Typed` trait provides static type info: `fn type_info() -> &'static TypeInfo` (bevy:crates/bevy_reflect/src/reflect.rs:L400-L450).

### 6.3 Property get/set by string name

Yes, via `GetPath` trait on `dyn Reflect`:
```rust
let value = my_struct.path::<f32>(".position.x").unwrap();
```

Also available via `Struct::field("name")` which returns `Option<&dyn Reflect>` (bevy:crates/bevy_reflect/src/path/mod.rs:L86-L200).

### 6.4 Authority checking for edits

The `ApplyError` enum provides typed errors for invalid operations:
- `MismatchedKinds` — Applying struct to enum, etc.
- `MismatchedTypes` — Type incompatibility
- `MissingEnumField` — Missing variant field
- `UnknownVariant` — Invalid enum variant

Property edits are type-checked at runtime via reflection. There is no separate "read-only" marker; immutability is enforced by ownership (`&` vs `&mut`) (bevy:crates/bevy_reflect/src/reflect.rs:L20-L69).

---

## Section 7: Composition — dynamic children

### 7.1 Runtime children modification

Yes, children can be added/removed at runtime via:
- `EntityWorldMut::add_child(entity)` — Add existing entity as child
- `EntityWorldMut::with_children(|p| { p.spawn(...); })` — Spawn new children
- `EntityWorldMut::detach_child(entity)` — Remove child relationship (does not despawn)
- `EntityWorldMut::despawn_children()` — Despawn all children recursively
- `EntityWorldMut::replace_children(&[entities])` — Replace entire child list

Children modification is immediate and reflected in the `Children` component via relationship hooks (bevy:crates/bevy_ecs/src/hierarchy.rs:L269-L378).

### 7.2 Structural vs external composition

Bevy uses a **structural** approach. The `ChildOf` relationship component is stored on the child and points to the parent. The `Children` component is stored on the parent and contains a `Vec<Entity>` of children. This bidirectional relationship is maintained automatically by component hooks. There is no "external configuration" concept; children are always entities in the world with relationship components (bevy:crates/bevy_ecs/src/relationship/mod.rs:L28-L39, L96-L200).

### 7.3 "Property value becomes a node"

**N/A** — Bevy does not have a concept where a property value becomes an entity/node. However, scenes/templates achieve similar composition:
- `Handle<T>` fields in components reference assets
- `RelatedScenes` spawn child entities from scene definitions
- The `bsn!` macro allows defining child entities inline

A `Gradient` component would typically hold a `Handle<GradientAsset>` rather than becoming a child entity.

### 7.4 Constraint model for children

Any entity can have any other entity as a child, with these constraints enforced:
- No self-parenting (inserting `ChildOf(self)` auto-removes) (bevy:crates/bevy_ecs/src/relationship/mod.rs:L154-L162)
- Target entity must exist (inserting `ChildOf(missing)` auto-removes) (bevy:crates/bevy_ecs/src/relationship/mod.rs:L191-L199)
- Only one parent per child (one-to-many relationship); adding a new `ChildOf` removes the old relationship

No type restrictions exist on what components a child can have.

---

## Section 8: Inter-node dependencies and execution ordering

### 8.1 "Node A reads node B's output" expression

In Bevy, this is expressed via:
- **Queries** — Systems declare what components they read/write via `Query<&A, &mut B>`
- **System ordering** — `.before()`/`.after()` constraints ensure A runs after B
- **Events** — B can emit an event that A observes
- **Resources** — Shared state via `Res<T>` / `ResMut<T>`
- **Relationship references** — Components can store `Entity` IDs of other entities

There is no explicit "port/cable" model; data flow is implicit in system queries and ordering (bevy:crates/bevy_ecs/src/schedule/schedule.rs:L200-L300).

### 8.2 Execution order determination

Systems are organized into a **directed acyclic graph (DAG)** based on:
- Explicit ordering constraints (`.before()`, `.after()`)
- System sets (`.in_set()`)
- Ambiguity detection (conflicting access without ordering is reported)

The schedule computes a topological sort at build time. Systems run in the determined order every frame. The executor can be single-threaded or multi-threaded (work-stealing) (bevy:crates/bevy_ecs/src/schedule/schedule.rs:L280-L500).

### 8.3 Push vs pull evaluation

**Pull evaluation**. Systems declare what data they need via queries, and the ECS scheduler ensures they run when dependencies are ready. Systems "pull" data from components/resources when they run. There is no "push" model where one system directly calls another; all inter-system communication is via:
- Shared component/resource access
- Events
- Commands (deferred structural changes)

### 8.4 Cycle detection/prevention

Cycles are detected at **schedule build time** via the DAG validation. A cycle results in a `ScheduleBuildError` panic. The topological sort fails if cycles exist. Users must fix the `.before()`/`.after()` constraints (bevy:crates/bevy_ecs/src/schedule/graph/dag.rs).

### 8.5 Dependency graph caching

The dependency graph is **cached and rebuilt only when systems are added/removed**. The `Schedule::initialize` method computes the system order, and subsequent runs use the cached execution plan. Systems and their dependencies are assumed static between rebuilds (bevy:crates/bevy_ecs/src/schedule/schedule.rs:L400-L600).

### 8.6 Cross-tree dependencies

Any system can access any component/resource, so "cross-tree" dependencies are allowed. The `Traversal` trait provides generalized path-following for event propagation through relationships. Systems can query for entities anywhere in the hierarchy via `Query` with `With<ChildOf>` or similar filters (bevy:crates/bevy_ecs/src/traversal.rs:L1-L52).

---

## Section 9: Node state, errors, and logging

### 9.1 Explicit operational state value

**N/A** — Entities do not carry an explicit operational state enum. State is implicit:
- "Loading" — Asset handles not yet loaded (detect via `AssetEvent`)
- "Ready" — Entity exists with all expected components
- "Error" — No built-in error state; users add custom components like `enum Status { Loading, Error(ErrorKind) }`

The `AssetEvent::LoadFailed` and `AssetLoadFailedEvent` provide asset-level error signaling (bevy:crates/bevy_asset/src/event.rs:L1-L45).

### 9.2 Error categories

Errors are categorized by type:
- `AssetLoadError` — Failed to load asset (file not found, parse error, etc.)
- `InvalidEntityError` — Entity ID generation mismatch (use-after-free)
- `EntityNotSpawnedError` — Entity valid but not yet spawned
- `SpawnSceneError` — Scene dependency missing or failed to resolve
- `ApplyError` — Reflection apply failed (type mismatch, etc.)

There is no unified error hierarchy; each subsystem has its own error types implementing `thiserror::Error` (bevy:crates/bevy_ecs/src/entity/mod.rs:L1111-L1178, bevy:crates/bevy_scene/src/scene.rs:L157-L170).

### 9.3 Load-time errors

When an asset is missing or malformed:
- `AssetLoadFailedEvent` is emitted
- Strong handles remain valid but dereference to `None` (via `Assets::get`)
- Entities with failed asset dependencies typically remain in the world (the component holds a handle to a non-existent asset)

Scene loading can fail with `ResolveSceneError::MissingSceneDependency` if dependencies aren't loaded (bevy:crates/bevy_scene/src/scene.rs:L157-L170, bevy:crates/bevy_scene/src/spawn.rs:L589-L652).

### 9.4 Runtime errors

**N/A** — Bevy does not have a unified runtime error handler for systems:
- Allocation failures panic (typical Rust behavior)
- System panics propagate up
- No automatic entity removal on panic

Systems can return `Result` and handle errors locally. The `bevy_ecs::error` module provides `Result` types but no automatic recovery.

### 9.5 Error storage format

Errors are **not stored on entities**. Asset loading errors are emitted as events (`AssetLoadFailedEvent`) and optionally logged. The format includes:
- `id: AssetId<A>` — Asset identifier
- `path: AssetPath` — Attempted path
- `error: AssetLoadError` — Structured error with source chain (bevy:crates/bevy_asset/src/event.rs:L1-L45)

### 9.6 Error logging mechanism

Bevy uses the standard Rust `log` crate and `tracing` for structured logging:
- `info!`, `warn!`, `error!` macros
- Logs go to stdout/stderr or configurable subscribers
- No built-in in-memory ring buffer for logs
- Filtering by severity/target via `RUST_LOG` env var

For editor integration, users would implement a custom `tracing_subscriber` to send logs over a wire protocol (bevy:crates/bevy_log/src/lib.rs).

### 9.7 Warnings vs errors

Warnings use the same logging infrastructure (`log::warn!`). There is no separate warning tracking; they are log messages with severity levels.

### 9.8 Behavior in error state and recovery

**N/A** — No built-in error state behavior. Typical patterns:
- Failed asset load: Entity continues with default/placeholder, or despawns itself in a system listening to `AssetLoadFailedEvent`
- Recovery: Re-queue the asset load, or reload from a fixed file (detected via `AssetEvent::Modified`)

---

## Section 10: Schema versioning and evolution

### 10.1 Schema version in saved files

**N/A** — Bevy's scene format (BSN - Bevy Scene Notation) does not embed explicit schema versions. Scenes are Rust-like syntax that directly describes components. Type information comes from the `TypeRegistry` at runtime.

For asset formats (GLTF, images, etc.), version handling is format-specific and handled by the respective loader.

### 10.2 Migration path

**N/A** — No built-in migration system. Users typically:
- Handle version detection in custom asset loaders
- Use conditional logic in `FromTemplate` implementations
- Maintain backward compatibility in component definitions (add optional fields)

The `ApplyError` from reflection can be used to detect incompatibilities during scene patching (bevy:crates/bevy_reflect/src/reflect.rs:L20-L69).

### 10.3 Forward compatibility

**N/A** — No explicit forward compatibility. Unknown fields in BSN scenes will cause parse errors. For custom formats, `#[reflect(ignore)]` can skip unknown fields if using reflection-based serialization.

### 10.4 Backward compatibility

**Partial support** via reflection:
- Adding new optional fields to components with `Default` values works (old scenes load, new field gets default)
- Removing fields requires keeping them as `#[reflect(ignore)]` or handling in `FromTemplate`
- Renaming requires maintaining aliases or custom deserialization

### 10.5 Deprecated/removed field handling

**N/A** — No built-in deprecation system. Options:
- Keep fields as `#[reflect(ignore)]` to allow loading old data
- Use custom `Deserialize` implementations to handle old field names
- Fail fast with parsing errors for unknown fields (default BSN behavior)

The `bevy_scene` system uses reflection for serialization; unknown fields typically fail to deserialize unless marked with `#[reflect(ignore)]` (bevy:crates/bevy_scene/src/scene.rs).

---

## Summary: Bevy's ECS Shape vs Node-Tree Questions

Bevy is an **ECS** (Entity-Component-System) engine, not a node-tree engine. Key mapping differences:

| Lightplayer Concept | Bevy Analog | Notes |
|---------------------|-------------|-------|
| Node | Entity + Components | Entities are just IDs; behavior comes from components |
| Node lifecycle | Component hooks (on_add, on_insert, on_remove, on_despawn) | No global entity hooks |
| Node tree | `ChildOf`/`Children` relationship | Bidirectional, maintained by hooks |
| Artifact | `Asset<T>` + `Handle<T>` | Refcounted, hot-reloadable |
| Property | Component field | Accessed via reflection or direct struct access |
| Path addressing | Queries (`Query<&Children>`), `Name` lookup | No string path syntax for entities |
| Inter-node data flow | System queries + ordering + Events | No explicit ports/cables |
| Per-frame update | Systems in Schedules | Not per-node hooks |
| Error state | User-defined components | No built-in Status enum |
| Client/server sync | N/A | Single-process design |

**Most useful areas for Lightplayer:**
- `bevy_asset` handle/refcount/hot-reload design (Section 3)
- `Relationship`/`RelationshipTarget` trait system for hierarchy (Section 7)
- `ComponentHooks` lifecycle system (Section 1)
- `bevy_reflect` for property introspection (Section 6)
- Scene/template system for instantiation (Section 4)

**Least applicable areas:**
- Editor/client sync (Section 5) — Bevy is single-process
- Node operational state (Section 9) — No built-in status system
- Schema versioning (Section 10) — Minimal built-in support
