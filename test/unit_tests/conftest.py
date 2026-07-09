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

"""Shared fixtures for the qirtoqasm Python test suite."""

from __future__ import annotations

import textwrap

import pytest

import qirtoqasm


def dedent(ir: str) -> str:
    """Normalize a multi-line IR literal for test use."""
    return textwrap.dedent(ir).strip() + "\n"


@pytest.fixture
def translate():
    """Return a helper that translates a QIR snippet and strips trailing whitespace.

    Keeps test code tight::

        def test_foo(translate):
            assert "h q[0];" in translate(IR)
    """

    def _translate(ir: str) -> str:
        return qirtoqasm.translate(dedent(ir)).strip()

    return _translate
