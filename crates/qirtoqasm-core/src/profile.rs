// Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License").

//! Builder registry keyed by QIR function name.

use indexmap::IndexMap;

/// Classification of what a QIR intrinsic lowers to.
#[derive(Debug, Clone)]
pub enum FunctionBuilder {
    /// A gate application: `<gate_name>(args...) qubits...;`, optionally
    /// with an `inv @` modifier.
    Gate {
        /// Emitted gate name (e.g. `"cnot"` for the `cx` QIR intrinsic).
        gate_name: String,
        /// Whether to emit `inv @` before the gate.
        adjoint: bool,
    },
    /// A single-qubit measurement: `c[i] = measure q[j];`.
    Measurement,
    /// Measure-and-reset (`__quantum__qis__mresetz__body`): emits
    /// `c[i] = measure q[j]; reset q[j];`.
    MeasureAndReset,
    /// A qubit reset: `reset q[i];`.
    Reset,
    /// Binds an SSA result to `c[i]` so downstream branches / compound
    /// Booleans can consume the measurement outcome.
    ReadResult,
    /// Drops the call (emits no OQ3 statements). Used for the
    /// `__quantum__rt__*_record_output` / `__quantum__rt__initialize` family.
    RecordOutputNoop,
    /// Variadic `generalizedInvokeWithRotationsControlsTargets`
    /// intrinsic. Delegates to a specialized builder that resolves
    /// the inner `__quantum__qis__<op>__ctl` function pointer and
    /// emits the matching Braket-native multi-controlled gate
    /// (`cnot`, `ccnot`, `cy`, `cz`, `cswap`). `numRotations > 0`
    /// is not yet supported.
    GeneralizedControlled,
}

/// A set of [`FunctionBuilder`] registrations keyed by QIR function name.
#[derive(Debug, Clone)]
pub struct Profile {
    /// Profile name (e.g. `"base_profile"`).
    pub name: String,
    builders: IndexMap<String, FunctionBuilder>,
}

impl Profile {
    /// Create an empty profile with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            builders: IndexMap::new(),
        }
    }

    /// Associate a QIR function name with a builder, overwriting any
    /// prior registration under the same name.
    pub fn register(&mut self, function_name: impl Into<String>, builder: FunctionBuilder) {
        self.builders.insert(function_name.into(), builder);
    }

    /// Look up the builder registered for `function_name`.
    ///
    /// Falls back to prefix matching for LLVM intrinsics (e.g.,
    /// `llvm.memcpy.p0i8.p0i8.i64` matches a registration for
    /// `llvm.memcpy`).
    pub fn get_builder(&self, function_name: &str) -> Option<&FunctionBuilder> {
        if let Some(b) = self.builders.get(function_name) {
            return Some(b);
        }
        if function_name.starts_with("llvm.") {
            for (key, builder) in &self.builders {
                if function_name.starts_with(key.as_str()) {
                    return Some(builder);
                }
            }
        }
        None
    }

    /// Whether `function_name` has a registered builder.
    pub fn contains(&self, function_name: &str) -> bool {
        self.builders.contains_key(function_name)
    }
}

/// Construct the default `BaseProfile` registrations.
///
/// Each entry maps a QIR function name (the `__quantum__qis__*` and
/// `__quantum__rt__*` intrinsics that QIR producers emit) to the
/// builder that lowers it to Braket-compatible OpenQASM 3. The gate
/// names in the table below (`cnot`, `ccnot`, `xx` / `yy` / `zz`,
/// `prx`, …) are the Braket-native spellings — not the OpenQASM
/// `stdgates.inc` names — so the emitted program is accepted by
/// `braket.ir.openqasm.Program` without any further rewriting.
pub fn base_profile() -> Profile {
    let mut p = Profile::new("base_profile");

    // Each tuple is (QIR-name suffix, Braket-OQ3 gate name, adjoint flag).
    // The full QIR name is `__quantum__qis__<suffix>`. Where the
    // Braket name differs from the QIR suffix, the comment notes
    // why — usually because Braket uses a Braket-native alias rather
    // than the OpenQASM stdgates.inc name.
    const GATES: &[(&str, &str, bool)] = &[
        // Self-adjoint single-qubit Cliffords.
        ("h__body", "h", false),
        ("x__body", "x", false),
        ("y__body", "y", false),
        ("z__body", "z", false),
        // Single-qubit gates with adjoints.
        ("s__body", "s", false),
        ("s__adj", "s", true),
        ("t__body", "t", false),
        ("t__adj", "t", true),
        // CNOT and its `cx` alias both emit Braket-native `cnot`.
        ("cnot__body", "cnot", false),
        ("cx__body", "cnot", false),
        // Two-qubit single-control Cliffords.
        ("cy__body", "cy", false),
        ("cz__body", "cz", false),
        ("swap__body", "swap", false),
        // Single-qubit rotations.
        ("rx__body", "rx", false),
        ("ry__body", "ry", false),
        ("rz__body", "rz", false),
        // Two-qubit Ising rotations: QIR rxx/ryy/rzz → Braket xx/yy/zz.
        ("rxx__body", "xx", false),
        ("ryy__body", "yy", false),
        ("rzz__body", "zz", false),
        // Toffoli aliases both emit `ccnot`.
        ("ccx__body", "ccnot", false),
        ("ccnot__body", "ccnot", false),
        // Two-parameter phased-X rotation: QIR phasedx → Braket prx.
        ("phasedx__body", "prx", false),
    ];
    for (suffix, gate, adjoint) in GATES {
        p.register(
            format!("__quantum__qis__{suffix}"),
            FunctionBuilder::Gate {
                gate_name: (*gate).into(),
                adjoint: *adjoint,
            },
        );
    }

    // (fully-qualified qir name, builder kind) for non-Gate entries.
    let non_gates: &[(&str, FunctionBuilder)] = &[
        // Measurement (`mz` / `m` aliases).
        ("__quantum__qis__mz__body", FunctionBuilder::Measurement),
        ("__quantum__qis__m__body", FunctionBuilder::Measurement),
        // Measure-and-reset.
        (
            "__quantum__qis__mresetz__body",
            FunctionBuilder::MeasureAndReset,
        ),
        // Reset.
        ("__quantum__qis__reset__body", FunctionBuilder::Reset),
        // Read-result (Adaptive Profile); both the QIS-namespaced
        // and runtime-namespaced spellings are accepted.
        (
            "__quantum__qis__read_result__body",
            FunctionBuilder::ReadResult,
        ),
        ("__quantum__rt__read_result", FunctionBuilder::ReadResult),
        // Variadic multi-controlled dispatch intrinsic.
        (
            "generalizedInvokeWithRotationsControlsTargets",
            FunctionBuilder::GeneralizedControlled,
        ),
    ];
    for (name, builder) in non_gates {
        p.register(*name, builder.clone());
    }

    // Runtime record-output no-ops + non-quantum allocator helpers.
    // `llvm.memcpy` uses prefix matching (see `get_builder`).
    for n in [
        "__quantum__rt__result_record_output",
        "__quantum__rt__array_record_output",
        "__quantum__rt__tuple_record_output",
        "__quantum__rt__integer_record_output",
        // Some producers spell this `int_record_output`; QIR-Alliance
        // drafts use `integer_record_output`. Accept both.
        "__quantum__rt__int_record_output",
        "__quantum__rt__bool_record_output",
        "__quantum__rt__double_record_output",
        "__quantum__rt__initialize",
        "malloc",
        "free",
        "llvm.memcpy",
    ] {
        p.register(n, FunctionBuilder::RecordOutputNoop);
    }

    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_profile_registers_expected_builders() {
        let p = base_profile();
        assert_eq!(p.name, "base_profile");

        // h body
        let FunctionBuilder::Gate { gate_name, adjoint } =
            p.get_builder("__quantum__qis__h__body").unwrap()
        else {
            panic!("expected Gate for h body")
        };
        assert_eq!(gate_name, "h");
        assert!(!adjoint);

        // t adjoint
        let FunctionBuilder::Gate { gate_name, adjoint } =
            p.get_builder("__quantum__qis__t__adj").unwrap()
        else {
            panic!("expected Gate for t adj")
        };
        assert_eq!(gate_name, "t");
        assert!(*adjoint);

        // cx → cnot alias
        let FunctionBuilder::Gate { gate_name, .. } =
            p.get_builder("__quantum__qis__cx__body").unwrap()
        else {
            panic!()
        };
        assert_eq!(gate_name, "cnot");

        // rxx → xx alias
        let FunctionBuilder::Gate { gate_name, .. } =
            p.get_builder("__quantum__qis__rxx__body").unwrap()
        else {
            panic!()
        };
        assert_eq!(gate_name, "xx");

        // ccx → ccnot alias
        let FunctionBuilder::Gate { gate_name, .. } =
            p.get_builder("__quantum__qis__ccx__body").unwrap()
        else {
            panic!()
        };
        assert_eq!(gate_name, "ccnot");

        // phasedx → prx alias
        let FunctionBuilder::Gate { gate_name, .. } =
            p.get_builder("__quantum__qis__phasedx__body").unwrap()
        else {
            panic!()
        };
        assert_eq!(gate_name, "prx");

        assert!(matches!(
            p.get_builder("__quantum__qis__mz__body"),
            Some(FunctionBuilder::Measurement)
        ));
        assert!(matches!(
            p.get_builder("__quantum__qis__mresetz__body"),
            Some(FunctionBuilder::MeasureAndReset)
        ));
        assert!(matches!(
            p.get_builder("__quantum__qis__reset__body"),
            Some(FunctionBuilder::Reset)
        ));
        assert!(matches!(
            p.get_builder("__quantum__qis__read_result__body"),
            Some(FunctionBuilder::ReadResult)
        ));
        assert!(matches!(
            p.get_builder("__quantum__rt__read_result"),
            Some(FunctionBuilder::ReadResult)
        ));
        assert!(matches!(
            p.get_builder("__quantum__rt__result_record_output"),
            Some(FunctionBuilder::RecordOutputNoop)
        ));
        // Both the `int_record_output` spelling and the newer-spec
        // `integer_record_output` must be accepted.
        assert!(matches!(
            p.get_builder("__quantum__rt__int_record_output"),
            Some(FunctionBuilder::RecordOutputNoop)
        ));
        assert!(matches!(
            p.get_builder("__quantum__rt__integer_record_output"),
            Some(FunctionBuilder::RecordOutputNoop)
        ));
        assert!(p.get_builder("__quantum__qis__imagined__body").is_none());
    }

    #[test]
    fn register_overwrites_prior_registration() {
        let mut p = base_profile();
        p.register(
            "__quantum__qis__h__body",
            FunctionBuilder::Gate {
                gate_name: "custom_h".into(),
                adjoint: false,
            },
        );
        let FunctionBuilder::Gate { gate_name, .. } =
            p.get_builder("__quantum__qis__h__body").unwrap()
        else {
            panic!()
        };
        assert_eq!(gate_name, "custom_h");
    }

    #[test]
    fn contains_matches_get_builder() {
        let p = base_profile();
        assert!(p.contains("__quantum__qis__h__body"));
        assert!(!p.contains("__quantum__qis__imagined__body"));
    }
}

#[cfg(test)]
mod more_tests {
    use super::*;

    #[test]
    fn profile_name_accessor_and_clone() {
        let p = base_profile();
        let cloned = p.clone();
        assert_eq!(cloned.name, "base_profile");
    }

    #[test]
    fn function_builder_debug_and_clone_for_every_variant() {
        let gate = FunctionBuilder::Gate {
            gate_name: "h".into(),
            adjoint: false,
        };
        let _ = format!("{gate:?}");
        let _ = gate.clone();
        let _ = format!("{:?}", FunctionBuilder::Measurement.clone());
        let _ = format!("{:?}", FunctionBuilder::MeasureAndReset.clone());
        let _ = format!("{:?}", FunctionBuilder::Reset.clone());
        let _ = format!("{:?}", FunctionBuilder::ReadResult.clone());
        let _ = format!("{:?}", FunctionBuilder::RecordOutputNoop.clone());
        let _ = format!("{:?}", FunctionBuilder::GeneralizedControlled.clone());
    }

    #[test]
    fn profile_into_string_and_str_both_accepted() {
        let mut p = Profile::new(String::from("via-string"));
        assert_eq!(p.name, "via-string");
        p.register(
            "foo",
            FunctionBuilder::Gate {
                gate_name: "h".into(),
                adjoint: false,
            },
        );
        p.register(String::from("bar"), FunctionBuilder::Measurement);
        assert!(p.contains("foo"));
        assert!(p.contains("bar"));
    }

    #[test]
    fn malloc_and_free_registered_as_noop() {
        let p = base_profile();
        assert!(matches!(
            p.get_builder("malloc"),
            Some(FunctionBuilder::RecordOutputNoop)
        ));
        assert!(matches!(
            p.get_builder("free"),
            Some(FunctionBuilder::RecordOutputNoop)
        ));
    }

    #[test]
    fn llvm_intrinsic_prefix_matching() {
        let p = base_profile();
        assert!(matches!(
            p.get_builder("llvm.memcpy.p0i8.p0i8.i64"),
            Some(FunctionBuilder::RecordOutputNoop)
        ));
        assert!(matches!(
            p.get_builder("llvm.memcpy.p0.p0.i64"),
            Some(FunctionBuilder::RecordOutputNoop)
        ));
        // Non-registered llvm intrinsic returns None
        assert!(p.get_builder("llvm.trap").is_none());
        // Non-llvm prefix doesn't trigger prefix search
        assert!(p.get_builder("llvm_not_a_real_prefix").is_none());
    }

    #[test]
    fn contains_reflects_prefix_matching() {
        let p = base_profile();
        assert!(p.contains("malloc"));
        assert!(p.contains("free"));
        assert!(!p.contains("unknown_function"));
    }

    #[test]
    fn initialize_registered_as_noop() {
        let p = base_profile();
        assert!(matches!(
            p.get_builder("__quantum__rt__initialize"),
            Some(FunctionBuilder::RecordOutputNoop)
        ));
    }
}
