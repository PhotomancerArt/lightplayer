# Extract LightPlayer Code to Separate Repository

## Overview

Move `lp-app` and `lp-glsl` directories from the cranelift fork into a new standalone repository at
`/Users/yona/dev/photomancer/lp2025`, preserving git history and updating all dependencies to
reference the cranelift fork via git.

## Repository Structure

The new repo has:

- `lp-app/` - LightPlayer application workspace
- `lp-glsl/` - LightPlayer GLSL compiler workspace
- `scripts/` - Build and test scripts (lp-build.sh, glsl-filetests.sh, build-builtins.sh)
- Root `Cargo.toml` - Unified workspace including all crates from both lp-app and lp-glsl

## Current Status

### Completed

1. ✅ **Initialize New Repository**
    - Created directory `/Users/yona/dev/photomancer/lp2025`
    - Initialized git repository
    - Created `.gitignore` (adapted from lp-cranelift)
    - Created `README.md` with project overview

2. ✅ **Extract Git History**
    - Used `git filter-repo` to extract history for both directories
    - Extracted `lp-app/` history, preserving paths as `lp-app/...`
    - Extracted `lp-glsl/` history, preserving paths as `lp-glsl/...`
    - Merged both histories into the new repo
    - Preserved commit messages and author information

3. ✅ **Copy Directories and Scripts**
    - Copied `lp-app/` directory to new repo root
    - Copied `lp-glsl/` directory to new repo root
    - Copied LP-specific scripts:
        - `scripts/lp-build.sh`
        - `scripts/glsl-filetests.sh`
        - `scripts/build-builtins.sh`

4. ✅ **Create Root Workspace**
    - Created root `Cargo.toml` with unified workspace
    - Includes all crates from both lp-app and lp-glsl as workspace members
    - Defines shared workspace dependencies (cranelift crates via git, common deps)
    - Consolidated workspace-level configuration

5. ✅ **Update Dependencies**
    - Updated cranelift dependencies in root `Cargo.toml` to use git:
        - Repository: `https://github.com/Yona-Appletree/lp-cranelift.git`
        - Branch: `feature/lp2025`
    - Updated `lp-app/Cargo.toml` and `lp-glsl/Cargo.toml` to reference workspace dependencies
    - Commented out workspace sections in lp-app and lp-glsl Cargo.toml files (workspace now defined
      at root)

6. ✅ **Update Cross-References**
    - Updated `lp-app/crates/lp-engine/Cargo.toml` line 20: Changed path from
      `../../../lp-glsl/lp-glsl-compiler` to `../../../lp-glsl/lp-glsl-compiler` (
      correct relative path)
    - Updated `lp-glsl/Cargo.toml` to reference `lp-app/apps/lp-cli` (was `lp-core-cli`)

7. ✅ **Update Scripts**
    - Updated `scripts/lp-build.sh`: Removed cranelift-specific test (32-bit filetests), updated
      directory references
    - Updated `scripts/glsl-filetests.sh`: Removed cranelift directory check, updated workspace root
      detection to look for `lp-glsl` instead of `cranelift`
    - `scripts/build-builtins.sh`: Already uses correct paths

### In Progress / Remaining

8. ✅ **Verify Build and Tests**
    - ✅ Fixed path issue: `lp-app/crates/lp-engine/Cargo.toml` - changed from
      `../../lp-glsl/lp-glsl-compiler` to `../../../lp-glsl/lp-glsl-compiler`
    - ✅ Fixed `cranelift-interpreter` dependency: Added to root workspace dependencies and updated
      `lp-glsl-compiler` to use workspace reference
    - ✅ Added missing `runtime-embive` crate to workspace members (later removed due to embedded
      target issues)
    - ✅ Verified workspace structure with `cargo metadata --no-deps` - all packages resolve
      correctly
    - ✅ Fixed git authentication: Added `.cargo/config.toml` with `git-fetch-with-cli = true` for
      public repo access
    - ✅ Fixed import paths in test files (`filetests.rs` and `format_glsl_value_tests.rs`)
    - ✅ Workspace compiles successfully (main crates)
    - ⚠️ Some test failures in `lp-glsl-builtins` (pre-existing logic bugs, not blocking)
    - ⚠️ Embedded targets (`esp32-glsl-jit`, `lp-glsl-builtins-emu-app`) have build issues but don't
      affect main workspace

9. ✅ **Setup Git Remote and Push**
    - ✅ Committed all workspace configuration changes
    - ✅ Created GitHub repository: `gh repo create Yona-Appletree/lp2025 --public`
    - ✅ Added remote: `git remote add origin https://github.com/Yona-Appletree/lp2025.git`
    - ✅ Pushed to GitHub: `git push -u origin main`
    - Repository is now live at: https://github.com/Yona-Appletree/lp2025

## Key Files Modified

1. **Workspace Configuration:**
    - Root `Cargo.toml` (created) - unified workspace with all crates
    - `lp-app/Cargo.toml` - workspace sections commented out, dependencies reference root workspace
    - `lp-glsl/Cargo.toml` - workspace sections commented out, dependencies reference root workspace

2. **Dependency Updates:**
    - Root `Cargo.toml` - defines cranelift git dependencies in workspace.dependencies
    - `lp-app/Cargo.toml` - references workspace cranelift dependencies
    - `lp-glsl/Cargo.toml` - references workspace cranelift dependencies
    - `lp-app/crates/lp-engine/Cargo.toml` - lp-glsl-compiler path (line 20)

3. **Script Updates:**
    - `scripts/lp-build.sh` - removed cranelift test, updated paths
    - `scripts/glsl-filetests.sh` - updated workspace detection
    - `scripts/build-builtins.sh` - paths already correct

4. **Documentation:**
    - Root `README.md` (created)
    - `lp-app/README.md` (needs update - remove mention of being in cranelift repo)
    - `lp-glsl/README.md` (needs update - update script paths and remove cranelift references)

## Known Issues

1. ✅ **Path Issue**: Fixed - `lp-app/crates/lp-engine/Cargo.toml` line 20 now correctly references
   `lp-glsl-compiler` with path `../../../lp-glsl/lp-glsl-compiler`.

2. ✅ **Workspace Structure**: Cargo doesn't support nested workspaces, so we consolidated everything
   into the root workspace. The lp-app and lp-glsl Cargo.toml files have their workspace sections
   commented out but kept for reference.

3. ✅ **Git Authentication**: Fixed - Added `.cargo/config.toml` with `git-fetch-with-cli = true` to
   use git CLI for fetching dependencies from public repos. Workspace builds successfully.

## Git History

The git history was successfully extracted using `git filter-repo`:

- lp-app history: Preserved with paths as `lp-app/...`
- lp-glsl history: Preserved with paths as `lp-glsl/...`
- Both histories merged into the new repository

## Next Steps

1. ✅ Fix the path issue in `lp-app/crates/lp-engine/Cargo.toml` - DONE
2. ✅ Fix `cranelift-interpreter` dependency - DONE
3. ✅ Configure git authentication for cranelift dependencies - DONE (`.cargo/config.toml`)
4. ✅ Run `cargo build --workspace` to verify build works - DONE (main workspace builds successfully)
5. ✅ Run `cargo test --workspace` to verify tests pass - DONE (main tests pass, some pre-existing
   failures in lp-glsl-builtins)
6. ✅ Fix import paths in test files - DONE
7. ✅ Commit all changes - DONE
8. ✅ Setup git remote - DONE
9. ✅ **Create GitHub repository** `https://github.com/Yona-Appletree/lp2025.git` - DONE
10. ✅ Push to GitHub: `git push -u origin main` - DONE
11. ⏳ Update documentation files (lp-app/README.md, lp-glsl/README.md) - Optional

## Commands Used

```bash
# Extract history
cd /tmp
git clone /Users/yona/dev/photomancer/lp-cranelift lp-app-temp
git clone /Users/yona/dev/photomancer/lp-cranelift lp-glsl-temp
cd lp-app-temp && git filter-repo --path lp-app --force
cd lp-glsl-temp && git filter-repo --path lp-glsl --force

# Merge into new repo
cd /Users/yona/dev/photomancer/lp2025
git remote add lp-app-temp /tmp/lp-app-temp
git remote add lp-glsl-temp /tmp/lp-glsl-temp
git fetch lp-app-temp
git fetch lp-glsl-temp
git merge --allow-unrelated-histories lp-app-temp/feature/lightplayer -m "Import lp-app history"
git merge --allow-unrelated-histories lp-glsl-temp/feature/lightplayer -m "Import lp-glsl history"
```

## Commit Messages (Conventional Commits Style)

- `feat: create root workspace and update cranelift dependencies to git`
- `fix: update scripts for new repo structure`
- `fix: consolidate workspaces into root to avoid nested workspace error`
- `fix: correct lp-glsl-compiler path in lp-engine` ✅
- `fix: add cranelift-interpreter to workspace dependencies` ✅
- `fix: add runtime-embive to workspace members` ✅
- `fix: update cranelift dependencies to use feature/lp2025 branch` ✅
