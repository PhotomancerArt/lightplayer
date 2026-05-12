# Resources

Resources are registry-owned runtime objects referenced by id.

They are useful when the payload is too large, mutable, or runtime-specific to
live directly inside a value.

Current resource concepts include:

- runtime buffers;
- future texture or protocol buffers.

## Resources Versus Products

A resource is an owned object in a registry.

A product is a lazy graph value that can be materialized by asking its producer.

Some products may render into resources or output-owned buffers, but the product
itself should stay a lightweight graph handle.

## Embedded Constraint

On ESP32-class targets, memory is scarce. Resource ownership should be explicit,
and large frame data should avoid unnecessary copies.
