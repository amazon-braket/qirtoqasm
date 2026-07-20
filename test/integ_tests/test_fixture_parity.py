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

"""Fixture-parity tests: byte-exact translation and cross-simulator correctness.

``test_translates_fixture_byte_for_byte`` — each .ll/.qasm pair is compared byte-exact.
``test_qirrunner_and_braket_agree_on_distribution`` — qirrunner vs Braket chi-squared comparison.
"""

from __future__ import annotations

import json
import subprocess
import sys
from collections import Counter
from pathlib import Path

import pytest

import qirtoqasm
from _helpers import SHOTS, assert_distributions_equivalent

pytest.importorskip("qirrunner")
braket_devices = pytest.importorskip("braket.devices")
braket_ir = pytest.importorskip("braket.ir.openqasm")

FIXTURES = Path(__file__).parent / "fixtures_qir"
QIRRUNNER = Path(__file__).parent / "_qirrunner_runner.py"

_FIXTURE_NAMES = sorted(
    p.stem for p in FIXTURES.glob("*.ll") if (FIXTURES / f"{p.stem}.qasm").exists()
)


# Fixtures excluded from cross-simulator correctness testing.
# Each entry documents why qirrunner cannot execute the fixture.
_QIRRUNNER_UNSUPPORTED: dict[str, str] = {
    # Multi-controlled gates dispatched through the generalizedInvoke... variadic intrinsic.
    "cudaq_cswap": "qirrunner has no implementation for controlled-swap via the generalizedInvoke... intrinsic",
    "cudaq_cy_via_ctrl": "qirrunner has no implementation for controlled-Y via the generalizedInvoke... intrinsic",
    "cudaq_toffoli": "qirrunner has no implementation for the Toffoli (CCX) via the generalizedInvoke... intrinsic",
    "cudaq_llvm16_cswap": "qirrunner has no implementation for controlled-swap via the generalizedInvoke... intrinsic (LLVM 15+ opaque-pointer variant)",
    "cudaq_llvm16_cy_via_ctrl": "qirrunner has no implementation for controlled-Y via the generalizedInvoke... intrinsic (LLVM 15+ opaque-pointer variant)",
    "cudaq_llvm16_toffoli": "qirrunner has no implementation for the Toffoli (CCX) via the generalizedInvoke... intrinsic (LLVM 15+ opaque-pointer variant)",
    # phasedx rotation gate: not in qirrunner's intrinsic table.
    "phasedx_decomposed_toffoli": "qirrunner has no implementation for the __quantum__qis__phasedx__body gate",
    "phasedx_discard_measurements": "qirrunner has no implementation for the __quantum__qis__phasedx__body gate",
    "phasedx_gates_zoo": "qirrunner has no implementation for the __quantum__qis__phasedx__body gate",
    "phasedx_ghz3": "qirrunner has no implementation for the __quantum__qis__phasedx__body gate",
    # Parametrized entry points: qirrunner requires a nullary void-returning main.
    "cudaq_list_param": "qirrunner rejects non-nullary entry points (fixture takes a runtime parameter)",
    "cudaq_param_kernel": "qirrunner rejects non-nullary entry points (fixture takes a runtime parameter)",
    # Return-value infrastructure allocates via malloc, which qirrunner does not link in.
    "cudaq_feedforward_with_return": "qirrunner cannot link the malloc-based return-value infrastructure",
    # These fixtures emit only integer_record_output with no result_record_output calls,
    # so qirrunner reports no measurement data to compare against Braket.
    "qsharp_int_record_output": "qirrunner has no measurement data to report — fixture emits only integer_record_output, not result_record_output",
    "qsharp_phi_i64_counter": "qirrunner has no measurement data to report — fixture emits only integer_record_output, not result_record_output",
    # Teleportation records a non-contiguous subset of qubits, so qirrunner's declared-outputs
    # bitstring cannot be aligned to Braket's all-qubits report for chi-squared comparison.
    "teleportation": "teleportation records a non-contiguous subset of qubits; qirrunner's declared-outputs bitstring cannot be aligned to Braket's all-qubits report",
    "phi_i64_landing_pad_teleportation": "teleportation records a non-contiguous subset of qubits; qirrunner's declared-outputs bitstring cannot be aligned to Braket's all-qubits report",
}


def _is_simulatable(ll_path: Path) -> bool:
    if ll_path.stem in _QIRRUNNER_UNSUPPORTED:
        return False
    text = ll_path.read_text()
    if "record_output" not in text:
        return False
    # Adaptive profile + reset: qirrunner reports pre-reset measurements,
    # Braket reports post-reset qubit state (semantic mismatch).
    return not ("adaptive_profile" in text and "reset__body" in text)


_SIMULATABLE_NAMES = sorted(
    p.stem
    for p in FIXTURES.glob("*.ll")
    if (FIXTURES / f"{p.stem}.qasm").exists() and _is_simulatable(p)
)


def _strip_generated_by(s: str) -> str:
    return "\n".join(line for line in s.splitlines() if not line.startswith("// generated-by:"))


@pytest.mark.parametrize("name", _FIXTURE_NAMES)
def test_translates_fixture_byte_for_byte(name: str) -> None:
    ir_path = FIXTURES / f"{name}.ll"
    expected_path = FIXTURES / f"{name}.qasm"
    actual = _strip_generated_by(qirtoqasm.translate(ir_path.read_text())).rstrip()
    expected = _strip_generated_by(expected_path.read_text()).rstrip()
    assert actual == expected, (
        f"\n--- Expected ({expected_path.name}) ---\n{expected}\n--- Actual ---\n{actual}\n"
    )


def _run_qirrunner(ll_path: Path, shots: int) -> dict[str, int]:
    """Run .ll through qirrunner in a subprocess (isolation against segfaults)."""
    completed = subprocess.run(
        [sys.executable, str(QIRRUNNER), str(ll_path), str(shots)],
        capture_output=True,
        text=True,
        timeout=60,
        check=False,
    )
    if completed.returncode != 0:
        pytest.fail(
            f"qirrunner failed on {ll_path.name}: exit {completed.returncode}\n"
            f"stderr:\n{completed.stderr}"
        )
    for line in reversed(completed.stdout.splitlines()):
        line = line.strip()
        if line.startswith("{") and line.endswith("}"):
            return json.loads(line)
    pytest.fail(f"no JSON payload from qirrunner:\n{completed.stdout}\n{completed.stderr}")


def _run_braket(ll_path: Path, shots: int) -> dict[str, int]:
    openqasm = qirtoqasm.translate(ll_path.read_text())
    result = (
        braket_devices.LocalSimulator()
        .run(braket_ir.Program(source=openqasm), shots=shots)
        .result()
    )
    return dict(result.measurement_counts)


@pytest.mark.integ
@pytest.mark.parametrize("name", _SIMULATABLE_NAMES)
def test_qirrunner_and_braket_agree_on_distribution(name: str) -> None:
    ll_path = FIXTURES / f"{name}.ll"
    qir_counts = _run_qirrunner(ll_path, SHOTS)
    braket_counts = _run_braket(ll_path, SHOTS)

    assert qir_counts, f"qirrunner produced no output for {name}"

    qir_width = len(next(iter(qir_counts)))
    braket_width = len(next(iter(braket_counts)))

    # qirrunner reports only declared outputs; Braket reports all qubits.
    # When widths differ, marginalize Braket to the last N bits (which
    # correspond to the declared result_record_output calls).
    if qir_width != braket_width:
        marginal: Counter = Counter()
        for bits, n in braket_counts.items():
            marginal[bits[-qir_width:]] += n
        braket_counts = dict(marginal)

    assert_distributions_equivalent(qir_counts, braket_counts)
