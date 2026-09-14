# Changelog

## v0.1.0.post0 (2026-09-02)

### Documentation Changes

 * add llms.txt generation

## v0.1.0 (2026-07-24)

### Features

 * final release-polish pass + advanced CodeQL setup
 * CUDA-Q → QIR → qirtoqasm → Braket end-to-end integ tier
 * Q# → QIR → qirtoqasm → Braket end-to-end integ tier
 * Braket LocalSimulator end-to-end integ tier
 * fixture-parity regression tests + qirrunner cross-simulator tests
 * full Python unit-test suite and integ-test infra
 * C ABI FFI tests and C/C++ smoke tests
 * complete Adaptive Profile lowering in the translator
 * add Base Profile end-to-end QIR to OpenQASM 3 translator
 * complete the QIR parser with complex instructions
 * add gate builders for QIS-call lowering
 * add classical-control lowering for i1 and integer ops
 * add QIR module parser and simple instructions
 * add text-level QIR signature extraction
 * add SSA symbol table for the translator
 * add structural CFG reduction to OpenQASM 3
 * add byte-exact OpenQASM 3 pretty-printer
 * add QIR data model and Operand test helpers
 * add QIR profile registry
 * add OpenQASM 3 AST
 * qirtoqasm translate interface and build infrastructure

### Bug Fixes and Other Changes

 * reject malformed inputs with clean errors
 * Initial commit
