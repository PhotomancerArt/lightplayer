//! Print/parse round-trip for [`IrFunction::sret_arg`] and [`ImportDecl::sret`].

use crate::lpir_module::LpirModule;
use crate::parse::parse_module;
use crate::print::print_module;
use crate::types::IrType;
use crate::types::VReg;
use crate::validate::validate_module;

#[test]
fn sret_function_roundtrip() {
    let src = "func @ret_arr(sret v1) {
  return
}
";
    let module = parse_module(src).expect("parse");
    validate_module(&module).expect("validate");
    let printed = print_module(&module);
    assert_eq!(src, printed, "{}", printed);
    let func = module.functions.values().next().expect("one func");
    assert_eq!(func.sret_arg, Some(VReg(1)));
    assert!(func.return_types.is_empty());
    assert_eq!(func.param_count, 0);
    assert_eq!(func.vreg_types[1], IrType::Pointer);
}

#[test]
fn sret_function_with_user_params_roundtrip() {
    let src = "func @ret_arr(sret v1, v2:f32, v3:i32) {
  return
}
";
    let module = parse_module(src).expect("parse");
    validate_module(&module).expect("validate");
    let printed = print_module(&module);
    assert_eq!(src, printed, "{}", printed);
    let func = module.functions.values().next().expect("one func");
    assert_eq!(func.sret_arg, Some(VReg(1)));
    assert_eq!(func.param_count, 2);
    assert_eq!(func.user_param_vreg(0), VReg(2));
    assert_eq!(func.user_param_vreg(1), VReg(3));
}

#[test]
fn sret_import_roundtrip() {
    let src = "import @vm::ret_thing(sret ptr)
";
    let module = parse_module(src).expect("parse");
    validate_module(&module).expect("validate");
    let printed = print_module(&module);
    assert_eq!(src, printed, "{}", printed);
    assert!(module.imports[0].sret);
    assert_eq!(module.imports[0].param_types, [IrType::Pointer]);
    assert!(module.imports[0].return_types.is_empty());
}

#[test]
fn sret_import_with_extra_params_roundtrips() {
    let src = "import @vm::ret_thing(sret ptr, i32)

func @c() {
  return
}
";
    let m = parse_module(src).expect("parse");
    validate_module(&m).expect("validate");
    let s = print_module(&m);
    let m2: LpirModule = parse_module(&s).expect("reparse");
    validate_module(&m2).expect("revalidate");
    assert_eq!(print_module(&m2), s);
    let imp = &m2.imports[0];
    assert!(imp.sret);
    assert_eq!(imp.param_types, [IrType::Pointer, IrType::I32]);
}
