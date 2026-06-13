// Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License").

//! Test-only helpers shared across `#[cfg(test)] mod tests` blocks.
//!
//! `Operand::PtrConst` constructors keyed by `(struct_name, index)`.

#![allow(dead_code)] // not every helper is used by every consumer module

use crate::ir::Operand;

/// `Operand::PtrConst` typed as a `%Qubit*` pointer to qubit `i`.
pub fn qubit(i: i64) -> Operand {
    Operand::PtrConst {
        struct_name: Some("Qubit".into()),
        index: i,
    }
}

/// `Operand::PtrConst` typed as a `%Result*` pointer to result `i`.
pub fn result(i: i64) -> Operand {
    Operand::PtrConst {
        struct_name: Some("Result".into()),
        index: i,
    }
}

/// `Operand::PtrConst` typed as an opaque `ptr` (LLVM-15+ form),
/// at index `i`.
pub fn opaque(i: i64) -> Operand {
    Operand::PtrConst {
        struct_name: None,
        index: i,
    }
}
