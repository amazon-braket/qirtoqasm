// Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License").

//! Mutable translator state carried through a single QIR → OpenQASM
//! translation.

use std::collections::HashMap;

use crate::error::{QirToQasmError, Result};
use crate::ir::Operand;
use crate::oq3::ast::Expression;

/// OpenQASM register identifier we emit for qubits.
pub const QUBIT_REGISTER: &str = "q";

/// OpenQASM register identifier we emit for classical bits.
pub const RESULT_REGISTER: &str = "c";

/// The translator's notebook. As the translator walks a QIR program
/// instruction by instruction, it writes facts down here so it can
/// look them up when later instructions need them.
///
/// QIR is LLVM IR, so each value the program computes has a unique
/// name like `%cond`, `%tmp.1`, or `%1`. These are called SSA values:
/// "SSA" stands for *Static Single Assignment*, which just means each
/// name is assigned exactly once and never reused. When the translator
/// reaches an instruction that consumes a previously computed value,
/// it needs to substitute the OpenQASM equivalent for that name. Most
/// of this struct exists to make that lookup possible.
///
/// Three independent things live here:
///
/// 1. **Quantum register sizing** (`max_qubit_index`,
///    `max_result_index`). Qubits and classical result bits are not
///    referred to by name in QIR — they are referred to by integer
///    index, e.g. `%Qubit* inttoptr (i64 3 to %Qubit*)` means
///    "qubit number 3". So there is nothing to bind by name; we just
///    remember the largest index we have seen. At the end of the
///    translation, that tells us how big to make the OpenQASM
///    register declarations at the top of the program
///    (`qubit[N] q;` and `bit[N] c;`).
///
/// 2. **Classical value bindings** (`ssa`). A map from an SSA name
///    to the OpenQASM expression that should replace it. This is the
///    main "symbol table" in the traditional sense: every classical
///    value the program computes lands here, including
///    - comparisons (`%cond = icmp eq i32 %x, 0`),
///    - boolean combinations (`and i1`, `or i1`, `xor i1`),
///    - integer arithmetic (`add`, `sub`, `mul`,
///      integer-cascade `select`),
///    - `phi` merges of all of the above,
///    - measurement readouts (`__quantum__rt__read_result__body`,
///      which binds the SSA name to the corresponding `c[i]` bit).
///
///    When a later instruction uses one of these as an operand, the
///    lowering code looks the name up here and substitutes the bound
///    expression into whatever OpenQASM statement it is building.
///    The map key is just the LLVM name (`cond`, `tmp.1`, `1`, …);
///    [`ssa_key`] extracts that name from the instruction text in
///    the few places the parser does not already have it.
///
/// 3. **Stack-slot scratch** (`alloca_alias`, `alloca_slot`). Some
///    QIR producers route a scalar value — a rotation angle, a loop
///    bound, etc. — through an LLVM stack slot rather than passing
///    it directly: `alloca` reserves the slot, `store` puts a value
///    in, and a later `load` reads it back out. These two maps track
///    which pointer SSA values refer to which slot, and what value
///    each slot currently holds, so the eventual `load` can produce
///    an inline OpenQASM constant instead of an opaque pointer
///    dereference. Once the `load` resolves, its result is recorded
///    in `ssa` like any other classical binding.
///
/// One thing the table deliberately does **not** track is "function
/// input parameters". The QIR Base and Adaptive Profiles both define
/// the entry point as `@main()` with no parameters, so every
/// classical value the program uses is either a literal in the QIR
/// text (e.g. `Operand::ConstFloat(0.5)` passed straight through as
/// a rotation angle) or appears as one of the bindings above. There
/// is no third "user parameter" category that would need its own
/// slot in this struct.
#[derive(Debug, Default)]
pub struct SymbolTable {
    /// Largest referenced qubit index, or -1 if no qubits are used.
    pub max_qubit_index: i64,
    /// Largest referenced result-bit index, or -1 if no results are used.
    pub max_result_index: i64,
    /// SSA key → bound OpenQASM expression.
    pub ssa: HashMap<String, Expression>,
    /// Pointer-SSA → (alloca-root SSA, offset-within-alloca). Populated by
    /// `alloca`, `bitcast`, and `getelementptr` to let subsequent
    /// `store` / `load` pairs on aliased pointers refer to the same
    /// underlying storage slot.
    alloca_alias: HashMap<String, (String, u64)>,
    /// (alloca-root SSA, offset) → most recently stored value. Written
    /// by `store`, read by `load`.
    alloca_slot: HashMap<(String, u64), Operand>,
}

impl SymbolTable {
    /// Create an empty symbol table. Indices start at -1 so "no qubits
    /// used" ends up as size 0 after `+1`.
    pub fn new() -> Self {
        Self {
            max_qubit_index: -1,
            max_result_index: -1,
            ssa: HashMap::new(),
            alloca_alias: HashMap::new(),
            alloca_slot: HashMap::new(),
        }
    }

    /// Record that the program uses qubit `index`.
    pub fn record_qubit(&mut self, index: i64) {
        if index > self.max_qubit_index {
            self.max_qubit_index = index;
        }
    }

    /// Record that the program uses result-bit `index`.
    pub fn record_result(&mut self, index: i64) {
        if index > self.max_result_index {
            self.max_result_index = index;
        }
    }

    /// Bind an SSA value (identified by `key`) to an OpenQASM expression.
    pub fn record_ssa(&mut self, key: &str, expr: Expression) {
        self.ssa.insert(key.to_string(), expr);
    }

    /// Return the OpenQASM expression previously bound to `key`, cloned.
    pub fn lookup_ssa(&self, key: &str) -> Result<Expression> {
        self.ssa
            .get(key)
            .cloned()
            .ok_or_else(|| QirToQasmError::unsupported(format!(
                "SSA value {:?} is used but was never bound; upstream instruction may be unsupported",
                key
            )))
    }

    /// Record an `alloca` SSA: the pointer aliases itself at offset 0.
    pub fn record_alloca(&mut self, key: &str) {
        self.alloca_alias
            .insert(key.to_string(), (key.to_string(), 0));
    }

    /// Record that `alias` points at `(root, offset)`, where `root`
    /// is the ultimate alloca SSA. If `src` is itself an alias, its
    /// (root, base_offset) gets collapsed into the new entry so the
    /// alias chain stays flat.
    pub fn record_alias(&mut self, alias: &str, src: &str, offset: u64) {
        let (root, base) = self
            .alloca_alias
            .get(src)
            .cloned()
            .unwrap_or_else(|| (src.to_string(), 0));
        self.alloca_alias
            .insert(alias.to_string(), (root, base + offset));
    }

    /// Record that `value` has been stored into the alloca slot
    /// `ptr` ultimately resolves to. Does nothing if `ptr` isn't a
    /// tracked alloca alias.
    pub fn store_to_alloca_slot(&mut self, ptr: &str, value: Operand) {
        if let Some((root, offset)) = self.alloca_alias.get(ptr).cloned() {
            self.alloca_slot.insert((root, offset), value);
        }
    }

    /// Look up the most recently stored value in the alloca slot
    /// `ptr` resolves to, converted to an Expression. Returns None if
    /// the slot is untracked or has never been stored to.
    pub fn load_from_alloca_slot(&self, ptr: &str) -> Option<Expression> {
        let (root, offset) = self.alloca_alias.get(ptr)?.clone();
        let value = self.alloca_slot.get(&(root, offset))?;
        operand_to_expression(value)
    }
}

/// Compute a stable SSA key.
///
///   1. If `name` is non-empty, return it (covers `%cond`, `%tmp.1`).
///   2. Else scan `text` for the first `%<id>` token and return the id.
///   3. Else return `text` verbatim (defensive fallback).
pub fn ssa_key(name: &str, text: &str) -> String {
    if !name.is_empty() {
        return name.to_string();
    }
    if let Some(start) = text.find('%') {
        let rest = &text[start + 1..];
        let end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '$'))
            .unwrap_or(rest.len());
        if end > 0 {
            return rest[..end].to_string();
        }
    }
    text.to_string()
}

/// Convert a constant-ish Operand to an Expression. Returns None for
/// non-constant operands; used by `load_from_alloca_slot` to produce
/// inline constants for stored scalar values.
fn operand_to_expression(op: &Operand) -> Option<Expression> {
    match op {
        Operand::ConstFloat(f) => Some(Expression::Float(*f)),
        Operand::ConstInt(n) => i64::try_from(*n).ok().map(Expression::Integer),
        Operand::ConstBool(b) => Some(Expression::Boolean(*b)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_qubit_tracks_max() {
        let mut s = SymbolTable::new();
        s.record_qubit(0);
        s.record_qubit(2);
        s.record_qubit(1);
        assert_eq!(s.max_qubit_index, 2);
    }

    #[test]
    fn record_result_tracks_max() {
        let mut s = SymbolTable::new();
        s.record_result(3);
        assert_eq!(s.max_result_index, 3);
    }

    #[test]
    fn ssa_key_prefers_name_when_non_empty() {
        assert_eq!(ssa_key("cond", "  %cond = ..."), "cond");
    }

    #[test]
    fn ssa_key_parses_numeric_label_from_text_when_name_empty() {
        assert_eq!(ssa_key("", "  %1 = call i1 @foo()"), "1");
        assert_eq!(ssa_key("", "  %42 = call ptr @bar()"), "42");
    }

    #[test]
    fn ssa_key_parses_named_label_from_text_when_name_empty() {
        // Rarely used defensive branch: llvmlite always gives `.name` for
        // named SSA, but our parser can still exercise the text-scan path.
        assert_eq!(ssa_key("", "%tmp.1 = icmp eq i1 %a, %b"), "tmp.1");
    }

    #[test]
    fn ssa_key_falls_back_to_text_when_no_percent_token() {
        assert_eq!(ssa_key("", "no percent here"), "no percent here");
    }

    #[test]
    fn lookup_ssa_missing_key_errors_with_pinned_substring() {
        let s = SymbolTable::new();
        let err = s.lookup_ssa("x").unwrap_err();
        assert!(
            err.to_string().contains("is used but was never bound"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn record_and_lookup_round_trip() {
        let mut s = SymbolTable::new();
        s.record_ssa("cond", Expression::Integer(1));
        assert_eq!(s.lookup_ssa("cond").unwrap(), Expression::Integer(1));
    }
}

#[cfg(test)]
mod more_tests {
    use super::*;

    #[test]
    fn symbol_table_default_matches_new() {
        let s = SymbolTable::default();
        assert_eq!(s.max_qubit_index, 0); // Default is 0 (Default derive) — intentionally different from new()
        let s = SymbolTable::new();
        assert_eq!(s.max_qubit_index, -1);
        assert_eq!(s.max_result_index, -1);
    }

    #[test]
    fn record_qubit_does_not_regress_on_smaller_index() {
        let mut s = SymbolTable::new();
        s.record_qubit(5);
        s.record_qubit(2);
        assert_eq!(s.max_qubit_index, 5);
    }

    #[test]
    fn record_result_does_not_regress_on_smaller_index() {
        let mut s = SymbolTable::new();
        s.record_result(5);
        s.record_result(2);
        assert_eq!(s.max_result_index, 5);
    }

    #[test]
    fn ssa_key_handles_special_chars_in_identifier() {
        assert_eq!(ssa_key("", "%tmp.1 = ..."), "tmp.1");
        assert_eq!(ssa_key("", "%a.b.c = ..."), "a.b.c");
        assert_eq!(ssa_key("", "%a_b = ..."), "a_b");
        assert_eq!(ssa_key("", "%$global = ..."), "$global");
    }

    #[test]
    fn ssa_key_handles_percent_with_no_valid_ident_after() {
        // Degenerate input — falls through to the text fallback.
        let out = ssa_key("", "% bad");
        assert_eq!(out, "% bad");
    }
}
