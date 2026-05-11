# Typed SrcArtifactSpec

## Plan

- Change `SrcArtifactSpec` from an opaque tuple string into a typed source-level reference.
- Keep authored TOML/JSON syntax as a string:
  - `./effects/tint.effect.toml` parses as a path spec.
  - `lib:core/visual/checkerboard` parses as a library spec.
- Keep `SrcArtifactSpec` distinct from `ArtifactLocation`:
  - `SrcArtifactSpec` is authored and contextual.
  - `ArtifactLocation` is resolved and cacheable.
  - `ArtifactId` is the dense runtime handle.
- Add a `SrcArtifactLibRef` source type for the `lib:` scheme.
- Update engine conversion so only path specs resolve to `ArtifactLocation::File` for now; library specs should return an explicit resolution error until the library resolver exists.
- Update constructors, tests, and docs to use `SrcArtifactSpec::path(...)` / `SrcArtifactSpec::lib_ref(...)` instead of tuple construction.

## Notes

- Prefer source variant name `Path`, not `File`, because source specs may be relative and unresolved.
- Do not make `SrcArtifactSpec` match `ArtifactLocation` one-for-one; future source schemes may resolve into richer runtime locations such as package/builtin identities.
- This is a small follow-up to `artifact-location-cleanup.md`; no separate phase files are needed.
