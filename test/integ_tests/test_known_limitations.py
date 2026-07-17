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

"""Known-limitation fixtures: QIR inputs that currently can't translate
but must produce a *clear, user-actionable* error.

If a future change makes one of these cases succeed (by implementing
the missing support), the test will fail and the author should move
the fixture into the fixture-parity suite and add an expected ``.qasm``
pair.

Today the only known limitation is the variadic
``generalizedInvokeWithRotationsControlsTargets`` dispatch for
multi-controlled gates that don't have a Braket-native counterpart
(controlled-H, CCZ with two Z-controls, 3-control X, …). The mapped
tuples (``x.ctrl`` 1-2 ctrls, ``y.ctrl``, ``z.ctrl``, ``swap.ctrl``,
``phaseshift.ctrl``) translate successfully and live in the
fixture-parity suite.
"""

from __future__ import annotations

from pathlib import Path

import pytest

import qirtoqasm
from qirtoqasm import QirToQasmError

FIXTURES = Path(__file__).parent / "fixtures_qir"


# Fixture filename → substrings that must all appear in the exception message.
_KNOWN_LIMITATIONS: dict[str, list[str]] = {
    # Controlled-H via the variadic ``generalizedInvoke...`` intrinsic: no
    # Braket-native CH and no lowering table entry. The error must name the
    # intrinsic (the root cause) AND direct the user to the workaround
    # (upstream decomposition).
    "cudaq_unsupported_ctrl_h.ll": [
        "generalizedInvokeWithRotationsControlsTargets",
        "decomposition",
    ],
}


@pytest.mark.parametrize("fixture_name,expected_substrings", list(_KNOWN_LIMITATIONS.items()))
def test_known_limitation_error_names_root_cause_and_workaround(
    fixture_name: str, expected_substrings: list[str]
) -> None:
    """Each known-limitation fixture must raise ``QirToQasmError`` with a
    message that names both the root cause (the intrinsic or opcode
    that can't be lowered) and the workaround (typically upstream
    decomposition). If this assertion fails because a fixture now
    translates successfully, move it into the fixture-parity suite
    and add an expected ``.qasm`` pair.
    """
    ir_path = FIXTURES / fixture_name
    assert ir_path.exists(), f"missing limitation fixture: {fixture_name}"
    with pytest.raises(QirToQasmError) as excinfo:
        qirtoqasm.translate(ir_path.read_text())
    message = str(excinfo.value)
    for substring in expected_substrings:
        assert substring in message, message
