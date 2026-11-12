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
            param_types: texture_import_params(argc),
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

/// The texture sampler builtins' ABI: the sret pointer, then the texture
/// descriptor (`ptr`, `width`, and for 2D `height`, `row_stride`; for 1D just
/// `row_stride`), the coordinates as **floats**, then the filter and wrap
/// words. The coordinates must be `f32`: under `FloatMode::F32` the builtin
/// takes real floats, and an `i32` declaration made wasm reject the call.
fn texture_import_params(argc: usize) -> Vec<lpir::IrType> {
    use lpir::IrType::{F32, I32, Pointer};
    debug_assert!(argc == 7 || argc == 10, "texture builtins take 7 or 10 args");
    if argc == 7 {
        // texture1d_*: sret, ptr, width, stride, u, filter, wrap_x
        alloc::vec![Pointer, I32, I32, I32, F32, I32, I32]
    } else {
        // texture2d_*: sret, ptr, width, height, stride, u, v, filter, wrap_x, wrap_y
        alloc::vec![Pointer, I32, I32, I32, I32, F32, F32, I32, I32, I32]
    }
}
