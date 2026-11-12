# Values

Values are logical data carried by slot leaves.

The generic value type is `LpValue`. It is the common representation used for
slot sync, generic UI, and dynamic data. Values can be simple scalars, semantic
domain values, resource refs, or product handles.

## Value Shapes

Values have shapes so generic tooling can understand how to display and edit
them. A value shape describes one whole value, not a versioned slot tree.

Examples:

- dimensions;
- color order;
- relative node refs;
- visual products;
- control products.

## Slot Versus Value

Slots are versioned and addressable. Values are the payload inside a slot.

For example, a fixture state record may have an `output` slot. That slot carries
a `ControlProduct` value. The slot has a revision; the product value does not
own its own per-field revisions.

This keeps sync granularity explicit and avoids adding version data to every
small value object.
