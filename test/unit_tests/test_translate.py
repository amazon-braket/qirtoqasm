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

"""Stub-asserting tests for the scaffolding `qirtoqasm.translate`.

These tests are intentionally minimal. They lock down the public
shape of the Python package (importable name, exported symbols, error
class, version string) and confirm that the scaffolding stub returns
the expected ``"translate is not yet implemented"`` error. The full
unit-test suite, plus the 100% Python-shim coverage gate, will be
added once the real PyO3 binding is in place.
"""

from __future__ import annotations

import pytest

import qirtoqasm


def test_module_exposes_public_api() -> None:
    assert hasattr(qirtoqasm, "translate")
    assert hasattr(qirtoqasm, "QIRTOQASMError")
    assert hasattr(qirtoqasm, "__version__")
    # ``__all__`` lists the supported public surface.
    assert set(qirtoqasm.__all__) == {"QIRTOQASMError", "__version__", "translate"}


def test_version_is_a_nonempty_string() -> None:
    assert isinstance(qirtoqasm.__version__, str)
    assert qirtoqasm.__version__


def test_translate_raises_not_yet_implemented() -> None:
    with pytest.raises(qirtoqasm.QIRTOQASMError, match="not yet implemented"):
        qirtoqasm.translate("anything")


def test_translate_with_producer_kwarg_still_raises() -> None:
    # The keyword-only ``producer`` argument is part of the public
    # signature; the stub still errors regardless.
    with pytest.raises(qirtoqasm.QIRTOQASMError, match="not yet implemented"):
        qirtoqasm.translate("anything", producer="mylib 0.1.2")


def test_translate_rejects_extra_positional_arg() -> None:
    # Guard against accidental signature drift: ``producer`` must remain
    # keyword-only so future options can be added without breaking
    # existing callers.
    with pytest.raises(TypeError):
        qirtoqasm.translate("anything", "mylib 0.1.2")  # type: ignore[misc]
