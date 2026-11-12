# VCV Rack Prior-Art Survey (Pass 1)

Reference token: `vcv`
Codebase: `/Users/yona/dev/photomancer/prior-art/VCVRack/`

---

## Section 1: Node lifecycle

### 1.1 Lifecycle hooks

VCV Rack's `Module` class (the node equivalent) defines these primary lifecycle hooks:

- **Enter phase:** `onAdd(const AddEvent& e)` — called after module is added to the Engine `(vcv:include/engine/Module.hpp:L376)`. `onSampleRateChange(const SampleRateChangeEvent& e)` — also fired on add `(vcv:include/engine/Module.hpp:L417)`.

- **Live phase:** `process(const ProcessArgs& args)` — per-sample DSP callback `(vcv:include/engine/Module.hpp:L331)`. `onBypass` / `onUnBypass` — when module is enabled/disabled `(vcv:include/engine/Module.hpp:L392-L397)`.

- **Leave phase:** `onRemove(const RemoveEvent& e)` — called before module is removed from Engine `(vcv:include/engine/Module.hpp:L384)`. `onSave(const SaveEvent& e)` — called when patch is saved `(vcv:include/engine/Module.hpp:L446)`.

Additional hooks: `onReset`, `onRandomize`, `onPortChange`, `onExpanderChange`, `onSetMaster`/`onUnsetMaster` `(vcv:include/engine/Module.hpp:L434-L452)`.

### 1.2 Enter-phase hook order

The dispatcher `Engine::addModule_NoLock` invokes hooks in this order: `(vcv:src/engine/Engine.cpp:L762-L788)`

1. Assign/generate `id` if unset (random 53-bit)
2. Add to internal vectors/cache
3. `onAdd(eAdd)` — AddEvent
4. `onSampleRateChange(eSrc)` — SampleRateChangeEvent (always fired on add)
5. Update ParamHandle module pointers

### 1.3 Per-frame hooks and tree order

VCV Rack is **flat**, not a tree. All modules live in a single vector `internal->modules` `(vcv:src/engine/Engine.cpp:L210)`. Per-frame execution order: `(vcv:src/engine/Engine.cpp:L552-L599)`

1. `stepBlock(frames)` acquires lock
2. For each frame: `Engine_stepFrame()` `(vcv:src/engine/Engine.cpp:L428-L473)`
   - Param smoothing (if active)
   - **Module processing:** First-come-first-serve work-stealing across worker threads `(vcv:src/engine/Engine.cpp:L339-L348)`
   - Cable routing (copies output voltages to inputs) `(vcv:src/engine/Engine.cpp:L352-L423)`
   - Expander message buffer flip

Execution order is **undefined/parallel** — modules within a frame are processed by multiple threads with no guaranteed order. Dataflow is enforced by cable copying happening *after* all modules process.

### 1.4 Teardown order and hooks

`Engine::removeModule_NoLock` performs: `(vcv:src/engine/Engine.cpp:L797-L844)`

1. `onRemove(eRemove)` — RemoveEvent dispatched
2. Update ParamHandles (module pointer cleared)
3. Unset master if this was master module
4. Verify no cables attached (assertion)
5. Clear expander references (this module removed from neighbors' expanders)
6. Erase from cache/vectors

There is no explicit "about to be destroyed" vs "being destroyed now" pair — just `onRemove`. The module instance is deleted by the caller after `removeModule` returns.

### 1.5 Eager vs deferred lifecycle

**Eager/synchronous:** Module add/remove, cable add/remove, and all lifecycle hooks are immediate and synchronous `(vcv:src/engine/Engine.cpp:L756-L789)`. The Engine mutex is held during these operations.

**No deferred mechanism:** There is no `queue_free` equivalent. Modules are removed immediately. The UI layer (RackWidget) manages selection and deletion in its own event handlers `(vcv:src/app/RackWidget.cpp)`.

### 1.6 Dispatcher error handling

VCV Rack uses C++ exceptions for error propagation. The Engine does not have a "hook panic" isolation mechanism — a crash in `process()` or any hook will crash the entire application. There is no per-node error isolation. Debug builds assert on preconditions (e.g., cable still connected on module removal `(vcv:src/engine/Engine.cpp:L819-L822)`).

---

## Section 2: Node identity and addressing

### 2.1 Runtime identifier

`int64_t id` — random 53-bit integer assigned when added to Engine `(vcv:include/engine/Module.hpp:L40)`. Generated in `addModule_NoLock` via `random::u64() % (1ull << 53)` `(vcv:src/engine/Engine.cpp:L768-L770)`. Stable for the module's lifetime in the Engine.

### 2.2 Stable across save/load

**Yes**, module IDs round-trip. `Module::toJson()` serializes `"id"` field `(vcv:src/engine/Module.cpp:L117-L118)`. `Module::fromJson()` restores it: `(vcv:src/engine/Module.cpp:L171-L177)`

```cpp
if (id < 0) {
    json_t* idJ = json_object_get(rootJ, "id");
    if (idJ)
        id = json_integer_value(idJ);
}
```

Legacy migration: pre-1.0 patches used array index as ID `(vcv:src/engine/Engine.cpp:L1309-L1311)`.

### 2.3 Path syntax

**N/A** — VCV Rack has no path grammar. Modules are referenced directly by `int64_t` ID or via pointer. The equivalent of "addressing" is cable connections between ports (identified by `moduleId + portId`).

### 2.4 Path grammar features

**N/A** — No path syntax exists. Modules are flat.

### 2.5 ID-based vs specifier-based references

**ID-based only:** All references use `int64_t moduleId`. Examples:
- `Cable::inputModuleId`, `Cable::outputModuleId` `(vcv:include/engine/Cable.hpp:L16-L19)`
- `Expander::moduleId` `(vcv:include/engine/Module.hpp:L66)`
- `ParamHandle::moduleId` `(vcv:include/engine/ParamHandle.hpp)`

There is no query/specifier system — lookups are by exact ID only.

### 2.6 Name uniqueness enforcement

**N/A** — Modules don't have names as identifiers. Model slugs (plugin+model) must be unique for instantiation `(vcv:include/plugin/Model.hpp:L37)`, but module instances are identified by random IDs, not names. Multiple instances of the same model can exist with identical slugs but different `id` values.

### 2.7 Lookup complexity

- **Module by ID:** O(1) via `std::map<int64_t, Module*> modulesCache` `(vcv:src/engine/Engine.cpp:L217-L218)`. Function: `getModule_NoLock()` `(vcv:src/engine/Engine.cpp:L861-L868)`.
- **Cable by ID:** O(1) via `cablesCache` map `(vcv:src/engine/Engine.cpp:L219)`.
- **ParamHandle by (moduleId, paramId):** O(1) via `paramHandlesCache` `(vcv:src/engine/Engine.cpp:L221)`.
- **Linear scan fallback:** `hasModule()`, `hasCable()` use vector scan O(N) `(vcv:src/engine/Engine.cpp:L847-L852)`.

---

## Section 3: Resource refcount / asset management

### 3.1 Refcounted asset type

`plugin::Model` — the "class-like prototype" for modules. Contains factory methods `createModule()` and `createModuleWidget()` `(vcv:include/plugin/Model.hpp:L34-L69)`. Models are owned by `Plugin` and stored in `Plugin::models` list `(vcv:include/plugin/Plugin.hpp:L21)`.

There is no explicit refcount on Model instances — they are singletons per plugin, destroyed when plugin is unloaded.

### 3.2 Handle type and refcount

**No refcount mechanism** — Models are raw pointers (`Module* model = NULL` in Module `(vcv:include/engine/Module.hpp:L34)`). Modules (instances) are owned by the UI layer (ModuleWidget) and deleted explicitly. The Engine holds non-owning pointers to Modules.

Memory management is manual: `new Module` → add to Engine → remove from Engine → `delete module` `(vcv:src/engine/Engine.cpp:L545-L548)`.

### 3.3 Loading mechanism

**Dynamic library loading at runtime.** Plugins are `.so`/`.dll`/`.dylib` files loaded via `dlopen`/`LoadLibrary` `(vcv:src/plugin.cpp:L60-L94)`. Model lookup: `plugin::modelFromJson()` searches all loaded plugins by `(pluginSlug, modelSlug)` tuple `(vcv:src/plugin.cpp:modelFromJson)`.

Loading is synchronous and triggered by patch deserialization or user browser selection.

### 3.4 Unloading/eviction

**Manual only.** Plugins are never unloaded during normal operation. Patch clear removes module instances but not Models: `clear_NoLock()` deletes modules but the Model (class) remains `(vcv:src/engine/Engine.cpp:L532-L549)`. Application exit destroys all Plugins.

### 3.5 Hot reload

**Limited.** Plugin browser can refresh plugin list. No automatic file watching. Module code changes require plugin rebuild and Rack restart for DSP changes. Patch assets (module data files) can be reloaded via `onSave`/`onAdd` pattern.

---

## Section 4: Scene / patch / instance instantiation

### 4.1 Disk to runtime instantiation

Entry point: `Engine::fromJson()` `(vcv:src/engine/Engine.cpp:L1277-L1365)` or `patch::Manager::load()` `(vcv:src/patch.cpp:L299-L319)`.

Trace from file to ready:
1. Patch archive (.vcv) extracted to autosave directory `(vcv:src/patch.cpp:L311-L316)`
2. `patch.json` parsed `(vcv:src/patch.cpp:L372-L376)`
3. For each module in `"modules"` array:
   - `plugin::modelFromJson()` resolves Model by `(plugin, model)` slugs `(vcv:src/engine/Engine.cpp:L1291-L1298)`
   - `model->createModule()` instantiates Module `(vcv:src/engine/Engine.cpp:L1302)`
   - `module->fromJson()` loads params, data, ID `(vcv:src/engine/Engine.cpp:L1306-L1316)`
   - Modules collected in vector
4. Engine locked, `addModule_NoLock()` called for each `(vcv:src/engine/Engine.cpp:L1325-L1327)`
5. Cables deserialized and added similarly `(vcv:src/engine/Engine.cpp:L1330-L1357)`

### 4.2 Multiple concurrent instances

**Yes.** Same Model can instantiate unlimited Module instances. Each instance is independent:
- Separate `params`, `inputs`, `outputs`, `lights` arrays `(vcv:include/engine/Module.hpp:L47-L50)`
- Separate unique `id` `(vcv:include/engine/Module.hpp:L40)`
- Per-instance `dataToJson()`/`dataFromJson()` for module-specific state `(vcv:include/engine/Module.hpp:L359-L365)`

### 4.3 Shared vs per-instance pieces

**Per-instance:** All runtime state (params, ports, lights, CPU meters). `(vcv:include/engine/Module.hpp:L40-L62)`

**Shared:** Only the `Model*` pointer (vtable, metadata, factory methods). The Model contains no mutable instance state.

### 4.4 Nested instantiation

**N/A** — Flat module structure. No scenes-within-scenes. Modules cannot contain other modules.

**Expander pattern (adjacent modules):** Modules can reference left/right "expander" modules via `leftExpander.moduleId` / `rightExpander.moduleId` `(vcv:include/engine/Module.hpp:L98-L99)`. This is adjacency in the flat list, not nesting. Messages passed via double-buffer `(vcv:include/engine/Module.hpp:L69-L96)`.

---

## Section 5: Change tracking and editor / wire sync

**Mostly N/A** — VCV Rack is single-process; editor and engine share memory. No wire protocol or client/server split.

### 5.1 Property change tracking

**No centralized change tracking.** Params are mutated directly via `Param::setValue()` `(vcv:include/engine/Param.hpp:L18-L20)`. UI polls for changes. History/undo managed separately in `history.cpp` (command pattern for actions, not property-level dirty flags).

### 5.2 Sync format

**N/A** — Single process. Direct memory access.

### 5.3 Opt-in vs automatic

**N/A**

### 5.4 Save format vs sync format

**Same format.** Both use JSON via Jansson. `Engine::toJson()` and `Module::toJson()` produce patch format `(vcv:src/engine/Engine.cpp:L1248-L1274)`. No separate wire schema.

### 5.5 Changes during render

Engine mutex (`SharedMutex`) protects state. Readers (UI) share-lock; writers (patch operations) exclusive-lock. Param smoothing happens in audio thread via `smoothModule` pointer `(vcv:src/engine/Engine.cpp:L244-L242)`. Direct param writes from UI use `setParamValue()` which cancels smoothing `(vcv:src/engine/Engine.cpp:L1118-L1125)`.

### 5.6 Reconciliation on divergence

**N/A** — Single process, no network.

---

## Section 6: Property reflection

### 6.1 Property enumeration

Via the four indexed arrays on Module: `(vcv:include/engine/Module.hpp:L47-L61)`
- `std::vector<Param> params`
- `std::vector<Input> inputs`
- `std::vector<Output> outputs`
- `std::vector<Light> lights`

Metadata arrays:
- `std::vector<ParamQuantity*> paramQuantities`
- `std::vector<PortInfo*> inputInfos/outputInfos`
- `std::vector<LightInfo*> lightInfos`

Size fixed at construction by `config(nParams, nInputs, nOutputs, nLights)` `(vcv:include/engine/Module.hpp:L118)`.

### 6.2 Static vs dynamic types

**Static.** Properties configured at module construction time via `configParam()`, `configInput()`, etc. `(vcv:include/engine/Module.hpp:L124-L228)`. Types known at compile-time; ParamQuantity derived classes can customize behavior `(vcv:include/engine/ParamQuantity.hpp:L21-L127)`.

### 6.3 String-based property access

**No.** Properties accessed by integer index only: `getParam(int index)`, `getInput(int index)` `(vcv:include/engine/Module.hpp:L272-L292)`. No string-to-index map.

### 6.4 Authority checking

Params have `resetEnabled`, `randomizeEnabled`, `smoothEnabled`, `snapEnabled` flags `(vcv:include/engine/ParamQuantity.hpp:L57-L67)`. No runtime type enforcement — direct array access to `params[index]` is allowed but discouraged.

---

## Section 7: Composition — dynamic children

### 7.1 Dynamic children at runtime

**N/A** — Modules are flat; no children concept. Module count changes via `addModule()`/`removeModule()` calls from UI/browser, but this is graph mutation, not parent-child composition.

### 7.2 Structural vs configured children

**N/A** — No structural children.

### 7.3 Property-as-node pattern

**N/A** — No equivalent. A "gradient parameter sourcing a Pattern" would be modeled as a cable connection from a Pattern module's output to an Effect module's input, not as a child node.

### 7.4 Constraint model

**N/A** — No child constraints. Any module can cable to any other module (subject to Input/Output port count limits). Expander adjacency is restricted to immediate left/right neighbors `(vcv:include/engine/Module.hpp:L98-L99)`.

---

## Section 8: Inter-node dependencies and execution ordering

### 8.1 Data flow expression

**Cables between ports.** `Cable` struct: `(vcv:include/engine/Cable.hpp:L10-L24)`

```cpp
struct Cable {
    int64_t id = -1;
    Module* inputModule = NULL;
    int inputId = -1;
    Module* outputModule = NULL;
    int outputId = -1;
};
```

Multiple cables can stack on one input (summed voltages) `(vcv:src/engine/Engine.cpp:L385-L418)`. Serializes as:
```json
{
  "id": 123,
  "outputModuleId": 1, "outputId": 0,
  "inputModuleId": 2, "inputId": 0
}
```
`(vcv:src/engine/Cable.cpp:L10-L18)`

### 8.2 Execution order determination

**Fixed flat traversal + dataflow via cable copy.** No topological sort. Each frame: `(vcv:src/engine/Engine.cpp:L428-L473)`

1. All modules process (parallel, undefined order)
2. Cable routing copies output voltages to inputs (after all modules finish)

This creates **one-sample delay** in feedback loops. Cycles are allowed and resolved by the sample delay.

### 8.3 Push vs pull

**Push with delayed pull.** Modules write to their `outputs[]` arrays (push). Cable routing phase copies/fan-outs to `inputs[]` (pull from outputs, push to inputs). Expanders use double-buffer message passing (push by writer, flip at frame end) `(vcv:include/engine/Module.hpp:L69-L96)`.

### 8.4 Cycle detection/prevention

**No detection.** Cycles are allowed. Feedback resolved by one-sample delay: input reads previous frame's output `(vcv:src/engine/Engine.cpp:L376-L383)`. This is standard for audio DSP.

### 8.5 Dependency graph caching

**N/A** — No dependency graph. Cable list is sorted by `(inputModule, inputId)` for efficient routing `(vcv:src/engine/Engine.cpp:L1007-L1010)`, but this is a spatial sort, not a topological sort.

### 8.6 Cross-tree dependencies

**N/A** — No tree. Any module can cable to any other module globally. ParamHandles allow cross-module parameter automation by `moduleId + paramId` `(vcv:include/engine/ParamHandle.hpp)`.

---

## Section 9: Node state, errors, and logging

### 9.1 Operational state enum

**No explicit state enum.** Modules don't have a `Loading`/`Ready`/`Error` state field. Operational status inferred from:
- Existence in Engine (pointer in modules vector)
- `internal->bypassed` boolean `(vcv:src/engine/Module.cpp:L23)`
- Valid `id >= 0` `(vcv:include/engine/Module.hpp:L40)`

Load failures result in skipped module (patch loads partially) `(vcv:src/engine/Engine.cpp:L1313-L1316)`.

### 9.2 Error categories

Uses C++ exceptions (`rack::Exception`) for:
- Missing plugin/model `(vcv:src/engine/Engine.cpp:L1294-L1298)`
- Malformed JSON `(vcv:src/patch.cpp:L372-L376)`
- Cable connection errors (missing module) `(vcv:src/engine/Cable.cpp:L33-L53)`

No typed error enum — exception carries message string only.

### 9.3 Load-time errors

In `Engine::fromJson()`: `(vcv:src/engine/Engine.cpp:L1287-L1327)`

- Missing model: Log warning (`WARN()`), skip module, continue loading
- Exception during `module->fromJson()`: Log warning, `delete module`, continue
- Missing cable endpoint: Exception in `Cable::fromJson()`, cable skipped

Result: Partial patch load. Missing modules reported via `checkUnavailableModulesJson()` `(vcv:src/patch.cpp:L564-L605)` which opens library browser.

### 9.4 Runtime errors

- **OOM:** C++ `std::bad_alloc` propagates (likely crashes)
- **Panic/exception:** Unhandled, crashes process
- **Arithmetic faults:** `finitize()` helper clamps NaN/inf to 0 in cable routing only `(vcv:src/engine/Engine.cpp:L353-L355)`. Module `process()` is unprotected.

### 9.5 Error storage

**Global log file only.** No per-node error storage. Log written to `logPath` (file + terminal) `(vcv:include/logger.hpp:L24-L41)`. Format: `[timestamp level file:line function] message`.

### 9.6 Logging mechanism

Four-level logging: `DEBUG`, `INFO`, `WARN`, `FATAL` `(vcv:include/logger.hpp:L13-L16)`. Thread-safe (messages don't overlap). No in-memory ring buffer; direct file write. Query via file I/O; no programmatic filtering API.

### 9.7 Warnings vs errors

Same stream, severity prefix in log line. No separate warning tracking. `WARN()` macro for warnings `(vcv:include/logger.hpp:L15)`.

### 9.8 Error behavior and recovery

- **Bypassed module:** `processBypass()` called instead of `process()` — routes inputs directly to outputs via `bypassRoutes` `(vcv:include/engine/Module.hpp:L101-L105)`, `(vcv:src/engine/Module.cpp:L99-L111)`
- **Missing plugin module:** Placeholder not created; module simply absent from rack
- **Recovery:** Fix file on disk, revert patch, or open library to install missing plugin `(vcv:src/patch.cpp:L596-L601)`.

---

## Section 10: Schema versioning and evolution

### 10.1 Schema version in files

**Version field at patch level only.** `patch.json` contains `"version": "2.x.x"` `(vcv:src/patch.cpp:L474-L475)`. No per-module or per-property version. Module-level version field records plugin version at save time for informational purposes only `(vcv:src/engine/Module.cpp:L126-L127)`.

### 10.2 Migration path

**Ad-hoc inline handling.** No declarative migrations. Examples:
- `"wires"` → `"cables"` rename handled by fallback lookup `(vcv:src/engine/Engine.cpp:L1332-L1333)`
- Pre-1.0 ID handling (array index vs random ID) `(vcv:src/engine/Engine.cpp:L1309-L1311)`
- Legacy `"disabled"` → `"bypass"` rename `(vcv:src/engine/Module.cpp:L186-L188)`
- Param ID fallback to index `(vcv:src/engine/Module.cpp:L228-L240)`

### 10.3 Forward compatibility

**Ignore unknown fields.** JSON objects merged; extra fields skipped by Jansson. Modules can store arbitrary data in `"data"` object `(vcv:include/engine/Module.hpp:L359-L365)`.

### 10.4 Backward compatibility

**Best-effort.** Version logged for information `(vcv:src/patch.cpp:L516-L521)`. No automatic migration of semantic changes — each module handles its own `dataFromJson()` evolution.

### 10.5 Deprecated field handling

Silently accepted with fallback lookups (see 10.2 examples). No warnings logged for deprecated fields. Hidden modules supported via `"hidden": true` in manifest `(vcv:include/plugin/Model.hpp:L54-L57)`.

---

## Summary of VCV-to-Lightplayer Mapping Challenges

| Lightplayer Concept | VCV Equivalent | Mapping Quality |
|---------------------|----------------|-----------------|
| Node | `Module` | Good |
| NodeTree | `Engine` module/cable vectors | Flat vs tree mismatch |
| params | `Param` array | Good |
| inputs/outputs | `Input`/`Output` ports + `Cable` | Good (DSP-idiomatic) |
| state | `dataToJson()` / internal fields | Ad-hoc, no schema |
| Artifact/Model | `plugin::Model` | Good |
| Path grammar | **N/A** | Major gap — VCV uses IDs only |
| Hot reload | **N/A** | Major gap |
| Editor sync | **N/A** (shared memory) | Inapplicable to network model |
| Nested scenes | **N/A** | Major gap — flat only |
| Error isolation | **N/A** | Major gap — crashes process |

VCV Rack's audio-thread architecture (worker threads, spin barriers, frame-by-frame cable copying) is highly specific to low-latency DSP and doesn't directly map to a single-threaded `no_std` LED controller. The flat module/cable model is simpler than a recursive node tree but loses hierarchical composition semantics.
