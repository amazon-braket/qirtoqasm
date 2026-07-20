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

"""Kernel with a ``list[float]`` argument.

cudaq lowers ``list[float]`` to a ``{ double*, i64 }`` struct-by-value
plus an alloca/store/load pattern in QIR. qirtoqasm's alloca-folding
pass recovers the stored constants at the gate-argument use sites,
so ``rx(angles[0], q[0])`` with ``angles=[0.1, 0.2]`` emits
``rx(0.1) q[0];`` with no ``input`` declaration.
"""

import cudaq


@cudaq.kernel
def kernel(angles: list[float]):
    q = cudaq.qvector(2)
    rx(angles[0], q[0])
    ry(angles[1], q[1])
    x.ctrl(q[0], q[1])
    mz(q)
