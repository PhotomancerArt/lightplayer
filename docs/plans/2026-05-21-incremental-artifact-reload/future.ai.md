# Future Work

## Project-Level Error Surface

- **Idea:** Add a first-class project-level reload/status channel for invalid
  `project.toml` and unsupported structural reload failures.
- **Why not now:** The urgent bug is node-local artifact/source failure tearing
  down runtime. Existing node status is enough for SVG and shader errors.
- **Useful context:** `ProjectRuntimeStatus` and `ServerRuntimeStatus` already
  exist under project-read runtime status.

## Fine-Grained Structural Diffs

- **Idea:** Incrementally apply `project.toml` tree edits without full engine
  fallback.
- **Why not now:** Requires careful node add/remove/reparent semantics and
  binding cleanup. It is bigger than making node artifact reload safe.
- **Useful context:** `NodeTree`, `tree_deltas_since`, and artifact handles are
  already close to supporting this.
