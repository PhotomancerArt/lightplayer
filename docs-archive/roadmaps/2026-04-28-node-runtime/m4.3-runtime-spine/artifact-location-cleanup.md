# Artifact Location Cleanup

## Plan

- Add `ArtifactLocation` in `lpc-engine::artifact` as the resolved runtime artifact address.
- Keep `SrcArtifactSpec` as the authored source-level specifier carried by `SrcNodeConfig`.
- Use `ArtifactLocation` as the `ArtifactManager` cache key and store it on `ArtifactEntry`.
- Keep `ArtifactId` as the opaque dense runtime handle returned by the manager.
- Update source-loader helpers and runtime-spine tests to follow `SrcArtifactSpec -> ArtifactLocation -> ArtifactId`.
- Fix the CI clippy lint reported in `lp-shader/lpir/src/print.rs` if still present locally.

## Notes

- M4.3 originally collapsed authored specifier, resolved location, and runtime handle more than we want.
- The desired naming model is:
  - `SrcArtifactSpec`: authored, context-dependent source reference.
  - `ArtifactLocation`: resolved load/cache address, currently `File(LpPathBuf)`.
  - `ArtifactId`: runtime handle into `ArtifactManager`.
- No separate phase files were needed; this is a scoped follow-up to the M4.3 runtime spine.
