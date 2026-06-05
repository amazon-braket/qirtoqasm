// Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License").

//! Structural CFG reduction: collapse a list of [`BlockLowering`]s into
//! a single sequence of OpenQASM statements.
//!
//! Implements a minimal directed-graph type keyed by block name rather
//! than pulling in `petgraph`.

use std::collections::{BTreeMap, HashSet};

use crate::error::{QirToQasmError, Result};
use crate::oq3::ast::*;

/// OQ3 statements plus branch info for a single parsed QIR basic block.
///
/// The translator constructs one of these per block and hands the list
/// to [`lower_cfg`]. The reducer mutates the statements/condition/targets
/// fields in-place as it merges nodes.
#[derive(Debug, Clone)]
pub struct BlockLowering {
    /// Canonical block name.
    pub name: String,
    /// OQ3 statements the block emits (so far).
    pub statements: Vec<Statement>,
    /// Branching condition when the block ends in `br i1 …`. `None` for
    /// unconditional branches and returns.
    pub condition: Option<Expression>,
    /// Successor block names: 0 entries for `ret`, 1 for unconditional
    /// `br`, 2 for conditional `br i1` (the `true` target first).
    pub targets: Vec<String>,
}

/// Reduce a list of block lowerings into a flat body.
///
/// Returns the OQ3 statements of the entry block after all reductions.
pub fn lower_cfg(blocks: Vec<BlockLowering>, entry_name: &str) -> Result<Vec<Statement>> {
    if blocks.is_empty() {
        return Ok(Vec::new());
    }
    let mut map: BTreeMap<String, BlockLowering> =
        blocks.into_iter().map(|b| (b.name.clone(), b)).collect();
    if !map.contains_key(entry_name) {
        return Err(QirToQasmError::unsupported_cfg(format!(
            "entry block {:?} not found among block lowerings",
            entry_name
        )));
    }
    let mut cfg = Cfg::new();
    for name in map.keys() {
        cfg.add_node(name);
    }
    for b in map.values() {
        for t in &b.targets {
            cfg.add_edge(&b.name, t);
        }
    }
    loop {
        if reduce_short_circuit_phi(&mut cfg, &mut map) {
            continue;
        }
        if reduce_sequential(&mut cfg, &mut map) {
            continue;
        }
        if reduce_self_loop(&mut cfg, &mut map) {
            continue;
        }
        if reduce_if_else(&mut cfg, &mut map) {
            continue;
        }
        if reduce_if_no_else(&mut cfg, &mut map) {
            continue;
        }
        if reduce_while(&mut cfg, &mut map) {
            continue;
        }
        break;
    }
    if cfg.node_count() != 1 || !cfg.contains_node(entry_name) {
        let mut remaining: Vec<&String> = cfg.nodes().collect();
        remaining.sort();
        return Err(QirToQasmError::unsupported_cfg(format!(
            "could not reduce CFG to a single block; remaining blocks: {:?}. \
             qirtoqasm supports only structured control flow (sequence, if/else, \
             simple while).",
            remaining
        )));
    }
    Ok(map
        .remove(entry_name)
        .expect("entry guaranteed present")
        .statements)
}

// ---------------------------------------------------------------------------
// Tiny directed-graph used only by this module
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Cfg {
    nodes: HashSet<String>,
    // out[src] = ordered list of destinations, preserving (true, false)
    // order for conditional branches. Duplicate edges are collapsed.
    out: BTreeMap<String, Vec<String>>,
    inn: BTreeMap<String, HashSet<String>>,
}

impl Cfg {
    fn new() -> Self {
        Self::default()
    }

    fn add_node(&mut self, n: &str) {
        self.nodes.insert(n.to_string());
        self.out.entry(n.to_string()).or_default();
        self.inn.entry(n.to_string()).or_default();
    }

    fn add_edge(&mut self, src: &str, dst: &str) {
        self.add_node(src);
        self.add_node(dst);
        let outs = self.out.get_mut(src).unwrap();
        if !outs.contains(&dst.to_string()) {
            outs.push(dst.to_string());
        }
        self.inn.get_mut(dst).unwrap().insert(src.to_string());
    }

    fn remove_node(&mut self, n: &str) {
        self.nodes.remove(n);
        // Remove outgoing edges.
        if let Some(outs) = self.out.remove(n) {
            for dst in outs {
                if let Some(s) = self.inn.get_mut(&dst) {
                    s.remove(n);
                }
            }
        }
        // Remove incoming edges.
        if let Some(preds) = self.inn.remove(n) {
            for p in preds {
                if let Some(v) = self.out.get_mut(&p) {
                    v.retain(|t| t != n);
                }
            }
        }
    }

    fn remove_edge(&mut self, src: &str, dst: &str) {
        if let Some(v) = self.out.get_mut(src) {
            v.retain(|t| t != dst);
        }
        if let Some(s) = self.inn.get_mut(dst) {
            s.remove(src);
        }
    }

    /// Replace `src`'s successor list with `new_targets`, updating the
    /// predecessor index for every dropped and added edge. Duplicate
    /// entries in `new_targets` are collapsed.
    fn set_out(&mut self, src: &str, new_targets: &[String]) {
        let old: Vec<String> = self.out.get(src).cloned().unwrap_or_default();
        for dst in &old {
            if let Some(s) = self.inn.get_mut(dst) {
                s.remove(src);
            }
        }
        if let Some(v) = self.out.get_mut(src) {
            v.clear();
        } else {
            self.out.insert(src.to_string(), Vec::new());
        }
        for dst in new_targets {
            self.add_edge(src, dst);
        }
    }

    fn successors(&self, n: &str) -> Vec<String> {
        self.out.get(n).cloned().unwrap_or_default()
    }

    fn predecessors(&self, n: &str) -> HashSet<String> {
        self.inn.get(n).cloned().unwrap_or_default()
    }

    fn out_degree(&self, n: &str) -> usize {
        self.out.get(n).map(|v| v.len()).unwrap_or(0)
    }

    fn in_degree(&self, n: &str) -> usize {
        self.inn.get(n).map(|v| v.len()).unwrap_or(0)
    }

    fn has_edge(&self, src: &str, dst: &str) -> bool {
        self.out
            .get(src)
            .map(|v| v.iter().any(|x| x == dst))
            .unwrap_or(false)
    }

    fn node_count(&self) -> usize {
        self.nodes.len()
    }

    fn contains_node(&self, n: &str) -> bool {
        self.nodes.contains(n)
    }

    fn nodes(&self) -> impl Iterator<Item = &String> {
        self.nodes.iter()
    }

    /// Snapshot of the current node set; used by reduction rules that
    /// iterate while mutating.
    fn node_snapshot(&self) -> Vec<String> {
        self.nodes.iter().cloned().collect()
    }
}

// ---------------------------------------------------------------------------
// Reduction rules
// ---------------------------------------------------------------------------

fn reduce_sequential(cfg: &mut Cfg, map: &mut BTreeMap<String, BlockLowering>) -> bool {
    let mut candidates: Vec<(String, String)> = Vec::new();
    for a in cfg.node_snapshot() {
        for b in cfg.successors(&a) {
            if cfg.out_degree(&a) == 1 && cfg.in_degree(&b) == 1 {
                candidates.push((a.clone(), b));
            }
        }
    }
    if let Some((a, b)) = candidates.into_iter().next() {
        merge_sequential(cfg, map, &a, &b);
        return true;
    }
    false
}

fn reduce_short_circuit_phi(cfg: &mut Cfg, map: &mut BTreeMap<String, BlockLowering>) -> bool {
    let nodes = cfg.node_snapshot();
    for a in &nodes {
        if cfg.out_degree(a) != 2 {
            continue;
        }
        let succs = cfg.successors(a);
        for (b, c) in [
            (succs[0].clone(), succs[1].clone()),
            (succs[1].clone(), succs[0].clone()),
        ] {
            if cfg.in_degree(&b) != 1 || cfg.out_degree(&b) != 1 {
                continue;
            }
            if cfg.successors(&b).first().map(|s| s.as_str()) != Some(c.as_str()) {
                continue;
            }
            let c_preds = cfg.predecessors(&c);
            if c_preds != HashSet::from([a.clone(), b.clone()]) {
                continue;
            }
            let cond_is_compound = map
                .get(&c)
                .and_then(|bl| bl.condition.as_ref())
                .map(is_compound_boolean)
                .unwrap_or(false);
            if !cond_is_compound {
                continue;
            }
            if !map
                .get(&b)
                .map(|bl| bl.statements.is_empty())
                .unwrap_or(false)
            {
                continue;
            }

            // Fold A, B, C into A.
            let b_block = map.remove(&b).unwrap();
            let c_block = map.remove(&c).unwrap();
            let a_block = map.get_mut(a).unwrap();
            a_block
                .statements
                .extend(b_block.statements.into_iter().chain(c_block.statements));
            a_block.condition = c_block.condition;
            a_block.targets = c_block.targets.clone();
            let new_targets = a_block.targets.clone();
            cfg.remove_node(&b);
            cfg.remove_node(&c);
            for t in new_targets {
                cfg.add_edge(a, &t);
            }
            return true;
        }
    }
    false
}

fn is_compound_boolean(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::Binary {
            op: BinaryOp::And | BinaryOp::Or,
            ..
        }
    )
}

fn reduce_self_loop(cfg: &mut Cfg, map: &mut BTreeMap<String, BlockLowering>) -> bool {
    for node in cfg.node_snapshot() {
        if !cfg.has_edge(&node, &node) {
            continue;
        }
        let successors: Vec<String> = cfg
            .successors(&node)
            .into_iter()
            .filter(|s| s != &node)
            .collect();
        if cfg.out_degree(&node) == 2 && successors.len() == 1 {
            let exit = successors[0].clone();
            let a = map.get_mut(&node).unwrap();
            let condition = a
                .condition
                .take()
                .expect("self-loop requires a conditional branch");
            let self_loop_true = a.targets.first().map(|s| s == &node).unwrap_or(false);
            let effective = if self_loop_true {
                condition
            } else {
                // Body on the false branch: emit `<cond> == 0`.
                bin(BinaryOp::Eq, condition, int(0))
            };
            let body = a.statements.clone();
            a.statements.push(Statement::WhileLoop {
                condition: effective,
                body,
            });
            a.targets = vec![exit.clone()];
            cfg.remove_edge(&node, &node);
            return true;
        }
    }
    false
}

fn reduce_if_else(cfg: &mut Cfg, map: &mut BTreeMap<String, BlockLowering>) -> bool {
    for a in cfg.node_snapshot() {
        if cfg.out_degree(&a) != 2 {
            continue;
        }
        let succs = cfg.successors(&a);
        let (b, c) = (succs[0].clone(), succs[1].clone());
        if b == c {
            continue;
        }
        if cfg.in_degree(&b) != 1 || cfg.in_degree(&c) != 1 {
            continue;
        }
        if cfg.out_degree(&b) != 1 || cfg.out_degree(&c) != 1 {
            continue;
        }
        let d_from_b = cfg.successors(&b)[0].clone();
        let d_from_c = cfg.successors(&c)[0].clone();
        if d_from_b != d_from_c {
            continue;
        }
        let a_targets = map.get(&a).unwrap().targets.clone();
        let (true_target, false_target) = (a_targets[0].clone(), a_targets[1].clone());
        let if_block = map.get(&true_target).unwrap().statements.clone();
        let else_block = map.get(&false_target).unwrap().statements.clone();
        let condition = map
            .get_mut(&a)
            .unwrap()
            .condition
            .take()
            .expect("conditional branch without a condition");
        let a_block = map.get_mut(&a).unwrap();
        // Skip a no-op `if (cond) {} else {}`; keep the merge-edge only.
        if !if_block.is_empty() || !else_block.is_empty() {
            a_block.statements.push(Statement::BranchingStatement {
                condition,
                if_block,
                else_block,
            });
        }
        a_block.targets = vec![d_from_b.clone()];
        cfg.remove_node(&b);
        cfg.remove_node(&c);
        cfg.add_edge(&a, &d_from_b);
        return true;
    }
    false
}

fn reduce_if_no_else(cfg: &mut Cfg, map: &mut BTreeMap<String, BlockLowering>) -> bool {
    for a in cfg.node_snapshot() {
        if cfg.out_degree(&a) != 2 {
            continue;
        }
        let succs = cfg.successors(&a);
        for (b, d) in [
            (succs[0].clone(), succs[1].clone()),
            (succs[1].clone(), succs[0].clone()),
        ] {
            if cfg.in_degree(&b) != 1 || cfg.out_degree(&b) != 1 {
                continue;
            }
            if cfg.successors(&b).first().map(|s| s.as_str()) != Some(d.as_str()) {
                continue;
            }
            let a_targets = map.get(&a).unwrap().targets.clone();
            let b_is_true = a_targets[0] == b;
            let b_stmts = map.get(&b).unwrap().statements.clone();
            let (if_block, else_block) = if b_is_true {
                (b_stmts, Vec::new())
            } else {
                (Vec::new(), b_stmts)
            };
            let condition = map
                .get_mut(&a)
                .unwrap()
                .condition
                .take()
                .expect("conditional branch without a condition");
            let a_block = map.get_mut(&a).unwrap();
            // Skip emitting a no-op `if (cond) {}`. The diamond collapses
            // to a straight edge through `d`; any predicate side-effects
            // (like phi-merged classical values) are already recorded.
            if !if_block.is_empty() || !else_block.is_empty() {
                a_block.statements.push(Statement::BranchingStatement {
                    condition,
                    if_block,
                    else_block,
                });
            }
            a_block.targets = vec![d.clone()];
            cfg.remove_node(&b);
            return true;
        }
    }
    false
}

fn reduce_while(cfg: &mut Cfg, map: &mut BTreeMap<String, BlockLowering>) -> bool {
    for a in cfg.node_snapshot() {
        if cfg.out_degree(&a) != 2 {
            continue;
        }
        let succs = cfg.successors(&a);
        for (b, exit) in [
            (succs[0].clone(), succs[1].clone()),
            (succs[1].clone(), succs[0].clone()),
        ] {
            if b == exit {
                continue;
            }
            if cfg.in_degree(&b) != 1 || cfg.out_degree(&b) != 1 {
                continue;
            }
            if cfg.successors(&b).first().map(|s| s.as_str()) != Some(a.as_str()) {
                continue;
            }
            let a_targets = map.get(&a).unwrap().targets.clone();
            let b_is_true = a_targets[0] == b;
            let loop_body_prefix = map.get(&b).unwrap().statements.clone();
            let a_body_prefix = map.get(&a).unwrap().statements.clone();
            let condition = map
                .get_mut(&a)
                .unwrap()
                .condition
                .take()
                .expect("while needs a conditional branch");
            let effective = if b_is_true {
                condition
            } else {
                // Body on the false branch: emit `<cond> == 0`.
                bin(BinaryOp::Eq, condition, int(0))
            };
            let mut body = loop_body_prefix;
            body.extend(a_body_prefix);
            let a_block = map.get_mut(&a).unwrap();
            a_block.statements.push(Statement::WhileLoop {
                condition: effective,
                body,
            });
            a_block.targets = vec![exit.clone()];
            cfg.remove_node(&b);
            // Rewrite a's out-edges so the cfg matches the block's targets.
            cfg.set_out(&a, &a_block.targets.clone());
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Low-level helpers
// ---------------------------------------------------------------------------

fn merge_sequential(cfg: &mut Cfg, map: &mut BTreeMap<String, BlockLowering>, a: &str, b: &str) {
    let b_block = map.remove(b).unwrap();
    let a_block = map.get_mut(a).unwrap();
    a_block.statements.extend(b_block.statements);
    a_block.condition = b_block.condition;
    a_block.targets = b_block.targets.clone();
    cfg.remove_node(b);
    for t in b_block.targets {
        cfg.add_edge(a, &t);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oq3::ast::index_expr;

    fn block(
        name: &str,
        stmts: Vec<Statement>,
        cond: Option<Expression>,
        tgts: &[&str],
    ) -> BlockLowering {
        BlockLowering {
            name: name.into(),
            statements: stmts,
            condition: cond,
            targets: tgts.iter().map(|s| (*s).to_string()).collect(),
        }
    }

    fn x(q: i64) -> Statement {
        Statement::QuantumGate {
            modifiers: vec![],
            name: "x".into(),
            arguments: vec![],
            qubits: vec![crate::oq3::ast::indexed_ident("q", q)],
        }
    }

    #[test]
    fn sequential_merge_concatenates_statements() {
        let blocks = vec![
            block("entry", vec![x(0)], None, &["next"]),
            block("next", vec![x(1)], None, &[]),
        ];
        let out = lower_cfg(blocks, "entry").unwrap();
        assert_eq!(out, vec![x(0), x(1)]);
    }

    #[test]
    fn if_no_else_triangle_produces_if_statement() {
        let cond = index_expr("c", 0);
        let blocks = vec![
            block("entry", vec![x(0)], Some(cond.clone()), &["then", "exit"]),
            block("then", vec![x(1)], None, &["exit"]),
            block("exit", vec![x(2)], None, &[]),
        ];
        let out = lower_cfg(blocks, "entry").unwrap();
        let want = vec![
            x(0),
            Statement::BranchingStatement {
                condition: cond,
                if_block: vec![x(1)],
                else_block: vec![],
            },
            x(2),
        ];
        assert_eq!(out, want);
    }

    #[test]
    fn if_else_diamond_produces_both_branches() {
        let cond = index_expr("c", 0);
        let blocks = vec![
            block("entry", vec![], Some(cond.clone()), &["t", "e"]),
            block("t", vec![x(1)], None, &["join"]),
            block("e", vec![x(2)], None, &["join"]),
            block("join", vec![x(3)], None, &[]),
        ];
        let out = lower_cfg(blocks, "entry").unwrap();
        let want = vec![
            Statement::BranchingStatement {
                condition: cond,
                if_block: vec![x(1)],
                else_block: vec![x(2)],
            },
            x(3),
        ];
        assert_eq!(out, want);
    }

    #[test]
    fn while_loop_wraps_body_in_while() {
        // Canonical while-loop CFG: entry → head → {body → head, exit}.
        // `head` is the conditional block with two successors; `body`
        // loops back to `head`; `exit` is the post-loop block.
        let cond = index_expr("c", 0);
        let blocks = vec![
            block("entry", vec![x(0)], None, &["head"]),
            block("head", vec![], Some(cond.clone()), &["body", "exit"]),
            block("body", vec![x(1)], None, &["head"]),
            block("exit", vec![x(2)], None, &[]),
        ];
        let out = lower_cfg(blocks, "entry").unwrap();
        // After reduction the entry block absorbs everything. The head's
        // original statements (empty) are followed by the `while`, which
        // wraps the body's statements plus the head's statements, then
        // the exit block's statements follow the while.
        let expected = vec![
            x(0),
            Statement::WhileLoop {
                condition: cond,
                body: vec![x(1)],
            },
            x(2),
        ];
        assert_eq!(out, expected);
    }

    #[test]
    fn irreducible_cfg_errors_with_pinned_substring() {
        // Diamond without a join — two terminal successors.
        let cond = index_expr("c", 0);
        let blocks = vec![
            block("entry", vec![], Some(cond), &["t", "e"]),
            block("t", vec![x(1)], None, &[]),
            block("e", vec![x(2)], None, &[]),
        ];
        let err = lower_cfg(blocks, "entry").unwrap_err();
        assert!(err.to_string().contains("could not reduce CFG"));
        assert!(err.to_string().contains("structured control flow"));
    }
}

#[cfg(test)]
mod more_tests {
    use super::*;
    use crate::oq3::ast::{index_expr, indexed_ident, BinaryOp, Statement};

    fn b(
        name: &str,
        stmts: Vec<Statement>,
        cond: Option<Expression>,
        targets: &[&str],
    ) -> BlockLowering {
        BlockLowering {
            name: name.into(),
            statements: stmts,
            condition: cond,
            targets: targets.iter().map(|s| (*s).to_string()).collect(),
        }
    }

    fn x(q: i64) -> Statement {
        Statement::QuantumGate {
            modifiers: vec![],
            name: "x".into(),
            arguments: vec![],
            qubits: vec![indexed_ident("q", q)],
        }
    }

    #[test]
    fn empty_block_list_returns_empty_body() {
        let out = lower_cfg(Vec::new(), "entry").unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn entry_not_in_block_list_errors() {
        let err = lower_cfg(vec![b("a", vec![], None, &[])], "missing").unwrap_err();
        assert!(err.to_string().contains("entry block"));
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn self_loop_negates_condition_when_loop_is_false_branch() {
        // a -> {exit, a}: loop body is taken on the false arm.
        let cond = index_expr("c", 0);
        let blocks = vec![
            b("a", vec![x(0)], Some(cond), &["exit", "a"]),
            b("exit", vec![x(1)], None, &[]),
        ];
        let out = lower_cfg(blocks, "a").unwrap();
        // Last statement of `a` should be a WhileLoop whose condition
        // is `<cond> == 0` (body on the false branch).
        let while_stmt = out
            .iter()
            .find(|s| matches!(s, Statement::WhileLoop { .. }))
            .unwrap();
        let Statement::WhileLoop { condition, .. } = while_stmt else {
            unreachable!()
        };
        let Expression::Binary { op, rhs, .. } = condition else {
            panic!("expected binary-Eq condition, got {condition:?}")
        };
        assert_eq!(*op, BinaryOp::Eq);
        assert!(matches!(rhs.as_ref(), Expression::Integer(0)));
    }

    #[test]
    fn while_with_body_on_false_branch_negates_condition() {
        // entry -> head; head -> {exit, body}; body -> head.
        let cond = index_expr("c", 0);
        let blocks = vec![
            b("entry", vec![], None, &["head"]),
            b("head", vec![], Some(cond), &["exit", "body"]),
            b("body", vec![x(1)], None, &["head"]),
            b("exit", vec![x(2)], None, &[]),
        ];
        let out = lower_cfg(blocks, "entry").unwrap();
        let while_stmt = out
            .iter()
            .find(|s| matches!(s, Statement::WhileLoop { .. }))
            .unwrap();
        let Statement::WhileLoop { condition, .. } = while_stmt else {
            unreachable!()
        };
        let Expression::Binary { op, rhs, .. } = condition else {
            panic!("expected binary-Eq condition, got {condition:?}")
        };
        assert_eq!(*op, BinaryOp::Eq);
        assert!(matches!(rhs.as_ref(), Expression::Integer(0)));
    }

    #[test]
    fn if_no_else_triangle_with_body_on_false_branch() {
        // entry -> {exit, body}; body -> exit.  Matches the
        // Python reducer's "b_is_true = false" path.
        let cond = index_expr("c", 0);
        let blocks = vec![
            b("entry", vec![], Some(cond), &["exit", "body"]),
            b("body", vec![x(1)], None, &["exit"]),
            b("exit", vec![], None, &[]),
        ];
        let out = lower_cfg(blocks, "entry").unwrap();
        // The BranchingStatement's `if_block` should be empty and
        // `else_block` should contain body's statements.
        let branch = out
            .iter()
            .find(|s| matches!(s, Statement::BranchingStatement { .. }))
            .unwrap();
        let Statement::BranchingStatement {
            if_block,
            else_block,
            ..
        } = branch
        else {
            unreachable!()
        };
        assert!(if_block.is_empty());
        assert_eq!(else_block.len(), 1);
    }

    #[test]
    fn sequential_merge_fires_before_if_else_on_predecessor_block() {
        // entry -> a -> b -> exit  (a linear chain).  Pure sequential,
        // no branches at all.
        let blocks = vec![
            b("entry", vec![x(0)], None, &["a"]),
            b("a", vec![x(1)], None, &["b"]),
            b("b", vec![x(2)], None, &["exit"]),
            b("exit", vec![], None, &[]),
        ];
        let out = lower_cfg(blocks, "entry").unwrap();
        assert_eq!(out, vec![x(0), x(1), x(2)]);
    }

    #[test]
    fn short_circuit_and_pattern_reduces_to_compound_if() {
        // entry -> {rhs, phi_block}  (short-circuit false-branch to phi_block)
        // rhs  -> phi_block
        // phi_block has compound condition: BinaryExpression(AND, ...)
        // phi_block -> {then, exit}
        // then -> exit
        let cond = index_expr("c", 0);
        let compound = Expression::Binary {
            op: BinaryOp::And,
            lhs: Box::new(Expression::Binary {
                op: BinaryOp::Eq,
                lhs: Box::new(index_expr("c", 0)),
                rhs: Box::new(Expression::Integer(1)),
            }),
            rhs: Box::new(Expression::Binary {
                op: BinaryOp::Eq,
                lhs: Box::new(index_expr("c", 1)),
                rhs: Box::new(Expression::Integer(1)),
            }),
        };
        let blocks = vec![
            b("entry", vec![], Some(cond), &["rhs", "phi_block"]),
            b("rhs", vec![], None, &["phi_block"]),
            b(
                "phi_block",
                vec![],
                Some(compound.clone()),
                &["then", "exit"],
            ),
            b("then", vec![x(1)], None, &["exit"]),
            b("exit", vec![], None, &[]),
        ];
        let out = lower_cfg(blocks, "entry").unwrap();
        // We should see a single BranchingStatement with a compound condition.
        let branch = out
            .iter()
            .find(|s| matches!(s, Statement::BranchingStatement { .. }))
            .unwrap();
        let Statement::BranchingStatement { condition, .. } = branch else {
            unreachable!()
        };
        assert_eq!(*condition, compound);
    }
}

#[cfg(test)]
mod derive_coverage {
    use super::*;
    #[test]
    fn block_lowering_debug_and_clone() {
        let b = BlockLowering {
            name: "entry".into(),
            statements: vec![],
            condition: None,
            targets: vec![],
        };
        let _ = format!("{b:?}");
        let _ = b.clone();
    }
}
