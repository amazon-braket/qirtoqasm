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

"""MCM + feedforward with explicit return — exercises cudaq.run path.

This is the return-typed variant of feedforward.py for use with
cudaq.run (which requires a return value). Also exercises that
qirtoqasm handles the return-value lowering correctly.
"""

import cudaq


@cudaq.kernel
def kernel() -> list[bool]:
    q = cudaq.qvector(2)
    h(q[0])
    b0 = mz(q[0])
    if b0:
        x(q[1])
    b1 = mz(q[1])
    return [b0, b1]
