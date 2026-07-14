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

"""CUDA-Q → QIR → qirtoqasm → Braket LocalSimulator e2e tests.

Each fixture runs in a subprocess (CUDA-Q's MLIR JIT has process-wide state).
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

import pytest

from _helpers import SHOTS, approx, assert_distributions_equivalent

pytestmark = [
    pytest.mark.integ,
    pytest.mark.skipif(sys.platform == "win32", reason="CUDA-Q has no Windows wheels"),
]

pytest.importorskip("cudaq")
pytest.importorskip("braket.devices")

FIXTURES = Path(__file__).parent / "fixtures_cudaq"
RUNNER = Path(__file__).parent / "_cudaq_runner.py"


def _run(fixture: str, *, profile: str = "qir-adaptive", kernel_args: tuple = ()) -> dict:
    cmd = [sys.executable, str(RUNNER), str(FIXTURES / fixture), str(SHOTS), profile]
    if kernel_args:
        cmd.append(json.dumps(kernel_args))

    completed = subprocess.run(cmd, capture_output=True, text=True, timeout=240, check=False)
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


def test_cudaq_bell_state() -> None:
    result = _run("bell.py")

    assert "cnot q[0], q[1];" in result["openqasm"]
    assert "stdgates.inc" not in result["openqasm"]

    counts = result["counts"]
    total = sum(counts.values())
    assert approx(counts.get("00", 0) / total, 0.5), counts
    assert approx(counts.get("11", 0) / total, 0.5), counts
    assert counts.get("01", 0) == 0, counts
    assert counts.get("10", 0) == 0, counts

    assert_distributions_equivalent(result["native_counts"], counts)


def test_cudaq_ghz3() -> None:
    result = _run("ghz3.py")

    counts = result["counts"]
    total = sum(counts.values())
    assert approx(counts.get("000", 0) / total, 0.5), counts
    assert approx(counts.get("111", 0) / total, 0.5), counts
    for bits, n in counts.items():
        if bits not in {"000", "111"}:
            assert n == 0, counts

    assert_distributions_equivalent(result["native_counts"], counts)


def test_cudaq_feedforward_mcm() -> None:
    """Void-return MCM kernel — no native cross-sim (cudaq.sample and
    cudaq.run both reject this shape)."""
    result = _run("feedforward.py")

    counts = result["counts"]
    total = sum(counts.values())
    assert approx(counts.get("00", 0) / total, 0.5), counts
    assert approx(counts.get("11", 0) / total, 0.5), counts
    assert counts.get("01", 0) == 0, counts
    assert counts.get("10", 0) == 0, counts


def test_cudaq_feedforward_with_return() -> None:
    """Return-typed MCM kernel — exercises cudaq.run native sim."""
    result = _run("feedforward_with_return.py")

    counts = result["counts"]
    total = sum(counts.values())
    assert approx(counts.get("00", 0) / total, 0.5), counts
    assert approx(counts.get("11", 0) / total, 0.5), counts
    assert counts.get("01", 0) == 0, counts
    assert counts.get("10", 0) == 0, counts

    assert_distributions_equivalent(result["native_counts"], counts)


def test_cudaq_bernstein_vazirani() -> None:
    result = _run("bernstein_vazirani.py")

    assert "cnot" in result["openqasm"]
    for bits, n in result["counts"].items():
        if n > 0:
            assert bits[:3] == "101", (bits, "BV must recover the hidden string")

    # Native counts are 3-bit (only data qubits measured).
    native = result["native_counts"]
    braket_trimmed: dict[str, int] = {}
    for bits, n in result["counts"].items():
        braket_trimmed[bits[:3]] = braket_trimmed.get(bits[:3], 0) + n
    assert_distributions_equivalent(native, braket_trimmed)


def test_cudaq_list_float_parameter() -> None:
    result = _run("list_float_param.py", kernel_args=([0.1, 0.2],))

    openqasm = result["openqasm"]
    assert "rx(0.1) q[0];" in openqasm
    assert "ry(0.2) q[1];" in openqasm
    assert "cnot q[0], q[1];" in openqasm
    assert "input " not in openqasm

    counts = result["counts"]
    total = sum(counts.values())
    assert counts.get("00", 0) / total > 0.9, counts

    assert_distributions_equivalent(result["native_counts"], counts)
