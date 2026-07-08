// Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License").

//! Top-level pipeline: QIR text → [`Module`] → [`Program`] → OpenQASM text.
//!
//! The Base Profile end of the pipeline is complete; classical-control
//! constructs used by the Adaptive Profile (measurement-conditional
//! branches, phi merges, i1 comparisons and arithmetic, integer-cascade
//! selects) are recognized by the parser but rejected here with the
//! pinned `"adaptive profile feature not yet implemented"` error.

use crate::builders::lower_call;
use crate::cfg::{lower_cfg, BlockLowering};
use crate::error::{QirToQasmError, Result};
use crate::ir::{Block, Instruction, Module};
use crate::oq3::ast::*;
use crate::oq3::printer;
use crate::profile::{base_profile, FunctionBuilder, Profile};
use crate::signatures::{extract_signatures, SignatureTable};
use crate::symbols::{SymbolTable, QUBIT_REGISTER, RESULT_REGISTER};

/// Error string returned when the translator encounters a classical-
/// control-flow shape (icmp, bitwise-i1, select, integer arithmetic,
/// phi, or measurement-conditional branch) that requires the Adaptive
/// Profile lowering — scaffolding stub, replaced once that lowering is
/// in place.
const ADAPTIVE_STUB: &str = "adaptive profile feature not yet implemented";

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
        out.push_str(&self.generated_by_line());
        Ok(out)
    }

    /// Render the trailing `// generated-by: {…}` line. Keys: `name`,
    /// `version`, then `profile`, then optionally `producer`.
    fn generated_by_line(&self) -> String {
        let mut s = String::from("// generated-by: {");
        s.push_str(r#""name":"qirtoqasm","version":""#);
        json_escape_into(crate::VERSION, &mut s);
        s.push('"');
        s.push_str(r#","profile":""#);
        json_escape_into(&self.profile.name, &mut s);
        s.push('"');
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
        let int_declarations: Vec<Statement> = Vec::new();
        for (i, block) in entry.blocks.iter().enumerate() {
            let canonical = &block_names[i];
            let lowering = self.lower_block(
                block,
                canonical,
                &signatures,
                &mut symbols,
                &mut block_lowerings,
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
            statements.push(Statement::QubitDeclaration {
                size: (symbols.max_qubit_index + 1) as u64,
                name: QUBIT_REGISTER.into(),
            });
        }
        if symbols.max_result_index >= 0 {
            let bit_size = (symbols.max_result_index + 1) as u64;
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
        // Classical int variables introduced by phi-integer lowering
        // land here once the Adaptive path is filled in.
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
        _prior_lowerings: &mut [BlockLowering],
    ) -> Result<BlockLowering> {
        let mut stmts: Vec<Statement> = Vec::new();
        let condition: Option<Expression> = None;
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
                                "variadic QIR function '{}' is not supported. Some producers \
                                 emit variadic calls to \
                                 'generalizedInvokeWithRotationsControlsTargets' for \
                                 multi-controlled gates (e.g. CCX/Toffoli, CCZ, CY via \
                                 y.ctrl). These must be decomposed into single- and \
                                 two-qubit gates before translation. See the qirtoqasm \
                                 README for the list of supported multi-controlled \
                                 gate patterns.",
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
                    // `RecordOutputNoop` and `GeneralizedControlled` handle
                    // the variadic operand list themselves; other variadic
                    // callees must be rejected or the default dispatch
                    // would drop operands past the fixed prefix.
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
                    // For fixed-arity callees, enforce an exact match against
                    // the declared signature. Variadic callees bypass this
                    // check since `param_types` only describes the fixed
                    // prefix.
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
                // Classical-control shapes require the Adaptive Profile
                // lowering. Scaffolding stub — replaced once the Adaptive
                // arms are wired in.
                Instruction::Icmp(_)
                | Instruction::BinaryI1 { .. }
                | Instruction::Select { .. }
                | Instruction::IntArith { .. }
                | Instruction::Phi { .. }
                | Instruction::BrCond { .. } => {
                    return Err(QirToQasmError::unsupported(ADAPTIVE_STUB));
                }
                Instruction::Br { target } => {
                    targets.push(target.clone());
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
                    // Constant-fold scalar stores into alloca slots. A
                    // subsequent `load` through any alias of the same
                    // slot picks up the stored value.
                    symbols.store_to_alloca_slot(ptr, value.clone());
                }
                Instruction::Load { result, ptr } => {
                    if let Some(expr) = symbols.load_from_alloca_slot(ptr) {
                        symbols.record_ssa(result, expr);
                    }
                    // If the slot wasn't constant-folded, leave the SSA
                    // unbound; downstream uses will fail with the usual
                    // "SSA not bound" error.
                }
                Instruction::Zext { result, src } => {
                    // `zext i1 %x to iN` promotes a Boolean to its
                    // integer value (0 or 1). We already treat i1 SSAs
                    // as integer-valued expressions in arithmetic
                    // contexts (e.g. `c[N]`), so binding the result to
                    // the source's existing expression is enough.
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

/// JSON-escape `s` into `buf` — quote, backslash, and ASCII control
/// characters only. Enough to keep the trailing `// generated-by:`
/// comment on a single well-formed line.
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
mod new_lowering_tests {
    //! Direct Rust coverage for the Base-Profile alloca/store/load,
    //! struct-by-value, and mresetz-order paths. Python tests exercise
    //! the same code but go through the PyO3 wheel; `cargo llvm-cov`
    //! only counts Rust-level coverage.

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
    fn custom_profile_name_appears_in_generated_by_line() {
        let mut custom = crate::profile::base_profile();
        custom.name = "my_custom_profile".into();
        let out = translate_minimal(Exporter::new().with_profile(custom));
        let last = last_non_empty_line(&out);
        assert!(
            last.contains(r#""profile":"my_custom_profile""#),
            "profile name must be surfaced: {last}"
        );
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
