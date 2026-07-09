// Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License").

//! Parsed-QIR in-memory model.

/// Parsed representation of a QIR module.
#[derive(Debug, Clone)]
pub struct Module {
    /// The original source text. Retained because the translator reads it
    /// directly in a couple of places (signature extraction is pure text).
    pub source_text: String,
    /// Every `declare` or `define` the parser saw, in source order.
    pub functions: Vec<Function>,
}

impl Module {
    /// Return the entry-point function, if any.
    pub fn entry_point(&self) -> Option<&Function> {
        self.functions
            .iter()
            .find(|f| !f.is_declaration && f.is_entry_point)
    }
}

/// A `declare` or `define` entry.
#[derive(Debug, Clone)]
pub struct Function {
    /// Function name, without the leading `@`.
    pub name: String,
    /// `true` if no `{ … }` body follows (i.e. a `declare` line).
    pub is_declaration: bool,
    /// `true` if any attribute on the function contains `entry_point`.
    pub is_entry_point: bool,
    /// Value of the `"qir_profiles"` attribute if the function declares
    /// one (e.g. `"base_profile"`, `"adaptive_profile"`), else `None`.
    pub qir_profile: Option<String>,
    /// Basic blocks in source order. Empty for declarations.
    pub blocks: Vec<Block>,
}

/// An LLVM basic block.
#[derive(Debug, Clone)]
pub struct Block {
    /// Explicit label from source (may be empty for entry-block without a label).
    pub name: String,
    /// Instructions in source order.
    pub instructions: Vec<Instruction>,
}

/// The LLVM instructions the translator recognizes.
///
/// Opcodes not listed here land as [`Instruction::Ignored`] (memory/plumbing
/// we deliberately skip) or surface at translation time as
/// [`Instruction::Unsupported`] with the raw opcode preserved for error messages.
#[derive(Debug, Clone)]
pub enum Instruction {
    /// `[%result =] call <retty> @<callee>(<args>)`.
    Call {
        /// SSA id assigned by this call, if non-void.
        result: Option<String>,
        /// Callee name, without leading `@`.
        callee: String,
        /// Call operands, in source order.
        args: Vec<Operand>,
        /// Return-type canonical string (e.g. `"void"`, `"i1"`).
        return_type: String,
    },
    /// `br label %target`  (unconditional).
    Br {
        /// Destination basic-block label.
        target: String,
    },
    /// `br i1 %cond, label %true, label %false`  (conditional).
    BrCond {
        /// Condition operand (SSA reference or `i1` constant).
        cond: BrCondOperand,
        /// True-branch label.
        true_target: String,
        /// False-branch label.
        false_target: String,
    },
    /// `ret <type> <value>` or `ret void`.
    Ret,
    /// `%result = icmp <pred> <ty> <lhs>, <rhs>`.
    Icmp(Icmp),
    /// `%result = xor <ty> <lhs>, <rhs>`, `%result = and <ty> <lhs>, <rhs>`,
    /// or `%result = or <ty> <lhs>, <rhs>`. The `op` field carries which
    /// of the three the source spelled. Split out from [`Instruction::IntArith`]
    /// because i1-bitwise ops lower to Boolean expressions rather than
    /// arithmetic ones.
    BinaryI1 {
        /// SSA id assigned by this instruction.
        result: String,
        /// Which i1 binary operation.
        op: BinaryI1Op,
        /// Left operand.
        lhs: Operand,
        /// Right operand.
        rhs: Operand,
    },
    /// `%result = select i1 <cond>, <ty> <true_val>, <ty> <false_val>`.
    /// `value_type` preserves the two branches' type spelling so the
    /// lowering can reject non-`i1` selects with a descriptive error.
    Select {
        /// SSA id assigned by this select.
        result: String,
        /// Value-type spelling of the two branches (e.g. `"i1"`, `"double"`).
        value_type: String,
        /// Branch condition operand.
        cond: Operand,
        /// Value when `cond` is true.
        true_value: Operand,
        /// Value when `cond` is false.
        false_value: Operand,
    },
    /// Integer arithmetic: `%result = <op> <ty> <lhs>, <rhs>`, where
    /// `<op>` is `add`, `sub`, or `mul`. Bitwise i1 ops (`and`, `or`,
    /// `xor`) have their own variants since they lower to Booleans.
    IntArith {
        /// SSA id assigned by this instruction.
        result: String,
        /// Which arithmetic operation.
        op: IntArithOp,
        /// Left operand.
        lhs: Operand,
        /// Right operand.
        rhs: Operand,
    },
    /// `%result = phi <ty> [<val>, %<block>], [<val>, %<block>]`.
    Phi {
        /// SSA id assigned by this phi.
        result: String,
        /// LLVM type token (`"i1"`, `"i64"`, …).
        value_type: String,
        /// Incoming pairs in source order.
        incomings: Vec<PhiIncoming>,
    },
    /// Memory / bit-plumbing opcodes we silently ignore.
    Ignored {
        /// The LLVM opcode that was skipped (`alloca`, `load`, …).
        opcode: String,
    },
    /// `%result = alloca <ty>[, i<N> <count>][, align <N>]` — we remember the
    /// SSA id so downstream bitcast / gep / store / load can reason about it.
    Alloca {
        /// SSA id assigned by this alloca.
        result: String,
    },
    /// `%result = bitcast <ty>* <src> to <ty>*` — used by cudaq to alias
    /// an alloca region. We track `%result -> %src` as an alias.
    BitcastAlias {
        /// SSA id assigned by the bitcast.
        result: String,
        /// Source SSA id (already in symbol table).
        src: String,
    },
    /// `%result = getelementptr <ty>, <ty>* <src>, i32 0, i32 <offset>` —
    /// cudaq's idiom for indexing into an alloca'd array. We only
    /// recognize the two-index form with a leading `0`.
    GetElementPtrOffset {
        /// SSA id assigned by the gep.
        result: String,
        /// Base pointer SSA id.
        src: String,
        /// Array-element offset.
        offset: u64,
    },
    /// `store <ty> <value>, <ty>* <ptr>` — cudaq stores constant scalar
    /// values into alloca slots that are subsequently loaded as gate
    /// arguments.
    Store {
        /// Stored value operand.
        value: Operand,
        /// Pointer SSA id (must be in the symbol table's alias map).
        ptr: String,
    },
    /// `%result = load <ty>, <ty>* <ptr>` — cudaq loads previously
    /// stored scalar values out of an alloca slot to use as a gate
    /// argument.
    Load {
        /// SSA id assigned by the load.
        result: String,
        /// Source pointer SSA id.
        ptr: String,
    },
    /// `%result = zext iN %src to iM` — integer width extension.
    /// For our purposes the i1-to-integer case is the interesting
    /// one: it promotes a Boolean measurement-result bit to an
    /// integer 0/1 suitable for arithmetic.
    Zext {
        /// SSA id assigned by the zext.
        result: String,
        /// Source SSA id (must already be bound).
        src: String,
    },
    /// Any other opcode that made it into a `define` body. Triggers
    /// an unsupported-construct error during translation.
    Unsupported {
        /// Raw opcode token (e.g. `"fadd"`).
        opcode: String,
    },
}

/// Parsed `icmp` instruction.
#[derive(Debug, Clone)]
pub struct Icmp {
    /// SSA id assigned by this icmp.
    pub result: String,
    /// The predicate (`eq`, `ne`). Other LLVM predicates are preserved here
    /// but rejected at translation time with a pinned error message.
    pub predicate: PredicateI1,
    /// Left operand.
    pub lhs: Operand,
    /// Right operand.
    pub rhs: Operand,
}

/// The icmp predicate spelling captured from source.
///
/// We keep the raw string (not just an enum of supported predicates) so
/// the "unsupported predicate" error can name it verbatim.
#[derive(Debug, Clone)]
pub struct PredicateI1(pub String);

/// Integer arithmetic opcode, mirroring the subset of LLVM binary
/// operators that qirtoqasm can lower into an OpenQASM 3 arithmetic
/// expression. `and`/`or`/`xor` on `i1` values go through their own
/// dedicated [`Instruction`] variants because they produce Boolean
/// expressions rather than arithmetic ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntArithOp {
    /// `add <ty> %a, %b` → `a + b`.
    Add,
    /// `sub <ty> %a, %b` → `a - b`.
    Sub,
    /// `mul <ty> %a, %b` → `a * b`.
    Mul,
}

/// LLVM i1 binary operator. `xor`/`and`/`or` on `i1` lower to Boolean
/// expressions, so they share a single [`Instruction::BinaryI1`]
/// variant rather than each having a dedicated one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryI1Op {
    /// `xor i1 %a, %b` → `a != b` (or `a == 0` when rhs is the constant 1).
    Xor,
    /// `and i1 %a, %b` → `a && b`.
    And,
    /// `or  i1 %a, %b` → `a || b`.
    Or,
}

/// A phi incoming pair.
#[derive(Debug, Clone)]
pub struct PhiIncoming {
    /// Value carried by this predecessor.
    pub value: Operand,
    /// Predecessor block label.
    pub pred: String,
}

/// Parsed operand.
///
/// The parser normalises the operand syntax so the translator can
/// dispatch by variant instead of re-parsing text. See the parser
/// module for acceptance rules (typed vs opaque pointers, hex vs
/// decimal floats, `true`/`false` vs `0`/`1` for `i1`).
#[derive(Debug, Clone)]
pub enum Operand {
    /// An integer constant carried in the largest width we model.
    ConstInt(i128),
    /// A boolean constant (an `i1` literal).
    ConstBool(bool),
    /// A floating-point constant.
    ConstFloat(f64),
    /// A pointer constant, either `null` or `inttoptr (iN N to <ptr>)`.
    ///
    /// `struct_name` is the struct's declared name when the source used
    /// the typed-pointer form (`%Qubit*`), or `None` for the opaque form
    /// (`ptr`). `index` is the recovered integer index.
    PtrConst {
        /// `Some("Qubit")` / `Some("Result")` for typed pointers; `None`
        /// for opaque `ptr`.
        struct_name: Option<String>,
        /// Resolved integer index.
        index: i64,
    },
    /// Reference to an SSA value. Key is stable under [`crate::symbols::ssa_key`].
    Ssa(String),
    /// A reference to an LLVM global (e.g. an `@cstr.*` string constant),
    /// captured by name. These appear only as payloads to `__quantum__rt__*_record_output`
    /// calls which the translator no-ops anyway.
    GlobalRef(String),
    /// A `getelementptr` expression inlined into a call operand — used
    /// for string constants. We represent it opaquely because, like
    /// `GlobalRef`, it only ever appears inside `record_output` call
    /// sites that the translator discards.
    GetElementPtr,
    /// A `bitcast (T* @<name> to i8*)` inlined into a call operand,
    /// used as the inner-function pointer in CUDA-Q's variadic
    /// `generalizedInvokeWithRotationsControlsTargets` intrinsic. We
    /// capture the underlying global name so the variadic builder can
    /// resolve it (e.g. `"__quantum__qis__x__ctl"` → controlled X).
    BitcastGlobal(String),
    /// A `null` literal typed as `i8*` (distinct from `ptr null`, which is
    /// `PtrConst`). Shows up as the argument to `__quantum__rt__initialize`.
    I8Null,
}

/// Operand of a conditional branch.
#[derive(Debug, Clone)]
pub enum BrCondOperand {
    /// Reference to an SSA value, keyed by its textual id (e.g. `"cond"`, `"1"`).
    Ssa(String),
    /// A constant `i1`: `br i1 0, ...` or `br i1 1, ...`.
    Const(bool),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Light Debug/Clone smoke test for every public model type —
    /// ensures the derived impls are actually instantiated, which
    /// bumps the coverage tool past the derive macro expansion.
    #[test]
    fn debug_and_clone_cover_model_derives() {
        let module = Module {
            source_text: "x".into(),
            functions: vec![Function {
                name: "main".into(),
                is_declaration: false,
                is_entry_point: true,
                qir_profile: None,
                blocks: vec![Block {
                    name: "entry".into(),
                    instructions: vec![
                        Instruction::Call {
                            result: None,
                            callee: "f".into(),
                            args: vec![
                                Operand::ConstInt(1),
                                Operand::ConstBool(true),
                                Operand::ConstFloat(0.5),
                                Operand::PtrConst {
                                    struct_name: Some("Qubit".into()),
                                    index: 0,
                                },
                                Operand::Ssa("x".into()),
                                Operand::GlobalRef("g".into()),
                                Operand::GetElementPtr,
                                Operand::I8Null,
                            ],
                            return_type: "void".into(),
                        },
                        Instruction::Br {
                            target: "next".into(),
                        },
                        Instruction::BrCond {
                            cond: BrCondOperand::Ssa("c".into()),
                            true_target: "t".into(),
                            false_target: "f".into(),
                        },
                        Instruction::BrCond {
                            cond: BrCondOperand::Const(true),
                            true_target: "t".into(),
                            false_target: "f".into(),
                        },
                        Instruction::Ret,
                        Instruction::Icmp(Icmp {
                            result: "r".into(),
                            predicate: PredicateI1("eq".into()),
                            lhs: Operand::Ssa("a".into()),
                            rhs: Operand::ConstBool(false),
                        }),
                        Instruction::BinaryI1 {
                            result: "r".into(),
                            op: BinaryI1Op::Xor,
                            lhs: Operand::Ssa("a".into()),
                            rhs: Operand::ConstBool(true),
                        },
                        Instruction::Phi {
                            result: "p".into(),
                            value_type: "i1".into(),
                            incomings: vec![PhiIncoming {
                                value: Operand::ConstBool(false),
                                pred: "b".into(),
                            }],
                        },
                        Instruction::Ignored {
                            opcode: "load".into(),
                        },
                        Instruction::Unsupported {
                            opcode: "fadd".into(),
                        },
                    ],
                }],
            }],
        };
        // Round-trip through Debug/Clone for every nested type.
        let cloned = module.clone();
        let dbg = format!("{cloned:?}");
        assert!(dbg.contains("Module"));
        assert!(dbg.contains("Function"));
        assert!(dbg.contains("Block"));
        // The .entry_point() accessor.
        assert_eq!(module.entry_point().map(|f| f.name.as_str()), Some("main"));
        // Non-entry-point / declaration variants.
        let decl_only = Module {
            source_text: "x".into(),
            functions: vec![Function {
                name: "f".into(),
                is_declaration: true,
                is_entry_point: false,
                qir_profile: None,
                blocks: vec![],
            }],
        };
        assert!(decl_only.entry_point().is_none());
    }
}
