# Project Reload Memory Bug Verification

## Summary

Verified. There are two related reload-path issues that can amplify ESP32 memory
pressure:

1. A newly loaded project starts with `last_fs_version = FsVersion::default()`,
   so file writes that happened before the load are still considered pending and
   can trigger an immediate reload on the next server tick.
2. `Project::reload` builds a complete new `Engine` while the old `self.runtime`
   remains resident, then assigns `self.runtime = runtime` only after the new
   runtime has loaded.

Together, this means button-sign can pay for initial project load and then
immediately attempt a second full engine load with the first engine still in
memory.

## Code Evidence

- `lp-app/lpa-server/src/project.rs:73` loads the initial core runtime in
  `Project::new`.
- `lp-app/lpa-server/src/project.rs:96` initializes `last_fs_version` to
  `FsVersion::default()` rather than the filesystem version at load time.
- `lp-app/lpa-server/src/server.rs:215` queries base filesystem changes since
  `project.last_fs_version()`.
- `lp-app/lpa-server/src/server.rs:253` calls `project.reload()` for any
  matching project changes.
- `lp-app/lpa-server/src/project.rs:139` starts `Project::reload`'s core load.
- `lp-app/lpa-server/src/project.rs:140-144` constructs a new local `runtime`.
- `lp-app/lpa-server/src/project.rs:148` assigns the new runtime into
  `self.runtime`, so the old runtime remains alive for the whole load.
- `lp-fw/fw-esp32/src/lp_fs_flash.rs:70-76` records writes by bumping the change
  version and storing the changed path.
- `lp-fw/fw-esp32/src/lp_fs_flash.rs:408-423` returns changes with
  `version >= since_version`.

## Trace Evidence

Failed hardware traces show file upload, initial load request, then OOM:

- `traces/2026-05-20T01-01-17--esp32c6--demo-button-sign/trace.txt:77` receives
  `loadProject`.
- `traces/2026-05-20T01-01-17--esp32c6--demo-button-sign/trace.txt:79-80`
  starts project load with about 230 KB free.
- `traces/2026-05-20T01-01-17--esp32c6--demo-button-sign/trace.txt:82-83`
  OOMs with only 316 bytes free.

Successful hardware traces show the immediate reload after initial load:

- `traces/2026-05-20T01-33-10--esp32c6--demo-button-sign/trace.txt:79-109`
  logs the initial project load, dropping from about 233 KB free to 165 KB free
  during child loading.
- `traces/2026-05-20T01-33-10--esp32c6--demo-button-sign/trace.txt:110-111`
  immediately logs `project reload start` and `project reload after root path`
  with only 121 KB free, before the reload has started its second full engine
  load.
- `traces/2026-05-20T01-52-27--esp32c6--demo-button-sign/trace.txt:110-111`
  shows the same immediate reload pattern.

## Assessment

This is a major memory bug, not merely a collection-efficiency issue. The
collection roadmap still matters, but this reload behavior can double peak
runtime load pressure at exactly the wrong time.

No fix was applied in this pass.

