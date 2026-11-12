# Phases: FW-Host Runtime Integration

1. Create Platform struct in `lp-core/src/app/platform.rs`
2. Create MsgIn/MsgOut enums in `lp-core/src/app/messages.rs`
3. Create LpApp structure and constructor in `lp-core/src/app/lp_app.rs`
4. Implement LpApp::load_project() method
5. Implement LpApp::tick() and message handling
6. Create HostOutputProvider in `fw-host/src/output_provider.rs`
7. Update LightPlayerApp to use LpApp
8. Set up update loop in fw-host main.rs
9. Create animated demo scene (rotating color wheel shader)
10. Testing and cleanup

