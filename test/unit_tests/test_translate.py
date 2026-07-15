# Copyright Amazon.com Inc. or its affiliates. All Rights Reserved.
#
# Licensed under the Apache License, Version 2.0 (the "License"). You
# may not use this file except in compliance with the License. A copy of
# the License is located at
#
#     http://aws.amazon.com/apache2.0/
#
# or in the "license" file accompanying this file. This file is
# distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF
# ANY KIND, either express or implied. See the License for the specific
# language governing permissions and limitations under the License.

"""Unit tests for the public ``qirtoqasm`` Python surface.

The Python package exposes exactly three symbols:

- :func:`qirtoqasm.translate` — QIR (LLVM textual IR) to Braket-compatible
  OpenQASM 3, one-stage.
- :class:`qirtoqasm.QirToQasmError` — the only exception type raised on
  translation failure.
- :data:`qirtoqasm.__version__` — package version string.

These tests cover that surface. The translator's internal correctness
(signature extraction, CFG reduction, boolean lowering, SSA keying,
gate builders, producer-specific idioms) is covered by the Rust core's
inline ``#[cfg(test)]`` unit tests and by the ``.ll`` → ``.qasm``
byte-exact regression fixtures under ``test/integ_tests/``.
"""

from __future__ import annotations

import re

import pytest

import qirtoqasm
from qirtoqasm import QirToQasmError

BELL_IR = """
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

attributes #0 = { "entry_point" "qir_profiles"="base_profile" "requiredQubits"="2" "requiredResults"="2" }
attributes #1 = { "irreversible" }
"""


def test_translate_is_callable() -> None:
    assert callable(qirtoqasm.translate)


def test_error_class_is_exception() -> None:
    assert issubclass(QirToQasmError, Exception)


def test_version_is_non_empty_string() -> None:
    assert isinstance(qirtoqasm.__version__, str)
    assert qirtoqasm.__version__
    # PEP 440 / pyproject ``dynamic = ["version"]`` round-trip.
    assert re.match(r"^\d+\.\d+\.\d+", qirtoqasm.__version__)


def test_dunder_all_lists_only_public_surface() -> None:
    # The Python shim deliberately exposes only translate,
    # QirToQasmError, and __version__. Extra symbols would drift
    # from the C ABI.
    assert set(qirtoqasm.__all__) == {"translate", "QirToQasmError", "__version__"}


def test_header_is_openqasm_3_0(translate) -> None:
    assert translate(BELL_IR).startswith("OPENQASM 3.0;")


def test_quantum_and_classical_register_declarations(translate) -> None:
    out = translate(BELL_IR)
    assert "qubit[2] q;" in out
    assert "bit[2] c;" in out


def test_gate_sequence(translate) -> None:
    out = translate(BELL_IR)
    assert "h q[0];" in out
    assert "cnot q[0], q[1];" in out
    assert "c[0] = measure q[0];" in out
    assert "c[1] = measure q[1];" in out


def test_cnot_emitted_even_when_qir_uses_cx(translate) -> None:
    # Braket accepts ``cnot`` natively but rejects ``cx`` unless
    # ``include "stdgates.inc";`` is present (and qirtoqasm never
    # emits an include).
    ir = """
    %Qubit = type opaque

    define void @main() #0 {
      call void @__quantum__qis__cx__body(%Qubit* null, %Qubit* inttoptr (i64 1 to %Qubit*))
      ret void
    }
    declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)
    attributes #0 = { "entry_point" "qir_profiles"="base_profile" "requiredQubits"="2" "requiredResults"="0" }
    """
    out = translate(ir)
    assert "cnot q[0], q[1];" in out
    assert "cx q[0], q[1];" not in out


def test_ccnot_emitted_for_toffoli(translate) -> None:
    ir = """
    %Qubit = type opaque

    define void @main() #0 {
      call void @__quantum__qis__ccx__body(
          %Qubit* null,
          %Qubit* inttoptr (i64 1 to %Qubit*),
          %Qubit* inttoptr (i64 2 to %Qubit*))
      ret void
    }
    declare void @__quantum__qis__ccx__body(%Qubit*, %Qubit*, %Qubit*)
    attributes #0 = { "entry_point" "qir_profiles"="base_profile" "requiredQubits"="3" "requiredResults"="0" }
    """
    out = translate(ir)
    assert "ccnot q[0], q[1], q[2];" in out
    assert "ccx" not in out


@pytest.mark.parametrize(
    "qir_name,emit_name",
    [("rxx", "xx"), ("ryy", "yy"), ("rzz", "zz")],
)
def test_ising_rotations_emit_braket_aliases(translate, qir_name: str, emit_name: str) -> None:
    # Two-qubit Ising rotations emit as Braket aliases (xx/yy/zz),
    # not the ``stdgates.inc`` names (rxx/ryy/rzz).
    ir = f"""
    %Qubit = type opaque

    define void @main() #0 {{
      call void @__quantum__qis__{qir_name}__body(
          double 0.5,
          %Qubit* null,
          %Qubit* inttoptr (i64 1 to %Qubit*))
      ret void
    }}
    declare void @__quantum__qis__{qir_name}__body(double, %Qubit*, %Qubit*)
    attributes #0 = {{ "entry_point" "qir_profiles"="base_profile" "requiredQubits"="2" "requiredResults"="0" }}
    """
    out = translate(ir)
    assert f"{emit_name}(0.5) q[0], q[1];" in out
    assert f"{qir_name}(" not in out


def test_classical_register_is_plain_bit_not_output(translate) -> None:
    # Default behavior: plain ``bit[N] c;``, NOT ``output bit[N] c;``.
    # Braket rejects the ``output`` qualifier today.
    out = translate(BELL_IR)
    assert "bit[2] c;" in out
    assert "output bit" not in out


def test_no_stdgates_include_emitted(translate) -> None:
    out = translate(BELL_IR)
    assert "include" not in out


def test_if_only_branch_on_read_result(translate) -> None:
    ir = """
    %Qubit = type opaque
    %Result = type opaque

    define void @main() #0 {
      call void @__quantum__qis__h__body(%Qubit* null)
      call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
      %c = call i1 @__quantum__qis__read_result__body(%Result* null)
      br i1 %c, label %t, label %join
    t:
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      br label %join
    join:
      ret void
    }
    declare void @__quantum__qis__h__body(%Qubit*)
    declare void @__quantum__qis__x__body(%Qubit*)
    declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1
    declare i1 @__quantum__qis__read_result__body(%Result*)
    attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "requiredQubits"="2" "requiredResults"="1" }
    attributes #1 = { "irreversible" }
    """
    out = translate(ir)
    assert "if (c[0]) {" in out
    assert "x q[1];" in out


def test_if_else_branch_emits_both_arms(translate) -> None:
    ir = """
    %Qubit = type opaque
    %Result = type opaque

    define void @main() #0 {
      call void @__quantum__qis__h__body(%Qubit* null)
      call void @__quantum__qis__mz__body(%Qubit* null, %Result* null)
      %c = call i1 @__quantum__qis__read_result__body(%Result* null)
      br i1 %c, label %t, label %f
    t:
      call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      br label %done
    f:
      call void @__quantum__qis__z__body(%Qubit* inttoptr (i64 1 to %Qubit*))
      br label %done
    done:
      ret void
    }
    declare void @__quantum__qis__h__body(%Qubit*)
    declare void @__quantum__qis__x__body(%Qubit*)
    declare void @__quantum__qis__z__body(%Qubit*)
    declare void @__quantum__qis__mz__body(%Qubit*, %Result*) #1
    declare i1 @__quantum__qis__read_result__body(%Result*)
    attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "requiredQubits"="2" "requiredResults"="1" }
    attributes #1 = { "irreversible" }
    """
    out = translate(ir)
    assert "if (c[0]) {" in out
    assert "} else {" in out
    assert "x q[1];" in out
    assert "z q[1];" in out


@pytest.mark.parametrize("gate", ["rx", "ry", "rz"])
def test_single_qubit_rotation(translate, gate: str) -> None:
    ir = f"""
    %Qubit = type opaque

    define void @main() #0 {{
      call void @__quantum__qis__{gate}__body(double 0.5, %Qubit* null)
      ret void
    }}
    declare void @__quantum__qis__{gate}__body(double, %Qubit*)
    attributes #0 = {{ "entry_point" "qir_profiles"="base_profile" "requiredQubits"="1" "requiredResults"="0" }}
    """
    out = translate(ir)
    assert f"{gate}(0.5) q[0];" in out


def test_adjoint_gate_uses_inv_modifier(translate) -> None:
    # Non-self-adjoint gates (s, t) emit ``inv @ <gate>`` per the
    # OpenQASM 3 gate-modifier syntax.
    ir = """
    %Qubit = type opaque

    define void @main() #0 {
      call void @__quantum__qis__s__adj(%Qubit* null)
      ret void
    }
    declare void @__quantum__qis__s__adj(%Qubit*)
    attributes #0 = { "entry_point" "qir_profiles"="base_profile" "requiredQubits"="1" "requiredResults"="0" }
    """
    assert "inv @ s q[0];" in translate(ir)


def test_malformed_qir_raises_qir_to_qasm_error() -> None:
    with pytest.raises(QirToQasmError):
        qirtoqasm.translate("not valid qir at all")


def test_empty_input_raises_qir_to_qasm_error() -> None:
    with pytest.raises(QirToQasmError):
        qirtoqasm.translate("")


def test_module_without_entry_point_names_the_problem() -> None:
    ir = """
    define void @not_an_entry_point() {
      ret void
    }
    """
    with pytest.raises(QirToQasmError, match="no entry-point function"):
        qirtoqasm.translate(ir)


def test_unknown_qis_intrinsic_names_the_callee() -> None:
    ir = """
    %Qubit = type opaque

    define void @main() #0 {
      call void @__quantum__qis__imagined__body(%Qubit* null)
      ret void
    }
    declare void @__quantum__qis__imagined__body(%Qubit*)
    attributes #0 = { "entry_point" "qir_profiles"="base_profile" "requiredQubits"="1" "requiredResults"="0" }
    """
    with pytest.raises(QirToQasmError, match="__quantum__qis__imagined__body"):
        qirtoqasm.translate(ir)


def test_unsupported_opcode_names_the_opcode() -> None:
    # ``fadd`` has no OpenQASM 3 value-position equivalent.
    ir = """
    %Qubit = type opaque

    define void @main() #0 {
      %x = fadd double 0.5, 0.5
      call void @__quantum__qis__h__body(%Qubit* null)
      ret void
    }
    declare void @__quantum__qis__h__body(%Qubit*)
    attributes #0 = { "entry_point" "qir_profiles"="base_profile" "requiredQubits"="1" "requiredResults"="0" }
    """
    with pytest.raises(QirToQasmError) as excinfo:
        qirtoqasm.translate(ir)
    msg = str(excinfo.value)
    assert "unsupported LLVM instruction" in msg, msg
    assert "fadd" in msg, msg


def test_unmapped_variadic_controlled_gate_names_intrinsic() -> None:
    # A variadic controlled-H invocation (no Braket-native CH) must
    # surface the intrinsic name in the error so the user can find
    # docs describing the supported decomposition patterns.
    ir = """
    %Qubit = type opaque
    %Array = type opaque

    define void @main() #0 {
      call void (i64, i64, i64, i64, i8*, ...)
        @generalizedInvokeWithRotationsControlsTargets(
          i64 0, i64 0, i64 1, i64 1,
          i8* bitcast (void (%Array*, %Qubit*)* @__quantum__qis__h__ctl to i8*),
          i8* null, i8* inttoptr (i64 1 to i8*))
      ret void
    }
    declare void @__quantum__qis__h__body(%Qubit*)
    declare void @__quantum__qis__h__ctl(%Array*, %Qubit*)
    declare void @generalizedInvokeWithRotationsControlsTargets(i64, i64, i64, i64, i8*, ...)
    attributes #0 = { "entry_point" "qir_profiles"="adaptive_profile" "requiredQubits"="2" "requiredResults"="0" }
    """
    with pytest.raises(QirToQasmError) as excinfo:
        qirtoqasm.translate(ir)
    msg = str(excinfo.value)
    assert "intrinsic" in msg, msg
    assert "generalizedInvokeWithRotationsControlsTargets" in msg, msg


def test_input_type_error_surfaces_from_pyo3() -> None:
    # ``translate`` only accepts ``str``; passing anything else
    # yields a TypeError from PyO3's argument conversion.
    with pytest.raises(TypeError):
        qirtoqasm.translate(None)
    with pytest.raises(TypeError):
        qirtoqasm.translate(123)


def test_output_is_str() -> None:
    assert isinstance(qirtoqasm.translate(BELL_IR), str)


def test_output_ends_with_newline() -> None:
    # The OpenQASM printer emits a trailing newline so concatenation
    # with caller-supplied text doesn't produce glued lines.
    assert qirtoqasm.translate(BELL_IR).endswith("\n")


def test_output_is_deterministic() -> None:
    # Two translations of the same input produce identical bytes.
    out1 = qirtoqasm.translate(BELL_IR)
    out2 = qirtoqasm.translate(BELL_IR)
    assert out1 == out2


def _generated_by_line(oq3: str) -> str:
    last = [line for line in oq3.splitlines() if line]
    assert last, "empty output"
    return last[-1]


def test_trailing_comment_always_present() -> None:
    last = _generated_by_line(qirtoqasm.translate(BELL_IR))
    assert last.startswith('// generated-by: {"name":"qirtoqasm",')


def test_comment_includes_version_and_profile_fields() -> None:
    last = _generated_by_line(qirtoqasm.translate(BELL_IR))
    expected_version = qirtoqasm.__version__.replace(".dev", "-dev")
    assert f'"version":"{expected_version}"' in last
    assert '"profile":"base_profile"' in last


def test_producer_kwarg_surfaces_in_comment() -> None:
    last = _generated_by_line(qirtoqasm.translate(BELL_IR, producer="mylib 0.1.2"))
    assert '"producer":"mylib 0.1.2"' in last


def test_producer_omitted_by_default() -> None:
    last = _generated_by_line(qirtoqasm.translate(BELL_IR))
    assert '"producer"' not in last


def test_producer_none_omits_field() -> None:
    last = _generated_by_line(qirtoqasm.translate(BELL_IR, producer=None))
    assert '"producer"' not in last


def test_producer_empty_string_omits_field() -> None:
    last = _generated_by_line(qirtoqasm.translate(BELL_IR, producer=""))
    assert '"producer"' not in last


def test_producer_is_keyword_only() -> None:
    # Positional ``producer`` must raise. Keyword-only is what
    # keeps the signature extensible.
    with pytest.raises(TypeError):
        qirtoqasm.translate(BELL_IR, "mylib 0.1.2")


def test_producer_with_special_characters_is_json_escaped() -> None:
    last = _generated_by_line(qirtoqasm.translate(BELL_IR, producer='weird "quoted" \\ path'))
    assert r'"producer":"weird \"quoted\" \\ path"' in last


def test_producer_with_newline_stays_on_one_line() -> None:
    out = qirtoqasm.translate(BELL_IR, producer="line1\nline2")
    generated_by = [line for line in out.splitlines() if line.startswith("// generated-by:")]
    assert len(generated_by) == 1
    assert r'"producer":"line1\nline2"' in generated_by[0]
