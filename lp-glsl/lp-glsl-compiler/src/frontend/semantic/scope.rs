use crate::error::{ErrorCode, GlslError};
use crate::frontend::semantic::types::Type;
use hashbrown::HashMap;

use alloc::{format, string::String, vec, vec::Vec};

pub struct SymbolTable {
    scopes: Vec<Scope>,
}

struct Scope {
    variables: HashMap<String, VarDecl>,
}

pub struct VarDecl {
    pub name: String,
    pub ty: Type,
    pub storage_class: StorageClass,
}

#[derive(Debug, Clone, Copy)]
pub enum StorageClass {
    Local,
    // Future: Uniform, In, Out
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {
            scopes: vec![Scope {
                variables: HashMap::new(),
            }],
        }
    }

    pub fn declare_variable(
        &mut self,
        name: String,
        ty: Type,
        storage: StorageClass,
    ) -> Result<(), GlslError> {
        let scope = self.scopes.last_mut().unwrap();
        if scope.variables.contains_key(&name) {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!("variable `{name}` already declared"),
            ));
        }
        scope.variables.insert(
            name.clone(),
            VarDecl {
                name,
                ty,
                storage_class: storage,
            },
        );
        Ok(())
    }

    pub fn lookup_variable(&self, name: &str) -> Option<&VarDecl> {
        for scope in self.scopes.iter().rev() {
            if let Some(var) = scope.variables.get(name) {
                return Some(var);
            }
        }
        None
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(Scope {
            variables: HashMap::new(),
        });
    }

    pub fn pop_scope(&mut self) {
        self.scopes.pop();
    }
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}
