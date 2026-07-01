// Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License").

//! In-memory data model for parsed QIR modules.

pub mod model;
pub mod parser;
pub(crate) mod parser_util;

pub use model::{
    BinaryI1Op, Block, BrCondOperand, Function, Icmp, Instruction, IntArithOp, Module, Operand,
    PhiIncoming, PredicateI1,
};
