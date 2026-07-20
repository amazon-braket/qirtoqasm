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

"""Q# → QIR → qirtoqasm → Braket LocalSimulator e2e tests.

Each fixture runs in a subprocess (qsharp.init resets interpreter state).
The *_reset_return.qs variants also run on the native Q# simulator for
cross-simulator correctness validation.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

import pytest

from _helpers import SHOTS, approx, assert_distributions_equivalent

pytestmark = pytest.mark.integ

pytest.importorskip("qsharp")
pytest.importorskip("braket.devices")

FIXTURES = Path(__file__).parent / "fixtures_qsharp"
RUNNER = Path(__file__).parent / "_qsharp_runner.py"


def _run(fixture: str, entry: str, profile: str, *, native_sim: bool = False) -> dict:
    cmd = [sys.executable, str(RUNNER), str(FIXTURES / fixture), entry, profile, str(SHOTS)]
    if native_sim:
        cmd.append("--native-sim")

    completed = subprocess.run(cmd, capture_output=True, text=True, timeout=180, check=False)
    if completed.returncode != 0:
        pytest.fail(
            f"subprocess exit {completed.returncode}\n"
            f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}"
        )
    for line in reversed(completed.stdout.splitlines()):
        line = line.strip()
        if line.startswith("{") and line.endswith("}"):
            return json.loads(line)
    pytest.fail(f"no JSON payload in:\n{completed.stdout}")


def test_qsharp_bell_state() -> None:
    result = _run("bell.qs", "Bell()", "Base")

    assert "cnot q[0], q[1];" in result["openqasm"]
    assert "include" not in result["openqasm"]

    counts = result["counts"]
    total = sum(counts.values())
    assert approx(counts.get("00", 0) / total, 0.5), counts
    assert approx(counts.get("11", 0) / total, 0.5), counts
    assert counts.get("01", 0) == 0, counts
    assert counts.get("10", 0) == 0, counts


def test_qsharp_ghz5() -> None:
    result = _run("ghz5.qs", "GHZ5()", "Base")

    counts = result["counts"]
    total = sum(counts.values())
    assert approx(counts.get("00000", 0) / total, 0.5), counts
    assert approx(counts.get("11111", 0) / total, 0.5), counts
    for bits, n in counts.items():
        if bits not in {"00000", "11111"}:
            assert n == 0, counts


def test_qsharp_feedforward_mcm() -> None:
    result = _run("feedforward.qs", "Feedforward()", "Adaptive_RI")
    assert "if (c[0]" in result["openqasm"], result["openqasm"]

    counts = result["counts"]
    total = sum(counts.values())
    assert approx(counts.get("00", 0) / total, 0.5), counts
    assert approx(counts.get("11", 0) / total, 0.5), counts
    assert counts.get("01", 0) == 0, counts
    assert counts.get("10", 0) == 0, counts


def test_qsharp_grover2() -> None:
    result = _run("grover2.qs", "Grover2()", "Base")
    counts = result["counts"]
    assert counts.get("11", 0) == sum(counts.values()), counts


def test_qsharp_teleport() -> None:
    result = _run("teleport.qs", "Teleport()", "Adaptive_RI")
    for bits, n in result["counts"].items():
        if n > 0:
            assert bits[2] == "0", (bits, result["counts"])


@pytest.mark.xfail(
    strict=False,
    reason=(
        "Blocked on amazon-braket/amazon-braket-default-simulator-python#388 "
        "(per-classical-bit MCM outcome overlay in `_run_branched`). Remove "
        "this marker once the fix ships to PyPI."
    ),
)
def test_qsharp_iterative_phase_estimation() -> None:
    result = _run("ipe4bit.qs", "IPE4Bit()", "Adaptive_RI")
    for bits, n in result["counts"].items():
        if n > 0:
            assert bits == "1100", (bits, "phase bits (LSB first) must be 1100")


@pytest.mark.xfail(
    strict=False,
    reason=(
        "Blocked on amazon-braket/amazon-braket-default-simulator-python#388 "
        "(per-classical-bit MCM outcome overlay in `_run_branched`). Remove "
        "this marker once the fix ships to PyPI."
    ),
)
def test_qsharp_iterative_phase_estimation_loop_form() -> None:
    result = _run("ipe4bit_loop.qs", "IPE4BitLoop()", "Adaptive_RI")
    for bits, n in result["counts"].items():
        if n > 0:
            assert bits == "1100", (bits, "phase bits (LSB first) must be 1100")


def test_qsharp_bell_reset_return() -> None:
    result = _run("bell_reset_return.qs", "BellSim()", "Base", native_sim=True)

    counts = result["counts"]
    total = sum(counts.values())
    assert approx(counts.get("00", 0) / total, 0.5), counts
    assert approx(counts.get("11", 0) / total, 0.5), counts

    assert_distributions_equivalent(result["native_counts"], counts)


def test_qsharp_ghz5_reset_return() -> None:
    result = _run("ghz5_reset_return.qs", "GHZ5Sim()", "Base", native_sim=True)

    counts = result["counts"]
    total = sum(counts.values())
    assert approx(counts.get("00000", 0) / total, 0.5), counts
    assert approx(counts.get("11111", 0) / total, 0.5), counts

    assert_distributions_equivalent(result["native_counts"], counts)


def test_qsharp_feedforward_reset_return() -> None:
    """Adaptive_RI resets zero qubits after measurement, so Braket reports
    post-reset state. Native Q# returns pre-reset measurement results."""
    result = _run("feedforward_reset_return.qs", "FeedforwardSim()", "Adaptive_RI", native_sim=True)

    assert "00" in result["counts"]

    native = result["native_counts"]
    total = sum(native.values())
    assert approx(native.get("00", 0) / total, 0.5), native
    assert approx(native.get("11", 0) / total, 0.5), native


def test_qsharp_grover2_reset_return() -> None:
    result = _run("grover2_reset_return.qs", "Grover2Sim()", "Base", native_sim=True)

    counts = result["counts"]
    assert counts.get("11", 0) == sum(counts.values()), counts

    assert_distributions_equivalent(result["native_counts"], counts)


def test_qsharp_teleport_reset_return() -> None:
    """Adaptive_RI resets zero qubits; validate via native sim only."""
    result = _run("teleport_reset_return.qs", "TeleportSim()", "Adaptive_RI", native_sim=True)

    assert result["counts"]

    for bits, n in result["native_counts"].items():
        if n > 0:
            assert bits[2] == "0", (bits, result["native_counts"])


def test_qsharp_ipe4bit_reset_return() -> None:
    """Adaptive_RI resets zero qubits; validate via native sim only."""
    result = _run("ipe4bit_reset_return.qs", "IPE4BitSim()", "Adaptive_RI", native_sim=True)

    assert result["counts"]

    for bits, n in result["native_counts"].items():
        if n > 0:
            assert bits[1] == "1", (bits, "q[1] (native)")
            assert bits[2:6] == "1100", (bits, "storage (native)")
