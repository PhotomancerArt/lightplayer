use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lps_shared::LpsType;

use super::arena::{ExprId, HirArena};
use super::types::{HirParam, ImportId, ImportInfo, ImportKey};

#[derive(Debug, Clone)]
pub(super) struct FunctionSig {
    pub(super) name: String,
    pub(super) return_ty: LpsType,
    pub(super) params: Vec<HirParam>,
}

#[derive(Debug, Clone)]
pub(super) struct GlobalConst {
    pub(super) arena: HirArena,
    pub(super) expr: ExprId,
}

/// The module's imports in registration order. Call nodes carry an
/// [`ImportId`] — the position here — so the order must be stable from
/// registration through [`Self::into_vec`]: a sorted map would renumber
/// earlier imports whenever a later key sorted before them.
#[derive(Debug, Default)]
pub(super) struct ImportRegistry {
    pub(super) imports: Vec<ImportInfo>,
}

impl ImportRegistry {
    /// The already-registered import matching `matches`, if any.
    ///
    /// [`ImportKey`] owns its name, so a keyed lookup would have to build
    /// the owned key first — an allocation discarded on every repeat call,
    /// and `sin(x)`-style imports repeat all over a shader. The registry
    /// holds a handful of entries, so a borrowed-key scan is the cheaper
    /// question to ask.
    fn registered(&self, matches: impl Fn(&ImportKey) -> bool) -> Option<ImportId> {
        self.imports
            .iter()
            .position(|info| matches(&info.key))
            .map(|i| ImportId(i as u32))
    }

    /// The id of the import pushed last — the same position
    /// [`Self::into_vec`] hands lowering.
    fn last_id(&self) -> ImportId {
        ImportId(self.imports.len() as u32 - 1)
    }

    pub(super) fn glsl(&mut self, name: &str, argc: usize) -> ImportId {
        if let Some(id) = self.registered(
            |key| matches!(key, ImportKey::Glsl { name: n, argc: a } if n == name && *a == argc),
        ) {
            return id;
        }
        let key = ImportKey::Glsl {
            name: String::from(name),
            argc,
        };
        self.imports.push(ImportInfo {
            key: key.clone(),
            module_name: String::from("glsl"),
            func_name: String::from(if name == "atan" && argc == 2 {
                "atan2"
            } else {
                name
            }),
            param_types: if name == "ldexp" && argc == 2 {
                alloc::vec![lpir::IrType::F32, lpir::IrType::I32]
            } else {
                alloc::vec![lpir::IrType::F32; argc]
            },
            return_types: alloc::vec![lpir::IrType::F32],
            lpfn_glsl_params: None,
            sret: false,
        });
        self.last_id()
    }

    pub(super) fn lpfn(
        &mut self,
        name: &str,
        glsl_params: String,
        param_types: Vec<lpir::IrType>,
        return_types: Vec<lpir::IrType>,
    ) -> ImportId {
        if let Some(id) = self.registered(|key| {
            matches!(key, ImportKey::Lpfn { name: n, glsl_params: p } if n == name && *p == glsl_params)
        }) {
            return id;
        }
        let key = ImportKey::Lpfn {
            name: String::from(name),
            glsl_params: glsl_params.clone(),
        };
        let func_name = format!("{name}_{}", self.imports.len());
        self.imports.push(ImportInfo {
            key: key.clone(),
            module_name: String::from("lpfn"),
            func_name,
            param_types,
            return_types,
            lpfn_glsl_params: Some(glsl_params),
            sret: false,
        });
        self.last_id()
    }

    pub(super) fn vm(&mut self, name: &str, argc: usize) -> ImportId {
        if let Some(id) = self.registered(
            |key| matches!(key, ImportKey::Vm { name: n, argc: a } if n == name && *a == argc),
        ) {
            return id;
        }
        let key = ImportKey::Vm {
            name: String::from(name),
            argc,
        };
        self.imports.push(ImportInfo {
            key: key.clone(),
            module_name: String::from("vm"),
            func_name: String::from(name),
            param_types: Vec::new(),
            return_types: alloc::vec![lpir::IrType::I32],
            lpfn_glsl_params: None,
            sret: false,
        });
        self.last_id()
    }

    pub(super) fn texture(&mut self, name: &str, argc: usize) -> ImportId {
        if let Some(id) = self.registered(
            |key| matches!(key, ImportKey::Texture { name: n, argc: a } if n == name && *a == argc),
        ) {
            return id;
        }
        let key = ImportKey::Texture {
            name: String::from(name),
            argc,
        };
        self.imports.push(ImportInfo {
            key: key.clone(),
            module_name: String::from("texture"),
            func_name: String::from(name),
            param_types: {
                let mut tys = Vec::with_capacity(argc);
                tys.push(lpir::IrType::Pointer);
                tys.extend((1..argc).map(|_| lpir::IrType::I32));
                tys
            },
            return_types: Vec::new(),
            lpfn_glsl_params: None,
            sret: true,
        });
        self.last_id()
    }

    pub(super) fn into_vec(self) -> Vec<ImportInfo> {
        self.imports
    }
}
