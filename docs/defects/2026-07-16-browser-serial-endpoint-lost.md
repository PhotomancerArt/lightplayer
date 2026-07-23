---
status: fixed
found: 2026-07-16      # how: hardware-walk (pre-M1 known bug)
fixed: 3ac4697df
area: lpa-link/registry
class: lifecycle-ownership
related: ["2026-07-22-flash-session-map-deleted"]
---
# Endpoint minted in one provider instance, looked up in a fresh one

**Symptom** — Connecting a browser-serial ESP32 right after
`request_access` failed with:

```
link endpoint not found: browser-serial-esp32-port-1
```

The port had just been granted; the endpoint it minted was gone by the
time connect ran.

**Root cause** — `LinkProviderRegistry::create_connector` built a
*fresh* `BrowserSerialEsp32Provider` on every call. The endpoint minted
during `request_access` lived in instance A's `RefCell` map; the
`connect_endpoint` call went through instance B, built moments later,
whose map was empty. The provider is nominally a factory but actually a
session-state holder — per-call construction silently forked its state.

**Fix** — The registry memoizes factory-built connectors per kind:
first `create_connector` for a kind builds the provider, subsequent
calls return the same shared `Rc`. `request_access` and
`connect_endpoint` now necessarily talk to the same instance and the
same endpoint map.

**Regression coverage** — Registry test
`factory_built_connector_is_memoized_and_shared`; controller test
`connect_flow_uses_the_connector_instance_the_registry_already_handed_out`.

**Lesson** — Provider-held state must have exactly one owner per kind.
A "factory" whose products carry session state is not a factory — it is
a singleton wearing a factory's signature, and the registry must treat
it as one (memoize) or the state must move out of the provider
entirely. Any API where step 1 and step 2 can land on different
instances of "the same" object is this bug waiting to happen.
