use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lps_shared::LpsType;

use super::types::{HirExpr, HirParam, ImportInfo, ImportKey};

#[derive(Debug, Clone)]
pub(super) struct FunctionSig {
    pub(super) name: String,
    pub(super) return_ty: LpsType,
    pub(super) params: Vec<HirParam>,
}

#[derive(Debug, Clone)]
pub(super) struct GlobalConst {
    pub(super) expr: HirExpr,
}

#[derive(Debug, Default)]
pub(super) struct ImportRegistry {
    pub(super) imports: BTreeMap<ImportKey, ImportInfo>,
}

impl ImportRegistry {
    pub(super) fn glsl(&mut self, name: &str, argc: usize) -> ImportKey {
        let key = ImportKey::Glsl {
            name: String::from(name),
            argc,
        };
        self.imports
            .entry(key.clone())
            .or_insert_with(|| ImportInfo {
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
            });
        key
    }

    pub(super) fn lpfn(
        &mut self,
        name: &str,
        glsl_params: String,
        param_types: Vec<lpir::IrType>,
        return_types: Vec<lpir::IrType>,
    ) -> ImportKey {
        let key = ImportKey::Lpfn {
            name: String::from(name),
            glsl_params: glsl_params.clone(),
        };
        let func_name = format!("{name}_{}", self.imports.len());
        self.imports
            .entry(key.clone())
            .or_insert_with(|| ImportInfo {
                key: key.clone(),
                module_name: String::from("lpfn"),
                func_name,
                param_types,
                return_types,
                lpfn_glsl_params: Some(glsl_params),
            });
        key
    }

    pub(super) fn into_vec(self) -> Vec<ImportInfo> {
        self.imports.into_values().collect()
    }
}
