# Phase 8: Add End-to-End Filesystem Sync Tests

## Goal

Create tests that verify client-server filesystem sync works correctly using in-memory transport. Include a clean, elegant integration test similar to `scene_render.rs`.

## Tasks

1. Create `lp-client/tests/fs_sync.rs`:
   - Main integration test: `test_filesystem_sync()`
   - Clean, elegant structure similar to `scene_render.rs`
   - Step-by-step flow with clear comments
   - Helper functions for common operations

2. Write main integration test (`test_filesystem_sync()`):
   ```rust
   #[test]
   fn test_filesystem_sync() {
       // Set up filesystem with initial files
       let mut fs = LpFsMemory::new();
       fs.write_file_mut("/projects/test/file1.txt", b"content1").unwrap();
       fs.write_file_mut("/projects/test/nested/file2.txt", b"content2").unwrap();
       
       // Create transport pair
       let (client_transport, server_transport) = MemoryTransport::new_pair();
       
       // Create server
       let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::new()));
       let mut server = LpServer::new(output_provider, Box::new(fs), "projects".to_string());
       
       // Create client
       let mut client = LpClient::new(client_transport);
       
       // Test read
       let content = client.fs_read("/projects/test/file1.txt").unwrap();
       assert_eq!(content, b"content1");
       
       // Test write
       client.fs_write("/projects/test/new.txt", b"new content").unwrap();
       
       // Test list (non-recursive)
       let entries = client.fs_list_dir("/projects/test", false).unwrap();
       assert!(entries.contains(&"/projects/test/file1.txt".to_string()));
       assert!(entries.contains(&"/projects/test/new.txt".to_string()));
       assert!(entries.contains(&"/projects/test/nested".to_string()));
       
       // Test list (recursive)
       let entries = client.fs_list_dir("/projects/test", true).unwrap();
       assert!(entries.contains(&"/projects/test/nested/file2.txt".to_string()));
       
       // Test delete file
       client.fs_delete_file("/projects/test/new.txt").unwrap();
       
       // Test delete directory (recursive)
       client.fs_delete_dir("/projects/test/nested").unwrap();
       
       // Verify deletions
       let entries = client.fs_list_dir("/projects/test", true).unwrap();
       assert!(!entries.contains(&"/projects/test/new.txt".to_string()));
       assert!(!entries.contains(&"/projects/test/nested".to_string()));
   }
   ```

3. Add helper functions:
   - `setup_server_and_client()` - Creates server and client with memory transport
   - `assert_file_content()` - Helper to verify file content
   - `assert_file_exists()` - Helper to verify file exists
   - `assert_file_not_exists()` - Helper to verify file doesn't exist

4. Add individual operation tests:
   - `test_fs_read()` - Test reading files
   - `test_fs_write()` - Test writing files
   - `test_fs_delete_file()` - Test deleting files
   - `test_fs_delete_dir()` - Test deleting directories
   - `test_fs_list_dir_non_recursive()` - Test non-recursive listing
   - `test_fs_list_dir_recursive()` - Test recursive listing

5. Add error handling tests:
   - `test_fs_read_not_found()` - Read non-existent file
   - `test_fs_delete_not_found()` - Delete non-existent file
   - `test_fs_delete_root()` - Attempt to delete "/" (should fail)
   - `test_fs_path_validation()` - Test path validation

6. Test request/response correlation:
   - `test_multiple_requests()` - Send multiple requests, verify responses match
   - `test_timeout()` - Test timeout handling

7. Test message serialization:
   - `test_message_serialization()` - Verify all message types round-trip through JSON
   - `test_nested_fs_request()` - Test nested `Filesystem(FsRequest)` serialization

8. Integration test with real filesystem:
   - `test_with_tempdir()` - Use tempdir for real filesystem test
   - Verify files persist correctly

## Success Criteria

- Clean, elegant integration test similar to `scene_render.rs` style
- All filesystem operations tested end-to-end
- Request/response correlation works correctly
- Error handling works correctly
- Path validation prevents dangerous operations
- Message serialization works correctly
- Helper functions make tests readable
- All tests pass
