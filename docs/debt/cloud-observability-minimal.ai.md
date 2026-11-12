# Cloud observability is fly-logs-only, by choice

**Condition.** The lp-cloud service has: /healthz (build sha + API
version), one-line request logs (fly logs, live + short retention),
fly machine metrics, and health-check auto-restart. It does NOT have:
persisted logs, app metrics, error aggregation, tracing, or external
uptime alerting (a free healthchecks.io ping is recommended in
infra/README.md but not yet set up).

**Why it stands.** Ruled at P11a: at single-machine crew scale,
`fly logs` answers every question we've had; a metrics stack is
maintenance surface with no current reader.

**Trigger to fix.** Recurring incidents where `fly logs` retention was
too short to diagnose, or real users whose failures we don't witness.

**Incident log.** (none yet)
