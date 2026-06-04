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

"""qirtoqasm: translate QIR to Braket-compatible OpenQASM 3.0.

Public API:

- :func:`translate` — QIR (LLVM textual IR, ``.ll``) → OQ3 text. The
  only positional argument is the QIR source. Every tunable is a
  keyword-only argument with a default, so new options can be added
  without breaking existing callers.
- :class:`QirToQasmError` — single exception raised on any failure.

Example::

    import qirtoqasm

    qasm = qirtoqasm.translate(open("bell.ll").read())
    qasm = qirtoqasm.translate(ir, producer="mylib 0.1.2")
"""

from __future__ import annotations

from qirtoqasm import _qirtoqasm_native as _rust
from qirtoqasm._version import __version__

QirToQasmError = _rust.QirToQasmError
translate = _rust.translate

__all__ = [
    "QirToQasmError",
    "__version__",
    "translate",
]
