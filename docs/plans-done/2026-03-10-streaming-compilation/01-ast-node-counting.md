# Phase 1: AST Node Counting + Function Ordering

## Scope

Add a utility to count AST nodes in a `TypedFunction` and a function to sort
`TypedFunction`s by size (ascending). This is used later to compile smallest
functions first.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation

### 1. Add `ast_node_count` to `TypedFunction`

File: `lp-shader/lp-glsl-compiler/src/frontend/semantic/mod.rs`

Add an `impl TypedFunction` block with a method that recursively counts AST
nodes. The glsl crate's `Statement` has two variants:

- `Statement::Simple(SimpleStatement)` — declarations, expressions, selection,
  iteration, jump
- `Statement::Compound(CompoundStatement)` — block with nested statements

The count should walk into:

- Compound statement bodies
- Selection (if/else) branches
- Iteration (while/do-while/for) bodies

We don't need to count into expressions — statement-level granularity is
sufficient for ordering. Each statement (including nested ones) counts as 1.

```rust
impl TypedFunction {
    /// Recursive count of AST statement nodes. Used as a heuristic for
    /// function size when ordering compilation (smallest first).
    pub fn ast_node_count(&self) -> usize {
        self.body.iter().map(count_statement_nodes).sum()
    }
}

fn count_statement_nodes(stmt: &glsl::syntax::Statement) -> usize {
    match stmt {
        glsl::syntax::Statement::Simple(simple) => count_simple_statement_nodes(simple),
        glsl::syntax::Statement::Compound(compound) => {
            1 + compound
                .statement_list
                .iter()
                .map(count_statement_nodes)
                .sum::<usize>()
        }
    }
}

fn count_simple_statement_nodes(stmt: &glsl::syntax::SimpleStatement) -> usize {
    match stmt {
        glsl::syntax::SimpleStatement::Selection(sel) => {
            1 + count_selection_nodes(&sel.rest)
        }
        glsl::syntax::SimpleStatement::Iteration(iter) => {
            1 + count_iteration_nodes(iter)
        }
        _ => 1, // Declaration, Expression, Jump, etc.
    }
}
```

Fill in `count_selection_nodes` (handles if/else branches — `SelectionRestStatement`)
and `count_iteration_nodes` (handles while/do-while/for bodies). Look at the
existing `validator.rs` for the exhaustive pattern matching on these types.

Place the `impl TypedFunction` block near the struct definition. Place the
helper `count_*` functions at the bottom of the file.

### 2. Tests

Add tests in the existing `mod tests` (or add one if there isn't one) in
`semantic/mod.rs`:

```rust
#[test]
fn test_ast_node_count_simple() {
    let func = TypedFunction {
        name: String::from("test"),
        return_type: types::Type::Void,
        parameters: Vec::new(),
        body: vec![
            // A simple return statement
            glsl::syntax::Statement::Simple(
                glsl::syntax::SimpleStatement::Jump(
                    glsl::syntax::JumpStatement::Return(None)
                )
            ),
        ],
    };
    assert_eq!(func.ast_node_count(), 1);
}
```

Also add a test that verifies a compound statement with nested statements counts
correctly (e.g., a compound with 2 simple statements inside → count 3: 1 for
compound + 2 for children).

If constructing glsl AST nodes by hand is too verbose, consider a simpler test
that parses a small GLSL snippet and checks the count. Use `CompilationPipeline::parse_and_analyze`
if available, or just test with hand-built nodes.

## Validate

```bash
cd lp-shader/lp-glsl-compiler && cargo test --features std -- semantic::tests
```

Ensure all existing tests still pass:

```bash
cd lp-shader/lp-glsl-compiler && cargo test --features std
```
