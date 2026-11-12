# LX Studio (heronarts/LX) — Prior Art Survey Answers

Reference token: `lx`

---

## Section 1: Node lifecycle

### 1.1 What named lifecycle hooks does this system define?

LX defines several lifecycle hooks across the component hierarchy:

**Enter phase:**
- Constructor (`LXComponent(LX lx, int id, String label)`) — creates and registers component with the registry `(lx:src/main/java/heronarts/lx/LXComponent.java:L500-L509)`
- `setParent(LXComponent parent)` — called when component is added to hierarchy `(lx:src/main/java/heronarts/lx/LXComponent.java:L608-L629)`
- `onActive()` — called when a pattern becomes active `(lx:src/main/java/heronarts/lx/pattern/LXPattern.java:L741-L742)`
- `onEnable()` — called when an effect is enabled `(lx:src/main/java/heronarts/lx/effect/LXEffect.java:L342-L344)`

**Live phase:**
- `loop(double deltaMs)` — per-frame tick `(lx:src/main/java/heronarts/lx/LXLoopTask.java:L22-L29)`
- `onLoop(double deltaMs)` — pattern/effect render hook `(lx:src/main/java/heronarts/lx/pattern/LXPattern.java:L670-L679)`
- `run(double deltaMs)` — abstract pattern implementation `(lx:src/main/java/heronarts/lx/pattern/LXPattern.java:L700)`

**Leave phase:**
- `onInactive()` — pattern no longer active `(lx:src/main/java/heronarts/lx/pattern/LXPattern.java:L748-L749)`
- `onDisable()` — effect disabled `(lx:src/main/java/heronarts/lx/effect/LXEffect.java:L346-L348)`
- `dispose()` — full cleanup and unregistration `(lx:src/main/java/heronarts/lx/LXComponent.java:L1082-L1132)`

### 1.2 When a new node is added, in what order are enter-phase hooks invoked?

Order for adding a pattern to a channel:
1. Constructor creates component, registers with `LXComponent.Registry` `(lx:src/main/java/heronarts/lx/LXComponent.java:L507)`
2. `setEngine()` sets parent via `setParent()` `(lx:src/main/java/heronarts/lx/pattern/LXPattern.java:L458-L461)`
3. Parent assignment triggers `lx.componentRegistry.register(this)` if not already registered `(lx:src/main/java/heronarts/lx/LXComponent.java:L624-L626)`
4. On first `loop()` call, if pattern becomes active, `onActive()` fires `(lx:src/main/java/heronarts/lx/pattern/LXPattern.java:L670-L673)`

### 1.3 What hooks fire each frame/tick, and in what order across the tree?

The engine loop runs `(lx:src/main/java/heronarts/lx/mixer/LXMixerEngine.java:L1027-L1270)`:
1. All channels loop: `channel.loop(deltaMs)` — parent (mixer) iterates children
2. Pattern engine runs active patterns: `patternEngine.loop(buffer, model, deltaMs)`
3. Effects applied after patterns: `effect.loop(deltaMs)` `(lx:src/main/java/heronarts/lx/effect/LXEffect.java:L356-L383)`
4. Modulators updated: `modulation.loop(deltaMs)` `(lx:src/main/java/heronarts/lx/modulation/LXModulationEngine.java:L82-L85)`

Traversal is sibling-ordered within each container; no strict root-first/leaf-first — components update when their parent calls them.

### 1.4 During teardown: parent-first or child-first? Explicit destruction hooks?

Child-first disposal. `LXComponent.dispose()` `(lx:src/main/java/heronarts/lx/LXComponent.java:L1082-L1132)`:
- Removes parameter listeners and modulations
- Does NOT auto-dispose children (subclasses handle explicitly)
- Unregisters from `componentRegistry.dispose(this)`

Pattern disposal `(lx:src/main/java/heronarts/lx/pattern/LXPattern.java:L826-L839)`:
1. `removeEffects()` disposes all effects (children) first
2. Removes listeners
3. `super.dispose()`
4. `disposeCompositeBlendOptions()`

### 1.5 Are lifecycle calls eager or deferred?

**Eager/synchronous.** Adding a pattern calls `addPattern()` which immediately invokes `setEngine()`, registers listeners, and fires `patternAdded` events `(lx:src/main/java/heronarts/lx/mixer/LXPatternEngine.java:L501-L567)`.

No `queue_free` equivalent — disposal is immediate via `LX.dispose(component)` which asserts disposal completed `(lx:src/main/java/heronarts/lx/LX.java:L728-L731)`.

### 1.6 What does the dispatcher do when a hook panics/throws?

Exceptions propagate up. The engine has an error queue system:
- `LX.pushError(Error error)` — adds to `errorQueue` `(lx:src/main/java/heronarts/lx/LX.java:L559-L563)`
- `errorChanged.bang()` notifies listeners
- `fail(Throwable x)` for unrecoverable errors sets `failure` parameter and logs to file `(lx:src/main/java/heronarts/lx/LX.java:L523-L546)`

Individual pattern exceptions during `run()` would bubble up through the mixer loop; there's no per-node isolation wrapper.

---

## Section 2: Node identity and addressing

### 2.1 What is the runtime node identifier?

**Integer ID**, auto-incrementing counter in `LXComponent.Registry`:
- `private int id` field on each `LXComponent` `(lx:src/main/java/heronarts/lx/LXComponent.java:L195)`
- `ID_UNASSIGNED = -1` sentinel for unregistered components `(lx:src/main/java/heronarts/lx/LXComponent.java:L231)`
- IDs minted in `Registry.register()` via `idCounter++` `(lx:src/main/java/heronarts/lx/LXComponent.java:L308-L318)`
- Reserved `ID_ENGINE = 1` for root

### 2.2 Are runtime ids stable across save/load?

**Yes, with remapping.** The `Registry` maintains:
- `projectIdMap` — maps old IDs to new when collisions occur `(lx:src/main/java/heronarts/lx/LXComponent.java:L255)`
- `registerId()` handles collision during load via `projectLoading` flag `(lx:src/main/java/heronarts/lx/LXComponent.java:L327-L364)`
- `load()` calls `lx.componentRegistry.registerId(this, obj.get(KEY_ID).getAsInt())` `(lx:src/main/java/heronarts/lx/LXComponent.java:L1524)`

If project ID collides with existing engine component, project component gets a new ID and mapping is recorded.

### 2.3 What's the path syntax for addressing nodes?

Hierarchical slash-delimited paths, OSC-compatible:
- Root: `/lx/engine/mixer/channel/1/pattern/2`
- Examples: `channel/1/pattern/3`, `master/effect/1`, `modulation/modulator/1`

Path resolution in `LXComponent.path(parts, index)` `(lx:src/main/java/heronarts/lx/LXComponent.java:L961-L1004)`:
```java
final LXPath path(String[] parts, int index)
```

### 2.4 Does the path grammar support relative/absolute/wildcards?

- **Absolute paths:** Yes, starting from root `/lx/`
- **Relative paths:** Implicit via parent-relative `getPath()`
- **Indexed segments:** Yes, 1-indexed for OSC: `pattern/1`, `effect/2` `(lx:src/main/java/heronarts/lx/LXComponent.java:L987-L1000)`
- **Wildcards/globs:** **N/A** — no glob support in path resolution
- **Array access:** `childArrays` accessed by numeric index in path

### 2.5 Id-based vs specifier/query-based references?

**Both exist:**
- **ID-based:** `LXComponent.getId()` returns int; `lx.getComponent(int id)` does O(1) lookup via `HashMap` `(lx:src/main/java/heronarts/lx/LXComponent.java:L263-L265)`
- **Path-based:** `LXPath.get(lx, "/lx/engine/mixer/channel/1")` `(lx:src/main/java/heronarts/lx/LXPath.java:L180-L193)`
- **Label-based:** `LXPatternEngine.getPattern(String label)` iterates O(N) `(lx:src/main/java/heronarts/lx/mixer/LXPatternEngine.java:L459-L466)`

IDs used for serialization references; paths used for OSC routing.

### 2.6 How is name uniqueness enforced?

**No strict sibling uniqueness for labels.** Multiple patterns can share the same label; lookup by label returns first match `(lx:src/main/java/heronarts/lx/mixer/LXPatternEngine.java:L459-L466)`.

Path segments must be unique within parent (collision check in `_checkPath()`) `(lx:src/main/java/heronarts/lx/LXComponent.java:L534-L547)`:
```java
if (this.parameters.containsKey(path)) { throw ... }
if (this.mutableChildren.containsKey(path)) { throw ... }
```

### 2.7 Lookup complexity?

- **By ID:** O(1) via `HashMap<Integer, LXComponent>` in `Registry` `(lx:src/main/java/heronarts/lx/LXComponent.java:L248)`
- **By path:** O(depth) recursive descent parsing `(lx:src/main/java/heronarts/lx/LXComponent.java:L961-L1004)`
- **By label:** O(N) linear scan `(lx:src/main/java/heronarts/lx/mixer/LXPatternEngine.java:L459-L466)`

---

## Section 3: Resource refcount / asset management

### 3.1 What's the refcounted "asset" or "resource" type?

**N/A explicit refcount system.** Java GC handles memory.

Class metadata cached via `LXRegistry` — patterns, effects, modulators registered by class name `(lx:src/main/java/heronarts/lx/LXRegistry.java)`. Patterns/effects are instances, not refcounted shared assets.

Blend modes are instantiated per-context `(lx:src/main/java/heronarts/lx/mixer/LXMixerEngine.java:L324-L334)` and disposed when context changes.

### 3.2 Handle type and refcount mechanism?

**N/A** — No manual refcounting. `dispose()` is explicit cleanup call; no `Drop`-like automatic destructor. Parent containers hold references in `ArrayList`s `(lx:src/main/java/heronarts/lx/mixer/LXPatternEngine.java:L185-L186)`.

### 3.3 Is loading synchronous, async, or lazy?

**Synchronous.** `instantiatePattern()` uses reflection `Class.forName().getConstructor().newInstance()` `(lx:src/main/java/heronarts/lx/LX.java:L1484-L1489)`. Loading from project file is blocking on engine thread.

### 3.4 How is unloading/eviction triggered?

**Explicit removal.** `removePattern()` → `LX.dispose(pattern)` → `pattern.dispose()` `(lx:src/main/java/heronarts/lx/mixer/LXPatternEngine.java:L570-L631)`. No automatic eviction; patterns stay until explicitly removed.

### 3.5 Is hot reload supported?

**Partial.** `LXRegistry` has file watch service for content packages (`autoReloadPackages`) `(lx:src/main/java/heronarts/lx/LX.java:L494-L496)`. When classes change, new instantiations use new classloader. Existing instances not auto-updated.

---

## Section 4: Scene / patch / instance instantiation

### 4.1 How does an authored thing become a runtime instance?

From project file to running pattern:
1. `openProject(File)` → `Gson.fromJson()` parses JSON `(lx:src/main/java/heronarts/lx/LX.java:L1181-L1205)`
2. `engine.load(lx, obj.getAsJsonObject(KEY_ENGINE))` `(lx:src/main/java/heronarts/lx/LX.java:L1218)`
3. `LXMixerEngine.load()` creates channels via `loadChannel()` `(lx:src/main/java/heronarts/lx/mixer/LXMixerEngine.java:L1375-L1391)`
4. Channel loads patterns: `patternEngine.load(lx, obj)` `(lx:src/main/java/heronarts/lx/mixer/LXChannel.java:L525)`
5. `loadPattern()` instantiates: `lx.instantiatePattern(className)` → `pattern.load(lx, obj)` → `addPattern(pattern)` `(lx:src/main/java/heronarts/lx/mixer/LXPatternEngine.java:L1139-L1152)`

### 4.2 Can the same authored thing be instantiated multiple times?

**Yes.** Same pattern class can be instantiated many times; each is independent instance with own parameter values. Pattern class acts as prototype; no shared state between instances.

### 4.3 What pieces are shared between instances vs per-instance?

- **Per-instance:** All parameters, modulations, effects, internal state
- **Shared:** Class definition (methods), blend mode class definitions, model reference (read-only)

Each pattern allocates its own `renderBuffer` `(lx:src/main/java/heronarts/lx/mixer/LXPatternEngine.java:L201-L235)`.

### 4.4 Does the framework support nested instantiation?

**Yes.** Channels contain patterns; patterns contain effects; channels can be in groups; groups can have sub-channels. No explicit depth limit enforced, but groups cannot contain other groups `(lx:src/main/java/heronarts/lx/mixer/LXMixerEngine.java:L869-L870)`.

---

## Section 5: Change tracking and editor / wire sync

**Note:** LX is a single-process system (not client/server split). Most of this section is N/A for network sync, but change tracking exists for OSC/UI updates.

### 5.1 How does the system track property changes?

`LXListenableParameter` with `LXParameterListener` interface:
- `addListener(LXParameterListener listener)` `(lx:src/main/java/heronarts/lx/parameter/LXListenableParameter.java)`
- Listeners invoked synchronously on `setValue()`
- OSC listener auto-added for `LXOscComponent` instances `(lx:src/main/java/heronarts/lx/LXComponent.java:L1225-L1230)`

### 5.2 What's sent over the wire?

**N/A** — Single process. OSC messages sent to external controllers (not internal sync):
- `osc.sendParameter(p)` broadcasts parameter changes `(lx:src/main/java/heronarts/lx/LXComponent.java:L1376-L1381)`

### 5.3 Is change tracking opt-in or automatic?

**Automatic** for all `LXListenableParameter`s. Any parameter added to a component via `addParameter()` gets listener support.

### 5.4 Relationship between save format and sync format?

**Same schema.** JSON serialization via `LXSerializable` interface used for both project files and OSC query responses. `save(lx, JsonObject)` writes; `load(lx, JsonObject)` reads `(lx:src/main/java/heronarts/lx/LXSerializable.java)`.

### 5.5 How are changes during render handled?

Parameters can be mutated from any thread; no frame-versioning. Thread-safe task queue exists in engine `(lx:src/main/java/heronarts/lx/LXEngine.java:L104)` for external thread operations. Parameter changes take effect immediately on next read.

### 5.6 Reconciliation on divergence?

**N/A** — Single process, no network sync. Undo/redo via `LXCommandEngine` with command pattern `(lx:src/main/java/heronarts/lx/command/LXCommandEngine.java)`.

---

## Section 6: Property reflection

### 6.1 How does the system enumerate a node's properties?

`LXComponent` exposes:
- `parameters` — `LinkedHashMap<String, LXParameter>` of public params `(lx:src/main/java/heronarts/lx/LXComponent.java:L1135)`
- `internalParameters` — hidden params `(lx:src/main/java/heronarts/lx/LXComponent.java:L1138)`
- `getParameters()` returns unmodifiable view `(lx:src/main/java/heronarts/lx/LXComponent.java:L1348-L1350)`
- `hasParameter(String path)`, `getParameter(String path)` for lookup `(lx:src/main/java/heronarts/lx/LXComponent.java:L1358-L1373)`

### 6.2 Are property types statically or dynamically known?

**Dynamically queried.** Parameters are objects implementing `LXParameter`; type determined at runtime via `instanceof` checks in OSC handling `(lx:src/main/java/heronarts/lx/LXComponent.java:L774-L810)`:
```java
if (parameter instanceof BooleanParameter booleanParameter) { ... }
else if (parameter instanceof StringParameter stringParameter) { ... }
```

### 6.3 Can properties be set/get by string name?

**Yes.** OSC path resolution handles this:
- `handleOscMessage(OscMessage, parts, index)` routes to parameter by path `(lx:src/main/java/heronarts/lx/LXComponent.java:L732-L769)`
- `getParameter(path)` returns parameter for get/set `(lx:src/main/java/heronarts/lx/LXComponent.java:L1368-L1373)`

### 6.4 Are property edits authority-checked?

**Limited.** Parameters can be marked non-mappable: `setMappable(false)` `(lx:src/main/java/heronarts/lx/parameter/LXParameter.java:L381-L383)`. Some parameters are internal (not exposed to OSC). No formal read-only enforcement — `setValue()` always works if you have the reference.

---

## Section 7: Composition — dynamic children

### 7.1 Can a node's children change at runtime?

**Yes.** Dynamic addition/removal is core to LX:
- `addPattern(pattern, index)` / `removePattern(pattern)` `(lx:src/main/java/heronarts/lx/mixer/LXPatternEngine.java:L501-L631)`
- `addEffect(effect, index)` / `removeEffect(effect)` `(lx:src/main/java/heronarts/lx/mixer/LXBus.java:L284-L316)`
- `addModulator(modulator)` / `removeModulator(modulator)` `(lx:src/main/java/heronarts/lx/LXModulatorComponent.java:L89-L131)`

### 7.2 Are there structurally-owned vs externally-composed children?

**Both:**
- **Structural:** Patterns belong to a `LXPatternEngine` owned by `LXChannel`. Effects belong to `LXBus` effect array. These are the primary composition model.
- **External:** Modulators can be added to any `LXModulationContainer`; not strictly hierarchical in the tree.

### 7.3 How is "this property's value is itself a node" expressed?

**N/A** — No direct property-as-node. Parameters hold values (double, string, enum). However, `ObjectParameter<T>` can reference selectable objects like `LXBlend` `(lx:src/main/java/heronarts/lx/parameter/ObjectParameter.java)`.

Patterns can reference other patterns only via parent container membership, not via parameter binding.

### 7.4 What's the constraint model for children?

Type restrictions enforced at add-time:
- `LXEffect.Container.addEffect()` requires `LXEffect` `(lx:src/main/java/heronarts/lx/effect/LXEffect.java:L64-L88)`
- `LXPatternEngine.addPattern()` requires `LXPattern` `(lx:src/main/java/heronarts/lx/mixer/LXPatternEngine.java:L505-L567)`
- `LXModulationEngine.addModulator()` requires `LXModulator` `(lx:src/main/java/heronarts/lx/modulation/LXModulationEngine.java:L290-L317)`

---

## Section 8: Inter-node dependencies and execution ordering

### 8.1 How is "node A reads node B's output" expressed?

**Direct buffer access.** Patterns write to `LXBuffer` (color arrays). Effects read previous effect's buffer output:
- `effect.setBuffer(getBuffer())` before `effect.loop()` `(lx:src/main/java/heronarts/lx/pattern/LXPattern.java:L682-L691)`
- Channel passes buffer down the effect chain

No explicit output port / cable metaphor at code level — it's method call passing.

### 8.2 How does runtime determine execution order?

**Fixed tree traversal.** `LXMixerEngine.loop()`:
1. All channels loop (sibling order in `mutableChannels` list) `(lx:src/main/java/heronarts/lx/mixer/LXMixerEngine.java:L1050)`
2. Pattern engine runs active pattern(s) `(lx:src/main/java/heronarts/lx/mixer/LXPatternEngine.java:L952-L1078)`
3. Effects applied in list order `(lx:src/main/java/heronarts/lx/mixer/LXChannel.java:L466-L474)`
4. Modulators updated `(lx:src/main/java/heronarts/lx/mixer/LXBus.java:L511-L514)`

No topological sort — order is explicit list order.

### 8.3 Push or pull evaluation?

**Push.** Parent (channel) calls `pattern.loop()` which pushes output to buffer. Effects don't pull — they receive buffer reference and write to it.

### 8.4 How are cycles detected or prevented?

**N/A** — Tree structure prevents cycles by construction. Patterns cannot reference other patterns as children; effects cannot contain patterns. Modulation routing (parameter-to-parameter) is the only cross-reference, and it's handled by the modulation engine without cyclic dependency checking.

### 8.5 Is the dependency graph cached or recomputed?

**No dependency graph.** Fixed hierarchy; execution order is list iteration. No caching needed.

### 8.6 What kinds of cross-tree dependencies are allowed?

**Modulation across scopes.** `LXModulationEngine` allows modulators to target any parameter in scope:
- Global modulation engine targets any parameter `(lx:src/main/java/heronarts/lx/modulation/LXModulationEngine.java:L117-L122)`
- Bus modulation engine targets descendants `(lx:src/main/java/heronarts/lx/modulation/LXModulationEngine.java:L113-L122)`
- `isTargetParameterInScope()` checks ancestry `(lx:src/main/java/heronarts/lx/modulation/LXModulationEngine.java:L113-L122)`

---

## Section 9: Node state, errors, and logging

### 9.1 Does each node carry an explicit operational state enum?

**Partial.** No single `Loading/Ready/Error` enum per node. Individual flags:
- `LXPattern.enabled` — eligible for cycling `(lx:src/main/java/heronarts/lx/pattern/LXPattern.java:L150-L152)`
- `LXEffect.enabled` — currently running `(lx:src/main/java/heronarts/lx/effect/LXEffect.java:L144-L146)`
- `LXChannel.performanceWarning` — runtime performance issue `(lx:src/main/java/heronarts/lx/mixer/LXMixerEngine.java:L103-L106)`
- `Placeholder` pattern for missing classes `(lx:src/main/java/heronarts/lx/pattern/LXPattern.java:L64-L113)`

### 9.2 What error categories does the system distinguish?

`LX.InstantiationException` with `Type` enum:
- `EXCEPTION` — reflection/constructor failure
- `LICENSE` — missing package license
- `PLUGIN` — required plugin not loaded
`(lx:src/main/java/heronarts/lx/LX.java:L79-L97)`

Runtime errors use Java exceptions; no typed error hierarchy beyond this.

### 9.3 Load-time errors: missing/malformed asset?

**Placeholder pattern created.** When pattern class missing:
```java
try {
  pattern = lx.instantiatePattern(patternClass);
} catch (LX.InstantiationException x) {
  pattern = new LXPattern.Placeholder(lx, x);
  lx.pushError(x, ...);
}
```
`(lx:src/main/java/heronarts/lx/mixer/LXPatternEngine.java:L1141-L1148)`

Placeholder preserves JSON, saves back on project save. Node exists in tree with error state.

### 9.4 Runtime errors: OOM, panic, exception?

Exceptions propagate up through loop. `LX.pushError()` adds to queue without stopping engine `(lx:src/main/java/heronarts/lx/LX.java:L559-L563)`. Unhandled exceptions may crash the render thread; `fail()` handles fatal errors `(lx:src/main/java/heronarts/lx/LX.java:L523-L546)`.

### 9.5 Where are errors stored?

- **Global queue:** `Queue<Error> errorQueue` in `LX` instance `(lx:src/main/java/heronarts/lx/LX.java:L377)`
- **Per-error:** `LX.Error` has `Throwable cause` and `String message` `(lx:src/main/java/heronarts/lx/LX.java:L230-L261)`
- **Placeholder components:** Store `InstantiationException` reference `(lx:src/main/java/heronarts/lx/pattern/LXPattern.java:L66-L68)`

### 9.6 Error logging mechanism?

- **Console/file:** `LX.log()`, `LX.error()` with optional `EXPLICIT_LOG_FILE` `(lx:src/main/java/heronarts/lx/LX.java:L1544-L1642)`
- **In-memory queue:** `errorQueue` queried via `getError()`, `popError()`
- **OSC:** Error changes broadcast via `errorChanged` parameter
- **Timestamped:** `LOG_DATE_FORMAT` prefixes all logs `(lx:src/main/java/heronarts/lx/LX.java:L1587)`

### 9.7 Are warnings tracked separately?

**No separate warning stream.** `LX.warning()` logs to stdout if `LOG_WARNINGS` enabled `(lx:src/main/java/heronarts/lx/LX.java:L1551-L1555)`. Same `Error` type used for all severity levels.

### 9.8 Behavior while in error state; recovery path?

**Placeholder patterns:** Continue to exist, can be inspected, re-saved. Recovery via:
- Installing missing plugin/class
- Reloading project (re-instantiation attempts fresh)
- Manual deletion and recreation

No automatic retry; user must fix underlying issue and reload.

---

## Section 10: Schema versioning and evolution

### 10.1 Is a schema version embedded in saved files?

**Yes.** Project files include:
- `version` — LX version string (e.g., "1.2.1") `(lx:src/main/java/heronarts/lx/LX.java:L983)`
- `timestamp` — save time millis `(lx:src/main/java/heronarts/lx/LX.java:L984)`
- Per-component `id` and `class` fields `(lx:src/main/java/heronarts/lx/LXComponent.java:L1466-L1467)`

### 10.2 Migration path from older schema?

**Legacy parameter mapping.** `addLegacyParameter()` routes old paths to new parameters:
```java
protected LXComponent addLegacyParameter(String legacyPath, LXParameter parameter)
```
`(lx:src/main/java/heronarts/lx/LXComponent.java:L1247-L1253)`

`loadParameters()` checks `legacyParameters` before primary map `(lx:src/main/java/heronarts/lx/LXComponent.java:L1484-L1496)`.

No automatic schema transformation — legacy support is manual per-parameter.

### 10.3 Forward compatibility (old code loads new files)?

**Graceful degradation.** Unknown classes become Placeholders. Unknown parameters ignored (no error). Version warning shown if file version > app version `(lx:src/main/java/heronarts/lx/LX.java:L1184-L1196)`.

### 10.4 Backward compatibility (new code loads old files)?

**Supported via legacy mappings.** Old parameter names map to new via `legacyParameters`. Pre-1.1.1 `compositeMode` mapped to `compositeBlend` `(lx:src/main/java/heronarts/lx/pattern/LXPattern.java:L815-L819)`. Boolean defaults changed via version checks `(lx:src/main/java/heronarts/lx/mixer/LXMixerEngine.java:L1331-L1339)`.

### 10.5 How are deprecated/removed fields handled?

**Silent ignore + legacy mapping.** Unknown JSON keys ignored during load. If field moved, `addLegacyParameter()` bridges old name. If field truly removed, data is lost on save (round-trip drops unknown keys).

---

## Summary Notes

**Sections with significant N/A:**
- Section 3 (Resource refcount): LX is Java/GC — no manual refcounting, no async loader, no hot-reload asset pipeline
- Section 5 (Change tracking for sync): Single-process; no client/server split, no wire sync protocol
- Section 8.4 (Cycle detection): Tree structure prevents cycles by construction

**Idioms that don't map cleanly:**
- **Buffer-passing vs slot-binding:** LX uses direct buffer passing between patterns/effects, not our "slot grammar" with bus bindings
- **No artifact/node distinction:** Patterns are class-instantiated; no separate "artifact" refcount layer
- **Single namespace:** Parameters, children, and child arrays all share one path namespace (collision-checked)
- **1-indexed arrays:** OSC-exposed arrays use 1-indexing for user-friendliness

**Closest analogs to Lightplayer concepts:**
| Lightplayer | LX Analog |
|-------------|-------------|
| Node | `LXComponent` |
| ArtifactSpec | Class name string (e.g., "heronarts.lx.pattern.color.SolidPattern") |
| Pattern | `LXPattern` (matches vocabulary) |
| Effect | `LXEffect` (matches vocabulary) |
| Stack | `LXBus` / `LXMixerEngine` channel list |
| Bus (modulation) | `LXModulationEngine` |
| Slot/Parameter | `LXParameter` hierarchy |
| NodePath | `LXPath` canonical path |
| Uid | `int id` in `LXComponent.Registry` |
