// Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License").

//! Top-level pipeline: QIR text → [`Module`] → [`Program`] → OpenQASM text.

use std::collections::HashMap;

use crate::boolean::{
    lower_binary_i1, lower_icmp_i1, lower_int_arith, lower_phi_i1_short_circuit, lower_select_i1,
};
use crate::builders::lower_call;
use crate::cfg::{lower_cfg, BlockLowering};
use crate::error::{QirToQasmError, Result};
use crate::ir::{Block, Instruction, Module, Operand, PhiIncoming};
use crate::oq3::ast::*;
use crate::oq3::printer;
use crate::profile::{base_profile, FunctionBuilder, Profile};
use crate::signatures::{extract_signatures, SignatureTable};
use crate::symbols::{SymbolTable, QUBIT_REGISTER, RESULT_REGISTER};

/// End-to-end QIR → OpenQASM 3 translator.
#[derive(Debug, Clone)]
pub struct Exporter {
    /// The active profile; defaults to [`base_profile`].
    pub profile: Profile,
    /// Any `include "..."` directives to emit at the top of the output.
    pub include_files: Vec<String>,
    /// If `true`, emits `output bit[N] c;` instead of `bit[N] c;`.
    pub emit_output_declarations: bool,
    /// Caller-supplied label surfaced in the trailing `// generated-by:`
    /// comment as the `"producer"` field (e.g. `"mylib 0.1.2"`).
    producer: Option<String>,
}

impl Default for Exporter {
    fn default() -> Self {
        Self::new()
    }
}

impl Exporter {
    /// Create an exporter with the [`base_profile`] and default options.
    pub fn new() -> Self {
        Self {
            profile: base_profile(),
            include_files: Vec::new(),
            emit_output_declarations: false,
            producer: None,
        }
    }

    /// Builder-style: swap in a custom profile.
    pub fn with_profile(mut self, profile: Profile) -> Self {
        self.profile = profile;
        self
    }

    /// Builder-style: set `include_files`.
    pub fn with_includes(mut self, files: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.include_files = files.into_iter().map(Into::into).collect();
        self
    }

    /// Builder-style: toggle `output` prefix on classical-bit declarations.
    pub fn with_output_declarations(mut self, yes: bool) -> Self {
        self.emit_output_declarations = yes;
        self
    }

    /// Builder-style: set the `"producer"` field emitted in the
    /// trailing `// generated-by:` comment (e.g. `"mylib 0.1.2"`).
    /// Quotes and control characters are JSON-escaped. Empty clears.
    pub fn with_producer(mut self, producer: impl Into<String>) -> Self {
        let s = producer.into();
        self.producer = if s.is_empty() { None } else { Some(s) };
        self
    }

    /// Translate a parsed module into an OpenQASM 3 program string.
    ///
    /// Output always ends with a trailing
    /// `// generated-by: {"name":"qirtoqasm",…}` line.
    pub fn dumps(&self, module: &Module) -> Result<String> {
        let program = self.build_program(module)?;
        let mut out = printer::print(&program);
        let input_profile = module.entry_point().and_then(|f| f.qir_profile.clone());
        out.push_str(&self.generated_by_line(input_profile.as_deref()));
        Ok(out)
    }

    /// Render the trailing `// generated-by: {…}` line. Keys: `name`,
    /// `version`, then optionally `profile` (the input QIR profile),
    /// then optionally `producer`.
    fn generated_by_line(&self, input_profile: Option<&str>) -> String {
        let mut s = String::from("// generated-by: {");
        s.push_str(r#""name":"qirtoqasm","version":""#);
        json_escape_into(crate::VERSION, &mut s);
        s.push('"');
        if let Some(p) = input_profile {
            s.push_str(r#","profile":""#);
            json_escape_into(p, &mut s);
            s.push('"');
        }
        if let Some(p) = &self.producer {
            s.push_str(r#","producer":""#);
            json_escape_into(p, &mut s);
            s.push('"');
        }
        s.push('}');
        s.push('\n');
        s
    }

    fn build_program(&self, module: &Module) -> Result<Program> {
        let signatures = extract_signatures(&module.source_text)?;
        let entry = module.entry_point().ok_or_else(|| {
            QirToQasmError::syntax(
                "no entry-point function found; QIR modules must have a function \
                 tagged with the 'entry_point' attribute",
            )
        })?;

        let mut symbols = SymbolTable::new();
        let block_names = assign_block_names(&entry.blocks);

        let mut block_lowerings: Vec<BlockLowering> = Vec::with_capacity(entry.blocks.len());
        let mut int_declarations: Vec<Statement> = Vec::new();
        for (i, block) in entry.blocks.iter().enumerate() {
            let canonical = &block_names[i];
            let lowering = self.lower_block(
                block,
                canonical,
                &signatures,
                &mut symbols,
                &mut block_lowerings,
                &mut int_declarations,
            )?;
            block_lowerings.push(lowering);
        }

        let entry_block_name = block_lowerings[0].name.clone();
        let body = lower_cfg(block_lowerings, &entry_block_name)?;

        let mut statements: Vec<Statement> = Vec::new();
        for inc in &self.include_files {
            statements.push(Statement::Include(inc.clone()));
        }
        if symbols.max_qubit_index >= 0 {
            let size = symbols.max_qubit_index.checked_add(1).ok_or_else(|| {
                QirToQasmError::unsupported("qubit index too large to size the qubit register")
            })? as u64;
            statements.push(Statement::QubitDeclaration {
                size,
                name: QUBIT_REGISTER.into(),
            });
        }
        if symbols.max_result_index >= 0 {
            let bit_size = symbols.max_result_index.checked_add(1).ok_or_else(|| {
                QirToQasmError::unsupported("result index too large to size the result register")
            })? as u64;
            if self.emit_output_declarations {
                statements.push(Statement::IODeclaration {
                    io_kind: IoKind::Output,
                    bit_size,
                    name: RESULT_REGISTER.into(),
                });
            } else {
                statements.push(Statement::ClassicalDeclaration {
                    bit_size,
                    name: RESULT_REGISTER.into(),
                });
            }
        }
        // Classical int variables introduced by phi-integer lowering.
        statements.extend(int_declarations);
        statements.extend(body);

        Ok(Program {
            version: "3.0".into(),
            statements,
        })
    }

    fn lower_block(
        &self,
        block: &Block,
        canonical_name: &str,
        signatures: &SignatureTable,
        symbols: &mut SymbolTable,
        prior_lowerings: &mut [BlockLowering],
        int_declarations: &mut Vec<Statement>,
    ) -> Result<BlockLowering> {
        let mut stmts: Vec<Statement> = Vec::new();
        let mut condition: Option<Expression> = None;
        let mut targets: Vec<String> = Vec::new();

        for inst in &block.instructions {
            match inst {
                Instruction::Call {
                    callee,
                    args,
                    result,
                    ..
                } => {
                    let sig = signatures.get(callee).ok_or_else(|| {
                        QirToQasmError::unsupported(format!(
                            "no signature for callee {:?} in QIR source",
                            callee
                        ))
                    })?;
                    let builder = self.profile.get_builder(callee).ok_or_else(|| {
                        if sig.is_variadic {
                            QirToQasmError::unsupported(format!(
                                "variadic QIR function '{}' is not supported; only \
                                 'generalizedInvokeWithRotationsControlsTargets' is; \
                                 multi-controlled gates must be decomposed into single- \
                                 and two-qubit gates before translation",
                                callee
                            ))
                        } else {
                            QirToQasmError::unsupported(format!(
                                "no builder registered for QIR function '{}'; extend the \
                                 Profile to support it",
                                callee
                            ))
                        }
                    })?;
                    let variadic_allowed = matches!(
                        builder,
                        FunctionBuilder::RecordOutputNoop | FunctionBuilder::GeneralizedControlled
                    );
                    if sig.is_variadic && !variadic_allowed {
                        return Err(QirToQasmError::unsupported(format!(
                            "QIR function {:?} is variadic and cannot be lowered by the \
                             default dispatch; variadic lowering requires a specialized \
                             builder that inspects the runtime operand list",
                            callee
                        )));
                    }
                    if !sig.is_variadic && args.len() != sig.param_types.len() {
                        return Err(QirToQasmError::unsupported(format!(
                            "arity mismatch calling {:?}: IR supplied {} operand(s) but \
                             signature expects {}",
                            callee,
                            args.len(),
                            sig.param_types.len()
                        )));
                    }
                    let new_stmts = lower_call(builder, sig, args, result.as_deref(), symbols)?;
                    stmts.extend(new_stmts);
                }
                Instruction::Icmp(icmp) => {
                    lower_icmp_i1(
                        &icmp.result,
                        &icmp.predicate.0,
                        &icmp.lhs,
                        &icmp.rhs,
                        symbols,
                    )?;
                }
                Instruction::BinaryI1 {
                    result,
                    op,
                    lhs,
                    rhs,
                } => {
                    lower_binary_i1(result, *op, lhs, rhs, symbols)?;
                }
                Instruction::Select {
                    result,
                    value_type,
                    cond,
                    true_value,
                    false_value,
                } => {
                    lower_select_i1(result, value_type, cond, true_value, false_value, symbols)?;
                }
                Instruction::IntArith {
                    result,
                    op,
                    lhs,
                    rhs,
                } => {
                    lower_int_arith(result, *op, lhs, rhs, symbols)?;
                }
                Instruction::Phi {
                    result,
                    value_type,
                    incomings,
                } => {
                    if let Some(inc) = incomings.iter().find(|i| i.pred == canonical_name) {
                        return Err(QirToQasmError::unsupported(format!(
                            "loop-carried phi (predecessor {:?} is the phi's own block \
                             {:?}) is not supported",
                            inc.pred, canonical_name
                        )));
                    }
                    if value_type == "i1" {
                        let pred_conditions: HashMap<String, Expression> = prior_lowerings
                            .iter()
                            .filter_map(|p| {
                                p.condition.as_ref().map(|c| (p.name.clone(), c.clone()))
                            })
                            .collect();
                        lower_phi_i1_short_circuit(result, incomings, &pred_conditions, symbols)?;
                    } else if value_type == "i64" || value_type == "i32" {
                        lower_phi_integer(
                            result,
                            incomings,
                            prior_lowerings,
                            int_declarations,
                            symbols,
                        )?;
                    } else {
                        return Err(QirToQasmError::unsupported(format!(
                            "phi {value_type} is not yet supported (SSA {result:?}); \
                             only `phi i1` and `phi i32`/`phi i64` are lowered today"
                        )));
                    }
                }
                Instruction::Br { target } => {
                    targets.push(target.clone());
                    break;
                }
                Instruction::BrCond {
                    cond,
                    true_target,
                    false_target,
                } => {
                    let expr = match cond {
                        crate::ir::BrCondOperand::Ssa(id) => symbols.lookup_ssa(id)?,
                        crate::ir::BrCondOperand::Const(b) => Expression::Boolean(*b),
                    };
                    condition = Some(expr);
                    targets.push(true_target.clone());
                    targets.push(false_target.clone());
                    break;
                }
                Instruction::Ret => break,
                Instruction::Ignored { .. } => continue,
                Instruction::Alloca { result } => {
                    symbols.record_alloca(result);
                }
                Instruction::BitcastAlias { result, src } => {
                    symbols.record_alias(result, src, 0);
                }
                Instruction::GetElementPtrOffset {
                    result,
                    src,
                    offset,
                } => {
                    symbols.record_alias(result, src, *offset);
                }
                Instruction::Store { value, ptr, .. } => {
                    symbols.store_to_alloca_slot(ptr, value.clone());
                }
                Instruction::Load { result, ptr } => {
                    if let Some(expr) = symbols.load_from_alloca_slot(ptr) {
                        symbols.record_ssa(result, expr);
                    }
                }
                Instruction::Zext { result, src } => {
                    // `zext i1 %x to iN` — bind the widened SSA to the source expression,
                    // which already reads as 0/1 in arithmetic contexts.
                    let expr = symbols.lookup_ssa(src)?;
                    symbols.record_ssa(result, expr);
                }
                Instruction::Unsupported { opcode } => {
                    return Err(QirToQasmError::unsupported(format!(
                        "unsupported LLVM instruction opcode {:?} in block {:?}",
                        opcode, canonical_name
                    )));
                }
            }
        }
        Ok(BlockLowering {
            name: canonical_name.to_string(),
            statements: stmts,
            condition,
            targets,
        })
    }
}

/// Lower a `phi i32`/`phi i64` if-merge into an `int` variable
/// declared at program top plus an assignment on the unconditional
/// predecessor.
fn lower_phi_integer(
    result: &str,
    incomings: &[PhiIncoming],
    prior_lowerings: &mut [BlockLowering],
    int_declarations: &mut Vec<Statement>,
    symbols: &mut SymbolTable,
) -> Result<()> {
    if incomings.len() != 2 {
        return Err(QirToQasmError::unsupported(format!(
            "phi integer with {} incoming value(s); only the two-incoming \
             if-merge shape (`mutable` + conditional increment) is supported",
            incomings.len()
        )));
    }

    if try_bind_phi_i64_landing_pad(result, incomings, prior_lowerings, symbols) {
        return Ok(());
    }

    let (uncond_inc, cond_inc) = {
        let inc0 = &incomings[0];
        let inc1 = &incomings[1];
        let pred0 = prior_lowerings
            .iter()
            .find(|b| b.name == inc0.pred)
            .ok_or_else(|| {
                QirToQasmError::unsupported(format!(
                    "phi integer predecessor {:?} not found among processed blocks",
                    inc0.pred
                ))
            })?;
        let pred1 = prior_lowerings
            .iter()
            .find(|b| b.name == inc1.pred)
            .ok_or_else(|| {
                QirToQasmError::unsupported(format!(
                    "phi integer predecessor {:?} not found among processed blocks",
                    inc1.pred
                ))
            })?;
        match (pred0.condition.is_some(), pred1.condition.is_some()) {
            (true, false) => (inc1, inc0),
            (false, true) => (inc0, inc1),
            _ => {
                return Err(QirToQasmError::unsupported(
                    "phi integer incomings do not match the expected if-merge \
                     shape: expected exactly one predecessor with a conditional \
                     branch (providing the init value) and one with an \
                     unconditional branch (providing the update value)",
                ));
            }
        }
    };

    let init_expr = resolve_phi_integer_operand(&cond_inc.value, symbols)?;
    let update_expr = resolve_phi_integer_operand(&uncond_inc.value, symbols)?;

    // Reuse a prior phi variable when the init side is one.
    let var_name = match &init_expr {
        Expression::Identifier(name) if is_declared_int_var(int_declarations, name) => name.clone(),
        _ => {
            let name = format!("cint_{}", int_declarations.len());
            int_declarations.push(Statement::IntDeclaration {
                name: name.clone(),
                init: init_expr,
            });
            name
        }
    };

    if let Some(pred_block) = prior_lowerings
        .iter_mut()
        .find(|b| b.name == uncond_inc.pred)
    {
        // Skip no-op `var = var;` updates.
        if !is_identity_update(&update_expr, &var_name) {
            pred_block.statements.push(Statement::Assignment {
                target: ident(&var_name),
                value: update_expr,
            });
        }
    }

    symbols.record_ssa(result, Expression::Identifier(var_name));
    Ok(())
}

/// Recognize `phi i64 [A, %bb_t], [B, %bb_f]` where the two preds are
/// empty unconditional landing pads sharing a conditional-br ancestor
/// and (A, B) is a permutation of (0, 1). Binds the phi SSA to the
/// ancestor's predicate (or `1 - predicate` when inverted). Returns
/// `false` to fall through to the regular if-merge handler.
fn try_bind_phi_i64_landing_pad(
    result: &str,
    incomings: &[PhiIncoming],
    prior_lowerings: &[BlockLowering],
    symbols: &mut SymbolTable,
) -> bool {
    let as_bit = |op: &Operand| -> Option<bool> {
        match op {
            Operand::ConstInt(0) => Some(false),
            Operand::ConstInt(1) => Some(true),
            Operand::ConstBool(b) => Some(*b),
            _ => None,
        }
    };
    let v0 = as_bit(&incomings[0].value);
    let v1 = as_bit(&incomings[1].value);
    let (v0, _v1) = match (v0, v1) {
        (Some(a), Some(b)) if a != b => (a, b),
        _ => return false,
    };

    let pred0_name = &incomings[0].pred;
    let pred1_name = &incomings[1].pred;
    let is_landing_pad = |name: &str| {
        prior_lowerings
            .iter()
            .find(|b| b.name == name)
            .map(|b| b.condition.is_none() && b.statements.is_empty() && b.targets.len() == 1)
            .unwrap_or(false)
    };
    if !is_landing_pad(pred0_name) || !is_landing_pad(pred1_name) {
        return false;
    }

    let ancestor = prior_lowerings.iter().find(|b| {
        b.condition.is_some()
            && b.targets.len() == 2
            && b.targets.contains(pred0_name)
            && b.targets.contains(pred1_name)
    });
    let Some(ancestor) = ancestor else {
        return false;
    };
    let cond_expr = ancestor.condition.clone().expect("checked above");
    let true_target = &ancestor.targets[0];
    let true_value = if pred0_name == true_target { v0 } else { _v1 };

    let bound = if true_value {
        cond_expr
    } else {
        // Inverted: emit `1 - <pred>` to keep the result integer-typed.
        bin(BinaryOp::Sub, int(1), cond_expr)
    };
    symbols.record_ssa(result, bound);
    true
}

fn resolve_phi_integer_operand(op: &Operand, symbols: &SymbolTable) -> Result<Expression> {
    match op {
        Operand::ConstInt(n) => i64::try_from(*n).map(Expression::Integer).map_err(|_| {
            QirToQasmError::unsupported(format!(
                "phi integer incoming constant {n} does not fit in i64"
            ))
        }),
        Operand::ConstBool(b) => Ok(Expression::Integer(if *b { 1 } else { 0 })),
        Operand::Ssa(id) => symbols.lookup_ssa(id),
        _ => Err(QirToQasmError::unsupported(format!(
            "phi integer incoming must be a constant or SSA reference, got {op:?}"
        ))),
    }
}

fn is_declared_int_var(int_declarations: &[Statement], name: &str) -> bool {
    int_declarations
        .iter()
        .any(|s| matches!(s, Statement::IntDeclaration { name: n, .. } if n == name))
}

fn is_identity_update(expr: &Expression, var_name: &str) -> bool {
    matches!(expr, Expression::Identifier(n) if n == var_name)
}

fn assign_block_names(blocks: &[Block]) -> Vec<String> {
    let mut out = Vec::with_capacity(blocks.len());
    for (i, b) in blocks.iter().enumerate() {
        if !b.name.is_empty() {
            out.push(b.name.clone());
        } else if i == 0 {
            out.push("0".to_string());
        } else {
            out.push(i.to_string());
        }
    }
    out
}

/// JSON-escape `s` into `buf`. Handles quote, backslash, and ASCII
/// control characters — enough for the `// generated-by:` line.
fn json_escape_into(s: &str, buf: &mut String) {
    for c in s.chars() {
        match c {
            '"' => buf.push_str(r#"\""#),
            '\\' => buf.push_str(r"\\"),
            '\n' => buf.push_str(r"\n"),
            '\r' => buf.push_str(r"\r"),
            '\t' => buf.push_str(r"\t"),
            '\u{0008}' => buf.push_str(r"\b"),
            '\u{000C}' => buf.push_str(r"\f"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                buf.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => buf.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::parser::parse_module;

    const BELL_LL: &str = "\
%Qubit = type opaque
%Result = type opaque

define void @main() #0 {
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__cnot__body(%Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  ret void
}

declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__cnot__body(%Qubit*, %Qubit*)
declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1

attributes #0 = { \"entry_point\" \"qir_profiles\"=\"base_profile\" \"requiredQubits\"=\"2\" \"requiredResults\"=\"2\" }
attributes #1 = { \"irreversible\" }
";

    #[test]
    fn translates_bell_state() {
        let module = parse_module(BELL_LL).unwrap();
        let out = Exporter::new().dumps(&module).unwrap();
        let expected = format!(
            "\
OPENQASM 3.0;
qubit[2] q;
bit[2] c;
h q[0];
cnot q[0], q[1];
c[0] = measure q[0];
c[1] = measure q[1];
// generated-by: {{\"name\":\"qirtoqasm\",\"version\":\"{version}\",\"profile\":\"base_profile\"}}
",
            version = crate::VERSION,
        );
        assert_eq!(out, expected);
    }

    #[test]
    fn no_entry_point_errors_with_pinned_substring() {
        let module = parse_module("%Qubit = type opaque\n").unwrap();
        let err = Exporter::new().dumps(&module).unwrap_err();
        assert!(err.to_string().contains("no entry-point function found"));
    }
}

#[cfg(test)]
mod more_tests {
    use super::*;
    use crate::ir::parser::parse_module;
    use crate::test_support::*;

    #[test]
    fn exporter_builders_return_fluent_self() {
        let e = Exporter::new()
            .with_includes(["stdgates.inc"])
            .with_output_declarations(true)
            .with_profile(crate::profile::base_profile());
        assert_eq!(e.include_files, vec!["stdgates.inc".to_string()]);
        assert!(e.emit_output_declarations);
    }

    #[test]
    fn emit_output_declarations_option_prepends_output_keyword() {
        let qir = qir_entry(
            "  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  ret void",
        );
        let module = parse_module(&qir).unwrap();
        let out = Exporter::new()
            .with_output_declarations(true)
            .dumps(&module)
            .unwrap();
        assert!(out.contains("output bit[1] c;"), "{out}");
    }

    #[test]
    fn include_files_emit_before_qubit_declaration() {
        let qir = qir_entry(
            "  call void @__quantum__qis__h__body(%Qubit* null)
  ret void",
        );
        let module = parse_module(&qir).unwrap();
        let out = Exporter::new()
            .with_includes(["stdgates.inc"])
            .dumps(&module)
            .unwrap();
        assert!(out.contains("include \"stdgates.inc\";"), "{out}");
    }

    #[test]
    fn default_impl_matches_new() {
        let a = Exporter::default();
        let b = Exporter::new();
        assert_eq!(a.include_files, b.include_files);
        assert_eq!(a.emit_output_declarations, b.emit_output_declarations);
    }

    #[test]
    fn unknown_callee_reports_missing_signature_when_declare_is_absent() {
        // No declaration for the callee → extract_signatures never learns
        // about it, so `no signature for callee` fires before `no builder`.
        let ir = "\
%Qubit = type opaque
define void @main() #0 {
  call void @phantom(%Qubit* null)
  ret void
}
attributes #0 = { \"entry_point\" }
";
        let module = parse_module(ir).unwrap();
        let err = Exporter::new().dumps(&module).unwrap_err();
        assert!(err.to_string().contains("no signature for callee"));
    }

    #[test]
    fn unknown_callee_with_declaration_reports_missing_builder() {
        let err = translate_err(
            "  call void @unknown_intrinsic(%Qubit* null)
  ret void",
        );
        assert!(err.contains("no builder registered"));
    }

    #[test]
    fn arity_mismatch_surfaces_pinned_error() {
        // h takes one qubit; hand it two.
        let err = translate_err(
            "  call void @__quantum__qis__h__body(%Qubit* null, %Qubit* null)
  ret void",
        );
        assert!(err.contains("arity mismatch"));
    }

    #[test]
    fn unsupported_opcode_in_block_surfaces_pinned_error() {
        let err = translate_err(
            "  %x = fadd double 0.0, 1.0
  ret void",
        );
        assert!(err.contains("unsupported LLVM instruction opcode"));
    }

    #[test]
    fn assign_block_names_renumbers_unlabelled_blocks() {
        // All blocks unlabelled except for the `next:` label.
        let ir = "\
%Qubit = type opaque
define void @main() #0 {
  call void @__quantum__qis__h__body(%Qubit* null)
  br label %next
next:
  ret void
}
declare void @__quantum__qis__h__body(%Qubit*)
attributes #0 = { \"entry_point\" }
";
        let module = parse_module(ir).unwrap();
        let out = Exporter::new().dumps(&module).unwrap();
        assert!(out.contains("h q[0]"));
    }
}

#[cfg(test)]
mod final_coverage {
    use super::*;
    use crate::ir::parser::parse_module;

    #[test]
    fn variadic_with_registered_builder_still_rejected_internal() {
        let mut profile = crate::profile::base_profile();
        profile.register(
            "variadic_fn",
            crate::profile::FunctionBuilder::Gate {
                gate_name: "nope".into(),
                adjoint: false,
            },
        );
        let ir = "\
%Qubit = type opaque
define void @main() #0 {
  call void (i64, ...) @variadic_fn(i64 0, i64 1)
  ret void
}
declare void @variadic_fn(i64, ...)
attributes #0 = { \"entry_point\" }
";
        let module = parse_module(ir).unwrap();
        let err = Exporter::new()
            .with_profile(profile)
            .dumps(&module)
            .unwrap_err();
        assert!(err
            .to_string()
            .contains("is variadic and cannot be lowered"));
    }

    #[test]
    fn block_name_falls_back_to_position_for_many_unlabelled_blocks() {
        // Several unlabelled blocks in sequence exercise the position-based
        // fallback in assign_block_names.
        let ir = "\
%Qubit = type opaque
define void @main() #0 {
  br label %1
1:
  call void @__quantum__qis__h__body(%Qubit* null)
  br label %2
2:
  ret void
}
declare void @__quantum__qis__h__body(%Qubit*)
attributes #0 = { \"entry_point\" }
";
        let module = parse_module(ir).unwrap();
        let out = Exporter::new().dumps(&module).unwrap();
        assert!(out.contains("h q[0]"), "{out}");
    }

    #[test]
    fn constant_branch_condition_propagates_to_oq3_boolean_literal() {
        // br i1 1 takes the true path; br i1 0 takes the false path.
        // Exercising this confirms the BrCondOperand::Const variant
        // reaches Expression::Boolean.
        let ir = "\
%Qubit = type opaque
define void @main() #0 {
  call void @__quantum__qis__h__body(%Qubit* null)
  br i1 1, label %t, label %f
t:
  call void @__quantum__qis__x__body(%Qubit* null)
  br label %join
f:
  call void @__quantum__qis__z__body(%Qubit* null)
  br label %join
join:
  ret void
}
declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__x__body(%Qubit*)
declare void @__quantum__qis__z__body(%Qubit*)
attributes #0 = { \"entry_point\" }
";
        let module = parse_module(ir).unwrap();
        let out = Exporter::new().dumps(&module).unwrap();
        assert!(out.contains("if (true)"), "{out}");
    }
}

#[cfg(test)]
mod more_fine_tests {
    use super::*;

    #[test]
    fn exporter_debug_format_covers_derive() {
        let dbg = format!("{:?}", Exporter::new());
        assert!(dbg.contains("Exporter"));
    }

    #[test]
    fn assign_block_names_position_fallback_for_unlabelled_non_entry_block() {
        // A define where llvmlite-style unlabelled blocks fall through
        // to the position-based "%i" fallback in assign_block_names.
        use crate::ir::Block;
        let blocks = vec![
            Block {
                name: String::new(),
                instructions: vec![],
            },
            Block {
                name: String::new(),
                instructions: vec![],
            },
            Block {
                name: String::new(),
                instructions: vec![],
            },
        ];
        let names = assign_block_names(&blocks);
        assert_eq!(
            names,
            vec!["0".to_string(), "1".to_string(), "2".to_string()]
        );
    }
}

#[cfg(test)]
mod new_lowering_dispatch {
    //! End-to-end coverage for the `And` / `Or` / `Select` / `IntArith`
    //! instruction-dispatch arms and the variadic-unregistered-callee
    //! error path. Each fixture is a minimal adaptive-profile QIR module
    //! that exercises exactly one new instruction variant and asserts
    //! the translator binds the result SSA into the resulting `if`
    //! condition.

    use super::*;
    use crate::ir::parser::parse_module;

    fn translate(ll: &str) -> String {
        let module = parse_module(ll).unwrap();
        Exporter::new().dumps(&module).unwrap()
    }

    #[test]
    fn and_i1_flows_into_if_condition() {
        let ll = "\
%Qubit = type opaque
%Result = type opaque

define void @main() #0 {
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  %a = call i1 @__quantum__qis__read_result__body(%Result* null)
  %b = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 1 to %Result*))
  %c = and i1 %a, %b
  br i1 %c, label %then, label %end
then:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  br label %end
end:
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
  ret void
}

declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__x__body(%Qubit*)
declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1
declare i1 @__quantum__qis__read_result__body(%Result*)

attributes #0 = { \"entry_point\" \"qir_profiles\"=\"adaptive_profile\" \"requiredQubits\"=\"3\" \"requiredResults\"=\"3\" }
attributes #1 = { \"irreversible\" }
";
        let out = translate(ll);
        assert!(out.contains("if (c[0] == 1 && c[1] == 1)"), "{out}");
    }

    #[test]
    fn or_i1_flows_into_if_condition() {
        let ll = "\
%Qubit = type opaque
%Result = type opaque

define void @main() #0 {
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  %a = call i1 @__quantum__qis__read_result__body(%Result* null)
  %b = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 1 to %Result*))
  %c = or i1 %a, %b
  br i1 %c, label %then, label %end
then:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  br label %end
end:
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
  ret void
}

declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__x__body(%Qubit*)
declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1
declare i1 @__quantum__qis__read_result__body(%Result*)

attributes #0 = { \"entry_point\" \"qir_profiles\"=\"adaptive_profile\" \"requiredQubits\"=\"3\" \"requiredResults\"=\"3\" }
attributes #1 = { \"irreversible\" }
";
        let out = translate(ll);
        assert!(out.contains("if (c[0] == 1 || c[1] == 1)"), "{out}");
    }

    #[test]
    fn select_i1_flows_into_if_condition() {
        // Emits the compiler short-circuit shape `select i1 %a, i1 %b, i1 false`
        // (`a && b`), which the lowering recognizes and reduces to `a && b`.
        let ll = "\
%Qubit = type opaque
%Result = type opaque

define void @main() #0 {
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  %a = call i1 @__quantum__qis__read_result__body(%Result* null)
  %b = call i1 @__quantum__qis__read_result__body(%Result* inttoptr (i64 1 to %Result*))
  %c = select i1 %a, i1 %b, i1 false
  br i1 %c, label %then, label %end
then:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
  br label %end
end:
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
  ret void
}

declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__x__body(%Qubit*)
declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1
declare i1 @__quantum__qis__read_result__body(%Result*)

attributes #0 = { \"entry_point\" \"qir_profiles\"=\"adaptive_profile\" \"requiredQubits\"=\"3\" \"requiredResults\"=\"3\" }
attributes #1 = { \"irreversible\" }
";
        let out = translate(ll);
        assert!(out.contains("if (c[0] == 1 && c[1] == 1)"), "{out}");
    }

    #[test]
    fn int_arith_add_flows_into_icmp_comparison() {
        // Exercises the IntArith dispatch arm plus an `icmp ult` ordered
        // predicate. The `add` result is inlined into the comparison, so
        // the emitted OpenQASM should contain a `... + 1 < ...` expression
        // inside the `if` condition.
        let ll = "\
%Qubit = type opaque
%Result = type opaque

define void @main() #0 {
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  %a = call i1 @__quantum__qis__read_result__body(%Result* null)
  %b = add i32 0, 1
  %c = icmp ult i32 %b, 3
  br i1 %c, label %then, label %end
then:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  br label %end
end:
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  ret void
}

declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__x__body(%Qubit*)
declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1
declare i1 @__quantum__qis__read_result__body(%Result*)

attributes #0 = { \"entry_point\" \"qir_profiles\"=\"adaptive_profile\" \"requiredQubits\"=\"2\" \"requiredResults\"=\"2\" }
attributes #1 = { \"irreversible\" }
";
        let out = translate(ll);
        assert!(out.contains("if (0 + 1 < 3)"), "{out}");
    }

    #[test]
    fn unregistered_variadic_callee_surfaces_descriptive_error() {
        // A variadic callee with no registered builder is rejected with
        // an error naming the callee and mentioning the variadic-multi-
        // controlled idiom that motivates the check.
        let ll = "\
%Qubit = type opaque
%Result = type opaque

define void @main() #0 {
  call void (i64, i64, ...) @someUnknownVariadic(i64 0, i64 0)
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  ret void
}

declare void @someUnknownVariadic(i64, i64, ...)
declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1

attributes #0 = { \"entry_point\" \"qir_profiles\"=\"adaptive_profile\" \"requiredQubits\"=\"1\" \"requiredResults\"=\"1\" }
attributes #1 = { \"irreversible\" }
";
        let err = parse_module(ll)
            .and_then(|m| Exporter::new().dumps(&m))
            .unwrap_err();
        assert!(err.to_string().contains("someUnknownVariadic"));
        assert!(err
            .to_string()
            .contains("generalizedInvokeWithRotationsControlsTargets"));
    }
}

#[cfg(test)]
mod new_lowering_tests {
    //! Direct Rust coverage for the phi-i64, select-integer,
    //! alloca/store/load, and Zext lowering paths. Python tests
    //! exercise the same code but go through the PyO3 wheel;
    //! `cargo llvm-cov` only counts Rust-level coverage.

    fn translate(ll: &str) -> crate::Result<String> {
        crate::translate(ll, &crate::TranslateOptions::default())
    }

    #[test]
    fn alloca_store_load_folds_scalar_constant() {
        let ll = r#"
%Qubit = type opaque
%Result = type opaque

define void @main() #0 {
  %buf = alloca [2 x double], align 8
  %p0 = bitcast [2 x double]* %buf to double*
  store double 0.5, double* %p0, align 8
  %p1 = getelementptr [2 x double], [2 x double]* %buf, i32 0, i32 1
  store double 0.25, double* %p1, align 8
  %v0 = load double, double* %p0, align 8
  call void @__quantum__qis__rx__body(double %v0, %Qubit* null)
  %v1 = load double, double* %p1, align 8
  call void @__quantum__qis__ry__body(double %v1, %Qubit* null)
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  ret void
}
declare void @__quantum__qis__rx__body(double, %Qubit*)
declare void @__quantum__qis__ry__body(double, %Qubit*)
declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1
attributes #0 = { "entry_point" "qir_profiles"="base_profile" "requiredQubits"="1" "requiredResults"="1" }
attributes #1 = { "irreversible" }
"#;
        let qasm = translate(ll).unwrap();
        assert!(qasm.contains("rx(0.5)"), "\n{qasm}");
        assert!(qasm.contains("ry(0.25)"), "\n{qasm}");
    }

    #[test]
    fn load_from_untracked_pointer_leaves_ssa_unbound() {
        // Load from a pointer that was never alloca'd or stored into.
        // The load SSA stays unbound; downstream use fails with the
        // "SSA value … is used but was never bound" error.
        let ll = r#"
%Qubit = type opaque
%Result = type opaque

define void @main(double* %external) #0 {
  %v = load double, double* %external, align 8
  call void @__quantum__qis__rx__body(double %v, %Qubit* null)
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  ret void
}
declare void @__quantum__qis__rx__body(double, %Qubit*)
declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1
attributes #0 = { "entry_point" "qir_profiles"="base_profile" "requiredQubits"="1" "requiredResults"="1" }
attributes #1 = { "irreversible" }
"#;
        let err = translate(ll).unwrap_err();
        assert!(err.to_string().contains("SSA value"), "{err}");
    }

    #[test]
    fn struct_by_value_param_parses() {
        let ll = r#"
%Qubit = type opaque
%Result = type opaque

define void @main({ double*, i64 } %0) #0 {
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  ret void
}
declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1
attributes #0 = { "entry_point" "qir_profiles"="base_profile" "requiredQubits"="1" "requiredResults"="1" }
attributes #1 = { "irreversible" }
"#;
        let qasm = translate(ll).unwrap();
        assert!(qasm.contains("h q[0];"), "\n{qasm}");
    }

    #[test]
    fn mresetz_emits_measure_and_reset_in_order() {
        let ll = r#"
%Qubit = type opaque
%Result = type opaque

define void @main() #0 {
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__mresetz__body(%Qubit* null, %Result* null)
  call void @__quantum__qis__x__body(%Qubit* null)
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* inttoptr (i64 1 to %Result*))
  ret void
}
declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__x__body(%Qubit*)
declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1
declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1
attributes #0 = { "entry_point" "qir_profiles"="base_profile" "requiredQubits"="1" "requiredResults"="2" }
attributes #1 = { "irreversible" }
"#;
        let qasm = translate(ll).unwrap();
        let m = qasm.find("c[0] = measure q[0];").unwrap();
        let r = qasm.find("reset q[0];").unwrap();
        let x = qasm.find("x q[0];").unwrap();
        assert!(m < r && r < x, "\n{qasm}");
    }

    #[test]
    fn phi_i64_if_merge_emits_int_declaration() {
        let ll = r#"
%Result = type opaque
%Qubit = type opaque

define i64 @main() #0 {
entry:
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  %c = call i1 @__quantum__rt__read_result(%Result* null)
  br i1 %c, label %inc, label %done
inc:
  br label %done
done:
  %count = phi i64 [0, %entry], [1, %inc]
  ret i64 0
}

declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1
declare i1 @__quantum__rt__read_result(%Result*)
attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "requiredQubits"="1" "requiredResults"="1" }
attributes #1 = { "irreversible" }
"#;
        let qasm = translate(ll).unwrap();
        assert!(qasm.contains("int cint_0 = 0;"), "\n{qasm}");
        assert!(qasm.contains("cint_0 = 1;"), "\n{qasm}");
    }

    #[test]
    fn phi_i32_same_lowering_as_i64() {
        let ll = r#"
%Result = type opaque
%Qubit = type opaque

define void @main() #0 {
entry:
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  %c = call i1 @__quantum__rt__read_result(%Result* null)
  br i1 %c, label %inc, label %done
inc:
  br label %done
done:
  %count = phi i32 [0, %entry], [1, %inc]
  ret void
}

declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1
declare i1 @__quantum__rt__read_result(%Result*)
attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "requiredQubits"="1" "requiredResults"="1" }
attributes #1 = { "irreversible" }
"#;
        let qasm = translate(ll).unwrap();
        assert!(qasm.contains("int cint_0 = 0;"), "\n{qasm}");
    }

    #[test]
    fn phi_with_three_incomings_errors() {
        let ll = r#"
%Qubit = type opaque

define void @main() #0 {
entry:
  call void @__quantum__qis__h__body(%Qubit* null)
  br label %done
mid:
  br label %done
other:
  br label %done
done:
  %bad = phi i64 [0, %entry], [1, %mid], [2, %other]
  ret void
}
declare void @__quantum__qis__h__body(%Qubit*)
attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "requiredQubits"="1" "requiredResults"="0" }
"#;
        let err = translate(ll).unwrap_err();
        assert!(
            err.to_string().contains("if-merge shape"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn phi_with_two_unconditional_predecessors_errors() {
        let ll = r#"
%Qubit = type opaque

define void @main() #0 {
entry:
  br label %a
a:
  br label %done
done:
  %bad = phi i64 [0, %entry], [1, %a]
  ret void
}
attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "requiredQubits"="0" "requiredResults"="0" }
"#;
        let err = translate(ll).unwrap_err();
        assert!(
            err.to_string().contains("if-merge shape"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn phi_with_bogus_value_type_errors() {
        let ll = r#"
%Result = type opaque
%Qubit = type opaque

define void @main() #0 {
entry:
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  %c = call i1 @__quantum__rt__read_result(%Result* null)
  br i1 %c, label %inc, label %done
inc:
  br label %done
done:
  %bad = phi i8 [0, %entry], [1, %inc]
  ret void
}
declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1
declare i1 @__quantum__rt__read_result(%Result*)
attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "requiredQubits"="1" "requiredResults"="1" }
attributes #1 = { "irreversible" }
"#;
        let err = translate(ll).unwrap_err();
        assert!(err.to_string().contains("phi i8"), "{err}");
    }

    #[test]
    fn loop_carried_phi_surfaces_clear_error() {
        // A phi whose incoming references the phi's own block is a
        // loop-carried merge. Out-of-scope per README; the error must
        // name the shape rather than pointing at "predecessor not
        // found among processed blocks".
        let ll = r#"
%Qubit = type opaque

define void @main() #0 {
entry:
  br label %loop
loop:
  %count = phi i64 [0, %entry], [%next, %loop]
  %next = add i64 %count, 1
  br label %loop
}
attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "requiredQubits"="0" "requiredResults"="0" }
"#;
        let err = translate(ll).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("loop-carried phi"), "{msg}");
        assert!(msg.contains("loop"), "{msg}");
    }

    #[test]
    fn select_i32_lowers_to_inline_arithmetic() {
        let ll = r#"
%Result = type opaque
%Qubit = type opaque

define void @main() #0 {
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  %c = call i1 @__quantum__rt__read_result(%Result* null)
  %v = select i1 %c, i32 5, i32 3
  %chk = icmp sge i32 %v, 4
  br i1 %chk, label %t, label %f
t:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  br label %f
f:
  ret void
}

declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__x__body(%Qubit*)
declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1
declare i1 @__quantum__rt__read_result(%Result*)
attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "requiredQubits"="2" "requiredResults"="1" }
attributes #1 = { "irreversible" }
"#;
        let qasm = translate(ll).unwrap();
        assert!(qasm.contains("c[0] * 5"), "\n{qasm}");
        assert!(qasm.contains("(1 - c[0]) * 3"), "\n{qasm}");
        assert!(qasm.contains(">= 4"), "\n{qasm}");
    }

    #[test]
    fn zext_i1_to_int_aliases_ssa() {
        let ll = r#"
%Qubit = type opaque
%Result = type opaque

define void @main() #0 {
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  %c = call i1 @__quantum__rt__read_result(%Result* null)
  %ci = zext i1 %c to i32
  %chk = icmp sge i32 %ci, 1
  br i1 %chk, label %t, label %f
t:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  br label %f
f:
  ret void
}
declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__x__body(%Qubit*)
declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1
declare i1 @__quantum__rt__read_result(%Result*)
attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "requiredQubits"="2" "requiredResults"="1" }
attributes #1 = { "irreversible" }
"#;
        let qasm = translate(ll).unwrap();
        // The `zext` result flows into `icmp sge, 1` which becomes
        // `c[0] >= 1` (or `c[0] == 1`) in the if condition.
        assert!(qasm.contains("c[0]"), "\n{qasm}");
    }

    /// `phi i64 [1, bb_true], [0, bb_false]` where both predecessors are
    /// empty unconditional landing pads sharing a common `br i1` ancestor
    /// lowers to the predicate expression itself (widened to i64).
    #[test]
    fn phi_i64_landing_pad_binds_to_predicate_c_bit() {
        let ll = r#"
%Qubit = type opaque
%Result = type opaque

define void @main() #0 {
entry:
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  %b = call i1 @__quantum__rt__read_result(%Result* null)
  br i1 %b, label %bb_t, label %bb_f
bb_t:
  br label %merge
bb_f:
  br label %merge
merge:
  %w = phi i64 [1, %bb_t], [0, %bb_f]
  %cmp = icmp eq i64 %w, 1
  br i1 %cmp, label %then, label %join
then:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  br label %join
join:
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  ret void
}

declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__x__body(%Qubit*)
declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1
declare i1 @__quantum__rt__read_result(%Result*)

attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "requiredQubits"="2" "requiredResults"="2" }
attributes #1 = { "irreversible" }
"#;
        let qasm = translate(ll).unwrap();
        assert!(
            qasm.contains("if (c[0] == 1) {\n  x q[1];\n}")
                || qasm.contains("if (c[0]) {\n  x q[1];\n}"),
            "\n{qasm}"
        );
        assert!(
            !qasm.contains("cint_"),
            "should not allocate an int variable"
        );
    }

    /// Same CFG shape but with the incomings swapped: `[0, bb_t], [1, bb_f]`.
    /// The phi value is `1 - predicate`, so the downstream `icmp eq i64 %w, 1`
    /// collapses to `(1 - c[0]) == 1`.
    #[test]
    fn phi_i64_landing_pad_inverted_incomings_negates_predicate() {
        let ll = r#"
%Qubit = type opaque
%Result = type opaque

define void @main() #0 {
entry:
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  %b = call i1 @__quantum__rt__read_result(%Result* null)
  br i1 %b, label %bb_t, label %bb_f
bb_t:
  br label %merge
bb_f:
  br label %merge
merge:
  %w = phi i64 [0, %bb_t], [1, %bb_f]
  %cmp = icmp eq i64 %w, 1
  br i1 %cmp, label %then, label %join
then:
  call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
  br label %join
join:
  ret void
}

declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__x__body(%Qubit*)
declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1
declare i1 @__quantum__rt__read_result(%Result*)

attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "requiredQubits"="2" "requiredResults"="1" }
attributes #1 = { "irreversible" }
"#;
        let qasm = translate(ll).unwrap();
        assert!(
            qasm.contains("if (1 - c[0] == 1) {\n  x q[1];\n}")
                || qasm.contains("if ((1 - c[0]) == 1) {\n  x q[1];\n}"),
            "\n{qasm}"
        );
    }

    /// Landing-pad shape with non-{0,1} incoming constants falls through to
    /// the existing unsupported error.
    #[test]
    fn phi_i64_landing_pad_non_binary_constants_still_errors() {
        let ll = r#"
%Qubit = type opaque
%Result = type opaque

define void @main() #0 {
entry:
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  %b = call i1 @__quantum__rt__read_result(%Result* null)
  br i1 %b, label %bb_t, label %bb_f
bb_t:
  br label %merge
bb_f:
  br label %merge
merge:
  %w = phi i64 [5, %bb_t], [7, %bb_f]
  ret void
}

declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1
declare i1 @__quantum__rt__read_result(%Result*)

attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "requiredQubits"="1" "requiredResults"="1" }
attributes #1 = { "irreversible" }
"#;
        let err = translate(ll).unwrap_err();
        assert!(err.to_string().contains("if-merge shape"), "{err}");
    }

    #[test]
    fn struct_return_type_with_malloc_and_insertvalue_translates() {
        let ll = r#"
%Qubit = type opaque
%Result = type opaque

declare i8* @malloc(i64)
declare void @free(i8*)
declare void @llvm.memcpy.p0i8.p0i8.i64(i8*, i8*, i64, i1)

define { i1*, i64 } @__nvqpp__mlirgen__kernel() #0 {
  call void @__quantum__qis__h__body(%Qubit* null)
  call void @__quantum__qis__cnot__body(%Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*))
  call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
  call void @__quantum__rt__result_record_output(%Result* null, i8* null)
  call void @__quantum__qis__mz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
  call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* null)
  %1 = alloca [2 x i8], align 1
  %2 = bitcast [2 x i8]* %1 to i8*
  %3 = call i8* @malloc(i64 2)
  call void @llvm.memcpy.p0i8.p0i8.i64(i8* %3, i8* %2, i64 2, i1 false)
  %4 = bitcast i8* %3 to i1*
  %5 = insertvalue { i1*, i64 } undef, i1* %4, 0
  %6 = insertvalue { i1*, i64 } %5, i64 2, 1
  ret { i1*, i64 } %6
}

declare void @__quantum__qis__h__body(%Qubit*)
declare void @__quantum__qis__cnot__body(%Qubit*, %Qubit*)
declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1
declare void @__quantum__rt__result_record_output(%Result*, i8*)

attributes #0 = { "entry_point" "qir_profiles"="base_profile" "requiredQubits"="2" "requiredResults"="2" }
attributes #1 = { "irreversible" }
"#;
        let qasm = translate(ll).unwrap();
        assert!(qasm.contains("h q[0];"), "\n{qasm}");
        assert!(qasm.contains("cnot q[0], q[1];"), "\n{qasm}");
        assert!(qasm.contains("c[0] = measure q[0];"), "\n{qasm}");
        assert!(qasm.contains("c[1] = measure q[1];"), "\n{qasm}");
    }
}

#[cfg(test)]
mod generated_by_tests {
    //! Tests for the trailing `// generated-by: {…}` comment.

    use super::*;
    use crate::ir::parser::parse_module;

    const MINIMAL_LL: &str = "\
%Qubit = type opaque

define void @main() #0 {
  call void @__quantum__qis__h__body(%Qubit* null)
  ret void
}
declare void @__quantum__qis__h__body(%Qubit*)
attributes #0 = { \"entry_point\" \"qir_profiles\"=\"base_profile\" \"requiredQubits\"=\"1\" \"requiredResults\"=\"0\" }
";

    fn translate_minimal(exporter: Exporter) -> String {
        let module = parse_module(MINIMAL_LL).unwrap();
        exporter.dumps(&module).unwrap()
    }

    fn last_non_empty_line(s: &str) -> &str {
        s.lines().rfind(|l| !l.is_empty()).unwrap()
    }

    #[test]
    fn default_exporter_emits_generated_by_as_last_line() {
        let out = translate_minimal(Exporter::new());
        // Trailing newline is preserved.
        assert!(out.ends_with('\n'), "output must end with newline: {out:?}");
        let last = last_non_empty_line(&out);
        assert!(
            last.starts_with("// generated-by: {"),
            "last line is not the provenance comment: {last:?}"
        );
    }

    #[test]
    fn generated_by_line_contains_name_version_profile_in_order() {
        let out = translate_minimal(Exporter::new());
        let last = last_non_empty_line(&out);
        let expected = format!(
            r#"// generated-by: {{"name":"qirtoqasm","version":"{}","profile":"base_profile"}}"#,
            crate::VERSION
        );
        assert_eq!(last, expected);
    }

    #[test]
    fn generated_by_line_omits_producer_when_not_set() {
        let out = translate_minimal(Exporter::new());
        let last = last_non_empty_line(&out);
        assert!(
            !last.contains("\"producer\":"),
            "unset producer must not appear: {last:?}"
        );
    }

    #[test]
    fn with_producer_round_trips_caller_string() {
        let out = translate_minimal(Exporter::new().with_producer("mylib 0.1.2"));
        let last = last_non_empty_line(&out);
        let expected = format!(
            r#"// generated-by: {{"name":"qirtoqasm","version":"{}","profile":"base_profile","producer":"mylib 0.1.2"}}"#,
            crate::VERSION
        );
        assert_eq!(last, expected);
    }

    #[test]
    fn with_producer_keys_are_ordered_name_version_profile_producer() {
        let out = translate_minimal(Exporter::new().with_producer("othertool 2.0"));
        let last = last_non_empty_line(&out);
        let name_idx = last.find("\"name\"").unwrap();
        let version_idx = last.find("\"version\"").unwrap();
        let profile_idx = last.find("\"profile\"").unwrap();
        let producer_idx = last.find("\"producer\"").unwrap();
        assert!(name_idx < version_idx);
        assert!(version_idx < profile_idx);
        assert!(profile_idx < producer_idx);
    }

    #[test]
    fn with_producer_escapes_embedded_quotes_and_backslashes() {
        let out = translate_minimal(Exporter::new().with_producer("weird \"quoted\" \\ path"));
        let last = last_non_empty_line(&out);
        assert!(
            last.contains(r#""producer":"weird \"quoted\" \\ path""#),
            "quote/backslash must be JSON-escaped: {last}"
        );
    }

    #[test]
    fn with_producer_escapes_embedded_newline_keeping_comment_single_line() {
        let out = translate_minimal(Exporter::new().with_producer("line1\nline2"));
        let generated_by_lines: Vec<&str> = out
            .lines()
            .filter(|l| l.starts_with("// generated-by:"))
            .collect();
        assert_eq!(
            generated_by_lines.len(),
            1,
            "producer newline must not split the comment: {out}"
        );
        assert!(generated_by_lines[0].contains(r#""producer":"line1\nline2""#));
    }

    #[test]
    fn input_qir_profile_appears_in_generated_by_line() {
        // MINIMAL_LL declares `"qir_profiles"="base_profile"`.
        let out = translate_minimal(Exporter::new());
        let last = last_non_empty_line(&out);
        assert!(
            last.contains(r#""profile":"base_profile""#),
            "input profile must be surfaced: {last}"
        );
    }

    #[test]
    fn adaptive_qir_profile_appears_in_generated_by_line() {
        let ll = "\
%Qubit = type opaque
define void @main() #0 {
  call void @__quantum__qis__h__body(%Qubit* null)
  ret void
}
declare void @__quantum__qis__h__body(%Qubit*)
attributes #0 = { \"entry_point\" \"qir_profiles\"=\"adaptive_profile\" \"requiredQubits\"=\"1\" \"requiredResults\"=\"0\" }
";
        let module = parse_module(ll).unwrap();
        let out = Exporter::new().dumps(&module).unwrap();
        let last = last_non_empty_line(&out);
        assert!(
            last.contains(r#""profile":"adaptive_profile""#),
            "adaptive input profile must be surfaced: {last}"
        );
    }

    #[test]
    fn omits_profile_field_when_input_declares_none() {
        // No `qir_profiles` attribute — the field should be omitted.
        let ll = "\
%Qubit = type opaque
define void @main() #0 {
  call void @__quantum__qis__h__body(%Qubit* null)
  ret void
}
declare void @__quantum__qis__h__body(%Qubit*)
attributes #0 = { \"entry_point\" }
";
        let module = parse_module(ll).unwrap();
        let out = Exporter::new().dumps(&module).unwrap();
        let last = last_non_empty_line(&out);
        assert!(
            !last.contains("\"profile\""),
            "profile field must be omitted when the input declares none: {last}"
        );
    }

    #[test]
    fn exporter_target_profile_no_longer_leaks_into_generated_by_line() {
        // Setting a custom target profile on the Exporter must not
        // change the `"profile"` field, which now tracks the input
        // QIR profile rather than the exporter's builder registry.
        let mut custom = crate::profile::base_profile();
        custom.name = "my_custom_target_profile".into();
        let out = translate_minimal(Exporter::new().with_profile(custom));
        let last = last_non_empty_line(&out);
        assert!(!last.contains("my_custom_target_profile"), "{last}");
        assert!(last.contains(r#""profile":"base_profile""#), "{last}");
    }

    #[test]
    fn with_producer_is_idempotent_last_value_wins() {
        let out = translate_minimal(
            Exporter::new()
                .with_producer("first")
                .with_producer("second"),
        );
        let last = last_non_empty_line(&out);
        assert!(last.contains(r#""producer":"second""#), "{last}");
        assert!(!last.contains(r#""producer":"first""#), "{last}");
    }

    #[test]
    fn with_producer_empty_string_omits_field() {
        let out = translate_minimal(Exporter::new().with_producer(""));
        let last = last_non_empty_line(&out);
        assert!(!last.contains("\"producer\""), "{last}");
    }

    #[test]
    fn with_producer_empty_string_overrides_prior_value() {
        let out = translate_minimal(
            Exporter::new()
                .with_producer("mylib 0.1.2")
                .with_producer(""),
        );
        let last = last_non_empty_line(&out);
        assert!(!last.contains("\"producer\""), "{last}");
    }

    #[test]
    fn json_escape_handles_control_characters_as_unicode_escape() {
        let mut buf = String::new();
        json_escape_into("\x01\x1f\x7f", &mut buf);
        assert_eq!(buf, r"\u0001\u001f\u007f");
    }

    #[test]
    fn json_escape_passes_through_printable_ascii_and_non_ascii() {
        let mut buf = String::new();
        json_escape_into("hello · π", &mut buf);
        assert_eq!(buf, "hello · π");
    }

    #[test]
    fn json_escape_handles_backspace_formfeed_tab() {
        let mut buf = String::new();
        json_escape_into("\x08\x09\x0c", &mut buf);
        assert_eq!(buf, r"\b\t\f");
    }

    #[test]
    fn json_escape_handles_carriage_return() {
        let mut buf = String::new();
        json_escape_into("a\rb", &mut buf);
        assert_eq!(buf, r"a\rb");
    }

    #[test]
    fn generated_by_line_matches_expected_regex_shape() {
        let out = translate_minimal(Exporter::new().with_producer("mylib 0.1.2"));
        let last = last_non_empty_line(&out);
        let rest = last.strip_prefix("// generated-by: ").expect("prefix");
        assert!(
            rest.starts_with('{') && rest.ends_with('}'),
            "shape: {last:?}"
        );
    }
}
