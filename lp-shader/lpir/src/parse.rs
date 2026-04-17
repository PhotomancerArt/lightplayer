//! Text format parser (`&str` → [`crate::lpir_module::LpirModule`]).
//!
//! Hand-rolled scanner (balanced braces, line-oriented body). The stage-II plan
//! mentioned `nom` + `nom_locate`; this implementation keeps the dependency
//! surface minimal while matching the spec grammar.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use crate::builder::{FunctionBuilder, ModuleBuilder};
use crate::lpir_module::{ImportDecl, IrFunction, LpirModule, VMCTX_VREG};
use crate::lpir_op::LpirOp;
use crate::types::{CalleeRef, FuncId, ImportId, IrType, SlotId, VReg};

/// Parse error (line/column best-effort).
#[derive(Debug)]
pub struct ParseError {
    pub line: u32,
    pub column: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.column, self.message)
    }
}

impl core::error::Error for ParseError {}

fn err(line: u32, column: usize, msg: impl Into<String>) -> ParseError {
    ParseError {
        line,
        column,
        message: msg.into(),
    }
}

/// Parse a full LPIR module.
pub fn parse_module(input: &str) -> Result<LpirModule, ParseError> {
    let mut mb = ModuleBuilder::new();
    let mut names: Vec<(String, CalleeRef)> = Vec::new();
    let mut s = input;
    while !s.is_empty() {
        s = skip_ws_comments(s);
        if s.is_empty() {
            break;
        }
        let line_start = s;
        if s.starts_with("import ") {
            let (decl, key) = parse_import(&mut s)?;
            let r = mb.add_import(decl);
            names.push((key, r));
            continue;
        }
        if s.starts_with("entry ") {
            let self_id = mb.next_local_func_id();
            let func = parse_func_decl(
                &mut s,
                true,
                &mut names,
                mb.import_count(),
                self_id,
                mb.imports(),
            )?;
            mb.add_function(func);
            continue;
        }
        if s.starts_with("func ") {
            let self_id = mb.next_local_func_id();
            let func = parse_func_decl(
                &mut s,
                false,
                &mut names,
                mb.import_count(),
                self_id,
                mb.imports(),
            )?;
            mb.add_function(func);
            continue;
        }
        let (line, col) = line_col(input, line_start);
        return Err(err(
            line,
            col,
            format!(
                "expected import, func, or entry func at {:?}",
                line_start.chars().take(20).collect::<String>()
            ),
        ));
    }
    Ok(mb.finish())
}

fn line_col(full: &str, pos: &str) -> (u32, usize) {
    let consumed = full.len().saturating_sub(pos.len());
    let prefix = &full[..consumed];
    let line = prefix.bytes().filter(|&b| b == b'\n').count() as u32 + 1;
    let col = prefix
        .rfind('\n')
        .map(|i| prefix.len() - i)
        .unwrap_or(prefix.len() + 1);
    (line, col)
}

fn skip_ws_comments(mut s: &str) -> &str {
    loop {
        s = s.trim_start();
        if s.starts_with(';') {
            s = s.split('\n').nth(1).unwrap_or("");
            continue;
        }
        break;
    }
    s
}

fn parse_import(s: &mut &str) -> Result<(ImportDecl, String), ParseError> {
    let raw = *s;
    let line_end = raw.find('\n').unwrap_or(raw.len());
    let line = raw[..line_end].trim();
    let after = &raw[line_end.saturating_add(1).min(raw.len())..];
    let rest = line
        .strip_prefix("import")
        .ok_or_else(|| err(1, 1, "import"))?;
    let rest = rest
        .trim_start()
        .strip_prefix('@')
        .ok_or_else(|| err(1, 1, "@"))?;
    let (mod_name, rest) = rest.split_once("::").ok_or_else(|| err(1, 1, "::"))?;
    let (fn_name, rest) = rest.split_once('(').ok_or_else(|| err(1, 1, "("))?;
    let fn_name = fn_name.trim();
    let (inside, rest) = rest
        .split_once(')')
        .ok_or_else(|| err(1, 1, "expected ) after import params"))?;
    let params = parse_type_list_csv(inside)?;
    let rest = rest.trim_start();
    let rets = if let Some(r) = rest.strip_prefix("->") {
        parse_return_types_after_arrow(r.trim_start())?
    } else {
        Vec::new()
    };
    *s = after;
    let key = format!("@{mod_name}::{fn_name}");
    let decl = ImportDecl {
        module_name: mod_name.trim().to_string(),
        func_name: fn_name.to_string(),
        param_types: params,
        return_types: rets,
        lpfn_glsl_params: None,
        needs_vmctx: false,
    };
    Ok((decl, key))
}

fn parse_type_list_csv(s: &str) -> Result<Vec<IrType>, ParseError> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(Vec::new());
    }
    s.split(',')
        .map(|t| match t.trim() {
            "f32" => Ok(IrType::F32),
            "i32" => Ok(IrType::I32),
            "ptr" => Ok(IrType::Pointer),
            _ => Err(err(1, 1, "type")),
        })
        .collect()
}

/// Parse return type(s) after `->` (single type or `(t, u, ...)`). Returns types only.
fn parse_return_types_after_arrow(s: &str) -> Result<Vec<IrType>, ParseError> {
    let s = s.trim();
    if s.starts_with('(') {
        let inner = paren_contents(s)?;
        parse_type_list_csv(inner)
    } else {
        let w = s.split_whitespace().next().unwrap_or(s);
        let t = match w {
            "f32" => IrType::F32,
            "i32" => IrType::I32,
            "ptr" => IrType::Pointer,
            _ => return Err(err(1, 1, "return type")),
        };
        Ok(alloc::vec![t])
    }
}

/// Skip the return-type spelling that [`parse_return_types_after_arrow`] just parsed (`f32` / `i32` / `(…)`).
fn skip_return_type_syntax(tail: &str) -> Result<&str, ParseError> {
    let tail = tail.trim_start();
    if tail.starts_with('(') {
        let inner = paren_contents(tail)?;
        Ok(&tail[inner.len() + 2..])
    } else {
        let w = tail
            .split_whitespace()
            .next()
            .ok_or_else(|| err(1, 1, "expected return type"))?;
        Ok(&tail[w.len()..])
    }
}

/// `s` starts with `(`; returns inner slice between outer parens.
fn paren_contents(s: &str) -> Result<&str, ParseError> {
    let s = s.strip_prefix('(').ok_or_else(|| err(1, 1, "("))?;
    let mut d = 1usize;
    for (i, c) in s.char_indices() {
        match c {
            '(' => d += 1,
            ')' => {
                d -= 1;
                if d == 0 {
                    return Ok(&s[..i]);
                }
            }
            _ => {}
        }
    }
    Err(err(1, 1, "unclosed ("))
}

fn parse_func_decl(
    s: &mut &str,
    is_entry: bool,
    names: &mut Vec<(String, CalleeRef)>,
    import_count: u32,
    self_func_id: FuncId,
    imports: &[ImportDecl],
) -> Result<IrFunction, ParseError> {
    let mut t = *s;
    if is_entry {
        t = t
            .strip_prefix("entry")
            .ok_or_else(|| err(1, 1, "entry"))?
            .trim_start();
        t = t
            .strip_prefix("func")
            .ok_or_else(|| err(1, 1, "func"))?
            .trim_start();
    } else {
        t = t
            .strip_prefix("func")
            .ok_or_else(|| err(1, 1, "func"))?
            .trim_start();
    }
    t = t.strip_prefix('@').ok_or_else(|| err(1, 1, "@name"))?;
    let paren = t
        .find('(')
        .ok_or_else(|| err(1, 1, "expected ( after func name"))?;
    let fname = t[..paren].to_string();
    t = &t[paren..];
    let (params, t) = parse_param_list_str(t)?;
    let t = t.trim_start();
    let (rets, t) = if let Some(after_arrow) = t.strip_prefix("->") {
        let tail = after_arrow.trim_start();
        let rets = parse_return_types_after_arrow(tail)?;
        let t = skip_return_type_syntax(tail)?.trim_start();
        (rets, t)
    } else {
        (Vec::new(), t)
    };
    let t = t.trim_start();
    let t = t.strip_prefix('{').ok_or_else(|| err(1, 1, "expected {"))?;
    let (body, rest) = extract_brace_inner(t)?;
    *s = rest.trim_start();
    let self_ref = CalleeRef::Local(self_func_id);
    names.push((format!("@{fname}"), self_ref));
    parse_function_body(
        fname.as_str(),
        is_entry,
        &params,
        &rets,
        body,
        names.as_slice(),
        import_count,
        imports,
    )
}

fn call_operands_with_vmctx(
    callee: CalleeRef,
    imports: &[ImportDecl],
    user: Vec<VReg>,
) -> Vec<VReg> {
    let need_vmctx = match callee {
        CalleeRef::Local(_) => true,
        CalleeRef::Import(ImportId(i)) => {
            imports.get(i as usize).is_some_and(|imp| imp.needs_vmctx)
        }
    };
    if need_vmctx {
        let mut v = alloc::vec![VMCTX_VREG];
        v.extend(user);
        v
    } else {
        user
    }
}

fn parse_param_list_str(s: &str) -> Result<(Vec<(VReg, IrType)>, &str), ParseError> {
    let s = s.trim_start();
    let s = s
        .strip_prefix('(')
        .ok_or_else(|| err(1, 1, "expected ( for param list"))?;
    let mut depth = 1usize;
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    let inside = s[..i].trim();
                    let rest = &s[i + 1..];
                    let params = if inside.is_empty() {
                        Vec::new()
                    } else {
                        inside
                            .split(',')
                            .map(|p| parse_one_param(p.trim()))
                            .collect::<Result<_, _>>()?
                    };
                    return Ok((params, rest));
                }
            }
            _ => {}
        }
        i += 1;
    }
    Err(err(1, 1, "unclosed ( in param list"))
}

fn parse_one_param(s: &str) -> Result<(VReg, IrType), ParseError> {
    let (v, ty) = s
        .split_once(':')
        .ok_or_else(|| err(1, 1, "param must be vN:ty"))?;
    let vreg = parse_vreg_token(v.trim())?;
    let ty = match ty.trim() {
        "f32" => IrType::F32,
        "i32" => IrType::I32,
        "ptr" => IrType::Pointer,
        _ => return Err(err(1, 1, "param type")),
    };
    Ok((vreg, ty))
}

fn extract_brace_inner(s: &str) -> Result<(&str, &str), ParseError> {
    let mut depth = 1usize;
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok((&s[..i], &s[i + 1..]));
                }
            }
            _ => {}
        }
        i += 1;
    }
    Err(err(1, 1, "unclosed {"))
}

fn next_nonempty_trimmed<'a>(lines: &'a [&'a str], mut j: usize) -> Option<&'a str> {
    while j < lines.len() {
        let t = lines[j].trim();
        if !t.is_empty() && !t.starts_with(';') {
            return Some(t);
        }
        j += 1;
    }
    None
}

fn parse_function_body(
    fname: &str,
    is_entry: bool,
    params: &[(VReg, IrType)],
    returns: &[IrType],
    body: &str,
    names: &[(String, CalleeRef)],
    import_count: u32,
    imports: &[ImportDecl],
) -> Result<IrFunction, ParseError> {
    let mut fb = FunctionBuilder::new(fname, returns);
    if is_entry {
        fb.set_entry();
    }
    let mut expect_v = VMCTX_VREG.0 + 1;
    for (v, t) in params {
        if v.0 != expect_v {
            return Err(err(
                1,
                1,
                alloc::format!(
                    "param vreg must be v{expect_v} next (v0 is VMContext); got v{}",
                    v.0
                ),
            ));
        }
        expect_v += 1;
        fb.add_param(*t);
    }
    let lines: Vec<&str> = body.lines().collect();
    let mut i = 0usize;
    while i < lines.len() {
        let line = lines[i].trim();
        if line.is_empty() || line.starts_with(';') {
            i += 1;
            continue;
        }
        if let Some(rest) = line.strip_prefix("slot ") {
            parse_slot_line(&mut fb, rest)?;
            i += 1;
            continue;
        }
        let peek_next = next_nonempty_trimmed(&lines, i + 1);
        parse_stmt_line(&mut fb, line, names, import_count, imports, peek_next)?;
        i += 1;
    }
    Ok(fb.finish())
}

fn parse_slot_line(fb: &mut FunctionBuilder, rest: &str) -> Result<(), ParseError> {
    let rest = rest.trim();
    let (name, size_s) = rest
        .split_once(',')
        .ok_or_else(|| err(1, 1, "slot line needs comma"))?;
    let name = name.trim();
    let size: u32 = size_s
        .trim()
        .parse()
        .map_err(|_| err(1, 1, "invalid slot size"))?;
    let id = name
        .strip_prefix("ss")
        .ok_or_else(|| err(1, 1, "slot name must be ssN"))?;
    let _n: u32 = id.parse().map_err(|_| err(1, 1, "invalid slot index"))?;
    let got = fb.alloc_slot(size);
    if got.0 != _n {
        return Err(err(
            1,
            1,
            "slot indices must be declared in order ss0, ss1, ...",
        ));
    }
    Ok(())
}

fn parse_stmt_line(
    fb: &mut FunctionBuilder,
    line: &str,
    names: &[(String, CalleeRef)],
    import_count: u32,
    imports: &[ImportDecl],
    peek_next_line: Option<&str>,
) -> Result<(), ParseError> {
    if line == "break" {
        fb.push(LpirOp::Break);
        return Ok(());
    }
    if line == "continue" {
        fb.push(LpirOp::Continue);
        return Ok(());
    }
    if line.starts_with("br_if_not ") {
        let v = parse_vreg_token(line.strip_prefix("br_if_not ").unwrap().trim())?;
        fb.push(LpirOp::BrIfNot { cond: v });
        return Ok(());
    }
    if line.starts_with("if ") {
        let inner = line.strip_prefix("if ").unwrap().trim();
        let cond_s = inner
            .split_whitespace()
            .next()
            .ok_or_else(|| err(1, 1, "if cond"))?;
        let cond = parse_vreg_token(cond_s)?;
        fb.push_if(cond);
        return Ok(());
    }
    if line.starts_with("} else {") {
        fb.push_else();
        return Ok(());
    }
    if line == "continuing:" {
        fb.push_continuing();
        return Ok(());
    }
    if line == "}" {
        fb.close_brace_for_text(peek_next_line)
            .map_err(|m| err(1, 1, m))?;
        return Ok(());
    }
    if line.starts_with("block {") {
        fb.push_block();
        return Ok(());
    }
    if line == "exit_block" {
        fb.push_exit_block();
        return Ok(());
    }
    if line.starts_with("loop {") {
        fb.push_loop();
        return Ok(());
    }
    if line.starts_with("switch ") {
        let rest = line.strip_prefix("switch ").unwrap().trim();
        let sel = parse_vreg_token(rest.split_whitespace().next().unwrap())?;
        fb.push_switch(sel);
        return Ok(());
    }
    if line.starts_with("case ") {
        let rest = line.strip_prefix("case ").unwrap();
        let num_s = rest
            .split_whitespace()
            .next()
            .ok_or_else(|| err(1, 1, "case value"))?;
        let v: i32 = parse_int_literal(num_s)?;
        fb.push_case(v);
        return Ok(());
    }
    if line.starts_with("default {") {
        fb.push_default();
        return Ok(());
    }
    if line.starts_with("return") {
        let rest = line.strip_prefix("return").unwrap().trim();
        if rest.is_empty() {
            fb.push_return(&[]);
        } else {
            let vs: Result<Vec<VReg>, _> = rest
                .split(',')
                .map(|t| parse_vreg_token(t.trim()))
                .collect();
            fb.push_return(&vs?);
        }
        return Ok(());
    }
    if line.starts_with("store ") {
        parse_store(fb, line)?;
        return Ok(());
    }
    if line.starts_with("memcpy ") {
        parse_memcpy(fb, line)?;
        return Ok(());
    }
    if line.contains(" = call ") {
        parse_call_assign(fb, line, names, import_count, imports)?;
        return Ok(());
    }
    if line.contains(" = ") {
        parse_assign_rhs(fb, line)?;
        return Ok(());
    }
    if line.starts_with("call ") {
        parse_call_void(fb, line, names, import_count, imports)?;
        return Ok(());
    }
    Err(err(1, 1, format!("unrecognized statement: {line}")))
}

fn parse_vreg_token(s: &str) -> Result<VReg, ParseError> {
    let s = s.trim();
    let n = s
        .strip_prefix('v')
        .ok_or_else(|| err(1, 1, "expected vN"))?;
    n.parse::<u32>()
        .map(VReg)
        .map_err(|_| err(1, 1, "invalid vreg"))
}

fn parse_int_literal(s: &str) -> Result<i32, ParseError> {
    if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        i32::from_str_radix(h, 16).map_err(|_| err(1, 1, "bad hex int"))
    } else {
        s.parse::<i32>().map_err(|_| err(1, 1, "bad int"))
    }
}

fn parse_store(fb: &mut FunctionBuilder, line: &str) -> Result<(), ParseError> {
    let rest = line.strip_prefix("store ").unwrap();
    let parts: Vec<&str> = rest.split(',').map(str::trim).collect();
    if parts.len() != 3 {
        return Err(err(1, 1, "store base, offset, value"));
    }
    let base = parse_vreg_token(parts[0])?;
    let offset: u32 = parts[1].parse().map_err(|_| err(1, 1, "offset"))?;
    let value = parse_vreg_token(parts[2])?;
    fb.push(LpirOp::Store {
        base,
        offset,
        value,
    });
    Ok(())
}

fn parse_memcpy(fb: &mut FunctionBuilder, line: &str) -> Result<(), ParseError> {
    let rest = line.strip_prefix("memcpy ").unwrap();
    let parts: Vec<&str> = rest.split(',').map(str::trim).collect();
    if parts.len() != 3 {
        return Err(err(1, 1, "memcpy dst, src, size"));
    }
    let dst = parse_vreg_token(parts[0])?;
    let src = parse_vreg_token(parts[1])?;
    let size: u32 = parts[2].parse().map_err(|_| err(1, 1, "size"))?;
    fb.push(LpirOp::Memcpy {
        dst_addr: dst,
        src_addr: src,
        size,
    });
    Ok(())
}

fn resolve_callee(s: &str, names: &[(String, CalleeRef)]) -> Result<CalleeRef, ParseError> {
    let key = s.trim();
    names
        .iter()
        .rev()
        .find(|(k, _)| k == key)
        .map(|(_, r)| *r)
        .ok_or_else(|| err(1, 1, format!("unknown callee {key}")))
}

fn parse_call_void(
    fb: &mut FunctionBuilder,
    line: &str,
    names: &[(String, CalleeRef)],
    _import_count: u32,
    imports: &[ImportDecl],
) -> Result<(), ParseError> {
    let rest = line.strip_prefix("call ").unwrap().trim();
    let (callee_s, args_s) = rest
        .split_once('(')
        .ok_or_else(|| err(1, 1, "call needs ("))?;
    let args_s = args_s
        .strip_suffix(')')
        .ok_or_else(|| err(1, 1, "call needs )"))?;
    let callee = resolve_callee(callee_s, names)?;
    let user = parse_vreg_list(args_s)?;
    let args = call_operands_with_vmctx(callee, imports, user);
    let results: Vec<VReg> = Vec::new();
    fb.push_call(callee, &args, &results);
    Ok(())
}

fn parse_call_assign(
    fb: &mut FunctionBuilder,
    line: &str,
    names: &[(String, CalleeRef)],
    _import_count: u32,
    imports: &[ImportDecl],
) -> Result<(), ParseError> {
    let (lhs, rhs) = line
        .split_once(" = call ")
        .ok_or_else(|| err(1, 1, "call assign"))?;
    let results: Vec<(VReg, Option<IrType>)> = parse_vreg_defs_lhs(lhs)?;
    for (v, ty) in &results {
        fb.record_vreg_def(*v, *ty).map_err(|m| err(1, 1, m))?;
    }
    let results: Vec<VReg> = results.into_iter().map(|(v, _)| v).collect();
    let rest = rhs.trim();
    let (callee_s, args_s) = rest
        .split_once('(')
        .ok_or_else(|| err(1, 1, "call needs ("))?;
    let args_s = args_s
        .strip_suffix(')')
        .ok_or_else(|| err(1, 1, "call needs )"))?;
    let callee = resolve_callee(callee_s.trim(), names)?;
    let user = parse_vreg_list(args_s)?;
    let args = call_operands_with_vmctx(callee, imports, user);
    fb.push_call(callee, &args, &results);
    Ok(())
}

fn parse_vreg_defs_lhs(s: &str) -> Result<Vec<(VReg, Option<IrType>)>, ParseError> {
    let parts: Vec<&str> = s.split(',').map(str::trim).collect();
    let mut out = Vec::new();
    for p in parts {
        out.push(parse_single_vreg_def(p)?);
    }
    Ok(out)
}

fn parse_vreg_list(s: &str) -> Result<Vec<VReg>, ParseError> {
    if s.trim().is_empty() {
        return Ok(Vec::new());
    }
    s.split(',').map(|t| parse_vreg_token(t)).collect()
}

fn parse_assign_rhs(fb: &mut FunctionBuilder, line: &str) -> Result<(), ParseError> {
    let (lhs, rhs) = line.split_once(" = ").unwrap();
    let (dst, maybe_ty) = parse_single_vreg_def(lhs.trim())?;
    fb.record_vreg_def(dst, maybe_ty)
        .map_err(|m| err(1, 1, m))?;
    let rhs = rhs.trim();
    let op = parse_rhs_op(dst, rhs)?;
    fb.push(op);
    Ok(())
}

fn parse_single_vreg_def(s: &str) -> Result<(VReg, Option<IrType>), ParseError> {
    if let Some((v, t)) = s.split_once(':') {
        let vr = parse_vreg_token(v)?;
        let ty = match t.trim() {
            "f32" => IrType::F32,
            "i32" => IrType::I32,
            "ptr" => IrType::Pointer,
            _ => return Err(err(1, 1, "type")),
        };
        Ok((vr, Some(ty)))
    } else {
        Ok((parse_vreg_token(s)?, None))
    }
}

fn parse_rhs_op(dst: VReg, rhs: &str) -> Result<LpirOp, ParseError> {
    let parts: Vec<&str> = rhs.split_whitespace().collect();
    if parts.is_empty() {
        return Err(err(1, 1, "empty rhs"));
    }
    match parts[0] {
        "fadd" => Ok(LpirOp::Fadd {
            dst,
            lhs: parse_vreg_token(parts[1].trim_end_matches(','))?,
            rhs: parse_vreg_token(parts[2])?,
        }),
        "fsub" => Ok(LpirOp::Fsub {
            dst,
            lhs: parse_vreg_token(parts[1].trim_end_matches(','))?,
            rhs: parse_vreg_token(parts[2])?,
        }),
        "fmul" => Ok(LpirOp::Fmul {
            dst,
            lhs: parse_vreg_token(parts[1].trim_end_matches(','))?,
            rhs: parse_vreg_token(parts[2])?,
        }),
        "fdiv" => Ok(LpirOp::Fdiv {
            dst,
            lhs: parse_vreg_token(parts[1].trim_end_matches(','))?,
            rhs: parse_vreg_token(parts[2])?,
        }),
        "fneg" => Ok(LpirOp::Fneg {
            dst,
            src: parse_vreg_token(parts[1])?,
        }),
        "fabs" => Ok(LpirOp::Fabs {
            dst,
            src: parse_vreg_token(parts[1])?,
        }),
        "fsqrt" => Ok(LpirOp::Fsqrt {
            dst,
            src: parse_vreg_token(parts[1])?,
        }),
        "fmin" => Ok(LpirOp::Fmin {
            dst,
            lhs: parse_vreg_token(parts[1].trim_end_matches(','))?,
            rhs: parse_vreg_token(parts[2])?,
        }),
        "fmax" => Ok(LpirOp::Fmax {
            dst,
            lhs: parse_vreg_token(parts[1].trim_end_matches(','))?,
            rhs: parse_vreg_token(parts[2])?,
        }),
        "ffloor" => Ok(LpirOp::Ffloor {
            dst,
            src: parse_vreg_token(parts[1])?,
        }),
        "fceil" => Ok(LpirOp::Fceil {
            dst,
            src: parse_vreg_token(parts[1])?,
        }),
        "ftrunc" => Ok(LpirOp::Ftrunc {
            dst,
            src: parse_vreg_token(parts[1])?,
        }),
        "fnearest" => Ok(LpirOp::Fnearest {
            dst,
            src: parse_vreg_token(parts[1])?,
        }),
        "iadd" => Ok(LpirOp::Iadd {
            dst,
            lhs: parse_vreg_token(parts[1].trim_end_matches(','))?,
            rhs: parse_vreg_token(parts[2])?,
        }),
        "isub" => Ok(LpirOp::Isub {
            dst,
            lhs: parse_vreg_token(parts[1].trim_end_matches(','))?,
            rhs: parse_vreg_token(parts[2])?,
        }),
        "imul" => Ok(LpirOp::Imul {
            dst,
            lhs: parse_vreg_token(parts[1].trim_end_matches(','))?,
            rhs: parse_vreg_token(parts[2])?,
        }),
        "idiv_s" => Ok(LpirOp::IdivS {
            dst,
            lhs: parse_vreg_token(parts[1].trim_end_matches(','))?,
            rhs: parse_vreg_token(parts[2])?,
        }),
        "idiv_u" => Ok(LpirOp::IdivU {
            dst,
            lhs: parse_vreg_token(parts[1].trim_end_matches(','))?,
            rhs: parse_vreg_token(parts[2])?,
        }),
        "irem_s" => Ok(LpirOp::IremS {
            dst,
            lhs: parse_vreg_token(parts[1].trim_end_matches(','))?,
            rhs: parse_vreg_token(parts[2])?,
        }),
        "irem_u" => Ok(LpirOp::IremU {
            dst,
            lhs: parse_vreg_token(parts[1].trim_end_matches(','))?,
            rhs: parse_vreg_token(parts[2])?,
        }),
        "ineg" => Ok(LpirOp::Ineg {
            dst,
            src: parse_vreg_token(parts[1])?,
        }),
        "feq" => Ok(LpirOp::Feq {
            dst,
            lhs: parse_vreg_token(parts[1].trim_end_matches(','))?,
            rhs: parse_vreg_token(parts[2])?,
        }),
        "fne" => Ok(LpirOp::Fne {
            dst,
            lhs: parse_vreg_token(parts[1].trim_end_matches(','))?,
            rhs: parse_vreg_token(parts[2])?,
        }),
        "flt" => Ok(LpirOp::Flt {
            dst,
            lhs: parse_vreg_token(parts[1].trim_end_matches(','))?,
            rhs: parse_vreg_token(parts[2])?,
        }),
        "fle" => Ok(LpirOp::Fle {
            dst,
            lhs: parse_vreg_token(parts[1].trim_end_matches(','))?,
            rhs: parse_vreg_token(parts[2])?,
        }),
        "fgt" => Ok(LpirOp::Fgt {
            dst,
            lhs: parse_vreg_token(parts[1].trim_end_matches(','))?,
            rhs: parse_vreg_token(parts[2])?,
        }),
        "fge" => Ok(LpirOp::Fge {
            dst,
            lhs: parse_vreg_token(parts[1].trim_end_matches(','))?,
            rhs: parse_vreg_token(parts[2])?,
        }),
        "ieq" => Ok(LpirOp::Ieq {
            dst,
            lhs: parse_vreg_token(parts[1].trim_end_matches(','))?,
            rhs: parse_vreg_token(parts[2])?,
        }),
        "ine" => Ok(LpirOp::Ine {
            dst,
            lhs: parse_vreg_token(parts[1].trim_end_matches(','))?,
            rhs: parse_vreg_token(parts[2])?,
        }),
        "ilt_s" => Ok(LpirOp::IltS {
            dst,
            lhs: parse_vreg_token(parts[1].trim_end_matches(','))?,
            rhs: parse_vreg_token(parts[2])?,
        }),
        "ile_s" => Ok(LpirOp::IleS {
            dst,
            lhs: parse_vreg_token(parts[1].trim_end_matches(','))?,
            rhs: parse_vreg_token(parts[2])?,
        }),
        "igt_s" => Ok(LpirOp::IgtS {
            dst,
            lhs: parse_vreg_token(parts[1].trim_end_matches(','))?,
            rhs: parse_vreg_token(parts[2])?,
        }),
        "ige_s" => Ok(LpirOp::IgeS {
            dst,
            lhs: parse_vreg_token(parts[1].trim_end_matches(','))?,
            rhs: parse_vreg_token(parts[2])?,
        }),
        "ilt_u" => Ok(LpirOp::IltU {
            dst,
            lhs: parse_vreg_token(parts[1].trim_end_matches(','))?,
            rhs: parse_vreg_token(parts[2])?,
        }),
        "ile_u" => Ok(LpirOp::IleU {
            dst,
            lhs: parse_vreg_token(parts[1].trim_end_matches(','))?,
            rhs: parse_vreg_token(parts[2])?,
        }),
        "igt_u" => Ok(LpirOp::IgtU {
            dst,
            lhs: parse_vreg_token(parts[1].trim_end_matches(','))?,
            rhs: parse_vreg_token(parts[2])?,
        }),
        "ige_u" => Ok(LpirOp::IgeU {
            dst,
            lhs: parse_vreg_token(parts[1].trim_end_matches(','))?,
            rhs: parse_vreg_token(parts[2])?,
        }),
        "iand" => Ok(LpirOp::Iand {
            dst,
            lhs: parse_vreg_token(parts[1].trim_end_matches(','))?,
            rhs: parse_vreg_token(parts[2])?,
        }),
        "ior" => Ok(LpirOp::Ior {
            dst,
            lhs: parse_vreg_token(parts[1].trim_end_matches(','))?,
            rhs: parse_vreg_token(parts[2])?,
        }),
        "ixor" => Ok(LpirOp::Ixor {
            dst,
            lhs: parse_vreg_token(parts[1].trim_end_matches(','))?,
            rhs: parse_vreg_token(parts[2])?,
        }),
        "ibnot" => Ok(LpirOp::Ibnot {
            dst,
            src: parse_vreg_token(parts[1])?,
        }),
        "ishl" => Ok(LpirOp::Ishl {
            dst,
            lhs: parse_vreg_token(parts[1].trim_end_matches(','))?,
            rhs: parse_vreg_token(parts[2])?,
        }),
        "ishr_s" => Ok(LpirOp::IshrS {
            dst,
            lhs: parse_vreg_token(parts[1].trim_end_matches(','))?,
            rhs: parse_vreg_token(parts[2])?,
        }),
        "ishr_u" => Ok(LpirOp::IshrU {
            dst,
            lhs: parse_vreg_token(parts[1].trim_end_matches(','))?,
            rhs: parse_vreg_token(parts[2])?,
        }),
        "fconst.f32" => {
            let v = parse_f32_literal(parts[1])?;
            Ok(LpirOp::FconstF32 { dst, value: v })
        }
        "iconst.i32" => {
            let v = parse_int_literal(parts[1])?;
            Ok(LpirOp::IconstI32 { dst, value: v })
        }
        "iadd_imm" => Ok(LpirOp::IaddImm {
            dst,
            src: parse_vreg_token(parts[1].trim_end_matches(','))?,
            imm: parse_int_literal(parts[2])?,
        }),
        "isub_imm" => Ok(LpirOp::IsubImm {
            dst,
            src: parse_vreg_token(parts[1].trim_end_matches(','))?,
            imm: parse_int_literal(parts[2])?,
        }),
        "imul_imm" => Ok(LpirOp::ImulImm {
            dst,
            src: parse_vreg_token(parts[1].trim_end_matches(','))?,
            imm: parse_int_literal(parts[2])?,
        }),
        "ishl_imm" => Ok(LpirOp::IshlImm {
            dst,
            src: parse_vreg_token(parts[1].trim_end_matches(','))?,
            imm: parse_int_literal(parts[2])?,
        }),
        "ishr_s_imm" => Ok(LpirOp::IshrSImm {
            dst,
            src: parse_vreg_token(parts[1].trim_end_matches(','))?,
            imm: parse_int_literal(parts[2])?,
        }),
        "ishr_u_imm" => Ok(LpirOp::IshrUImm {
            dst,
            src: parse_vreg_token(parts[1].trim_end_matches(','))?,
            imm: parse_int_literal(parts[2])?,
        }),
        "ieq_imm" => Ok(LpirOp::IeqImm {
            dst,
            src: parse_vreg_token(parts[1].trim_end_matches(','))?,
            imm: parse_int_literal(parts[2])?,
        }),
        "ftoi_sat_s" => Ok(LpirOp::FtoiSatS {
            dst,
            src: parse_vreg_token(parts[1])?,
        }),
        "ftoi_sat_u" => Ok(LpirOp::FtoiSatU {
            dst,
            src: parse_vreg_token(parts[1])?,
        }),
        "itof_s" => Ok(LpirOp::ItofS {
            dst,
            src: parse_vreg_token(parts[1])?,
        }),
        "itof_u" => Ok(LpirOp::ItofU {
            dst,
            src: parse_vreg_token(parts[1])?,
        }),
        "ffrom_i32_bits" => Ok(LpirOp::FfromI32Bits {
            dst,
            src: parse_vreg_token(parts[1])?,
        }),
        "slot_addr" => {
            let name = parts[1].trim();
            let n = name
                .strip_prefix("ss")
                .ok_or_else(|| err(1, 1, "ssN"))?
                .parse::<u32>()
                .map_err(|_| err(1, 1, "slot"))?;
            Ok(LpirOp::SlotAddr {
                dst,
                slot: SlotId(n),
            })
        }
        "load" => Ok(LpirOp::Load {
            dst,
            base: parse_vreg_token(parts[1].trim_end_matches(','))?,
            offset: parts[2].parse().map_err(|_| err(1, 1, "offset"))?,
        }),
        "copy" => Ok(LpirOp::Copy {
            dst,
            src: parse_vreg_token(parts[1])?,
        }),
        "select" => Ok(LpirOp::Select {
            dst,
            cond: parse_vreg_token(parts[1].trim_end_matches(','))?,
            if_true: parse_vreg_token(parts[2].trim_end_matches(','))?,
            if_false: parse_vreg_token(parts[3])?,
        }),
        _ => Err(err(1, 1, format!("unknown op {}", parts[0]))),
    }
}

fn parse_f32_literal(s: &str) -> Result<f32, ParseError> {
    match s {
        "inf" => Ok(f32::INFINITY),
        "-inf" => Ok(f32::NEG_INFINITY),
        "nan" => Ok(f32::NAN),
        _ => s.parse().map_err(|_| err(1, 1, "float")),
    }
}
