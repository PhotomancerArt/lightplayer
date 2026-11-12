# Plan Phases

1. Create lp-core crate structure and basic types
2. Implement ProjectConfig data structures
3. Implement ProjectRuntime data structures
4. Implement node type definitions (outputs, textures, shaders, fixtures)
5. Implement command protocol types
6. Implement error types and Display trait
7. Implement abstraction traits (filesystem, transport, led_output)
8. Create fw-esp32 app structure and basic setup
9. Implement ESP32 filesystem integration (esp-storage + littlefs2 adapter)
10. Implement ESP32 serial communication protocol handler
11. Implement ESP32 LED output (RMT driver integration)
12. Create fw-host app structure and basic setup
13. Implement host filesystem abstraction (std::fs)
14. Implement host serial communication (stdio)
15. Implement host LED visualization (egui)
16. Integration: Link all components and establish basic communication
17. Testing and cleanup
18. Debug UI - Texture Visualization
19. Debug UI - Mapping Overlay
20. Debug UI - LED Visualization Enhancement

