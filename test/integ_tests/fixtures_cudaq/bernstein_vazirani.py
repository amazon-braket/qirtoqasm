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

"""Bernstein-Vazirani with hidden bit string ``101``.

Three data qubits + one ancilla. Every shot deterministically
recovers the hidden string in the data qubits. Exercises
``x.ctrl`` on the oracle bits (which lowers through cudaq's
variadic multi-controlled dispatch) plus Hadamards on both sides.
"""

import cudaq


@cudaq.kernel
def kernel():
    q = cudaq.qvector(3)
    aux = cudaq.qubit()
    x(aux)
    h(aux)
    h(q[0])
    h(q[1])
    h(q[2])
    x.ctrl(q[0], aux)
    x.ctrl(q[2], aux)
    h(q[0])
    h(q[1])
    h(q[2])
    mz(q)
