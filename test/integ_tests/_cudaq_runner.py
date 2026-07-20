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

"""Subprocess runner: python _cudaq_runner.py <fixture> <shots> [profile] [kernel_args_json]"""

from __future__ import annotations

import importlib.util
import json
import sys
from collections import Counter

import cudaq
import qirtoqasm
from braket.devices import LocalSimulator
from braket.ir.openqasm import Program


_SAMPLE_UNSUPPORTED = (
    "conditional feedback",
    "branch on measurement",
    "return type",
    "return None",
)
_RUN_UNSUPPORTED = ("must return a value", "non-void return")


def load_fixture(fixture_path: str):
    spec = importlib.util.spec_from_file_location("fixture", fixture_path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def native_sim(mod, kernel_args: tuple, shots: int) -> dict | None:
    try:
        result = cudaq.sample(mod.kernel, *kernel_args, shots_count=shots)
        return {bs: result.count(bs) for bs in result}
    except RuntimeError as e:
        if not any(k in str(e) for k in _SAMPLE_UNSUPPORTED):
            raise

    try:
        run_results = cudaq.run(mod.kernel, *kernel_args, shots_count=shots)
        c = Counter()
        for r in run_results:
            bits = (
                "".join("1" if b else "0" for b in r)
                if isinstance(r, list)
                else ("1" if r else "0")
            )
            c[bits] += 1
        return dict(c)
    except RuntimeError as e:
        if not any(k in str(e) for k in _RUN_UNSUPPORTED):
            raise

    return None


def braket_pipeline(mod, kernel_args: tuple, profile: str, shots: int) -> tuple[dict, str]:
    qir_text = cudaq.translate(mod.kernel, *kernel_args, format=profile)
    openqasm = qirtoqasm.translate(qir_text)
    result = LocalSimulator().run(Program(source=openqasm), shots=shots).result()
    return dict(result.measurement_counts), openqasm


def main():
    fixture_path = sys.argv[1]
    shots = int(sys.argv[2])
    profile = sys.argv[3] if len(sys.argv) > 3 else "qir-adaptive"
    kernel_args = tuple(json.loads(sys.argv[4])) if len(sys.argv) > 4 else ()

    mod = load_fixture(fixture_path)
    native_counts = native_sim(mod, kernel_args, shots)
    braket_counts, openqasm = braket_pipeline(mod, kernel_args, profile, shots)

    payload = {"counts": braket_counts, "openqasm": openqasm}
    if native_counts is not None:
        payload["native_counts"] = native_counts
    print(json.dumps(payload))


if __name__ == "__main__":
    main()
