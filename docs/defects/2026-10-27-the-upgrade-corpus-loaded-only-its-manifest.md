---
status: fixed
found: 2026-10-27      # how: live-debugging (mapping rebuild M3-0, recording the landing goldens)
fixed: 03bd40648
area: lpa-upgrade — the corpus test that proves migrated projects load (tests/corpus_goldens.rs)
class: opt-in-degradation   # a node that fails to parse is an inventory state, not an Err, and the test never asked
related:
  - lp-app/lpa-upgrade/tests/corpus_goldens.rs (every_migrated_project_loads_through_the_real_registry)
  - lp-app/lpa-upgrade/tests/corpus/v5/old-uid-sign/module.json
  - lp-core/lpc-registry/src/registry/project_registry.rs (ProjectRegistry::load_root)
  - docs/defects/2026-07-27-created-package-unloadable.md (the defect the test was written for)
  - lp2025/2026-10-27-0114-mapping-rebuild (M3 P3, P4)
---
# The upgrade corpus test loaded only each project's manifest

**Symptom** — Recording the landing goldens for the mapping rebuild, the
live engine refused the upgrade chain's own output
`tests/corpus/v5/_expected/old-uid-sign`:
`parse error: uid: unknown field "uid". Expected one of: nodes, bindings,
inputs, exports, meta, provenance.` (its `module.json` carried a
`"uid"`). Yet `every_migrated_project_loads_through_the_real_registry`
passed for it, as it had since the corpus was added on 2026-08-07, and its
doc said it proved every migrated project loads.

**Root cause** — Two things, each hiding the other:

1. **The test loaded the wrong root.** It called
   `ProjectRegistry::load_root` with `/project.json`, the container
   manifest. The engine's root is `/module.json`
   (`ProjectLoader::load_from_root`). So the registry tried to read the
   manifest as a node, failed, and never looked at `module.json` or any
   node beneath it, for every corpus project.
2. **`load_root` returns `Ok` when nodes fail.** It gates only the
   manifest's format; a node file that does not parse stays in the
   inventory as `NodeDefState::ParseError`. The test checked only the
   `Result`, so a project with no loadable node passed.

The input itself was malformed: no `module.json` at format 5 ever took a
`uid` (the format-5 `module.schema.json` has none, and `ModuleDef` has
refused one since project/module mitosis). The corpus author added it to
exercise the v5→v6 transcode on a second prefix; the transcode ran, and
nothing that read the result noticed it could not load.

**Fix** — The test loads `/module.json`, as the engine does, and asserts
every node definition in the inventory is `Loaded`. The v5 input no longer
carries a module uid (the project uid in `project.json` still exercises the
transcode, and `the_uid_transcode_is_reported_by_name` still asserts it);
its golden was re-blessed, and its landing golden, which had recorded the
refusal, was deleted and recorded again from the loading project.

**Regression coverage** — `every_migrated_project_loads_through_the_real_registry`,
checked both ways in the fixing change: with the old input it fails, naming
`v5/old-uid-sign` and `/module.json`; with the fix it passes.

**Lesson** — A load check is only as good as the root it loads and the
states it reads. Where a reader degrades a bad file to a state instead of
an error, a test that says "it loads" must walk the states, and must start
where the real loader starts.
