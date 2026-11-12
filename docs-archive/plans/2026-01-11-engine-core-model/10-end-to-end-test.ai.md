# Phase 10: End-to-End Test

## Goal

Create a comprehensive end-to-end test that loads a project, initializes nodes, renders a frame, syncs with a client, and tests hot-reload.

## Dependencies

- All previous phases

## Implementation

### 1. Test Helper Builder

**File**: `lp-engine/src/test_util/builder.rs`
```rust
use lp_model::{
    LpPath, NodeSpecifier,
    nodes::{
        texture::TextureConfig,
        shader::ShaderConfig,
        output::OutputConfig,
        fixture::FixtureConfig,
    },
};
use lp_shared::fs::{LpFs, LpFsMemory};
use alloc::string::String;

/// Test project builder
pub struct TestProjectBuilder {
    fs: LpFsMemory,
}

impl TestProjectBuilder {
    /// Create new builder
    pub fn new() -> Self {
        let mut fs = LpFsMemory::new();
        
        // Create project.json
        let project_json = r#"{
            "uid": "test-project",
            "name": "Test Project"
        }"#;
        fs.write_file("/project.json", project_json.as_bytes()).unwrap();
        
        Self { fs }
    }
    
    /// Add a basic texture node
    pub fn add_texture(&mut self, name: &str) -> LpPath {
        let path = LpPath::from(format!("/src/{}.texture", name));
        let node_dir = format!("/src/{}.texture", name);
        
        let config = TextureConfig::Memory {
            width: 100,
            height: 100,
        };
        let config_json = serde_json::to_string(&config).unwrap();
        
        self.fs.write_file(&format!("{}/node.json", node_dir), config_json.as_bytes()).unwrap();
        
        path
    }
    
    /// Add a basic shader node
    pub fn add_shader(&mut self, name: &str, texture_path: &LpPath) -> LpPath {
        let path = LpPath::from(format!("/src/{}.shader", name));
        let node_dir = format!("/src/{}.shader", name);
        
        let config = ShaderConfig {
            glsl_path: "main.glsl".to_string(),
            texture_spec: NodeSpecifier::from(texture_path.as_str()),
            render_order: 0,
        };
        let config_json = serde_json::to_string(&config).unwrap();
        
        // Create main.glsl
        let glsl_code = r#"
            void main() {
                gl_FragColor = vec4(1.0, 0.0, 0.0, 1.0);
            }
        "#;
        
        self.fs.write_file(&format!("{}/node.json", node_dir), config_json.as_bytes()).unwrap();
        self.fs.write_file(&format!("{}/main.glsl", node_dir), glsl_code.as_bytes()).unwrap();
        
        path
    }
    
    /// Add a basic output node
    pub fn add_output(&mut self, name: &str) -> LpPath {
        let path = LpPath::from(format!("/src/{}.output", name));
        let node_dir = format!("/src/{}.output", name);
        
        let config = OutputConfig::GpioStrip {
            pin: 18,
        };
        let config_json = serde_json::to_string(&config).unwrap();
        
        self.fs.write_file(&format!("{}/node.json", node_dir), config_json.as_bytes()).unwrap();
        
        path
    }
    
    /// Add a basic fixture node
    pub fn add_fixture(&mut self, name: &str, texture_path: &LpPath, output_path: &LpPath) -> LpPath {
        let path = LpPath::from(format!("/src/{}.fixture", name));
        let node_dir = format!("/src/{}.fixture", name);
        
        let config = FixtureConfig {
            output_spec: NodeSpecifier::from(output_path.as_str()),
            texture_spec: NodeSpecifier::from(texture_path.as_str()),
            mapping: "linear".to_string(),
            lamp_type: "rgb".to_string(),
            transform: [[1.0; 4]; 4],
        };
        let config_json = serde_json::to_string(&config).unwrap();
        
        self.fs.write_file(&format!("{}/node.json", node_dir), config_json.as_bytes()).unwrap();
        
        path
    }
    
    /// Get filesystem (consumes builder)
    pub fn build(self) -> LpFsMemory {
        self.fs
    }
}

/// Helper: create basic texture config
pub fn basic_texture() -> TextureConfig {
    TextureConfig::Memory {
        width: 100,
        height: 100,
    }
}

/// Helper: create basic shader config
pub fn basic_shader(texture_spec: NodeSpecifier) -> ShaderConfig {
    ShaderConfig {
        glsl_path: "main.glsl".to_string(),
        texture_spec,
        render_order: 0,
    }
}

/// Helper: create basic output config
pub fn basic_output() -> OutputConfig {
    OutputConfig::GpioStrip {
        pin: 18,
    }
}

/// Helper: create basic fixture config
pub fn basic_fixture(texture_spec: NodeSpecifier, output_spec: NodeSpecifier) -> FixtureConfig {
    FixtureConfig {
        output_spec,
        texture_spec,
        mapping: "linear".to_string(),
        lamp_type: "rgb".to_string(),
        transform: [[1.0; 4]; 4],
    }
}
```

**File**: `lp-engine/src/test_util/mod.rs`
```rust
pub mod builder;

pub use builder::{TestProjectBuilder, basic_fixture, basic_output, basic_shader, basic_texture};
```

### 2. Integration Test

**File**: `lp-engine/tests/integration_test.rs`
```rust
use lp_engine::{ProjectRuntime, Error};
use lp_engine::test_util::TestProjectBuilder;
use lp_engine_client::{ClientApi, ClientProjectView};
use lp_model::project::api::{ProjectRequest, ApiNodeSpecifier};
use lp_shared::fs::LpFsMemory;

/// Mock client API implementation for testing
struct MockClientApi {
    runtime: ProjectRuntime,
}

impl ClientApi for MockClientApi {
    fn get_changes(&self, request: ProjectRequest) -> Result<lp_model::project::api::ProjectResponse, String> {
        match request {
            ProjectRequest::GetChanges { since_frame, detail_specifier } => {
                self.runtime.get_changes(since_frame, &detail_specifier)
                    .map_err(|e| format!("{}", e))
            }
        }
    }
}

#[test]
fn test_end_to_end() {
    // Create project
    let mut builder = TestProjectBuilder::new();
    let texture_path = builder.add_texture("my-texture");
    let shader_path = builder.add_shader("my-shader", &texture_path);
    let output_path = builder.add_output("my-output");
    let _fixture_path = builder.add_fixture("my-fixture", &texture_path, &output_path);
    
    let fs: LpFsMemory = builder.build();
    
    // Load project
    let mut runtime = ProjectRuntime::new(Box::new(fs)).unwrap();
    runtime.load_nodes().unwrap();
    
    // Initialize nodes
    runtime.initialize_nodes().unwrap();
    
    // Render a frame
    runtime.tick();
    runtime.render().unwrap();
    
    // Create client and sync
    let mut client_view = ClientProjectView::new();
    let mock_api = MockClientApi { runtime };
    
    // Request detail for texture
    client_view.request_detail(vec![lp_model::NodeHandle::new(1)]);
    
    // Sync
    let request = ProjectRequest::GetChanges {
        since_frame: lp_model::FrameId::default(),
        detail_specifier: client_view.detail_specifier(),
    };
    let response = mock_api.get_changes(request).unwrap();
    client_view.sync(&response).unwrap();
    
    // Verify client has nodes
    assert!(!client_view.nodes.is_empty());
    
    // Test hot-reload: update shader file
    // todo!("Add filesystem update and reload test")
    // For now, manual reload would be: runtime.load_nodes() again
}

#[test]
fn test_hot_reload() {
    // Create project with shader
    let mut builder = TestProjectBuilder::new();
    let texture_path = builder.add_texture("tex");
    let shader_path = builder.add_shader("shader", &texture_path);
    let mut fs: LpFsMemory = builder.build();
    
    // Load and initialize
    let mut runtime = ProjectRuntime::new(Box::new(fs)).unwrap();
    runtime.load_nodes().unwrap();
    runtime.initialize_nodes().unwrap();
    
    // Update shader file
    let new_glsl = r#"
        void main() {
            gl_FragColor = vec4(0.0, 1.0, 0.0, 1.0);
        }
    "#;
    // todo!("Update file in runtime's fs - may need fs to be mutable or have update method")
    
    // Reload nodes
    runtime.load_nodes().unwrap();
    
    // Verify change was detected
    // todo!("Check that shader was reloaded")
}
```

## Success Criteria

- All code compiles
- End-to-end test passes
- Can create project, load, initialize, render
- Client can sync and see nodes
- Hot-reload test works (manual trigger is fine)

## Notes

- Mock client API allows testing without real network
- Hot-reload test may need filesystem update mechanism
- Some `todo!()` items may remain for filesystem watching
- Test verifies the full flow works end-to-end
