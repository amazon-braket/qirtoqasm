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

"""qirtoqasm → Braket LocalSimulator e2e tests (in-process, no subprocess)."""

from __future__ import annotations

from collections import Counter
from pathlib import Path

import pytest

from _helpers import SHOTS, assert_distributions_equivalent

pytestmark = pytest.mark.integ

braket_devices = pytest.importorskip("braket.devices")
braket_ir = pytest.importorskip("braket.ir.openqasm")

import qirtoqasm  # noqa: E402

FIXTURES = Path(__file__).parent / "fixtures_qir"


def _run(ir_name: str) -> Counter:
    openqasm = qirtoqasm.translate((FIXTURES / ir_name).read_text())
    result = (
        braket_devices.LocalSimulator()
        .run(braket_ir.Program(source=openqasm), shots=SHOTS)
        .result()
    )
    return Counter(result.measurement_counts)


def test_bell_state_on_braket() -> None:
    counts = _run("bell.ll")
    assert_distributions_equivalent(dict(counts), {"00": SHOTS // 2, "11": SHOTS // 2})
    assert counts["01"] == 0, counts
    assert counts["10"] == 0, counts


def test_ghz3_on_braket() -> None:
    counts = _run("ghz.ll")
    assert_distributions_equivalent(dict(counts), {"000": SHOTS // 2, "111": SHOTS // 2})
    for bits, n in counts.items():
        if bits not in {"000", "111"}:
            assert n == 0, counts


def test_mcm_if_else_on_braket() -> None:
    counts = _run("mcm_if_else.ll")
    assert_distributions_equivalent(dict(counts), {"00": SHOTS // 2, "11": SHOTS // 2})


def test_mcm_if_only_on_braket() -> None:
    counts = _run("mcm_if.ll")
    assert_distributions_equivalent(dict(counts), {"00": SHOTS // 2, "11": SHOTS // 2})


def test_teleportation_on_braket() -> None:
    # Braket's default simulator returns measurement counts as a bitstring
    # sized to the classical register width (``bit[2] c`` here → 2-char
    # keys), so the four ``(c[0], c[1])`` classical outcomes are what's
    # observable end-to-end. Their distribution must be uniform — that's
    # the classical byproduct signature of teleportation.
    counts = _run("teleportation.ll")
    by_leading: Counter = Counter()
    for bits, n in counts.items():
        by_leading[bits[:2]] += n
    expected = {"00": SHOTS // 4, "01": SHOTS // 4, "10": SHOTS // 4, "11": SHOTS // 4}
    assert_distributions_equivalent(dict(by_leading), expected)
