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


# Fixture filename → regex that must appear in the exception message.
_KNOWN_LIMITATIONS: dict[str, str] = {
    # Controlled-H via the variadic ``generalizedInvoke...`` intrinsic:
    # no Braket-native CH and no lowering table entry — must raise with
    # the intrinsic name so the user knows to decompose upstream.
    "cudaq_unsupported_ctrl_h.ll": "generalizedInvokeWithRotationsControlsTargets",
}


@pytest.mark.parametrize("fixture_name,expected_substring", list(_KNOWN_LIMITATIONS.items()))
def test_known_limitation_produces_actionable_error(
    fixture_name: str, expected_substring: str
) -> None:
    """Each known-limitation fixture must raise ``QirToQasmError`` with a
    message that mentions the root cause.
    """
    ir_path = FIXTURES / fixture_name
    assert ir_path.exists(), f"missing limitation fixture: {fixture_name}"
    with pytest.raises(QirToQasmError, match=expected_substring):
        qirtoqasm.translate(ir_path.read_text())


def test_unmapped_controlled_gate_error_names_decomposition_workaround() -> None:
    """Users hitting the controlled-H variadic path need two signals
    from the error message: what's failing (a multi-controlled gate)
    and why (the variadic lowering intrinsic).
    """
    ir_text = (FIXTURES / "cudaq_unsupported_ctrl_h.ll").read_text()
    try:
        qirtoqasm.translate(ir_text)
    except QirToQasmError as e:
        message = str(e)
        assert "generalizedInvoke" in message, message
    else:  # pragma: no cover
        pytest.fail(
            "cudaq_unsupported_ctrl_h fixture unexpectedly succeeded. If "
            "controlled-H is now supported, move the fixture to a "
            ".qasm-paired fixture-parity test."
        )
