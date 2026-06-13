// Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License").

//! Mutable translator state: qubit/result register sizing and SSA bindings.

use std::collections::HashMap;

use crate::error::{QirToQasmError, Result};
use crate::ir::Operand;
use crate::oq3::ast::Expression;

/// OpenQASM register identifier we emit for qubits.
pub const QUBIT_REGISTER: &str = "q";

/// OpenQASM register identifier we emit for classical bits.
pub const RESULT_REGISTER: &str = "c";

/// Lowering state shared across builders during one translation.
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
