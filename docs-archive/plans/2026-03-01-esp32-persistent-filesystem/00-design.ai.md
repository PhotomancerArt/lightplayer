# Design: ESP32 Persistent Filesystem

## Scope of Work

Implement persistent flash-backed filesystem on fw-esp32 so projects pushed via `just demo-esp32c6-host` survive device reboots.

**In scope**:
- Flash-backed littlefs2 filesystem in a dedicated partition (1MB)
- `LpFs` implementation backed by littlefs2
- Partition table: app ~3MB, lpfs 1MB
- Boot behavior: load persisted project(s) from flash (lexical first; optional config file)
- Remove `demo_project` feature from fw-esp32
- New `lp-cli upload` command – push project to host/device and exit
- New just command: flash firmware → lp-cli upload examples/basic serial:auto
- `flash_fs` feature with fallback to LpFsMemory on init failure

## File Structure

```
lp-core/lp-model/src/
├── config.rs                                   # NEW: LightplayerConfig { startup_project: Option<String> }

lp-fw/fw-esp32/
├── partitions.csv                              # NEW: Custom partition table (app ~3M, lpfs 1M)
├── .cargo/config.toml                          # UPDATE: Partition table env or espflash args
├── Cargo.toml                                  # UPDATE: Add flash_fs feature, esp-storage, littlefs2
├── src/
│   ├── main.rs                                 # UPDATE: flash_fs init, boot auto-load, remove demo_project
│   ├── fs/
│   │   ├── mod.rs                              # NEW: fs module
│   │   ├── flash_storage.rs                    # NEW: littlefs2 Storage impl wrapping esp-storage
│   │   ├── lp_fs_flash.rs                      # NEW: LpFs impl backed by littlefs2
│   │   └── boot_config.rs                      # NEW (optional): Read lightplayer.json for startup_project
│   └── demo_project.rs                         # DELETE

lp-cli/
├── src/
│   ├── main.rs                                 # UPDATE: Add `upload` subcommand
│   └── commands/
│       ├── mod.rs                              # UPDATE: Add upload module
│       └── upload/
│           ├── mod.rs                         # NEW
│           ├── args.rs                        # NEW: project dir, host specifier
│           └── handler.rs                     # NEW: connect, stop_all, push, load, exit

justfile                                     # UPDATE: demo-esp32c6-host uses upload command
```

## Conceptual Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Boot (fw-esp32)                                       │
├─────────────────────────────────────────────────────────────────────────────┤
│  [flash_fs enabled?]                                                          │
│       ├─ Yes: Try mount littlefs on lpfs partition                           │
│       │         ├─ Success → base_fs = LpFsFlash                             │
│       │         └─ Fail → base_fs = LpFsMemory (warning)                      │
│       └─ No:  base_fs = LpFsMemory                                           │
│                                                                              │
│  [projects in base_fs?]                                                       │
│       ├─ Yes: Read lightplayer.json (optional) → startup_project name         │
│       │        Else: lexical-first project                                   │
│       │        → LpServer.load_project()                                      │
│       └─ No:  (wait for host or remain idle)                                  │
└─────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────┐
│  LpFsFlash (LpFs impl)                                                       │
│  - Wraps littlefs2 Filesystem                                                │
│  - Paths: /projects/<name>/... (same as LpFsMemory/LpFsStd)                  │
│  - Change tracking: simple in-RAM buffer on write (for hot-reload)            │
│  - chroot: use LpFsView to wrap (already supported)                          │
└─────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────┐
│  littlefs2 Storage adapter                                                   │
│  - Implements littlefs2::driver::Storage                                     │
│  - Wraps esp-storage::FlashStorage (from esp-hal Flash at partition offset)  │
│  - READ_SIZE=4, WRITE_SIZE=4, BLOCK_SIZE=4096, BLOCK_COUNT=256 (1MB)        │
└─────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────┐
│  lp-cli upload <project-dir> <host>                                          │
│  - Connect to host (serial:auto, ws://..., etc.)                             │
│  - Stop all projects, push project, load project, exit                      │
└─────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────┐
│  just demo-esp32c6-host                                                      │
│  1. cargo espflash flash (with partition table)                               │
│  2. lp-cli upload examples/basic serial:auto                                  │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Main Components

| Component | Responsibility |
|-----------|----------------|
| **partitions.csv** | Defines app (~3MB) and lpfs (1MB) partitions |
| **flash_storage.rs** | Implements `littlefs2::driver::Storage` over esp-storage |
| **lp_fs_flash.rs** | Implements `LpFs` over littlefs2; change tracking in RAM |
| **main.rs** | Chooses flash vs memory based on `flash_fs`; mounts; boot auto-load |
| **boot_config.rs** | Optional: reads `lightplayer.json` for `startup_project` |
| **lp-cli upload** | New command: connect, stop_all, push, load, exit |
| **just demo-esp32c6-host** | Flash → upload |

## Partition Layout (4MB flash)

```csv
# Name,    Type,  SubType, Offset,   Size,     Flags
nvs,       data,  nvs,     0x9000,   0x6000,
phy_init,  data,  phy,     0xf000,   0x1000,
factory,   app,   factory,  0x10000,  0x300000,  # 3MB
lpfs,      data,  spiffs,  0x310000, 0x100000,  # 1MB
```
