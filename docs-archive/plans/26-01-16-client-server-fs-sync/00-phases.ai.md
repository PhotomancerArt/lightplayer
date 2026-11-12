# Phases: Client-Server Filesystem Sync

1. Move OutputProvider to lp-shared
2. Add delete operations and recursive listing to LpFs trait
3. Create transport traits in lp-shared
4. Create message protocol in lp-model
5. Implement LpServer with tick() API
6. Implement LpClient with blocking operations
7. Create in-memory transport for tests
8. Add end-to-end filesystem sync tests
9. Add project commands (load, unload, list, send messages)
